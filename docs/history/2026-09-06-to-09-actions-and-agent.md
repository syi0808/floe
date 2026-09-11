# Approved Actions and Connected Agent — 2026-09-06 to 2026-09-09

> Historical implementation record. Current counts live in [`PROGRESS.md`](../../PROGRESS.md).

## S3 Review and Action Authority

- Calendar proposals became internal action intents projected into shared Review and Activity
  surfaces with Person-scoped `allow` / `ask` / `deny` policy.
- The native action bridge, review UI and executor added immutable proposals, atomic claims,
  preflight checks, receipt matching and lookup-only ambiguous recovery.
- Signed-app approval/create/collection/restart and a real response-loss recovery were recorded;
  rejection, blocking, provider-failure and dogfood gates remained.
- Evidence: [decision bridge](../validation/s3-action-bridge.md),
  [review UI](../validation/s3-action-review-ui.md),
  [native executor](../validation/s3-native-executor.md), and
  [Calendar action](../validation/s3-calendar-action.md).

## S4 Agent Foundation

- The bounded Agent contract, encrypted sessions, vault host, native model adapter, Expert registry,
  durable View bindings and atomic Calendar Expert setup were implemented.
- Registry management, consent UI, Calendar management transport, proposal inspection and Manager-to-
  S3 action routing connected the foundation to the product.
- Calendar turns gained native dispatch, app integration, progress/stop/retry feedback and encrypted
  source-scoped session resume/recovery.
- Managed instructions, compact Markdown, time context and Agent/Schedule Expert context
  generalization followed without expanding action authority.
- Evidence: [Agent foundation](../validation/s4-agent-foundation.md),
  [Expert foundation](../validation/s4-expert-foundation.md),
  [Calendar setup](../validation/s4-calendar-setup.md),
  [Calendar app turn](../validation/s4-calendar-app-turn.md), and
  [Agent context generalization](../validation/s4-agent-context-generalization.md).

## Historical Pause

- A 2026-09-07 handoff captured baseline `e57c342` and three uncommitted Calendar-turn draft files.
  The 2026-09-08 tested native/Dart transport and controller/UI wiring superseded that draft state.
- The exact handoff record remains in [S4 handoff validation](../validation/s4-handoff.md).

## Checkpoint Limits

- Fixture, build and synthetic model evidence did not satisfy live key, model, Calendar source or
  privacy gates. S4 remained **0/14** at these checkpoints.
- S3 reached **2/5** only after signed-app evidence; S1 verification and the remaining live action
  matrix continued to block acceptance.
