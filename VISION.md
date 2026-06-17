# Browsterm — Project Vision

> **Tagline:** A first-class graphical terminal *for* any Linux machine, *in* your browser — with WSL the friendliest of many doors.

---

## 1. The Idea

**Browsterm** (a portmanteau of **brow**ser + ter**minal**) is a single, self-contained workspace that turns a Linux system into a polished multi-tab terminal-and-files environment you drive from a browser. It is born on WSL — its friendliest home — but it is just as much at home on macOS, on a headless Linux server, on a Raspberry Pi, on a development VM, or on a homelab cluster that wants to be useful from a phone or a Chromebook.

From one command — `browsterm` — you have a browser tab containing a workspace that is **simpler than VS Code, richer than Windows Terminal, more portable than Codespaces, and more private than SaaS IDEs** — without installing anything on the host browser side, without plugins, without a separate frontend project, without giving your data to anyone.

The core insight: a POSIX PTY is the universal substrate for shell work, and a modern browser is the universal substrate for UI. Browsterm is the version of this principle a senior engineer would describe as **"the polished, recommended way to drive any Linux box — laptop, server, Pi, dev VM, or a WSL install — from a browser on a different machine, locally or over the network."**

---

## 2. What it does for you

### A real terminal that just works
The terminal pane is the heart of the workspace. It renders shell sessions faithfully — full color, mouse support that lets vim, tmux, htop, lazygit, btop, k9s and other TUIs behave exactly as they do in any other terminal. Search-as-you-type, click-and-drag selection, generous scrollback, and graphic protocols (kitty, iTerm2, sixel) so that inline images in modern TUIs render at full fidelity. The terminal is sacred; anything that compromises it is treated as the most serious kind of bug.

### Files, not folders
The sidebar is a live navigable tree of whatever Linux filesystem Browsterm is sitting on. Selecting a file shows a preview that handles whatever the browser natively renders: text as syntax-highlighted code, images inline, PDFs and SVGs, audio and video, a tabular view of CSVs, a hex fallback for the rest. There is a command palette for jumping to any path by fuzzy match, breadcrumbs, sortable columns, hover previews, and a hidden-file toggle for people who like hidden files.

### Drag, split, tear off
The workspace reshapes itself around the work. Split panes — horizontal or vertical, nested arbitrarily — host terminals, file explorers, and previews together. Keyboard shortcuts drive splits even faster than dragging. Tabs each carry their own split tree; tabs can be reordered by dragging, torn off into a separate browser window so a long-running log follows you, pinned so they survive a reload, renamed, and duplicated when an environment is worth cloning.

### Every terminal, one workspace
Bookmarks remember the directories you live in — with labels, tags, and the ability to import or export so the same set travels between machines. Themes fit how you see the world: built-in dark, light, solarized, catppuccin, nord, dracula, high-contrast, plus a custom theme editor where every color is editable and the result is live previewed against your current layout. Clipboard flows in both directions — text and images — between the active terminal and the browser.

### Files cross the boundary easily
Browsterm treats file transfer as a routine operation rather than an afterthought. Drag a file from your desktop into the browser workspace and it lands in the active directory or the configured drop folder; right-click any file in the explorer or preview to download it back to the host. The terminal itself can emit a click-to-download attachment for any output, and the workspace can read a dropped file's contents back into a terminal paste buffer.

On WSL, this matters even more because two operating systems are involved at once. A photo dragged in from the Windows desktop lands naturally in the WSL project's working directory. A log pulled out of WSL arrives in the host's Downloads without switching worlds. Files on Windows drives (the `/mnt/c/*` paths inside WSL) and files in WSL's Linux filesystem are equally first-class — Browsterm does not make the user choose which side of the boundary they are on.

### The WSL door is the friendliest one
Browsers auto-open on launch, on the Windows host and without ceremony. Right-clicking a file offers "Open in Windows app" — using the host's default association via wslview, falling back to `explorer.exe`, falling back to `cmd.exe /c start` — or "Reveal in Windows Explorer" for directories. Window, terminal, and path translations between the two operating systems are handled transparently. The current working directory of every terminal is known to the workspace so that opening "the project I'm in" takes one click.

### On your phone, on your server
The same browser UI works on a phone or a tablet — sidebar becomes a drawer, splits become a tab strip inside the active tab, touch targets meet accessibility expectations, and a PWA manifest is supplied so users can install Browsterm as a home-screen app. The same workspace also runs over an encrypted, authenticated remote connection for the homelab case: a real TLS endpoint, real authentication, an audit log of every session, and a read-only mode for sharing a session safely with a reviewer.

### Operational honesty
Browsers auto-open on launch. Errors don't crash the workspace — they show as toasts with an expandable details panel. A `doctor` command sanity-checks connectivity, PTY spawning, theme files, and the audit logger. Cold start is fast; idle memory is modest; nothing phones home; the binary is fully usable offline after first load.

---

## 3. Who it's for

| Audience | Use Case |
|---|---|
| **Linux developer on Windows** | Day-to-day work in WSL with a polished, themed, fully-featured workspace that never leaves the browser. |
| **Linux developer on macOS or Linux** | The same workspace, locally on the laptop and remotely to every Linux box they touch. |
| **DevOps / SRE** | Tailing logs, running tmux/zellij sessions, sshing between boxes — from a tablet on the couch. |
| **Data engineer** | Browsing project files, peeking at logs, CSVs, parquet, and Arrow metadata, while working in a PTY in the same view. |
| **Educator / student** | Demoing the Linux shell to a Windows or macOS user without any setup; or running a remote lab from a phone. |
| **Homelabber / self-hoster** | Driving a headless server, a NAS, a Pi, a Linux VM, or a cluster from a phone or a Chromebook without paying for SaaS IDEs. |
| **Reviewer / pair-debugger** | Sharing a session over an encrypted link with a colleague for an incident — credentials are revocable and session-scoped. |

The common thread: people who want a single tool that is **more** than a TTY forwarder and **less** than an IDE, available anywhere they have a browser — on any Linux machine, in any way they choose to reach it.

---

## 4. What finished looks like

### The "polished product" demo
A senior engineer on a Windows laptop runs `browsterm`. In under two seconds, a browser tab opens. They split the workspace three ways: a `tail -f` pane, a `lazygit` pane, and a file explorer showing the project tree. They drag a CSV from the desktop into the workspace and it lands in the project; they preview it in the panel as a sortable table. They save the project root as a bookmark, switch to Catppuccin Latte, drag a terminal pane into a new browser window to keep an eye on logs while they work in the main tab.

That same engineer, on a Sunday afternoon, is on their phone at a café. They reach their homelab Linux box over an encrypted, authenticated link and have the same workspace — three terminals simultaneously, watching logs, in Catppuccin Mocha, on the phone.

### The "stop writing custom TTY forwarders" demo
A homelabber installs Browsterm on a small Pi cluster. From a Chromebook on the sofa, they have bookmarked paths to each machine, distinct themes per environment (red for production, green for the lab), and a read-only sharing URL they paste into a chat when they need a friend to debug. Files move from the cluster to their laptop over the same workspace, not via `scp` and a separate browser tab.

### The "ship it" criteria
The workspace is shipped as a single self-contained binary that opens a polished browser tab on whichever host the user wants. The CLI's own help text is sufficient to find the warm path. There is no post-launch backlog of half-built features: every described capability works end-to-end or it isn't mentioned. The product description here is the source of truth — if implementation diverges from it, this document is updated first.

---

## 5. Principles

1. **Loopback is the default, the network is an opt-in feature, every Linux box is a first-class home.** Use the loopback first; reach the network only when the user asks; treat WSL, macOS, headless servers, and Pis equally well, with WSL getting the deepest Windows↔Linux boundary care.
2. **One self-contained deliverable beats a stack.** No separate frontend repo, no bundler, no companion service, no per-feature add-on. The binary *is* the product.
3. **The terminal is sacred.** Anything that compromises terminal fidelity is treated as the most serious kind of bug.
4. **Every capability earns its place.** Every feature must *demonstrably help real shell work*, not just decorate.
5. **The launch version is complete.** A capability either ships in full when the launch version is cut, or it does not appear in the product description until it does.
6. **Reconnect gracefully.** Connectivity is ephemeral; persistent state lives on disk; layout is reproducible; remote sessions resume after a drop.
7. **Privacy and offline are non-negotiable.** No telemetry, no analytics, no auto-update calls home. The product is fully usable offline after first load.
8. **Windows↔Linux is a first-class boundary.** When Browsterm runs on WSL, the act of opening a browser on the Windows host, opening a file in a Windows app, or moving a file across the Windows/WSL divide feels like one machine, not two.

---

*This document is the single source of truth for what Browsterm is. When implementation diverges from it, this file is updated first.*
