# S5 User-facing Saved Memory View

Date: 2026-09-10

## Scope

- Added a dedicated read-only `memory` vault action and a typed Memory overview result.
- Added a Data & privacy summary card and a dedicated Memory settings page containing
  pending Review and confirmed Saved memories.
- Added strict Dart projections, controller lifecycle handling and committed refresh
  after candidate decisions.

## Data boundary

- The encrypted vault query accepts only a bounded limit from trusted host code and
  returns active Personal Memory revisions for the current Person.
- Saved and pending counts use database counts; pending/rejected/tombstoned revisions
  cannot enter the saved list.
- The first response returns at most 100 newest active revisions and exposes statement,
  category, epistemic status, confidence, source count, origin, revision and validity
  metadata. It does not expose raw conversation content, prompts or hidden reasoning.
- The transport action has no mutation fields. Unknown target, statement, actor, delete
  or Person fields fail deserialization.

## User experience

- The Data & privacy card shows saved and pending totals and opens **Manage memory**.
- The dedicated page presents pending candidates through the existing Review control
  and confirmed records under **Saved memories**.
- Primary rows show the statement, friendly category, user-provided/approved-learning
  origin and date. Technical confidence and identifiers remain outside the list.
- Empty, loading and independent overview failure states are explicit. Locking or losing
  the Person vault clears saved Memory from controller state.

## Validation

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- `flutter test test/agent_memory_test.dart test/agent_memory_settings_test.dart test/agent_memory_review_test.dart test/settings_screen_test.dart` — 14 passed.
- `flutter analyze --no-pub` — only the pre-existing unused import warning in
  `test/settings_screen_test.dart`.

## Remaining work

- Add Memory detail/source inspection and revision history.
- Add revision-checked explicit user Edit.
- Add tombstone-first Forget and deletion propagation.
- Add persisted Memory controls, search and cursor pagination.

This is a read-only management increment and does not by itself complete S5-A5.
