# Browsterm

> A first-class graphical terminal **for** any Linux machine, **in** your browser — with WSL the friendliest of many doors.

A single self-contained Rust binary. One command — `browsterm` — opens a browser tab containing a workspace that is simpler than VS Code, richer than Windows Terminal, more portable than Codespaces, and more private than SaaS IDEs.

The product, the audience, the principles: see [`VISION.md`](VISION.md).
Operating rules for the agent: see [`AGENTS.md`](AGENTS.md).

## Quickstart

```
cargo run --release
```

Open `http://127.0.0.1:<port>/` (the binary prints the URL on launch). The browser tab contains the workspace.

## Flags

```
browsterm [OPTIONS]

  --port <PORT>      Port to bind on (default: 0 → random)
  --host <HOST>      Address to bind on (default: 127.0.0.1)
  --shell <SHELL>    Shell command (default: $SHELL or /bin/sh)
  --no-browser       Do not auto-open a browser tab on launch
  --log-level <LVL>  tracing-subscriber filter (default: info)
```
