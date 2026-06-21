// Browsterm workspace client.
// Wires the browser terminal (xterm.js + FitAddon) to the local WebSocket
// exposed at /ws. PTY bytes arrive as binary frames so graphic-protocol
// data (sixel, kitty) survives the round trip untouched.
//
// The WebSocket is allowed to drop in the background — when the network blips
// or the server restarts, the client reconnects with capped exponential
// backoff plus jitter and refits the terminal on the new socket. Each new
// connection spawns a fresh PTY on the server; xterm.js keeps the visible
// scrollback so the user keeps their history.

(function () {
  "use strict";

  const status = document.getElementById("conn");
  const dims = document.getElementById("dims");
  const host = document.getElementById("term");

  if (typeof window.Terminal === "undefined") {
    setStatus("xterm.js failed to load — refresh once online", "error");
    host.textContent = "xterm.js is required.";
    return;
  }

  const term = new window.Terminal({
    cursorBlink: true,
    fontFamily:
      '"JetBrains Mono", "Fira Code", "SF Mono", Menlo, Consolas, monospace',
    fontSize: 14,
    lineHeight: 1.2,
    scrollback: 10000,
    theme: {
      background: "#0e1116",
      foreground: "#e6edf3",
      cursor: "#e6edf3",
      selectionBackground: "#264f78",
    },
  });
  const fitAddon =
    typeof window.FitAddon !== "undefined"
      ? new window.FitAddon.FitAddon()
      : null;
  if (fitAddon) term.loadAddon(fitAddon);
  term.open(host);

  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const wsPath = proto + "//" + location.host + "/ws";

  // Capped exponential backoff for WS reconnects. The geometric floor stops
  // the very first retry from feeling instant; the cap keeps a long server
  // outage from spinning so tight it pegs a CPU.
  const BACKOFF_MIN_MS = 250;
  const BACKOFF_MAX_MS = 30000;

  let ws = null;
  let attempt = 0;
  let reconnectTimer = null;
  let stopped = false;

  function setStatus(text, cls) {
    status.textContent = text;
    status.className = cls || "";
  }

  function sendEnvelope(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
  }

  function reportDims() {
    if (!fitAddon) return { cols: term.cols, rows: term.rows };
    const proposed = fitAddon.proposeDimensions();
    return proposed || { cols: term.cols, rows: term.rows };
  }

  function nextBackoffMs() {
    const exp = Math.min(
      BACKOFF_MAX_MS,
      BACKOFF_MIN_MS * Math.pow(2, attempt)
    );
    // Jitter up to 25% of the current delay (capped) so concurrent tabs do
    // not hammer the server in lockstep.
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
      // On pagehide we close the socket while it is still connecting; the
      // browser will still fire `open` once before `close`. Guard so we
      // don't briefly mark the workspace as "connected" while tearing down.
      if (stopped) {
        ws.close();
        return;
      }
      attempt = 0;
      setStatus("connected", "connected");
      if (fitAddon) fitAddon.fit();
      const { cols, rows } = reportDims();
      dims.textContent = `${cols}×${rows}`;
      // The server treats the first envelope we send as the cue to spawn
      // the PTY at our actual grid size. Sending `ready` (instead of
      // `resize`) is what eliminates the banner-flash Tier-1 bug: the PTY
      // never paints a frame at 80x24 before it is told the right dims.
      sendEnvelope({ type: "ready", cols, rows });
      term.focus();
    });

    ws.addEventListener("close", () => {
      ws = null;
      scheduleReconnect("disconnected");
    });

    ws.addEventListener("error", () => {
      // Browsers fire 'error' immediately before 'close' on a failed connect;
      // let 'close' own the timer so we never schedule twice.
      setStatus("disconnected — reconnecting…", "reconnecting");
    });

    ws.addEventListener("message", (event) => {
      if (typeof event.data === "string") {
        term.write(event.data);
      } else {
        // ArrayBuffer: wrap and pass to xterm.js, which accepts Uint8Array for
        // graphic-protocol data without UTF-8 lossy conversion.
        term.write(new Uint8Array(event.data));
      }
    });
  }

  term.onData((data) => sendEnvelope({ type: "input", data }));

  term.onResize(({ cols, rows }) => {
    dims.textContent = `${cols}×${rows}`;
    sendEnvelope({ type: "resize", cols, rows });
  });

  window.addEventListener("resize", () => {
    if (fitAddon) {
      fitAddon.fit();
      const { cols, rows } = reportDims();
      sendEnvelope({ type: "resize", cols, rows });
    }
  });

  // Expose a manual refit so other parts of the workspace (the file
  // preview pane) can ask the terminal to re-measure its host box after
  // a layout change that does not go through `window.resize` (i.e. when
  // a sibling flex child appears or disappears).
  window.__browsterm_refit = () => {
    if (!fitAddon) return;
    fitAddon.fit();
    const { cols, rows } = reportDims();
    sendEnvelope({ type: "resize", cols, rows });
  };

  // Stop reconnecting once the page is going away; otherwise a closed tab
  // would keep its reconnect timer alive in the background.
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
      } else if (entry.is_file) {
        const rowPath = joinRelative(sidebar.currentPath, entry.name);
        const rowName = entry.name;
        const rowMime = entry.mime || "";
        row.addEventListener("click", () => openFile(rowPath, rowName, rowMime));
      } else if (entry.is_symlink) {
        // Symlinks often act like their target, so we route by target kind
        // (set in fs.rs through one level of follow via std::fs::metadata).
        // The server canonicalises on every request, so the symlink path
        // is forwarded verbatim — the user sees the target's bytes / its
        // listing without having to chase the link manually.
        const rowPath = joinRelative(sidebar.currentPath, entry.name);
        const rowName = entry.name;
        const rowMime = entry.mime || "";
        if (entry.target_is_dir === true) {
          row.addEventListener("click", () => navigate(rowPath));
        } else if (entry.target_is_file === true) {
          row.addEventListener("click", () => openFile(rowPath, rowName, rowMime));
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
    previewEl.hidden = false;
    previewEl.classList.add("is-active");
    workspaceEl.classList.add("previewing");
    previewNameEl.textContent = name;
    previewMimeEl.textContent = mime || "";
    previewRender(target, name, mime);
    requestAnimationFrame(refitTerminal);
  }

  function closePreview() {
    if (!previewReady || !previewEl.classList.contains("is-active")) return;
    previewEl.classList.remove("is-active");
    previewEl.hidden = true;
    workspaceEl.classList.remove("previewing");
    previewNameEl.textContent = "Preview";
    previewMimeEl.textContent = "";
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
})();
