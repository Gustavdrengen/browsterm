// Browsterm workspace client.
// Wires the browser terminal (xterm.js + FitAddon) to the local WebSocket
// exposed at /ws. PTY bytes arrive as binary frames so graphic-protocol
// data (sixel, kitty) survives the round trip untouched.

(function () {
  "use strict";

  const status = document.getElementById("conn");
  const dims = document.getElementById("dims");
  const host = document.getElementById("term");

  if (typeof window.Terminal === "undefined") {
    status.textContent = "xterm.js failed to load — refresh once online";
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
  const ws = new WebSocket(proto + "//" + location.host + "/ws");
  ws.binaryType = "arraybuffer";

  function sendEnvelope(obj) {
    if (ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
  }

  function reportDims() {
    if (!fitAddon) return { cols: term.cols, rows: term.rows };
    const proposed = fitAddon.proposeDimensions();
    return proposed || { cols: term.cols, rows: term.rows };
  }

  ws.addEventListener("open", () => {
    status.textContent = "connected";
    const { cols, rows } = reportDims();
    dims.textContent = `${cols}×${rows}`;
    sendEnvelope({ type: "resize", cols, rows });
    term.focus();
  });

  ws.addEventListener("close", () => {
    status.textContent = "disconnected — refresh to retry";
  });

  ws.addEventListener("error", () => {
    status.textContent = "error — see console";
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
})();
