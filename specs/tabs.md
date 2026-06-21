# specs/tabs.md — Terminal Tab System Spec

> Feature spec for the Tier-2 "renameable tab strip" capability. Source of
> truth for what happens to the hardcoded `Terminal 1` after this commit.
> Mirrors the spec-driven workflow in `AGENTS.md` §6.

## Goal

Replace the hardcoded single-PTY "Terminal 1" UX with a tab strip. Each tab
is a live PTY under a name (default `Terminal N`; renames stick). The user
can:

- Open a new tab (Ctrl+T or `＋` button).
- Switch focus by clicking a tab header.
- Rename a tab (double-click).
- Close a tab (`×` in the tab strip or Ctrl+W on the active tab).

This sets up future INBOX items — most importantly "open text file in
neovim on a new terminal tab" — to land on top without re-shaping the
wire.

## Wire format

Single multiplexed WebSocket at `/ws`. JSON envelopes for control, binary
frames (with a 4-byte u32 LE tab-id prefix) for PTY output. The single-socket
design keeps the existing reconnect plumbing in one place, lets one
status cell describe N tabs, and means a "spawn a custom shell in a new
tab" future flow is just a server-side lookup, not a fresh TCP socket.

### Client → server envelopes (JSON, kebab-case `type`)

| Type         | Fields                                  | Effect                                                             |
|--------------|-----------------------------------------|--------------------------------------------------------------------|
| `create-tab` | `cols: u16, rows: u16, label?: string`  | Spawn a new PTY at these dims under the given (or auto) label      |
| `close-tab`  | `tab_id: u32`                           | Kill the named tab's PTY and remove it from the roster             |
| `rename`     | `tab_id: u32, label: string`            | Rename the named tab (trim, cap 64 chars; reject empty/whitespace) |
| `resize`     | `tab_id: u32, cols: u16, rows: u16`     | Forward dims to the named tab's PTY                                |
| `input`      | `tab_id: u32, data: string`             | Forward raw UTF-8 input bytes to the named tab's PTY               |

The legacy `ready` envelope and the legacy bare `resize` / `input` envelopes
(no `tab_id`) are no longer accepted: clients always address PTYs by tab
id through JSON. The legacy "client → server binary frame" path is gone
too — clients send input as JSON `input` envelopes.

### Server → client envelopes (JSON, kebab-case `type`)

| Type        | Fields                                                             | Sent when                                                     |
|-------------|--------------------------------------------------------------------|---------------------------------------------------------------|
| `hello`     | `tabs: [{ tab_id: u32, label: string }]`                           | Immediately on every WS upgrade, listing the current roster   |
| `tab-ack`   | `tab_id: u32, label: string`                                       | Reply to `create-tab` (server spawns PTY and acknowledges)    |
| `tab-event` | `tab_id: u32, kind: "closed"\|"renamed", label?: string`            | Lifecycle notification                                        |

### Server → client binary frames

Every binary frame from server is `[u32 LE tab_id][pty bytes]`. The
4-byte prefix demuxes into the named tab's xterm.js Terminal. PTY bytes
remain binary so sixel / kitty graphic protocols survive untouched.

## Server state

`ServerState` holds:

```text
inner.tabs        : Mutex<HashMap<TabId, TabRecord>>  // TabRecord { label, pty }
inner.next_tab_id : Mutex<u32>                        // starts at 1; wraps to 1
inner.next_label_n: Mutex<u32>                        // "Terminal N" auto-numbering
```

`TabRecord` is `Clone` (`PtySession` already `Clone` via `Arc`s).

The tab roster survives a WebSocket drop. On the next `/ws` upgrade the
server sends `hello` with whatever is still alive. A network blip on the
browser preserves the user's working tabs; a server restart wipes the
roster.

## Lifecycle

1. **WS upgrade.** Server calls `ensure_first_tab(state)`. If the roster
   is empty, it spawns a default tab eagerly at 80×24 dims, allocates a
   tab id, increments `next_default_label_n`, and inserts. Then sends
   `hello` with the roster sorted by tab id ascending.
2. **Client receives `hello`.** Builds a `TabState` and matching DOM for
   each tab. Activates the first one. Sends `resize` for the active tab.
   Output for inactive tabs continues to stream into their xterm.js
   Terminal buffers (xterm.js paints only when the pane swaps to
   `.is-active`).
3. **`create-tab`.** Server spawns a fresh `PtySession` at the supplied
   dims, inserts into the roster, spawns a per-tab PTY → browser
   forwarder task, replies `tab-ack`. Client adds to roster + tab strip
   + workspace panes host.
4. **`close-tab`.** Server kills the tab's PTY, removes from roster,
   aborts the forwarder task, emits `tab-event {kind:"closed"}`. Client
   removes DOM. If the active tab was closed, the next closest remaining
   tab becomes active. If the last tab is closed, the workspace stays
   open with an empty strip + `＋` button.
5. **`rename`.** Server validates (`trim().take(64)`; non-empty),
   updates `TabRecord.label`, emits `tab-event {kind:"renamed", label}`.
6. **PTY output.** Each tab has a per-tab broadcast subscription owned
   by its forwarder task. The forwarder prefixes the bytes with `tab_id`
   and sends a binary frame. The client demuxes by tab id.
7. **Input / resize.** Routed by tab id; silently dropped for unknown
   ids (defensive against racing reconnects).

## Validation

- `label`: trim → cap at 64 chars → reject empty / whitespace-only.
  An empty rename from the client is a no-op rather than blanking the
  visible label.
- `tab_id`: zero reserved; out-of-range treated as unknown slot.
- `cols` / `rows`: `max(1)` before forwarding to `PtySession`.

## Edge cases

- **WS upgrade races on empty roster** — `ensure_first_tab` holds the
  roster mutex across the spawn, so the second proceeeder sees a
  non-empty roster and short-circuits. No phantom duplicate default.
- **Closing the last tab** — roster becomes empty; UI shows the empty
  strip with `＋`. Server keeps the WS open awaiting the next create-tab.
- **Server restart** — roster is wiped; first WS upgrade after restart
  auto-creates one default tab. Clients see the new tab in `hello`.
- **Forwarder drop → broadcast cleanup** — on close-tab the broadcast
  receiver is dropped with the forwarder task; further bytes from the
  PTY broadcast go to "no receivers" (already a documented silent gap).
- **Binary-prefix frame for unknown tab id** — client ignores (defensive
  against in-flight frames during a close).
- **`create-tab` while another tab's spawn is in flight** — both succeed;
  each gets a distinct id because `allocate_tab_id` is monotonic.

## Acceptance tests

`terminal::tests` (Rust):

- `create_tab_envelope_parses` — wire-format conformance.
- `close_tab_envelope_parses` — same.
- `rename_envelope_parses` — same.
- `resize_with_tab_id_parses` — same.
- `input_with_tab_id_parses` — same.
- `unknown_envelope_fails_loudly` — guards against mis-routing.
- `hello_envelope_serializes_round_trip` — emission shape.
- `tab_event_envelope_serializes_round_trip` — same; asserts `closed`
  variant skips the optional label field.
- `encode_tab_bytes_prepends_little_endian_id` — wire-level PTY framing.

`server::tests` (Rust, optional Tier-3 follow-up — listed here so the
next author knows the seam exists):

- `allocate_tab_id_increments_monotonically` — counter never returns the
  same id twice within reason.
- `next_default_label_is_increasing` — adjacent calls produce N, N+1.

## Acceptance tests (manual / smoke)

1. `cargo run --release -- --no-browser --port 8770` ⇒ `curl :8770/` is
   200. Embedded `index.html` contains `id="tab-new"` and does not
   contain the legacy `data-pane="t1"` hardcoded button.
2. Launch a browser against `:8770`. The single default tab renders a
   PTY. Click `＋` twice: three tabs exist; each gets a fresh shell.
   Click `×` on the middle tab: the other two are still alive.
   Double-click the third tab to rename it. Type a name and press Enter:
   the strip label updates. Close the third tab. The remaining terminal
   in tab 1 is still responsive.
3. Refresh the browser. `hello` lists the tabs that were alive at
   reload. The PTYs in those tabs continue from wherever they were —
   the resize-driven prompt re-emit documented in `pty.rs` rescues
   visible state.
4. The dev server stays clean: `cargo build --release` produces no
   warnings; `cargo test --release` runs all prior + new tests green.
