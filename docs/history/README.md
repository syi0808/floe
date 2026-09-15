# Progress history

This directory contains dated product history. Current refactoring status is maintained only in the [migration ledger](../architecture/migration-ledger.md); [PROGRESS.md](../../PROGRESS.md) points there rather than duplicating a board.

## Refactoring history

The [refactor history index](../architecture/history/README.md) preserves the original implementation bundle and the complete migration ledger through source commit `89452eb5523ef6b1c76b7fe857095748d76de22d`. It also links the previous product progress board at its original commit.

## Product history

| Period | Theme | Record |
|---|---|---|
| 2026-09-11 | S5.5 provider parity, native context and durable domain Experts | [Connected-domain expansion](2026-09-11-connected-domains.md) |
| 2026-09-10 | S4 conversation lifecycle, S5 governed Memory and S5.5 connector foundations | [Agent, Memory and connector foundations](2026-09-10-agent-memory-connectors.md) |
| 2026-09-06–09 | S3 review/action authority and S4 Agent/Calendar integration | [Approved actions and connected Agent](2026-09-06-to-09-actions-and-agent.md) |
| 2026-09-03–05 | Personal Day baseline, EventKit read and approved-action foundation | [Personal Day and Calendar foundations](2026-09-03-to-05-personal-day-calendar.md) |

## Maintenance

Keep detailed executed evidence in [validation](../validation/) or a dated historical checkpoint with its source snapshot and environment. Preserve historical acceptance values as historical; do not promote them to current acceptance. The active refactor uses structure completion first and product validation second, as specified in the [implementation plan](../architecture/implementation-plan.md).
