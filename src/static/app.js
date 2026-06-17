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
  const breadcrumbsEl = document.getElementById("breadcrumbs");
  const entriesEl = document.getElementById("entries");

  if (!breadcrumbsEl || !entriesEl || !refreshEl || !statusEl) {
    return; // sidebar markup not present
  }

  const sidebar = {
    currentPath: null,
    inFlight: 0,
    pollHandle: null,
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
      const url = "/api/fs/list?path=" + encodeURIComponent(target);
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
      setStatus(`${data.entries.length} item${data.entries.length === 1 ? "" : "s"}`);
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
        // Preview pane ships in Commit D. Until then the row is visibly
        // disabled so the affordance is clear without a tooltip pop-up.
        row.classList.add("is-disabled");
        row.disabled = true;
        row.title = "Preview pane ships in Commit D";
      }
      entriesEl.appendChild(row);
    }
  }

  refreshEl.addEventListener("click", () => refresh());

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
})();
