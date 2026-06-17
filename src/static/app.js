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
      // Each WS connect is a fresh PTY; the resize triggers the shell to
      // re-emit its prompt, so the user sees the new session starting
      // without a manual refresh.
      sendEnvelope({ type: "resize", cols, rows });
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
