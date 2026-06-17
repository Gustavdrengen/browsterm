# AGENTS.md — Browsterm Operating Manual

> The repository operating manual for the Browsterm agent. Read end-to-end at the start of every session.

## 1. Mission

Browsterm is a single self-contained Rust binary that turns any Linux machine (laptop, server, Pi, dev VM, WSL) into a polished graphical terminal-and-files workspace that you drive from a browser. `VISION.md` is the source of truth for **what** Browsterm is and **who** it is for. This file is the source of truth for **how the agent works on it**.

## 2. Role of `VISION.md`, `INBOX.md`, `BLOCKED.md`

- `VISION.md` — user-owned. **Read-only** to the agent. Describes the product, audience, principles. Never rewritten, "improved," or reinterpreted without an explicit user instruction.
- `INBOX.md` — programmer → agent. The agent reads it at every session start. Items are addressed, declined-with-reason, or escalated as a vision hole. Items removed when resolved — these files describe the current state, not history.
- `BLOCKED.md` — agent → programmer. The agent writes here whenever it cannot proceed without external help (credentials, environment, vision-level decision it cannot reasonably make). Falls back to higher-tier work that does not need the unblock.

A blank `INBOX.md` and `BLOCKED.md` are signs the agent is in a fully agent-executable environment.

## 3. Decision hierarchy

1. System and safety constraints.
2. The user's direct instruction in the current conversation.
3. `VISION.md`.
4. `AGENTS.md`.
5. Other repository docs (`docs/`, specs, READMEs).
6. Existing code conventions.
7. General best practice.

In conflicts: vision wins over implementation details; current user instruction refines the current task; technical decisions are the agent's unless they materially change the vision.

## 4. Autonomy model

The agent **owns** — without asking — file layout, naming, formatting, linting, type-checks, test setup, build setup, documentation organization, internal workflow files, helper prompts, skills, reusable templates, scripts, automation, dependency choices for the chosen stack, and structural shape of the codebase.

The agent **does not own** the user-facing product intent expressed in `VISION.md`. Vision-shaped changes require user confirmation.

The agent **does not ask** the user to choose between approaches outside of (a) vision creation or modification, or (b) addressing an item in `INBOX.md` that genuinely needs clarification. Inside those two situations the agent asks the minimum required to remove the ambiguity. Outside them, the agent decides, briefly documents the decision inline (commit / code / this file), and keeps moving.

## 5. Priority discipline and state-of-play gate

### State-of-play gate (mandatory before choosing a task)

Before selecting the next task — and again whenever work resumes after any break — the agent:

1. Reads `INBOX.md` and `BLOCKED.md`. Processes open items per the cross-cutting-files contract.
2. Builds and runs the project. Confirms it boots, runs, and exits cleanly.
3. Exercises the product as a user would: opens the browser, types into the terminal, scrolls, resizes, closes.
4. Writes a dated **State of play** entry in §10 below: what works, what is broken / rough / missing in a way a user would notice, and what is "there" but feels bad to use.

The state-of-play note is the source of truth for what the product actually does today. Tests are evidence; the note is the truth.

### Priority tiers (strict order)

- **Tier 0 — Product is broken or unplayable.** Crashes on launch. Core loop doesn't function. UI blocks interaction. **Fix immediately.** Do not add features while a Tier 0 item exists.
- **Tier 1 — Product is painful, empty, or unfaithful to vision.** Core loop works but is boring, punishing, or unrewarding; first ten seconds are not fun; key feature described in `VISION.md` is missing. Resolve before adding new content.
- **Tier 2 — Missing capabilities explicitly in the vision.** Features the vision says the product should have that are not yet implemented. Resolve in vision order.
- **Tier 3 — Polish, depth, nice-to-haves.** Visual polish, sound, additional content beyond the vision, refactors for elegance, performance tuning. Resolve opportunistically.

Pick the **lowest-numbered** tier with unresolved items. Stay within the tier — promoting a Tier 3 task to skip Tier 0 or Tier 1 is forbidden.

### Anti-patterns

- "I'll add [feature] first because it's a natural next step" — features are not the default next step.
- "The current implementation is rough, but my time is better spent on [new thing]" — almost always wrong; rough core is Tier 0/1, the new thing is Tier 2/3.
- Adding auxiliary UI (menus, settings, dashboards, splash, on-boarding) while core experience is Tier 0/1.
- Treating the test suite as proof the product works — the state-of-play note beats tests.

## 6. Spec-driven development

For non-trivial behavior, the agent writes or updates a spec before or alongside implementation. Specs live next to or below the module they govern:

- Project-level: `VISION.md` (what) + `docs/architecture.md` (how the parts fit).
- Module-level: in a `specs/` directory adjacent to the module, named after the module and its version.
- Feature-level: for any feature that affects more than one file or has subtle invariants, write a short spec under `specs/<feature>.md`.

Spec workflow for non-trivial changes:
1. Identify the relevant spec scope.
2. Create or update the spec before or alongside the implementation.
3. Implement against the spec.
4. Add or update tests derived from the spec.
5. Validate that behavior matches the spec.
6. Update the spec if the implementation decision legitimately changes the intended behavior.

The state-of-play note is the source of truth for what the product does; the spec describes what it should do; the gap is the work.

## 7. Testing and verification

Testing is first-class, not a late-stage gate. Required by tier:

- **Tier 0 fix** requires a regression test that fails on the broken version and passes on the fix.
- **Tier 1 fix** requires a playtest observation recorded in the state-of-play note (or analogous user-exercise note) plus a regression test where applicable.
- **Tier 2 feature** requires a test that fails before the feature and passes after, derived from the spec.

Tier-bound requirements activate once the project has a build and a test runner (i.e., from "first runnable" onward). During bootstrap, the agent is creating the build and runner, not satisfying tier-based test requirements.

Tests live next to the module they cover whenever practical (`#[cfg(test)] mod tests`); integration tests live in `tests/`. Verification commands live in this file's §9. Browsterm-specific extra: a playtest always complements a Tier 0/1 change — terminal fidelity is sacred.

## 8. Commit policy

The agent commits aggressively at well-defined checkpoints. A change is commit-ready when **all three** hold:

- It compiles, type-checks, and the relevant test suite passes.
- No previously-working functionality is broken.
- The change can be described in one sentence.

**Mandatory commit triggers**: after any new feature, bug fix, refactor/rename/reorg, spec/doc/`AGENTS.md` change, test update, dependency manifest change, build config change. Always before starting an unrelated task. Always before ending a session. A non-trivial session that ends with zero commits is a failure mode.

**One logical change per commit.** A commit message that needs "and" in the headline is split. Tests, docs, config that belong with a code change share its commit. Re-run verification before each commit.

The repository-local git identity is mandatory and derived from `VISION.md`'s project name:

```
git config user.name "Browsterm Agent"
git config user.email "browsterm-agent@local"
```

## 9. Verification commands

The standard local verification recipe:

```
cargo build --release
cargo run --release -- --no-browser --port 8765
# In a second terminal:
curl http://127.0.0.1:8765/
```

For the agent's own self-test in a CI / headless context the same commands apply; a happy-path unit test in `src/terminal.rs::tests` exercises the WS protocol envelope.

## 10. State of play

The agent appends a dated entry here after every state-of-play gate. The section is capped at 10 entries; older entries are rotated out so trends stay visible without unbounded growth.

<!-- State-of-play entries inserted below. -->

#### - [2026-06-17] Tier-2 file-explorer sidebar UI

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (15/15 pass, +1 for `list_endpoint_defaults_to_cwd_when_path_empty`).
  - Workspace layout rebuilt as `flex`: `.sidebar { flex: 0 0 240px }` + `.pane { flex: 1 1 auto }`. The terminal pane still owns the right-of-sidebar region; the topbar/statusbar rows are unchanged.
  - `src/static/app.js` ships a sidebar IIFE: 5-second polling tick (paused on `document.visibilityState !== "visible"`, stopped on `pagehide`). Initial `navigate(".")` hits the Rust cwd default.
  - Race-safe navigate: each call takes an `inFlight` ticket; stale responses after a newer click are dropped before any DOM mutation, so rapid folder hops never paint an out-of-order listing.
  - Folders are click-to-navigate. Files render in the disabled visual state (no hover, muted opacity, cursor: default, tooltip "Preview pane ships in Commit D") so the affordance stays honest while the preview pane is not yet wired.
  - Symlinks are visually distinct: italic accent name + `U+2192` + target path with `text-overflow: ellipsis` so long targets can't push the row past the 240px column. `.entries` has `overflow-x: hidden` as the second line of defence.
  - Embedded `index.html` `/app.js` `/app.css` confirmed post-edit via curl on `/workspace/assets`: `/app.js` has the `navigate / renderBreadcrumbs / renderEntries / pagehide / setInterval(/ is-file.is-disabled` markers; `/app.css` has the new `.sidebar / .breadcrumbs / .fs-row / .fs-meta` selectors.

- **Broken / rough / missing** (user-visible):
  - **Clicking a file does nothing** until Commit D lands. The disabled visual state is the contract for now.
  - **No hidden-file toggle** (vision §2 explicit feature). MVP shows hidden files by default; a future polish commit adds the toggle.
  - **No recursive tree expansion.** Flat list + breadcrumbs is the MVP; recursive depth-first expansion ships later.
  - **No keyboard navigation** for the sidebar (arrow keys, Enter to open, Esc to go up). Tier-3 polish.
  - **No shared cwd pointer** between the sidebar and the active terminal pane. vision §2 wants the terminal cwd known to the workspace so "opening the project I'm in" takes one click; that's a Tier-2 follow-up.

- **Feels bad** (code is there but a user would notice):
  - `fs-error` rows render as plain text; a future polish pass adds a tinted background so they don't blend with normal entries.

> **Decision:** Ship the sidebar separately from the preview pane so each commit has a tight surface; the disabled-file affordance is honest about the missing seam. **Tier:** T2. **Evidence:** 15 passing unit tests; `curl` confirms the embedded assets contain the sidebar IIFE + new CSS selectors; the Bash binary boots clean on a fresh port with the post-edit assets served. **Trade-off:** until Commit D, file rows are visually inert; the tooltip + tooltip-style muted state is the load-bearing contract.

#### - [2026-06-17] Tier-2 file-explorer backend

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (14/14 pass, +7 from `fs`): `sanitize_path_rejects_nul_bytes`, `sanitize_path_rejects_empty_input`, `sort_layout_dirs_first_then_files_case_insensitive_no_follow`, `symlinks_are_listed_with_target_not_followed`, `circular_symlinks_do_not_hang_listing`, `read_file_bytes_rejects_oversize` (1 KiB cap, fast), `read_file_bytes_rejects_symlink`, `read_file_bytes_accepts_small_file`.
  - `GET /api/fs/list?path=...` returns JSON `{path, entries:[{name,is_dir,is_file,is_symlink,size,mtime_secs?,mime?,symlink_target?}]}`. Sort: directories first, then case-insensitive alphabetical. Hidden files visible by default. Symlinks carry `is_symlink:true` and `symlink_target`, never resolved.
  - `GET /api/fs/file?path=...` returns raw bytes with the correct `Content-Type`, capped at 8 MiB. Symlink chains resolve through `std::fs::canonicalize`; a symlink whose *canonicalised* form is still a symlink is refused with a `BadRequest`. Per the doc comment on the handler, the file endpoint matches the user's terminal visibility on the same socket, exactly like the rest of the workspace.
  - Uniform `{error:{code,message}}` JSON envelope with HTTP codes 400/403/404/500 mapped to `bad_request` / `forbidden` / `not_found` / `internal`.
  - Smoke (`./target/release/browsterm --no-browser --port 8769`): `/healthz` ok; `/api/fs/list?path=/tmp/.../smoke` returned `{... entries: [{name: "abc.txt", is_file:true, mime:"text/plain"...}, {name: "link.txt", is_symlink:true, symlink_target:"abc.txt"…}, {name: "sub", is_dir:true…}]}`; `/api/fs/file?path=.../abc.txt` returned the bytes; `/api/fs/list?path=/no/such/path` returned 404 + `{"error":{"code":"not_found","message":"path not found"}}`.

- **Broken / rough / missing** (user-visible):
  - **Sidebar UI not yet wired.** The endpoints are reachable but visually orphaned until the next commit. Vision §2 still wants the tree, breadcrumbs, command palette, hidden-file toggle, sortable columns, hover previews.
  - **No preview pane.** Today `/api/fs/file` serves bytes; the browser does the right thing for inline images and downloads but we don't yet have a syntax-highlighted text view, sortable CSV table, or hex fallback.
  - **No Range support** on `/api/fs/file`; an oversized file is rejected wholesale, not chunked. Tier-3 follow-up.
  - **No `--fs-max-bytes` CLI flag.** Cap is hard-coded at 8 MiB. Tier-3 follow-up.
  - **No integration test** that exercises the deferred-spawn protocol over an axum-in-process `/ws`. Carried forward from the deferred-spawn commit.

- **Feels bad** (code is there but a user would notice):
  - The two endpoints have deliberately different symlink semantics (see doc on `fs::file`). A future author might assume they're symmetric; the in-file comment is the load-bearing thing that prevents that mistake.

> **Decision:** Ship the listing + file endpoints first; defer the sidebar UI to the next commit so each commit's scope stays small. **Tier:** T2. **Evidence:** 14 unit tests + an end-to-end smoke run reproducing the contract against `/tmp` fixtures. **Trade-off:** until the next commit ships the sidebar, the user can curl the endpoints but not see them in the browser; that gap is acceptable for a single-commit cycle.

#### - [2026-06-17] Defer PTY spawn to client dims (Tier 1)

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (6/6 pass, including the new `ready_envelope_parses`).
  - The WS protocol now requires the client's first envelope to be `{type:"ready", cols, rows}`; the PTY does not exist until the server receives it. Subsequent envelopes are still `resize` (TIOCSWINSZ) and `input` (PTY write). `src/static/app.js` sends `ready` on every open, including reconnects, so the lifecycle is uniform.
  - Smoke (`./target/release/browsterm --no-browser --port 8768`): `/healthz` → `ok`, `/` → 200, `/app.js` → 200, `/app.css` → 200. Embedded `/app.js` line 124 sends `type:"ready"` on open; subsequent `type:"resize"` envelopes only fire on real size changes.
  - The original Tier-1 bug ("PTY spawned at 80 × 24 before the browser reports real dims, so banner-heavy shells flash briefly") is no longer reachable: there is no PTY to paint anything before the browser reports its dims. Comments in `app.js` and `terminal.rs` call this out so the next editor does not undo it.

- **Broken / rough / missing** (user-visible):
  - **Vision §2 backlog** still open (file explorer, splits/tabs/tear-off, bookmarks/themes, drag-drop, WSL door polish, PWA phone-mode, doctor command).
  - No axum-level integration test for the deferred-spawn contract. Unit tests cover envelope parse + PTY round-trip; a WS-lifecycle test needs an axum-in-process harness. Logged as the next Tier-3 harden.
  - No SRI on the two CDN scripts (existing TODO).
  - No Origin / Host validation on `/ws`.

- **Feels bad** (code is there but a user would notice):
  - On any pre-Ready noise from a misbehaving client (e.g. accidental binary frames before the first resize), the server logs only at `debug!` level. Misbehaving clients stay connected indefinitely until something closes the WS — by intentional choice; a hostile-client retry cap is a Tier-3 harden.

> **Decision:** Server waits for first `Ready` envelope before constructing the PTY, rather than sending a `ready` signal from the server and letting the client start. **Tier:** T1. **Evidence:** state-of-play above + new test `terminal::tests::ready_envelope_parses`. **Trade-off:** the server emits no startup envelope; the next author has to know to send `ready` first. Intentional: it puts the source-of-truth for "what dims is the user looking at" on the browser side, which is the only place it can be measured accurately.

#### - [2026-06-17] Reconnect Tier-1 fix

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` and `cargo test --release` still pass; all 5 unit tests (terminal envelope × 3, pty round-trip × 2) green.
  - Smoke (`./target/release/browsterm --no-browser --port 8768` then `curl`): `/healthz` → `ok`, `/` → 200, `/app.js` → 200, `/app.css` → 200. Tail of `/app.js` shows the new `reconnectTimer` / `pagehide` paths; `/app.css` shows the new `#conn.connected` / `#conn.reconnecting` / `#conn.error` color rules.
  - WS now self-heals: `close`/`error` schedule a fresh socket with capped exponential backoff (250 ms → 30 s) plus ≤ 25%-of-delay jitter (capped at 1 s); on `open` the client refits the terminal + sends a `resize` envelope. Status bar shows a live countdown in amber (`"disconnected — reconnecting (attempt N) in Xs…"`). xterm scrollback is preserved across the transition; the resize-driven prompt re-emit (already documented in `pty.rs`) gives the user a visible prompt before the second paint.
  - Stopped-on-pagehide: `pagehide` flips a `stopped` flag, clears the pending timer, and closes the socket; the `open` handler also guards against `stopped` so a tab being torn down during an in-flight connect cannot briefly flash "connected".

- **Broken / rough / missing** (user-visible):
  - **Vision §2 backlog** still open: no file explorer, no splits/tabs/tear-off, no bookmarks/themes/clipboard image flow, no CSV/image/PDF preview, no remote/encrypted mode, no PWA phone-mode polish.
  - **No Rust integration test** exercising the multiple-WS-upgrade lifecycle end-to-end. The change is purely client-side and existing Rust tests still pass; such a test would require an axum-in-process broker harness. Logged here as a Tier-3 harden.
  - WS auto-reconnect spawns a fresh PTY on every reconnect. In-flight output of a long-running pipe is gone, but the prompt comes back on the first paint. Acceptable; matches vision principle #6.
  - Banner-heavy shells still flash briefly before the first `resize` lands (carries forward from foundation commit).
  - No SRI on the two CDN script tags (existing TODO, Tier 3).
  - No Origin / Host validation on `/ws` (loopback-only today).

- **Feels bad** (code is there but a user would notice):
  - On long outages the status bar does the talking; the rest of the workspace reads as healthy. That is the *intended* invariant. Leave it.
  - Within one reconnect tick, the status text flips once from the `error` handler's `"disconnected — reconnecting…"` to the `close` handler's `"disconnected — reconnecting (attempt N) in Xs…"`. Eye won't catch it; could be unified behind a single source of truth later.

> **Decision:** Auto-reconnect lives in the browser only; the server treats every `/ws` connect as a fresh PtySession and the previous session's `terminal::run` task shuts the old shell down via `pty_arc.shutdown()` on its own drop. **Tier:** T1. **Evidence:** state-of-play bullet above + `terminal::run`'s drop-finalizer. **Trade-off:** no per-tab rate limit on new PTYs; a hot reconnect storm can briefly spawn many shells. Server-side backpressure is owed a Tier-3 harden before this becomes a problem.

#### - [2026-06-17] Foundation commit

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` produces the single `browsterm` binary (≈12 MB, `lto=thin` + `strip=symbols`).
  - `cargo test --release` runs 5 unit tests, all pass in ~0.05 s:
    - terminal.rs: JSON `resize` envelope parses, JSON `input` envelope parses, unknown `type` fails loudly.
    - pty.rs: round-trip pump (write `echo READYMARK` → see the echo back through `subscribe`).
    - pty.rs: late subscriber still receives bytes that arrive after `subscribe()`.
  - `./target/release/browsterm --no-browser --port 8768` boots, binds `127.0.0.1`, prints the URL, then `curl http://127.0.0.1:8768/` returns the embedded `index.html` (1480 bytes, includes `<!DOCTYPE html>` + `xterm.js` script tag), `/app.js` and `/app.css` return 200 with their actual content, `/healthz` returns the literal string `ok`, and any other path returns the literal `not found` (404).
  - Browser auto-opens to the URL on launch unless `--no-browser` is passed.
  - PTY bridge: spawns `$SHELL` (or `/bin/sh` fallback) under `portable-pty` with `TERM=xterm-256color` + `COLORTERM=truecolor`; broadcast channel streams PTY bytes to the WS handler; resize forwards to the kernel via TIOCSWINSZ.
  - WS protocol: client → server uses JSON envelopes `{type:"resize"|"input"}`; server → client uses binary frames carrying raw PTY bytes (so sixel/kitty graphic data survives the round trip untouched).
  - WS handler close-on-spawn-failure path: a clean `socket.close().await` is sent when `portable-pty` cannot spawn the shell, and the failure reason lands in the binary log (`tracing::warn!`).

- **Broken / rough / missing** (user-visible):
  - **No file explorer.** Vision §2 explicit feature; not yet started.
  - **No splits / tabs / tear-off.** Workspace is one pane, one tab.
  - **No bookmarks / themes / clipboard image flow / CSV, image, PDF previews.** Vision §2 missing.
  - **No remote / encrypted / authenticated mode.** Vision §2 friendliest goal post-cut.
  - **No Rust integration test exercising the WS↔PTY bridge end-to-end.** We unit-test envelope parsing and PTY byte flow but not the axum-internal websocket plumbing.
  - **No reconnect in `app.js`.** On WS close, the status bar says "disconnected — refresh to retry".
  - **No `sha384` SRI pinned on the CDN scripts.** A TODO comment exists in `index.html`; jsDelivr is the only supply-chain attack surface right now.
  - **No Origin / Host validation on `/ws`.** Acceptable for loopback; documented future-hardening.
  - **Linux / WSL is the verified home.** macOS native + Windows ConPTY paths are not exercised yet.

- **Feels bad** (code is there but a user would notice):
  - PTY is spawned at 80×24 before the browser reports actual cols/rows. Banner-heavy shells flash briefly until the `resize` envelope lands; rescued in practice by the resize-driven prompt re-emit.
  - WS reconnect after a transient network blip kills the only pane; the user has to refresh the browser tab to recover.

## 11. Decision recording

For non-obvious decisions, set a precedent, or could surprise the user on review, record a one-liner:

> **Decision:** [one-sentence description]. **Tier:** T0/T1/T2/T3. **Evidence:** [link to state-of-play bullet / file path / test result / run output]. **Trade-off:** [what is being deferred and why, if anything].

Tier and evidence are required. The trade-off line is required when the chosen action is not the most obvious one. A decision that cannot point to evidence is a guess — gather the evidence first.

## 12. Session-done checklist

A session is done when **all** of the following are true:

1. All Tier 0 and Tier 1 items from the state-of-play note are resolved, or blocked on a vision decision surfaced via the documented alert mechanism.
2. Every open item in `INBOX.md` is addressed, declined with reason, or escalated as a vision hole.
3. `BLOCKED.md` has been scanned and is up to date — no stale entries; every open entry still has a current "Tried / Needed / Impact" note.
4. All in-flight work is committed.
5. Build, tests, and smoke checks all pass.
6. State-of-play note is updated and the 10-entry cap is enforced.
7. The next session has a clear, evidenced starting point.

A session that adds new Tier 2/3 work while Tier 0/1 is open is not done.

## 13. Examples of good and bad decisions

**Good (recorded inline):**

> **Decision:** Vendor xterm.js + fit addon inside the binary per vision principle #1 (one self-contained deliverable). **Tier:** T0. **Evidence:** `VISION.md` §5 ("No telemetry, offline-after-first-load") + `Cargo.toml` `rust-embed` dependency. **Trade-off:** larger binary; deferred CDN for later.

**Bad (do not):**

> Add file-explorer UI as Tier 2 even though the terminal pane crashes intermittently — skipping Tier 0 because the new feature felt more "productive."

## 14. Doc model and size discipline

This file must stay under ~400 lines. Detailed conventions live under `docs/`, per-module specs under `specs/<module>/`, and recurring workflow instructions under `skills/` or another repository-owned support file when the host agent supports them. This file is the index; rest of the repo holds the detail.

Signs the doc model needs restructuring, not more content:

- A single section here is over ~100 lines.
- The same rule is repeated in three places.
- A workflow described here would be clearer as an invokable skill.
- A spec has outgrown its folder and is hiding a chunk of `AGENTS.md` as inline policy.
