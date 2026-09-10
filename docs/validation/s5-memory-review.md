# S5 Memory Review Boundary Validation

Date: 2026-09-10 (Asia/Seoul)

## Delivered

- A versioned `memory_review` Agent Vault action lists pending Memory candidates and
  optionally submits one explicit approve/reject decision.
- The native host derives Person scope from the opened vault and supplies both the
  `User` actor and decision timestamp. The wire contract has no fields for candidate
  payloads, evidence, actors, revisions, mutation execution, or credentials.
- The endpoint checks that the referenced pending candidate is Memory knowledge before
  calling the existing atomic decision path. It returns the refreshed pending list and
  the durable decision result.
- The Flutter gateway parses only pending Memory payloads into a bounded presentation
  model. Data & privacy shows the proposed statement, epistemic label, source count,
  and explicit Approve/Reject controls. Lock and fatal vault failures clear the review
  projection from controller memory.

## Automated Evidence

- `cargo test --workspace`: passed, including 36 `floe-ffi` tests and 18 protocol
  tests after adding the Memory Review boundary cases.
- `cargo build -p floe-ffi`: passed.
- `flutter test test/agent_memory_review_test.dart`: 2 passed.
- `flutter analyze`: no new issue; one pre-existing unused import remains in
  `test/settings_screen_test.dart`.
- `cargo fmt --all` and `dart format` applied; `git diff --check` passes.

## Known Limits

- No Learner yet extracts explicit remember/correction observations from completed
  conversations, so the surface reviews candidates staged by the governed core path
  but does not create them itself.
- Playbook review, rollback execution, curation, deletion propagation, relevance
  ranking, and live end-to-end acceptance remain open.
- The full Flutter suite is not green in this checkout: unrelated existing fixture
  timing/outcome assertions, Registry screen expectations, and golden comparisons
  fail. The new focused transport/parser tests pass.
