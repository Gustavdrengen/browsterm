# INBOX — programmer → agent

Programmer drops dated items here to direct work, request vision changes, reprioritize, or give any other instruction the agent would not infer from the codebase or the vision on its own. The agent reads this file at the start of every session, before the state-of-play gate. Every open item here outranks anything the agent would otherwise pick from the tiers.

Format:

```markdown
- [YYYY-MM-DD] <one-sentence ask>
  - **Context:** <why the programmer added this, optional>
  - **Acceptance:** <how the agent knows it is done, optional>
```

Items are removed once they are addressed, declined-with-reason, or escalated as a vision hole. This file describes the current state, not history.

## Inbox

- [2026-06-21] Terminal tab system — open / close / rename / click-switch multiple PTYs in a top tab strip; default label `Terminal N`.
  - **Context:** Workspace is currently locked to a single hardcoded "Terminal 1", which means the user has no way to organise long-running work and the upcoming "open text file in neovim on a new terminal tab" workflow has no seam to land in.
  - **Acceptance:** Top tab strip renders one tab per open PTY; default label `Terminal N`; double-click to rename; click-to-focus; × to close; a new-tab button. Server keeps `PtySession`s per tab id and multiplexes them over a single `/ws` connection. Reconnect restores tab membership and labels.

- [2026-06-21] Edit text files in the sidebar via "Open in neovim" — opens the file in a new terminal tab running nvim.
  - **Context:** Reaches for the editor a senior engineer already has installed; vision §2 names the workflow. Depends on Terminal tab system.
  - **Acceptance:** Right-click (or button) on a text-file row creates a new tab whose shell is `nvim <path>`; tab label is `<basename>` until renamed; closing the tab kills nvim. The same primitive is reusable for any future "open with …" affordance.

- [2026-06-21] CRUD on file-explorer rows — rename / new file / new folder / move within the workspace / delete (with confirmation).
  - **Context:** The current listing endpoint is read-only; vision §2 already promises CRUD. Closing this gap is what makes the sidebar feel like an editor and not a viewer.
  - **Acceptance:** Right-click + button bar on rows exposes rename / move / delete; sidebar header has new-file / new-folder. Server gets `POST /api/fs/{move, write-text, mkdir, delete}` returning the existing `{error:{code,message}}` envelope on failure. Listing refreshes after every successful op.

- [2026-06-21] Read-only git status overlay in the sidebar (branch + per-row icons).
  - **Context:** When you're browsing a repo, knowing which files have changed is the difference between a sidebar you can act on and one that's pretty. The overlay must never mutate the working tree — read-only is a load-bearing promise.
  - **Acceptance:** Sidebar header shows the active branch (or `(not a git repository)`). Each row gets an icon for added / modified / untracked / deleted when applicable. Server exposes `GET /api/git/status?path=...` returning `{branch?, entries:[{name,status}]}` parsed from `git status --porcelain=v1 -b`. No POST endpoints under `/api/git/*` — the overlay is read-only by contract.

- [2026-06-21] Syntax highlighting for text-file previews (MIME / extension-based).
  - **Context:** Vision §2 names "text as syntax-highlighted code"; the MVP ships exactly `<pre>` text. This is the polish that makes a `git diff` preview feel deliberate.
  - **Acceptance:** Vendor a small highlighter. Preview renders a tokenised view instead of `<pre>` for known languages; languages inferred by MIME extension; copy-to-clipboard still works; falls back to `<pre>` for unknown MIME so no preview regresses.

- [2026-06-21] Themes — built-in dark / light / solarized / catppuccin / nord / dracula / high-contrast + custom-theme CSS-variable editor.
  - **Context:** Vision §2 already names the presets; a senior engineer expects to switch palettes without bouncing through a settings tree.
  - **Acceptance:** Theme switcher in the topbar applies `data-theme="…"` on `<body>`; CSS variables override the existing palette. Live custom-editor preview. Persisted via `localStorage` under `browsterm.theme`. No flicker on reload.

- [2026-06-21] Richer navigation in the file explorer — command palette + sortable columns + hover-preview side-channel.
  - **Context:** Vision §2 names these affordances at a high level; the inbox asks for the "richer navigation" surface to be made concrete. The hidden-file toggle is already shipped; this commit closes the other named capabilities.
  - **Acceptance:** Ctrl+P command palette (fuzzy path match); sortable column headers (name / size / modified / kind) with persistent ordering across navigates; row-hover preview side-channel.
