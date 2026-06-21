// Browsterm workspace client.
// Test it before you change it. The IIFEs below run in order: first the
// terminal tab manager, then the file-explorer sidebar, then the preview
// pane. Each owns an independent slice of the DOM so they do not fight
// over the same hooks.

(function () {
  "use strict";

  // Browsterm terminal tab manager.
  // Owns the WebSocket, the tab roster (each tab = one xterm.js Terminal
  // + one PtySession on the server), the tab strip DOM, and the
  // workspace-panes flex container.
  //
  // The single multiplexed /ws connection carries JSON control envelopes
  // (`create-tab`/`close-tab`/`rename`/`resize`/`input`/`hello`/`tab-ack`/
  // `tab-event`) and binary PTY bytes prefixed with a 4-byte u32 LE tab
  // id. A network blip reconnects with capped exponential backoff plus
  // jitter; the server keeps the tab roster alive across WS drops, so a
  // reconnect greets the new socket with `hello` listing whatever tabs
  // were running.
  const status = document.getElementById("conn");
  const dimsEl = document.getElementById("dims");
  const tabStrip = document.querySelector(".tabs");
  const panesHost = document.getElementById("workspace-panes");
  const newTabBtn = document.getElementById("tab-new");

  if (
    !tabStrip ||
    !panesHost ||
    !newTabBtn ||
    typeof window.Terminal === "undefined"
  ) {
    if (status) {
      status.textContent =
        "xterm.js or tab markup missing — refresh once online";
      status.className = "error";
    }
    return;
  }

  // Drop any leftover hardcoded tab children from older markup. The
  // strip is now tab-manager owned; the ＋ button is anchored at the
  // right edge.
  while (tabStrip.firstChild && tabStrip.firstChild !== newTabBtn) {
    tabStrip.removeChild(tabStrip.firstChild);
  }

  const PROTO = location.protocol === "https:" ? "wss:" : "ws:";
  const wsPath = PROTO + "//" + location.host + "/ws";

  // Capped exponential backoff for WS reconnects. Same constants the
  // pre-tabs manager used so the visual feels identical.
  const BACKOFF_MIN_MS = 250;
  const BACKOFF_MAX_MS = 30000;

  /** Map<TabId, TabState> — single source of truth for tab state. */
  const tabs = new Map();
  /** Currently-rendered tab id (the one whose .pane.is-active is on). */
  let activeTabId = null;

  let ws = null;
  let attempt = 0;
  let reconnectTimer = null;
  let stopped = false;

  const theme = {
    background: "#0e1116",
    foreground: "#e6edf3",
    cursor: "#e6edf3",
    selectionBackground: "#264f78",
  };
  const fontConfig = {
    cursorBlink: true,
    fontFamily:
      '"JetBrains Mono", "Fira Code", "SF Mono", Menlo, Consolas, monospace',
    fontSize: 14,
    lineHeight: 1.2,
    scrollback: 10000,
    theme,
  };

  function setStatus(text, cls) {
    status.textContent = text;
    status.className = cls || "";
  }

  function sendEnvelope(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
  }

  function applyLocalRename(tabId, label) {
    const tab = tabs.get(tabId);
    if (!tab) return;
    tab.label = label;
    const span = tab.tabBtn.btn.querySelector(".tab-name");
    if (span) span.textContent = label;
  }

  function switchToTab(tabId) {
    if (activeTabId === tabId) {
      // No-op, but still refit + report dims so the active terminal
      // recovers after a layout shift that did not come through
      // window.resize (e.g. preview pane toggling).
      refitActiveAndReport();
      return;
    }
    if (activeTabId != null) {
      const prev = tabs.get(activeTabId);
      if (prev) {
        prev.pane.classList.remove("is-active");
        prev.tabBtn.btn.classList.remove("is-active");
      }
    }
    const next = tabs.get(tabId);
    if (!next) {
      activeTabId = null;
      return;
    }
    next.pane.classList.add("is-active");
    next.tabBtn.btn.classList.add("is-active");
    activeTabId = tabId;
    // Defer fit+focus until the new flex layout has settled. rAF
    // ensures CSS painted the visible pane before xterm measures.
    requestAnimationFrame(() => {
      refitActiveAndReport();
      try {
        next.term.focus();
      } catch (_) {}
    });
  }

  function refitActiveAndReport() {
    if (activeTabId == null) {
      dimsEl.textContent = "–";
      return;
    }
    const tab = tabs.get(activeTabId);
    if (!tab || !tab.fitAddon) return;
    tab.fitAddon.fit();
    const proposed = tab.fitAddon.proposeDimensions();
    if (!proposed) return;
    dimsEl.textContent = `${proposed.cols}×${proposed.rows}`;
    sendEnvelope({
      type: "resize",
      tab_id: activeTabId,
      cols: proposed.cols,
      rows: proposed.rows,
    });
  }

  // Expose manual refit helper so the preview iframe / sidebar etc.
  // can ask the active terminal to re-measure its host box after a
  // layout change that did not go through window.resize.
  window.__browsterm_refit = () => {
    refitActiveAndReport();
  };

  function nextBackoffMs() {
    const exp = Math.min(BACKOFF_MAX_MS, BACKOFF_MIN_MS * Math.pow(2, attempt));
    const jitter = Math.floor(Math.random() * Math.min(exp * 0.25, 1000));
    return exp + jitter;
  }

  function scheduleReconnect(reason) {
    if (stopped || reconnectTimer != null) return;
    attempt += 1;
    const delay = nextBackoffMs();
    const secs = (delay / 1000).toFixed(1);
    setStatus(
      `${reason} — reconnecting (attempt ${attempt}) in ${secs}s…`,
      "reconnecting"
    );
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
  }

  function connect() {
    if (stopped) return;
    ws = new WebSocket(wsPath);
    ws.binaryType = "arraybuffer";

    ws.addEventListener("open", () => {
      // On pagehide we close the socket while it is still connecting;
      // the browser will still fire `open` once before `close`. Guard
      // so we don't briefly mark the workspace as "connected" while
      // tearing down.
      if (stopped) {
        ws.close();
        return;
      }
      attempt = 0;
      setStatus("connected", "connected");
      // Tabs are server-driven from here: hello lists the current
      // roster, then create-tab/close-tab/rename envelopes mutate it.
    });

    ws.addEventListener("close", () => {
      ws = null;
      // Server keeps tabs alive across WS drops; a greet re-hello
      // reconciling the strip will arrive when the new socket opens.
      scheduleReconnect("disconnected");
    });

    ws.addEventListener("error", () => {
      setStatus("disconnected — reconnecting…", "reconnecting");
    });

    ws.addEventListener("message", (event) => {
      if (typeof event.data === "string") {
        try {
          handleServerJson(JSON.parse(event.data));
        } catch (_) {
          /* malformed JSON — ignore */
        }
        return;
      }
      // Binary frame: first 4 bytes = u32 LE tab_id, then PTY bytes.
      const u8 = new Uint8Array(event.data);
      if (u8.byteLength < 4) return;
      const tabId = new DataView(u8.buffer, u8.byteOffset, 4).getUint32(0, true);
      const payload = u8.subarray(4);
      const tab = tabs.get(tabId);
      if (tab) tab.term.write(payload);
    });
  }

  function handleServerJson(msg) {
    if (!msg || typeof msg.type !== "string") return;
    switch (msg.type) {
      case "hello": {
        // Reconcile: drop any locally-tracked tabs the server doesn't
        // list (defensive — possible during relaunch with a stale
        // local state) and ensure every server-listed tab has a
        // matching local entry.
        const incomingIds = new Set((msg.tabs || []).map((t) => t.tab_id));
        for (const id of Array.from(tabs.keys())) {
          if (!incomingIds.has(id)) removeLocalTab(id);
        }
        for (const t of msg.tabs || []) {
          ensureLocalTab(t.tab_id, t.label);
        }
        if (activeTabId == null && msg.tabs && msg.tabs.length > 0) {
          switchToTab(msg.tabs[0].tab_id);
        } else {
          refitActiveAndReport();
        }
        break;
      }
      case "tab-ack": {
        ensureLocalTab(msg.tab_id, msg.label);
        switchToTab(msg.tab_id);
        break;
      }
      case "tab-event": {
        if (msg.kind === "closed") {
          removeLocalTab(msg.tab_id);
        } else if (msg.kind === "renamed" && msg.label != null) {
          applyLocalRename(msg.tab_id, msg.label);
        }
        break;
      }
    }
  }

  function ensureLocalTab(tabId, label) {
    if (tabs.has(tabId)) {
      if (label && tabs.get(tabId).label !== label) {
        applyLocalRename(tabId, label);
      }
      return tabs.get(tabId);
    }

    const pane = document.createElement("section");
    pane.className = "pane";
    pane.dataset.tabId = String(tabId);
    const host = document.createElement("div");
    host.className = "terminal-host";
    pane.appendChild(host);
    panesHost.appendChild(pane);

    const tabBtn = makeTabButton(tabId, label);
    // Insert the new tab button right before newTabBtn so the ＋
    // control always trails the strip.
    tabStrip.insertBefore(tabBtn.btn, newTabBtn);

    const term = new window.Terminal(fontConfig);
    const fitAddon =
      typeof window.FitAddon !== "undefined"
        ? new window.FitAddon.FitAddon()
        : null;
    if (fitAddon) term.loadAddon(fitAddon);
    term.open(host);

    const tab = {
      tab_id: tabId,
      label,
      term,
      host,
      pane,
      tabBtn,
      fitAddon,
    };

    // Per-Terminal handlers capture `tabId` via closure so xterm.js
    // events route by tab_id even when multiple terminals are alive.
    term.onData((data) => {
      sendEnvelope({ type: "input", tab_id: tabId, data });
    });
    term.onResize(({ cols, rows }) => {
      if (tabId === activeTabId) {
        dimsEl.textContent = `${cols}×${rows}`;
      }
      sendEnvelope({ type: "resize", tab_id: tabId, cols, rows });
    });

    tabs.set(tabId, tab);
    return tab;
  }

  function removeLocalTab(tabId) {
    const tab = tabs.get(tabId);
    if (!tab) return;
    try {
      if (tab.fitAddon) tab.fitAddon.dispose();
    } catch (_) {}
    try {
      tab.term.dispose();
    } catch (_) {}
    if (tab.pane.parentNode) tab.pane.parentNode.removeChild(tab.pane);
    if (tab.tabBtn.btn.parentNode) {
      tab.tabBtn.btn.parentNode.removeChild(tab.tabBtn.btn);
    }
    tabs.delete(tabId);
    // If the active tab is the one we just removed, hand focus to the
    // closest surviving neighbour: same direction the strip already
    // reads (left for left, right for right). Surviving smaller ids
    // take precedence so chain-closes from the right don't make focus
    // jump all the way to the left edge.
    if (activeTabId === tabId) {
      const remaining = Array.from(tabs.keys()).sort((a, b) => a - b);
      if (remaining.length === 0) {
        activeTabId = null;
        dimsEl.textContent = "–";
        refitActiveAndReport();
        return;
      }
      const smaller = remaining.filter((id) => id < tabId);
      const larger = remaining.filter((id) => id > tabId);
      let nextId;
      if (smaller.length > 0) {
        nextId = smaller[smaller.length - 1];
      } else {
        nextId = larger[0];
      }
      switchToTab(nextId);
    }
  }

  function makeTabButton(tabId, label) {
    const btn = document.createElement("button");
    btn.className = "tab";
    btn.type = "button";
    btn.dataset.tabId = String(tabId);

    const nameSpan = document.createElement("span");
    nameSpan.className = "tab-name";
    nameSpan.textContent = label || "";
    btn.appendChild(nameSpan);

    const closeBtn = document.createElement("button");
    closeBtn.className = "tab-close";
    closeBtn.type = "button";
    closeBtn.title = "Close tab (Ctrl+W)";
    closeBtn.setAttribute("aria-label", "Close tab");
    closeBtn.textContent = "✕";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      sendEnvelope({ type: "close-tab", tab_id: tabId });
    });
    btn.appendChild(closeBtn);

    btn.addEventListener("click", () => switchToTab(tabId));
    btn.addEventListener("dblclick", (e) => {
      e.preventDefault();
      startRename(tabId, nameSpan);
    });

    return { btn, nameSpan };
  }

  function startRename(tabId, nameSpan) {
    if (!nameSpan) return;
    const tab = tabs.get(tabId);
    if (!tab) return;
    const input = document.createElement("input");
    input.type = "text";
    input.value = tab.label;
    input.size = Math.max(8, Math.min(40, tab.label.length + 2));
    let committed = false;
    const parentBtn = nameSpan.parentElement;
    parentBtn.classList.add("is-renaming");
    nameSpan.textContent = "";
    nameSpan.appendChild(input);
    input.focus();
    input.select();

    const finish = (commit) => {
      if (committed) return;
      committed = true;
      const value = (input.value || "").trim();
      // Drop the input; restore the label span.
      nameSpan.textContent = tab.label;
      parentBtn.classList.remove("is-renaming");
      if (commit && value && value !== tab.label) {
        sendEnvelope({ type: "rename", tab_id: tabId, label: value });
      }
    };

    input.addEventListener("blur", () => finish(true));
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        input.blur();
      } else if (e.key === "Escape") {
        e.preventDefault();
        finish(false);
      }
    });
  }

  function createNewTab(label) {
    // Pull dims from the current active tab if any so the new PTY
    // starts at the user's grid and avoids the Tier-1 banner flash.
    let cols = 80;
    let rows = 24;
    if (activeTabId != null) {
      const tab = tabs.get(activeTabId);
      if (tab) {
        if (tab.fitAddon) {
          const p = tab.fitAddon.proposeDimensions();
          if (p) {
            cols = p.cols;
            rows = p.rows;
          }
        } else {
          cols = tab.term.cols;
          rows = tab.term.rows;
        }
      }
    }
    sendEnvelope({
      type: "create-tab",
      cols,
      rows,
      label: typeof label === "string" ? label : undefined,
    });
  }

  function cycleActive(direction) {
    const ids = Array.from(tabs.keys()).sort((a, b) => a - b);
    if (ids.length === 0) return;
    let idx = activeTabId == null ? -1 : ids.indexOf(activeTabId);
    // Modulo so direction wraps from end to start and vice versa.
    idx = (idx + direction + ids.length) % ids.length;
    switchToTab(ids[idx]);
  }

  // Global keyboard shortcuts. Ctrl/Cmd+T : new tab; Ctrl/Cmd+W : close
  // active tab; Ctrl/Cmd+Tab : cycle. Skip when focus is in an input
  // (rename editing) or text area so the keystroke still works as typed.
  window.addEventListener("keydown", (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    const tag = (document.activeElement && document.activeElement.tagName) || "";
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    const k = e.key.toLowerCase();
    if (k === "t") {
      e.preventDefault();
      createNewTab();
    } else if (k === "w") {
      e.preventDefault();
      if (activeTabId != null) {
        sendEnvelope({ type: "close-tab", tab_id: activeTabId });
      }
    } else if (e.key === "Tab") {
      e.preventDefault();
      cycleActive(e.shiftKey ? -1 : +1);
    }
  });

  newTabBtn.addEventListener("click", () => createNewTab());

  window.addEventListener("resize", () => {
    refitActiveAndReport();
  });

  // Stop reconnecting once the page is going away; otherwise a closed
  // tab would keep its reconnect timer alive in the background.
  window.addEventListener("pagehide", () => {
    stopped = true;
    if (reconnectTimer != null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) ws.close();
  });

  setStatus("connecting…", "");
  connect();
})();

// Browsterm file-explorer sidebar.
// Polls GET /api/fs/list?path=... and renders a flat view of one directory
// at a time with click-to-navigate folders. Clicking a file is a no-op
// until the preview pane (Commit D) wires up /api/fs/file.
(function () {
  "use strict";

  const POLL_MS = 5000;

  const statusEl = document.getElementById("fs-status");
  const refreshEl = document.getElementById("fs-refresh");
  const showHiddenEl = document.getElementById("fs-show-hidden");
  const breadcrumbsEl = document.getElementById("breadcrumbs");
  const entriesEl = document.getElementById("entries");

  if (!breadcrumbsEl || !entriesEl || !refreshEl || !statusEl) {
    return; // sidebar markup not present
  }

  const sidebar = {
    currentPath: null,
    inFlight: 0,
    pollHandle: null,
    // Session-local toggle for POSIX dotfiles. Vision §2 names this as
    // a first-class sidebar feature; the value travels on every request
    // to /api/fs/list so the server filters cheaply (saves round-tripping
    // hidden entries a user has explicitly opted out of seeing).
    showHidden: showHiddenEl ? !!showHiddenEl.checked : true,
  };

  function setStatus(text) {
    statusEl.textContent = text;
  }

  function joinRelative(parent, child) {
    if (parent === "/") return "/" + child;
    return parent.replace(/\/+$/, "") + "/" + child;
  }

  // Single source of truth for "this row opens a previewable file".
  // Used by the sidebar click router in `renderEntries` *and* by the
  // preview-pane keyboard cycler so the two can never disagree about
  // which siblings are skippable. Adding a new file kind (e.g. CSV-as-
  // sortable-table in Tier 3) only needs to update this one predicate.
  function isOpenableRow(entry) {
    if (!entry) return false;
    if (entry.is_file) return true;
    return entry.is_symlink === true && entry.target_is_file === true;
  }

  async function navigate(target) {
    if (!target) return;
    const ticket = ++sidebar.inFlight;
    setStatus("loading\u2026");
    try {
      // `show_hidden` rides on every navigate so the server never hands
      // the browser entries the user has toggled off. `true` is the
      // MVP default (also the server's `serde(default)` fallback); we
      // only send it explicitly when the user flips the toggle off, so
      // the URL stays short for the common case.
      let url = "/api/fs/list?path=" + encodeURIComponent(target);
      if (!sidebar.showHidden) url += "&show_hidden=false";
      const res = await fetch(url, { headers: { Accept: "application/json" } });
      if (ticket !== sidebar.inFlight) return; // a newer call superseded us
      if (!res.ok) {
        let msg = res.statusText;
        try {
          const body = await res.json();
          if (body && body.error && body.error.message) msg = body.error.message;
        } catch (_) {}
        entriesEl.textContent = "";
        entriesEl.dataset.empty = "false";
        const err = document.createElement("div");
        err.className = "fs-error";
        err.textContent = `${res.status} \u2014 ${msg}`;
        entriesEl.appendChild(err);
        setStatus(res.status.toString());
        return;
      }
      const data = await res.json();
      if (ticket !== sidebar.inFlight) return;
      sidebar.currentPath = data.path;
      // Cache the listing so the preview-pane sibling cycler can step
      // through sibling files without a round-trip back to the server.
      // Survives navigate() so a single ArrowDown keystroke during a
      // preview never triggers a refetch before it resolves.
      sidebar.lastEntries = data.entries;
      renderBreadcrumbs();
      renderEntries(data.entries);
      // Surface the active filter inline so the count never reads as
      // ambiguous. Without this, a user toggling dotfiles away sees the
      // same `"5 items"` text as before and wonders why their `ls -la`
      // count disagrees; the parenthetical clamp makes the source of
      // the discrepancy obvious at a glance.
      setStatus(
        `${data.entries.length} item${data.entries.length === 1 ? "" : "s"}${sidebar.showHidden ? "" : " (hidden filtered)"}`
      );
    } catch (err) {
      if (ticket !== sidebar.inFlight) return;
      setStatus("error");
      entriesEl.textContent = "";
      const msg = document.createElement("div");
      msg.className = "fs-error";
      msg.textContent = "request failed: " + (err && err.message ? err.message : err);
      entriesEl.appendChild(msg);
    }
  }

  async function refresh() {
    if (sidebar.currentPath) await navigate(sidebar.currentPath);
    else await navigate(".");
  }

  function renderBreadcrumbs() {
    breadcrumbsEl.textContent = "";
    if (!sidebar.currentPath) return;
    const root = document.createElement("a");
    root.href = "#";
    root.textContent = "/";
    root.addEventListener("click", (e) => {
      e.preventDefault();
      navigate("/");
    });
    breadcrumbsEl.appendChild(root);
    if (sidebar.currentPath === "/") return;
    const parts = sidebar.currentPath.split("/").filter((p) => p.length > 0);
    let acc = "";
    for (const part of parts) {
      acc += "/" + part;
      const sep = document.createElement("span");
      sep.className = "sep";
      sep.textContent = "/";
      breadcrumbsEl.appendChild(sep);
      const a = document.createElement("a");
      a.href = "#";
      a.textContent = part;
      const linkTo = acc;
      a.addEventListener("click", (e) => {
        e.preventDefault();
        navigate(linkTo);
      });
      breadcrumbsEl.appendChild(a);
    }
  }

  function renderEntries(entries) {
    entriesEl.textContent = "";
    // Reset keyboard-nav selection on every render so a brand-new
    // directory never inherits the highlight from the previous one.
    // The first ArrowUp/ArrowDown keystroke lands on row 0.
    sidebarRowIndex = -1;
    if (!entries || entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "fs-empty";
      empty.textContent = "(empty directory)";
      entriesEl.appendChild(empty);
      return;
    }
    for (const entry of entries) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "fs-row";
      if (entry.is_dir) row.classList.add("is-dir");
      if (entry.is_file) row.classList.add("is-file");
      if (entry.is_symlink) row.classList.add("is-symlink");

      const name = document.createElement("span");
      name.className = "fs-name";
      name.textContent = entry.name;
      row.appendChild(name);

      if (entry.is_symlink && entry.symlink_target) {
        const meta = document.createElement("span");
        meta.className = "fs-meta";
        meta.textContent = "\u2192 " + entry.symlink_target;
        row.appendChild(meta);
      }

      if (entry.is_dir) {
        row.addEventListener("click", () => navigate(joinRelative(sidebar.currentPath, entry.name)));
      } else if (isOpenableRow(entry)) {
        const rowPath = joinRelative(sidebar.currentPath, entry.name);
        const rowName = entry.name;
        const rowMime = entry.mime || "";
        row.addEventListener("click", () => openFile(rowPath, rowName, rowMime));
      } else if (entry.is_symlink) {
        // Symlinks often act like their target, so we route by target kind
        // (set in fs.rs through one level of follow via std::fs::metadata).
        // The server canonicalises on every request, so the symlink path
        // is forwarded verbatim — the user sees the target's bytes / its
        // listing without having to chase the link manually. Only symlinks
        // that resolve to directories or that are broken / special end up
        // here now; the "resolves to a file" case is handled by the
        // `isOpenableRow` branch above.
        const rowPath = joinRelative(sidebar.currentPath, entry.name);
        const rowName = entry.name;
        const rowMime = entry.mime || "";
        if (entry.target_is_dir === true) {
          row.addEventListener("click", () => navigate(rowPath));
        } else if (
          entry.target_is_dir === false ||
          entry.target_is_file === false
        ) {
          // Resolved but neither dir nor file: a device, pipe, or socket.
          // The row stays visually inert so the affordance stays honest.
          row.classList.add("is-disabled");
          row.disabled = true;
          row.title =
            "special file (device, pipe, or socket) \u2014 not previewable";
        } else {
          // Broken symlink: target missing or unreadable. The row stays
          // visually inert so the affordance stays honest.
          row.classList.add("is-disabled");
          row.disabled = true;
          row.title = "broken symlink \u2014 target is missing or unreadable";
        }
      }
      entriesEl.appendChild(row);
    }
  }

  refreshEl.addEventListener("click", () => refresh());

  if (showHiddenEl) {
    // Toggle fires a refresh against the current path. We do not call
    // `navigate("/")` here so a user mid-tree who flips the dotfile
    // visibility sees the change in place without losing their spot.
    showHiddenEl.addEventListener("change", () => {
      sidebar.showHidden = !!showHiddenEl.checked;
      refresh();
    });
  }

  // --- Sidebar keyboard navigation ----------------------------------------
  // Up/Down/Enter/Backspace/Home/End mirror the row-step convention
  // every system file picker uses (Finder, VS Code's explorer, Windows
  // Explorer's tree). Disabled rows (broken symlinks, special files)
  // are filtered out of the navigation set so the user cannot activate
  // them — the `.fs-row.is-disabled` style already keeps them visually
  // inert and the tooltip is the same hint whatever the input device.
  let sidebarRowIndex = -1;

  function getNavigableRows() {
    // Live querySelector on every keypress is fine: this list is at most
    // thousands of nodes, the call only walks the entries container,
    // and the browser caches the tree for one frame.
    return Array.from(entriesEl.querySelectorAll(".fs-row:not(.is-disabled)"));
  }

  function setSidebarRowIndex(next, rows) {
    if (rows.length === 0) {
      sidebarRowIndex = -1;
      return;
    }
    sidebarRowIndex = Math.max(0, Math.min(rows.length - 1, next));
    for (let i = 0; i < rows.length; i++) {
      rows[i].classList.toggle("is-active", i === sidebarRowIndex);
    }
    // Pull focus onto the row so screen readers and the next keystroke
    // both land on the highlighted entry.
    rows[sidebarRowIndex].focus();
    // Scroll into view if needed so a long listing doesn't strand the
    // selected row under the breadcrumb bar.
    rows[sidebarRowIndex].scrollIntoView({ block: "nearest" });
  }

  entriesEl.addEventListener("keydown", (e) => {
    const rows = getNavigableRows();
    if (rows.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSidebarRowIndex(sidebarRowIndex < 0 ? 0 : sidebarRowIndex + 1, rows);
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSidebarRowIndex(sidebarRowIndex < 0 ? rows.length - 1 : sidebarRowIndex - 1, rows);
      return;
    }
    if (e.key === "Home") {
      e.preventDefault();
      setSidebarRowIndex(0, rows);
      return;
    }
    if (e.key === "End") {
      e.preventDefault();
      setSidebarRowIndex(rows.length - 1, rows);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const row = rows[sidebarRowIndex];
      if (row) row.click();
      return;
    }
    if (e.key === "Backspace") {
      e.preventDefault();
      // Step the sidebar one level up. From `/` we no-op so a user
      // holding Backspace doesn't endlessly try to chdir into a non-
      // existent parent.
      if (!sidebar.currentPath || sidebar.currentPath === "/") return;
      const parent = sidebar.currentPath.replace(/\/[^/]*$/, "") || "/";
      navigate(parent);
      return;
    }
  });

  // Poll. Pause when the tab is hidden so a backgrounded workspace does
  // not hammer the FS every five seconds. Stop entirely on pagehide so
  // a tab that the user closed stops fetching on the BOS timer.
  const tick = () => {
    if (document.visibilityState === "visible") refresh();
  };
  sidebar.pollHandle = setInterval(tick, POLL_MS);

  window.addEventListener("pagehide", () => {
    if (sidebar.pollHandle != null) {
      clearInterval(sidebar.pollHandle);
      sidebar.pollHandle = null;
    }
  });

  // Initial navigation; backend defaults to process cwd on empty path.
  navigate(".");

  // Expose for debugging only; not part of the contract.
  window.__browsterm_sidebar = sidebar;

  // --- Preview pane ---------------------------------------------------------
  // Click a file row to populate the preview pane. Browser-native rendering
  // for anything the browser already knows how to display (images, audio,
  // video, PDF, HTML in a sandboxed iframe); fetch+pre for text-y MIME;
  // a centred download button otherwise. Esc or the × button returns the
  // workspace to single-pane mode. The helpers below no-op when the
  // preview markup is missing so the sidebar IIFE stays portable.
  const workspaceEl = document.querySelector(".workspace");
  const previewEl = document.getElementById("preview");
  const previewNameEl = document.getElementById("preview-name");
  const previewMimeEl = document.getElementById("preview-mime");
  const previewBodyEl = document.getElementById("preview-body");
  const previewCloseEl = document.getElementById("preview-close");
  const previewReady =
    !!workspaceEl &&
    !!previewEl &&
    !!previewNameEl &&
    !!previewMimeEl &&
    !!previewBodyEl;

  // Tracks the *name* of the file currently shown in the preview pane
  // so the keyboard cycler (defined further down) can locate the row
  // inside `sidebar.lastEntries`. Reset by closePreview.
  let currentEntryName = null;

  function previewEmpty() {
    previewBodyEl.textContent = "";
    const empty = document.createElement("div");
    empty.className = "preview-empty";
    empty.textContent = "Select a file to preview.";
    previewBodyEl.appendChild(empty);
  }

  function previewError(status, message) {
    previewBodyEl.textContent = "";
    const err = document.createElement("div");
    err.className = "preview-error";
    err.textContent = status + " \u2014 " + message;
    previewBodyEl.appendChild(err);
  }

  function fileUrl(target) {
    return "/api/fs/file?path=" + encodeURIComponent(target);
  }

  function isTextyMime(mime) {
    const m = (mime || "").toLowerCase();
    if (m.startsWith("text/")) return true;
    return (
      m === "application/json" ||
      m === "application/xml" ||
      m === "application/ld+json" ||
      m === "application/yaml" ||
      m === "application/x-yaml" ||
      m === "application/toml" ||
      m === "application/javascript" ||
      m === "application/x-shellscript" ||
      m === "application/sql" ||
      m === "application/x-ndjson"
    );
  }

  function previewRender(target, name, mime) {
    const m = (mime || "").toLowerCase();
    if (m.startsWith("image/")) {
      previewBodyEl.textContent = "";
      const wrap = document.createElement("div");
      wrap.className = "preview-img";
      const img = document.createElement("img");
      img.alt = name;
      img.src = fileUrl(target);
      wrap.appendChild(img);
      previewBodyEl.appendChild(wrap);
      return;
    }
    if (m.startsWith("audio/")) {
      previewBodyEl.textContent = "";
      const wrap = document.createElement("div");
      wrap.className = "preview-audio";
      const audio = document.createElement("audio");
      audio.controls = true;
      audio.preload = "metadata";
      audio.src = fileUrl(target);
      wrap.appendChild(audio);
      previewBodyEl.appendChild(wrap);
      return;
    }
    if (m.startsWith("video/")) {
      previewBodyEl.textContent = "";
      const wrap = document.createElement("div");
      wrap.className = "preview-video";
      const video = document.createElement("video");
      video.controls = true;
      video.preload = "metadata";
      video.src = fileUrl(target);
      wrap.appendChild(video);
      previewBodyEl.appendChild(wrap);
      return;
    }
    if (m === "application/pdf" || m.startsWith("application/pdf")) {
      // Browser-native PDF rendering via <iframe>. Works in Chromium and
      // Firefox without backend help. Falls back to download if the user's
      // browser lacks a built-in PDF viewer; they get the bytes anyway.
      // `startsWith` is defensive: MIME parameters (e.g. `application/pdf;
      // charset=binary`) shouldn't, but a future mime_guess or vendor
      // prefix shouldn't strand us either.
      previewBodyEl.textContent = "";
      const iframe = document.createElement("iframe");
      iframe.className = "preview-iframe";
      iframe.title = "PDF: " + name;
      iframe.src = fileUrl(target);
      previewBodyEl.appendChild(iframe);
      return;
    }
    if (m === "text/html" || m.startsWith("text/html")) {
      // sandbox="" with no allow-tokens guarantees a hostile local HTML
      // file cannot run scripts or postMessage back to the parent.
      previewBodyEl.textContent = "";
      const iframe = document.createElement("iframe");
      iframe.className = "preview-iframe";
      iframe.title = "HTML: " + name;
      iframe.sandbox = "";
      iframe.src = fileUrl(target);
      previewBodyEl.appendChild(iframe);
      return;
    }
    if (isTextyMime(mime)) {
      // Fetch the bytes as text and render them in <pre>. Errors fall
      // through to the same unified error view as everywhere else.
      previewBodyEl.textContent = "";
      const loading = document.createElement("div");
      loading.className = "preview-empty";
      loading.textContent = "loading\u2026";
      previewBodyEl.appendChild(loading);
      fetch(fileUrl(target), { headers: { Accept: "text/plain,*/*" } })
        .then((res) => {
          if (!res.ok) {
            return res
              .json()
              .then((body) =>
                previewError(
                  res.status,
                  (body && body.error && body.error.message) || res.statusText
                )
              )
              .catch(() => previewError(res.status, res.statusText));
          }
          return res.text();
        })
        .then((text) => {
          if (typeof text !== "string") return; // already rendered an error
          previewBodyEl.textContent = "";
          const pre = document.createElement("pre");
          pre.className = "preview-pre";
          pre.textContent = text;
          previewBodyEl.appendChild(pre);
        })
        .catch((err) =>
          previewError("network", (err && err.message) || String(err))
        );
      return;
    }
    // Catch-all: a centred Download button. The browser fetches the URL,
    // saves the bytes, and the response carries the right Content-Type.
    previewBodyEl.textContent = "";
    const a = document.createElement("a");
    a.className = "preview-download";
    a.href = fileUrl(target);
    a.download = name;
    a.textContent = "\u2193 Download " + name;
    previewBodyEl.appendChild(a);
  }

  function refitTerminal() {
    // Defer the terminal refit until the new flex layout has actually
    // settled. xterm.js measures the terminal-host box at fit() time, so
    // calling it before the browser has repainted would re-fit against
    // the stale width and not pick up the preview's share of the row.
    if (typeof window.__browsterm_refit === "function") {
      window.__browsterm_refit();
    }
  }

  function openFile(target, name, mime) {
    if (!target || !previewReady) return;
    // Track the name so cycleSibling can locate the current row in
    // the cached listing. Setting this *before* the rAF means a fast
    // ArrowDown keystroke immediately after open resolves against
    // the right entry.
    currentEntryName = name;
    previewEl.hidden = false;
    previewEl.classList.add("is-active");
    workspaceEl.classList.add("previewing");
    previewNameEl.textContent = name;
    previewMimeEl.textContent = mime || "";
    previewRender(target, name, mime);
    // Make the preview pane focusable on first open so the user can
    // immediately keystroke-arrow between siblings without an extra
    // click. focus({preventScroll:true}) keeps the layout-shaped
    // refit we just kicked via rAF from being undone by a viewport
    // jump.
    // tabIndex is the DOM-property equivalent of setAttribute("tabindex","0")
    // and idempotent if already set, so just assign outright instead of
    // guarding a redundant setAttribute call.
    previewEl.tabIndex = 0;
    requestAnimationFrame(() => {
      refitTerminal();
      previewEl.focus({ preventScroll: true });
    });
  }

  function closePreview() {
    if (!previewReady || !previewEl.classList.contains("is-active")) return;
    previewEl.classList.remove("is-active");
    previewEl.hidden = true;
    workspaceEl.classList.remove("previewing");
    previewNameEl.textContent = "Preview";
    previewMimeEl.textContent = "";
    // Drop the sibling-cycler's pointer; without this, a reopen of
    // the same name in a different directory would pretend to step
    // from the previous-directory selection.
    currentEntryName = null;
    // Blur the pane so a stray ArrowUp on a no-longer-rendered
    // preview doesn't kick off a fetch loop against a hidden file.
    previewEl.blur();
    previewEmpty();
    requestAnimationFrame(refitTerminal);
  }

  if (previewCloseEl) previewCloseEl.addEventListener("click", closePreview);

  // Esc closes the preview regardless of which pane currently has focus.
  // The check on `is-active` keeps the listener a no-op when the preview
  // is closed, so the keystroke is free to reach the terminal/xterm.
  window.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!previewReady) return;
    if (!previewEl.classList.contains("is-active")) return;
    e.preventDefault();
    closePreview();
  });

  if (previewReady) previewEmpty();

  // --- Preview-pane keyboard navigation (Arrows / Home / End) -----------.
  // ArrowUp/Down cycle through sibling files of the currently previewed
  // file; Home/End scroll the preview body to top/bottom. The handler
  // is attached to previewEl itself so the keystrokes only fire when
  // the user has clicked into the preview pane (so they don't steal
  // arrows from the terminal). The window-level Escape handler above
  // stays in place so closing-the-preview from anywhere still works
  // (matches browsers' modal-close convention).
  function cycleSibling(direction) {
    const last = sidebar.lastEntries;
    if (!last || last.length === 0) return;
    if (!currentEntryName) return;
    const idx = last.findIndex((e) => e.name === currentEntryName);
    if (idx < 0) return;
    const step = direction === "ArrowDown" ? 1 : -1;
    for (let i = idx + step; i >= 0 && i < last.length; i += step) {
      const e = last[i];
      // Same routing as the sidebar click handler: file-symlinks count
      // as files when their target resolves to one. Directories, broken
      // symlinks, and special files are skipped so a kbd-cycling user
      // never lands on a non-previewable row.
      if (!isOpenableRow(e)) continue;
      const path = joinRelative(sidebar.currentPath, e.name);
      openFile(path, e.name, e.mime || "");
      return;
    }
  }

  if (previewReady) {
    previewEl.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        cycleSibling(e.key);
        return;
      }
      if (e.key === "Home") {
        e.preventDefault();
        // Scroll both the body container and any inner scroller (the
        // <pre> for text, the <img> for images — though <img> doesn't
        // actually overflow). Snap to top is unconditionally 0 on both;
        // cheap to set, no-op for non-overflowing content.
        previewBodyEl.scrollTop = 0;
        const inner = previewBodyEl.firstElementChild;
        if (inner) inner.scrollTop = 0;
        return;
      }
      if (e.key === "End") {
        e.preventDefault();
        // Snap to *visible* bottom. Setting `scrollTop = scrollHeight`
        // pushes past the bottom edge when `scrollHeight === clientHeight`;
        // `Math.max(0, scrollHeight - clientHeight)` puts the bottom edge
        // on the screen whether the content fits or overflows.
        previewBodyEl.scrollTop = Math.max(
          0,
          previewBodyEl.scrollHeight - previewBodyEl.clientHeight
        );
        const inner = previewBodyEl.firstElementChild;
        if (inner) {
          inner.scrollTop = Math.max(
            0,
            inner.scrollHeight - inner.clientHeight
          );
        }
        return;
      }
    });
  }
})();
