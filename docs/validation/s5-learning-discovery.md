# S5 Explicit Conversation Learning Discovery

Date: 2026-09-10 (Asia/Seoul)

## Delivered

- `explicit_learning_signal` recognizes a deliberately narrow Korean/English set of
  explicit remember, forget and correction requests. Narrative uses such as “I
  remember…” and ordinary questions do not trigger it.
- A trigger is eligibility metadata only. It cannot create, revise, retire or activate
  Memory and does not bypass the existing candidate Review boundary.
- `discover_explicit_learner_reviews` scans at most 64 encrypted sessions and creates at
  most 8 jobs per pass. The caller must choose a non-zero bounded limit.
- Discovery accepts only unscoped Personal sessions with no active/pending output and a
  completed outcome. It uses the latest user message only when a later assistant answer
  exists for the same turn.
- The digest contains the signal label and bounded user/assistant evidence. UTF-8 is
  truncated only at character boundaries and the exact source turn remains available
  through the immutable evidence reference.
- Confirmed active Memory is snapshotted through the existing policy-filtered context
  projection. Queue enqueue and claim still revalidate the exact source revision.
- Repeated scans replay the same idempotent job even when the caller time or provisional
  run ID changes.

## Automated Evidence

- `cargo test -p floe-agent explicit_signal`: passed.
- `cargo test -p floe-core --test agent_vault`: 22 passed, including bounded multilingual
  discovery, ordinary-conversation exclusion, Unicode truncation and duplicate replay.
- `cargo fmt --all` and `git diff --check`: passed.

## Known Limits

- Discovery is not invoked by the idle Agent Vault worker yet.
- The phrase set is intentionally conservative and is not a semantic classifier. The
  production Learner must return no proposal for irrelevant or ambiguous evidence.
- No production device-local Learner adapter is connected, so queued jobs currently do
  not become candidates automatically.
- Cross-turn contradictions, Tool/Expert conflicts and reusable procedure triggers are
  deferred until evaluation and Playbook support exist.
