# BLOCKED — agent → programmer

The agent writes here whenever it cannot proceed without external help: credentials, environment access, vision-level decisions it cannot reasonably make, etc. Falls back to higher-tier work that does not need the unblock.

Format:

```markdown
- [YYYY-MM-DD] <what is blocked>
  - **Tried:** <what the agent has already attempted>
  - **Needed:** <the specific action the programmer can take>
  - **Impact:** <which tier of work is affected, and what fallback the agent is using in the meantime>
```

Entries are added on detection, updated on status change, and removed once cleared. `BLOCKED.md` describes the current state, not history.
