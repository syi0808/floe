# Implement S2 Day storage

Base: the publication commit containing local frozen contract checkpoint c71761ad7bf858f055a26a813c994ac6c1be361f. This is deliberately incomplete S2, not a passing compiler snapshot. Read AGENTS/relevant skills and exact Day types first. No formatter, compiler, build, tests, architecture checker, migration, reset, provider operation, or push.

## Exclusive files

- crates/adapters/vault/src/repositories/day.rs
- new crates/adapters/vault/src/repositories/day_refresh.rs
- new Day-specific storage helper modules under crates/adapters/vault/src/repositories/
- Day SQL portions only of crates/adapters/vault/src/engine.rs; preserve all non-Day SQL
- exact module registration in repositories/mod.rs only

Do not edit Day owner types/service, source repositories, Actions, Task, or other Vault domains. Return one reviewable patch and exact touched paths; no tests.

## Fixed contract and transaction rules

Implement the frozen object-safe DayRepository BoxFuture CRUD and DayRefreshRepository in day/domain/{refresh,calendar}.rs and day/ports/{day_repository,refresh_repository,calendar_acquisition}.rs. No direct mirror write port. Preserve local CRUD semantics.

Admission queries Person/command replay before mirror lookup. Compare original device and intent; identical replay returns Existing unchanged. New admission captures MirrorExpectation within the same short transaction. Pure read uses exact actor key. transition_refresh compares the complete previous record and revision and verifies the owner transition.

commit_refresh verifies previous Running to next Completed, exact query/acquisition/actor/operation, original mirror expectation, complete current Calendar inventory, and every successful-source fence, then writes mirror and operation atomically. Inventory includes every non-Disconnected calendar.event_kit/calendar.google/calendar.microsoft source for the Person, including Pending as unavailable; sort by connection_id. Maximum64 with explicit overflow failure. configuration_digest is SHA256(serde_json::to_vec(validated SourceConnection)); compare IDs, connector, revision, authority and resources too. A pending source_operation fence rejects successful-source commit. Any drift leaves the whole mirror unchanged and returns Conflict; owner separately records failure. Never rebase/reacquire in storage.

interrupt_refreshes compares old-generation Pending/Running records and records Interrupted without acquisition. Existing-store read/open must never initialize data. Explicit new-store initialization adds the required Day refresh table/schema marker. Move Day SQL from engine.rs into repository ownership without deleting non-Day SQL. No legacy migration or compatibility parser.

Pure transition/policy helpers belong to Day; storage verifies their output and exact SQL/CAS/evidence, never creates a competing source policy. If any frozen type cannot express a required invariant, stop that part and report the exact gap rather than inventing an API.
