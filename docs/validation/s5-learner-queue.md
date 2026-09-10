# S5 Durable Learner Review Queue Validation

Date: 2026-09-10 (Asia/Seoul)

## Delivered

- Learner review jobs live in the encrypted Person vault and retain the immutable
  `LearnerReviewInput`, state, attempt, lease, candidate reference and typed failure.
- Enqueue derives an idempotency key from Person, source session/revision, evidence
  turns, terminal outcome and normalized digest. Core assigns the run ID; a caller's
  supplied run ID is not trusted.
- Queued and explicitly deferred jobs become claimable only at `available_at`. Running
  work has a 30-second lease, allowing crash/restart recovery without concurrent claim
  inside the single-owner vault host.
- Claims increment an attempt identity used by settlement CAS. Completed, stale and
  failed records are terminal. Three abandoned claims become `Stalled` instead of
  remaining permanently `running`.
- Defer accepts only typed transient cancellation/model availability failures and a
  future retry instant. Invalid, policy or provenance failures cannot be converted
  into an endless retry loop.
- Source session ownership, Personal classification, completed outcome, exact revision
  and evidence turn existence are revalidated transactionally at every claim. Stale
  foreground revisions fail before any Learner model call.
- A completed job may link only to a candidate whose Learner run ID and source turns
  match the claimed job. Missing or foreign candidate IDs fail without settling it.

## Automated Evidence

- `cargo test -p floe-core --test agent_vault learner_review_queue`: 2 passed.
- Coverage includes duplicate enqueue, lease exclusion/recovery, explicit defer,
  candidate-bound completion, attempt exhaustion, reopen persistence and stale source
  rejection before claim.
- `cargo test -p floe-agent learner`: 4 passed for the isolated runtime boundary.
- `cargo test --workspace` and `cargo check --workspace`: passed.
- `cargo fmt --all` and `git diff --check`: passed.

## Known Limits

- The Agent Vault worker does not yet enqueue completed conversational signals or run
  queued jobs while idle.
- Foreground submit does not yet cancel/defer an active background lease.
- The production device-local Learner model adapter and bounded conversation digest
  builder are not implemented; no autonomous candidate is produced yet.
- Playbook proposals, evaluation, rollback, curation and deletion propagation remain.
