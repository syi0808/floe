# Personal Day and Calendar Foundations — 2026-09-03 to 2026-09-05

> Historical implementation record. Current counts live in [`PROGRESS.md`](../../PROGRESS.md).

## Personal Day Baseline

- Rust-owned embedded Turso persistence, versioned JSON/C ABI and a dedicated Dart FFI isolate backed
  Event, Task, Note and Capture lifecycles.
- Day Canvas provided deterministic Now/Next/overdue projection, typed capture, task completion and
  reopen, deletion and the initial responsive native shell.
- Event/Task/Note editing, general conflict recovery, dense-day folding and two-week dogfood remained
  outside the delivered baseline.
- The last pre-Calendar validation on 2026-09-03 recorded Rust, Flutter, Clippy, analyzer, signed
  release symbols and native reopen checks as passing.

## S1 Calendar Read

- EventKit listing, explicit permission, bounded date-range reads, selected/all scope, per-source
  recovery and disconnect tombstones were connected to Rust mirror storage and Day Canvas.
- Provenance, stable occurrence identity, revisions, civil-day ranges and external update/delete
  reconciliation were covered by automated and disposable live PoC work.
- A disposable iCloud test event validated create response-loss recovery, re-import, external edit
  and deletion. The PoC also exposed an ordinary-refresh calendar-discovery defect that was later
  addressed by the selected/all scope work.
- Controlled permission deny/revoke, DST/recurrence, partial failure, offline restart and full signed-
  app lifecycle evidence remained open, so S1 acceptance stayed **0/4**.
- Evidence: [S1 Calendar](../validation/s1-calendar.md),
  [scope and recovery](../validation/s1-scope-recovery.md), and
  [EventKit live PoC](../validation/eventkit-live-poc.md).

## Compatibility Recovery

- A user-visible retry-only failure was traced to legacy `External` source decoding. Lossless legacy
  provenance round-trip and visible error/retry handling restored the existing local store without
  deleting or rewriting private data.
- The repaired debug app loaded the existing store and displayed Day Canvas plus Calendar connection
  controls. This validated local-data compatibility, not live EventKit acceptance.

## S3 Foundation

- Immutable Calendar-create proposals, durable execution IDs, atomic transitions, trusted Person/
  policy/target checks, provider preflight and receipt matching established the executor boundary.
- Fixture coverage included concurrent execution, cancellation after write, typed failure and
  ambiguous lookup-only recovery; no Flutter action surface or live native write acceptance was yet
  claimed at this checkpoint.
- Evidence: [Calendar action](../validation/s3-calendar-action.md).
