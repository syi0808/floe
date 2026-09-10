# S5 Isolated Learner Runtime Validation

Date: 2026-09-10 (Asia/Seoul)

## Delivered

- `LearnerReviewInput` is a versioned immutable snapshot identifier containing Person,
  session/revision, evidence turn IDs, terminal outcome, bounded digest, confirmed
  Memory projection, run ID and observation time.
- `LearnerRuntime` accepts only a dedicated `LearnerModel` and `MemoryCandidateSink`.
  There is no general Tool, Expert, A2A, shell, network, credential or authoritative
  Memory mutation interface.
- Initial policy permits only device-local placement. Independent serialized input,
  serialized output, token, cost and deadline limits fail closed.
- Cancellation and deadline race the model future. Cancellation is checked again
  immediately before staging; after the candidate sink commits, its result is returned
  rather than being rewritten as cancellation.
- Zero-candidate output is valid. At most one Memory proposal can be staged per run,
  eliminating partial multi-candidate batches in this first path.
- The runtime owns actor/run provenance, extractor version, prompt version, source
  references and observation time. The model controls only the bounded proposed Memory
  value and optional target/base revision.
- Vault staging now requires the exact completed source-session revision. This prevents
  a stale review from writing after a newer foreground turn changes the source session.

## Automated Evidence

- `cargo test -p floe-agent learner`: 4 passed, covering successful staging, policy and
  pre-start cancellation, halted/over-budget rejection, and in-flight deadline/cancel.
- `cargo test -p floe-core --test agent_vault`: 19 passed, including stale revision CAS,
  synthetic/untrusted-source rejection, idempotency, activation and reopen persistence.
- `cargo check -p floe-agent -p floe-core`: passed.
- `cargo fmt --all` and `git diff --check`: passed.

## Known Limits

- The durable background queue, idle scheduling, foreground preemption handoff and
  production learner model adapter are not connected yet.
- No automatic conversation signal classifier or digest builder exists yet. Current
  conversation turns therefore do not autonomously invoke this runtime.
- Playbook proposal, evaluation, rollback, pin/stale/archive and deletion propagation
  remain later S5 increments.
