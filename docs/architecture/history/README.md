# Historical refactor records

These are read-only historical inputs, not additional active plans or progress boards. The active [plan](../implementation-plan.md), [prompt](../agent-prompt.md) and [ledger](../migration-ledger.md) supersede their execution instructions.

| Record | Original identity | Use |
|---|---|---|
| [Initial implementation bundle](initial-plan-2026-09-14/START_HERE.md) | Git tree `7db9d0ad416cc696bd2ccfaf9e01477b93daa823` | Original P/T definitions, source anchors, generated graphs and scripts; preserved without content changes |
| [Migration ledger through 89452eb](migration-ledger-through-89452eb.md) | Git blob `950cf658ffef69cbb0e0bb3ec77964967cde5bb6` | Original inventory and all recorded implementation/validation checkpoints; preserved without content changes |

The imported bundle was moved out of the repository root. Its internal files and checksum manifest remain unchanged. Do not run its old scheduling, schema-bump or live-demo instructions as current policy.

The archived ledger's top-level Pending statuses and next-demo text may be superseded by later checkpoints in the same file. Read the relevant dated section. A historical success, failure or blocker describes only that run and environment.

For original relative links and full source context, use these immutable commit views:

- [Original ledger location](https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/docs/architecture/migration-ledger.md)
- [Previous active plan](https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/docs/architecture/implementation-plan.md)
- [Former product progress board](https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/PROGRESS.md)

No validation was rerun simply by archiving these records. Future implementation checkpoints belong in the current ledger until they are deliberately archived.
