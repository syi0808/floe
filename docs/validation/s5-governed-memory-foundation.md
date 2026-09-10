# S5 Governed Memory Foundation

> Date: 2026-09-10  
> Scope: encrypted observation, staged Memory candidate, Review decision and revision ledger  
> Acceptance status: foundation only; S5-A1–A6 remain pending

## Delivered path

This increment implements the first host-governed persistence boundary from ADR 0019:

```text
completed encrypted Personal AgentSession
→ source-checked LearningObservation
→ idempotent pending Memory candidate
→ user-only approve or reject decision
→ atomic active immutable Memory revision + mutation ledger
→ revision CAS for later corrections
→ reopen with the same Person vault key
```

The records use the existing encrypted Person vault. Synthetic sessions, incomplete
turns, missing evidence turns and non-user decisions fail before authoritative Memory
is written. Pending and rejected candidates are excluded from the active Memory query.

Observation and candidate hashes use SHA-256 over typed serialized inputs. The
idempotency key includes observation content, extractor/prompt versions, knowledge
kind and target, so a retry returns the existing candidate rather than staging a
duplicate. A revision candidate captures the current payload hash and base revision;
approval revalidates both within the write transaction.

The public Core boundary currently exposes:

- `stage_memory_candidate`;
- `pending_knowledge_candidates`;
- `decide_knowledge_candidate`;
- `active_personal_memories`;
- `knowledge_mutations`.

## Automated evidence

- `cargo test -p floe-core --test agent_vault`: 19 tests pass.
- `cargo test --workspace`: passes across all Rust workspace targets.
- `cargo check --workspace`: passes.
- `cargo fmt --all -- --check`: passes.

The focused regression covers duplicate staging, pending exclusion, user approval,
rejection, non-user decision denial, synthetic-source denial, immutable revision,
rollback pointer material, mutation history and persistence after vault reopen.

## Remaining gates

- Candidate creation is a typed host API; no conversation extractor, background
  Learner or automatic trigger is connected yet.
- Active Memory is not yet retrieved into `ContextEnvelope`, and no context manifest
  or ranking policy consumes these records.
- The shared Review protocol/FFI and Flutter surfaces are not connected.
- Edit-and-approve, rollback execution, pin/stale/archive, tombstone/delete
  propagation and usage/outcome evaluation remain open.
- Playbook candidates and durable Playbook revisions are modeled but not persisted or
  activated by this increment.
- S4 is not Accepted and the P0-D corpus is still open, so no S5 acceptance criterion
  advances yet.
