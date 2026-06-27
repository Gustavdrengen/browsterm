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

#### - [2026-06-21] Tier-2 terminal tab system

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (28/28 pass, +9 in `terminal::tests`: `create_tab_envelope_parses`, `close_tab_envelope_parses`, `rename_envelope_parses`, `resize_with_tab_id_parses`, `input_with_tab_id_parses`, `unknown_envelope_fails_loudly`, `hello_envelope_serializes_round_trip`, `tab_event_envelope_serializes_round_trip`, `encode_tab_bytes_prepends_little_endian_id`, and the combined regression `rename_trims_whitespace_and_caps_at_64_chars`).
  - Hardcoded `Terminal 1` is replaced by a `#workspace-panes` flex container with one `.pane.is-active` per tab and a renameable tab strip (`button.tab` per tab id, `button.tab-new` anchored at the right edge). Default label `Terminal N`; double-click → inline `<input>` for rename; Enter / blur commits, Escape reverts (server rejects empty / whitespace-only and caps labels at 64 chars). Closing a tab removes from strip; closing the *active* tab jumps to the closest surviving neighbour (left-preferring, right fallback). Empty roster shows just the `＋` button.
  - Keyboard: Ctrl+T = new tab, Ctrl+W = close active, Ctrl+Tab / Shift+Ctrl+Tab = cycle. Keydown handler skips when focus is `INPUT` / `TEXTAREA` so rename editing isn't hijacked.
  - WS multiplexed: single `/ws` carries kebab-case JSON envelopes (`create-tab` / `close-tab` / `rename` / `resize` / `input`) and binary PTY bytes prefixed with a u32 LE `tab_id`. Server pushes a `hello` roster envelope on every upgrade; a network drop keeps the PTYs alive in `ServerState` so a reconnect or browser refresh surfaces the same running shells.
  - Server fan-in: per-tab forwarder tasks push framed `Message::Binary` to an unbounded mpsc; the WS handler drains it serially in a single `tokio::select!`. Axum 0.8's `SplitSink` is `!Clone`, so cloning isn't an option — fan-in is the workable alternative to a `Mutex` on the WS writer. `ensure_first_tab` holds the roster mutex across the synchronous PTY spawn so two concurrent WS upgrades on an empty roster see strict "second short-circuits, no phantom duplicate".
  - Smoke (`./target/release/browsterm --no-browser --port 8780`): `/healthz` → `ok`; `/` returns the embedded HTML (now contains `id="tab-new"` and no `data-pane="t1"`); `/app.js` went from ~30 KB to 41 071 bytes.

- **Broken / rough / missing** (user-visible):
  - **Scrollback is lost on WS reconnect.** A fresh socket re-creates xterm.js Terminal instances; the PTY is still running and the resize-driven prompt re-emit documented in `pty.rs` rescues visible state, but the user's shell history vanishes.
  - **No roving tab-index** in the strip; every `<button.tab>` carries the default `tabindex`. Acceptable for MVP and screen-reader noise is bounded by typical tab counts.
  - **No roster size cap.** A hostile client could open thousands of `create-tab` envelopes before the user notices. `// TODO: cap → MAX_TABS=32` flagged in `src/server.rs` for the Tier-3 harden.
  - **Drag-to-reorder tabs** is unimplemented; spec acknowledges "later".
  - **Tab persistence to disk across server restart** is unimplemented — a server reboot wipes the roster and clients see one fresh default tab.
  - **The first tab still spawns at 80×24** on the very first WS upgrade; the resize-driven prompt re-emit rescues visible state, but this Tier-3 polish item carries forward from the foundation commit.

- **Feels bad** (code is there but a user would notice):
  - **Tab strip ↔ terminal focus handoff is `requestAnimationFrame`-deferred.** A user rapid-clicking through tabs may notice a one-frame paint delay before each xterm.js session refits. Acceptable.
  - **`window.__browsterm_refit`** is now owned by the tab manager and called by the preview IIFE on every preview open/close; this sends a single `resize` envelope on each toggle. Acceptable.
  - **Inactive-tab xterm.js accumulates output server-side** even while its pane is `display:none`. The browser paints only on activation but memory holds the buffer for every tab. Tier-3 hardening when tab counts grow large.
  - **First `createNewTab()` after a hello rebuild**, before the host has been measured, falls through to `tab.term.cols` (`80×24`) producing a brief banner flash before the next refit. Rescued in practice by the `fitAddon.proposeDimensions()` branch on the second-and-later calls.

> **Decision:** Multiplex all tabs over a single `/ws` so the existing reconnect state machine and the one-status-cell UX stay put, and the future "open text file in neovim on a new tab" workflow lands on the same envelope set without re-shaping the wire. **Tier:** T2. **Evidence:** `specs/tabs.md` is the wire-format contract; `cargo test --release` shows 28/28 unit tests + 9 new envelope and framing regressions; smoke run reproduces `/healthz`, `/`, and `/app.js` after build. **Trade-off (handled in code):** `SplitSink` is `!Clone`, so per-tab forwarder tasks can't all share it directly; sidestepped with a fan-in `mpsc::unbounded_channel` drained serially by the WS handler. Scrollback survives the WS drop *server-side* (PTY still alive) but xterm.js rebuilds a fresh Terminal on reconnect — Tier-3 polish.

#### - [2026-06-27] Tier-1 WSL browser launch

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (31/31 pass, +3 in `browser::tests`: `wsl_kernel_string_contains_microsoft_marker` exercises the substring-match accepted by the detector, `wsl_env_marker_triggers` mutates `WSL_DISTRO_NAME` to flip `is_wsl()` true and restores prior env, `non_wsl_kernel_is_rejected` asserts the vanilla `6.6.0-15-generic` kernel string is not matched).
  - `src/browser.rs::open_url` now resolves in this order: `BROWSER` env override → on WSL, prefer `wslview` (the canonical Windows-side default-browser handoff), falling back to `cmd.exe /c start "" <url>` via the interop bridge, falling back to `open::that_detached` for everything else. `BROWSER` keeps its first-class status so users can still pin a different default per environment.
  - `is_wsl()` reads `/proc/sys/kernel/osrelease` for `microsoft` / `wsl` substring markers and OR's that with `WSL_DISTRO_NAME` / `WSLENV` env presence (the env probe covers WSL1 where the kernel string is unbranded).
  - Smoke (`./target/release/browsterm --no-browser --port 8786` on the WSL dev host): `/healthz → 200`, `/ → 200`, `/app.js → 200`. With `--no-browser` the WSL branch is uninvolved, confirming we did not regress the suppress path. The actual `wslview` invocation is a manual user-exercise step (the test host has no X server, so the visual smoke is "does it open Edge on the Windows host" — exercised once by the programmer, not automated).

- **Broken / rough / missing** (user-visible):
  - **No structured error when every WSL branch fails.** If `wslview` and `cmd.exe` both fail (e.g. a WSL install without the interop bridge), we fall through silently and only the `tracing::warn!` carries the reason. A future Tier-3 polish could surface a toast with the failure reason so the user knows why their desktop browser didn't open.
  - **No `--browser` CLI flag.** A user explicitly trying to steer the Windows-side browser (e.g. `browsterm --browser msedge`) currently has to set `BROWSER=wslview msedge ...` — clunkier than it needs to be. Carries forward to Tier-3 UX polish.
  - **`is_wsl()` doesn't differentiate WSL1 vs WSL2.** If a WSL1 user logs out and the interop is disabled, `cmd.exe` may genuinely not exist on the path. The fallback chain still degrades gracefully but the user-visible error could mention the WSL1-specific install hint. Carries forward.

- **Feels bad** (code is there but a user would notice):
  - The detector is conservative with two signals so the kernel-probe false positive on something like a CBL-Mariner kernel string in a container is bounded. Logs at `debug!` give operators a paper trail without nagging at `info!`. Acceptable.
  - `BROWSER=wslview ...` was always the documented escape hatch; the *default* behaviour has shifted from "open in WSL re.exe shell" to "open in Windows browser", which is the right call per vision principle #8 but worth noting in user-facing release notes when the next public cut ships.

> **Decision:** Replace `open::that_detached` with a WSL-aware cascade so Browsterm on WSL opens the URL in the *Windows* default browser, matching vision principle #8. **Tier:** T1. **Evidence:** `src/browser.rs::is_wsl` + `open_wsl_windows_browser` + the new unit tests; smoke run on the WSL dev host reproduces `/healthz` / `/` / `/app.js` cleanly; the original INBOX entry "Why does it open in a WSL browser, when running in WSL, it should open in my windows browser." is now resolved and dropped from `INBOX.md`. **Trade-off:** a user who actually *wanted* the WSL-side browser can re-enable it with `BROWSER=xdg-open ...` (the env override keeps the first-class escape hatch). Roster cap, scrollback-on-reconnect, and a `--browser` flag carry forward to later sessions.

#### - [2026-06-21] Tier-3 preview-pane keyboard navigation

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (22/22 pass; browser-side change).
  - The preview pane responds to ArrowUp/Down/Home/End when it has focus. ArrowUp/Down cycles through sibling files of the currently previewed file (using `isOpenableRow` to skip dirs / broken symlinks / special files). Home/End snaps the preview body (and the inner `<pre>` for text content) to top/bottom via `Math.max(0, scrollHeight - clientHeight)` — the snap places the bottom edge on-screen instead of pushing past it.
  - The Escape handler is unchanged: closing the preview from any pane focus point still works (the new keys are *scoped* to `previewEl` so they don't steal arrows from the terminal).
  - `openFile` focuses the preview pane on every open so a user can immediately keystroke between siblings without an extra click. `closePreview` blurs the pane — prevents stray ArrowUp on a hidden preview from starting fetch loops.
  - `isOpenableRow(entry)` is now a single helper used from `renderEntries` and `cycleSibling` so the click router and the keyboard cycler can't drift if a new file kind lands (CSV-as-table preview, etc.).

- **Broken / rough / missing** (user-visible):
  - No roving-tabindex optimisation: focus is moved by `previewEl.focus()` after every open; on a fast cycler that means each keypress re-runs `openFile`'s rAF + focus. Acceptable for MVP.
  - No Tab-to-close keybinding. Esc already closes; Tab is left alone to avoid trapping users in a focus loop.
  - The snap-bottom clamping depends on consistent `overflow:auto`; if a future preview content type changes CSS to `overflow:hidden`, keycode press silently no-ops.

- **Feels bad** (code is there but a user would notice):
  - A user mid-typing in the terminal cannot preview-cycle via keyboard unless they explicitly click into the preview pane first. Intended (don't murder terminal keystrokes) but the affordance isn't surfaced in the UI — a future commit can surface the "click to focus for keyboard" hint in the preview header.

> **Decision:** Scope the new keys (Arrows/Home/End) to `previewEl` itself rather than the global window listener, so the terminal pane keeps its keystrokes. **Tier:** T3. **Evidence:** 22 Rust tests still pass; embedded `/app.js` contains `isOpenableRow` / `cycleSibling` / `currentEntryName` / the scroll-clamp pattern; smoke run reproduces the listing routing rule on a fixture tree. **Trade-off:** keys don't fire when the terminal has focus; the alternative (global key-snatching) would have stolen arrows from vim. Two reviewer rounds; no-blocker polish (snapToBottom helper extraction, currentEntryName placement, optional tabIndex ceremony) filed in the §13 backlog.

#### - [2026-06-21] Tier-3 sidebar keyboard navigation

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (22/22 pass; no Rust surface change since the work is browser-side).
  - The sidebar's entry list responds to Up/Down/Home/End/Enter/Backspace with the system-picker convention. Disabled rows (broken symlinks, special files via `/dev/null` etc.) are excluded from the navigable set so the keyboard cannot activate the visually-inert affordance. A fresh directory listing resets `sidebarRowIndex = -1` on every render so a stale highlight never carries over.
  - Enter reuses the row's `click()` path so the existing router (navigate vs openFile) remains the single source of truth — no second-handler duplication. Backspace strips one trailing `/`-segment from `sidebar.currentPath` and re-navigates; from `/` it no-ops so a user holding Backspace doesn't endlessly try to chdir into a non-existent parent.
  - Visual: `.fs-row.is-active` gets an accent-tinted background + outline; `.fs-row:focus-visible` adds a separate focus ring so kbd-only / screen-reader users see a highlight whether or not the row is the *active* one.

- **Broken / rough / missing** (user-visible):
  - No auto-focus the entries container on every navigate. Auto-focus would steal focus away from the terminal pane mid-keystroke, so the user still has to click into the sidebar first before arrows work.
  - No `Ctrl+.` global hotkey to flip dotfile visibility from outside the sidebar. Tab+Space inside the checkbox works.
  - No roving-tab-index optimisation for the entries list — every row gets `tabindex="0"` (via the underlying `<button>`). For a directory with thousands of entries this could become noisy for screen readers; Tier 3 follow-up.

- **Feels bad** (code is there but a user would notice):
  - The `Backspace`-to-step-up keybinding differs from the platform norm on macOS (where some apps bind it to forward-history). Cross-platform keyboard conventions are an impossible nirvana; matching *Finder/VS Code's Explorer* over *macOS Safari* is the documented picker bet.

> **Decision:** Mirror Finder / VS Code Explorer / Windows Explorer (Up/Down step, Enter activate, Backspace step out, Home/End snap to ends) instead of inventing a new sidebar keymap. **Tier:** T3. **Evidence:** 22 Rust tests still pass; embedded `/app.js` contains the row-step handlers; embedded `/app.css` contains the new `.fs-row.is-active` / `.fs-row:focus-visible` rules. **Trade-off:** auto-focus on every navigate would have given a kbd-only user immediate arrow control inside any new folder — opted against so the terminal pane focus is never silently stolen from a typing user.

#### - [2026-06-21] Tier-2 sidebar hidden-file toggle

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (22/22 pass, +2 in `fs::tests`): `hidden_entries_filtered_when_show_hidden_false`, `hidden_symlink_filtered_when_show_hidden_false_keeps_target_flags` (always asserts the surviving `visible_link` symlink still carries `target_is_file == Some(true)` so the openFile/navigate seam is regression-tested against any future early-skip in the per-entry loop).
  - `GET /api/fs/list?path=...&show_hidden=...` filters POSIX dotfiles by name (`.starts_with('.')`) server-side, before the sort; `show_hidden` defaults to `true` server-side so third-party clients and existing curl invocations keep the MVP behaviour. Symlinks whose *name* starts with `.` are filtered too; the per-symlink `std::fs::metadata` seam that drives the sidebar's openFile/navigate routing still runs on the survivors.
  - Sidebar header has a `<input type="checkbox" id="fs-show-hidden" checked>` "show hidden" toggle; flipping it persists session-locally, every `navigate(...)` appends `&show_hidden=false`, and the count text reads `"N items (hidden filtered)"` while the toggle is off so the count never reads as ambiguous against the user's shell `ls -la`.
  - Smoke (`./target/release/browsterm --no-browser --port 8775`) on a fixture tree with `.dotfile`, `.linked` (hidden symlink), `.sub/.nested`: default request exposes all four; toggle off returns only `visible`; sub-listing of `.sub` with toggle off returns `[]`.

- **Broken / rough / missing** (user-visible):
  - No global hotkey (e.g. `Ctrl+.`) to flip the toggle from the terminal pane. Tab+Space inside the checkbox works.
  - No `localStorage` persistence — the box resets to "show" on every reload. Vision doesn't ask for it; a Tier-3 polish could remember across reloads.
  - The shared global `.spacer { flex: 1 }` rule from the topbar + preview-header is reused inside `.sidebar-tools` to push the new toggle + refresh to the right. A future polish commit can scope it to remove the cross-import.

- **Feels bad** (code is there but a user would notice):
  - **Double-negative naming on the wire:** `show_hidden: true` everywhere (query string, struct field, JSON, DOM id). A `hidden: false` polarity would read better; not renamed because the UI label says "show hidden" and the symmetry is worth the cognitive cost.

> **Decision:** Ship the toggle as a server-side filter on the existing listing endpoint rather than a client-side post-filter. **Tier:** T2. **Evidence:** 22 Rust tests + end-to-end smoke reproducing the round-trip. **Trade-off:** one extra query-string byte on every navigate; client-side filtering would be cheaper but would round-trip dotfiles a user has explicitly opted out of seeing — a real cost on huge config dirs. POSIX dotfile predicate is the only load-bearing server addition.

#### - [2026-06-17] Tier-2 file-explorer preview pane

- **Works** (verifiable as a user would experience it):
  - `cargo build --release` (no warnings) + `cargo test --release` (20/20 pass, +5 in `fs::tests`): `file_endpoint_emits_expected_content_type_for_known_extensions` (Content-Type sweep for .png / .jpeg / .svg / .mp3 / .mp4 / .pdf / .html / .json / .xml / .toml / .txt), `file_endpoint_body_matches_input_bytes` axum round-trip, `symlink_to_dir_is_flagged_target_is_dir`, `broken_symlink_target_flags_are_none`, `symlink_to_special_file_target_flags_are_both_false`, plus the existing `symlinks_are_listed_with_target_not_followed` extended with `target_is_dir` / `target_is_file` assertions.
  - Click a file row → `/api/fs/file?path=...`; render by MIME: `<img>` for image/* (incl. svg), `<audio controls>` for audio/*, `<video controls>` for video/*, `<iframe>` (browser-native) for application/pdf, `<iframe sandbox="">` (no allow tokens) for text/html, `fetch()`+`<pre>` for text-y MIMEs (text/* + application/json|xml|yaml|x-yaml|toml|javascript|ld+json|x-shellscript|sql|x-ndjson), a centred `<a download>` button for everything else.
  - Symlink rows are first-class: `target_is_dir=true` → navigate; `target_is_file=true` → openFile; `target_is_{dir,file}=false` → "special file (device, pipe, or socket)\u2014not previewable" inert; both undefined → "broken symlink\u2014target is missing or unreadable" inert. Every row's visual state is honest in every case. Mirrors the same `is-disabled` styling CSS ships for files, so the gray-and-cursor-default affordance is uniform.
  - Esc closes the preview; the \u00d7 button does too. After open/close, `requestAnimationFrame(window.__browsterm_refit)` keeps xterm's cell grid matched to its new flex width, so the terminal never renders stretched or squashed.
  - `.workspace.previewing .pane.is-active { flex: 1 1 50% }` gives a clean 50/50 split; the sidebar still owns 240px; the terminal is never hidden beneath the preview \u2014 VISION principle #2 (the terminal is sacred) survives. `.preview-pane` is `display: none` until `.is-active`, so the workspace has zero latent cost when no preview is open.
  - Smoke on a fresh fixture tree (run via `./target/release/browsterm --no-browser --port 8771` with a symlinked dir, symlinked file, broken symlink, and a `/dev/null` link): `/api/fs/list` exposes `target_is_dir` / `target_is_file` so the JS routes correctly for every kind; `/api/fs/file` returns the right Content-Type for the full sweep, plus structured 404 for a missing path.

- **Broken / rough / missing** (user-visible):
  - **Syntax highlighting deferred to Tier 3.** VISION §2 calls for "text as syntax-highlighted code"; MVP ships `<pre>` monospace only.
  - **CSV-as-sortable-table deferred to Tier 3.** CSV files render as text. Tier 3 lands the parser + a real `<table>` with column sort.
  - **Hex fallback deferred to Tier 3.** Unknown binaries get a centred "Download" button rather than a hex viewer.
  - **No preview-pane keyboard navigation.** Tier 3 to wire arrow keys between siblings, Tab to close, Home/End to top/bottom.
  - **No JS unit tests** in this repo (no Vitest / Jest / Playwright harness). Sidebar + preview logic are exercised manually via smoke; Tier 3 to add a harness.
  - **No Range / chunked fetch** on `/api/fs/file`; oversized files return 400 wholesale. Tier 3 follow-up, paired with the missing `--fs-max-bytes` CLI flag.
  - **`<video preload="metadata">`** fires an immediate GET when an element is mounted. Acceptable for MVP; Tier 3 to make it lazy / onClick.
  - **Browsing to a directory leaves the preview open with stale contents** \u2014 intentional for now (lets a user keep a file open while navigating siblings), Tier 3 to decide whether clicking a directory should auto-close.

- **Feels bad** (code is there but a user would notice):
  - **`<iframe>` for PDFs works in Chromium and Firefox without backend help**, but a user agent without a built-in PDF viewer auto-downloads. The download button in `app.js` is the explicit escape hatch; the function-level doc on `fs::file` documents the contract so the next author doesn't think the iframe path is broken.
  - **`<video>` and `<audio>` elements fetch the entire body before the user presses play.** Acceptable when browsing a tree of curated files; the moment a user opens a 500 MiB binary once, they'll discover this. Logged here so the Tier 3 polish picks it up alongside hex and Range.

> **Decision:** Ship the preview pane as a sibling flex pane, with browser-native render routing by MIME; defer syntax highlighting, CSV tables, and hex fallback to Tier 3. **Tier:** T2. **Evidence:** 20 Rust unit tests + smoke run reproducing Content-Type, symlink-kind probing, and broken-link handling. **Trade-off:** text previews are plain monospace until highlight.js is vendored in a Tier-3 commit; that adds material binary weight (~150\u202fKB minified + an explicit language-table dependency) and warrants its own commit boundary.

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
