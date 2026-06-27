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

- *empty — the 2026-06-27 WSL browser launch commit resolves the previous entry.*
