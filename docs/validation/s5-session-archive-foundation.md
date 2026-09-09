# S5 Session Archive Foundation

> Date: 2026-09-10  
> Scope: encrypted Session Archive search, compaction and exact recovery foundation  
> Acceptance status: foundation only; S5-A1–A6 remain pending

## Delivered path

The first S5 increment extends the existing Person-scoped encrypted Agent vault rather
than introducing a plaintext or fixture-only store.

```text
inactive AgentSession
→ bounded lexical search projection
→ turn-boundary compaction with revision CAS
→ typed summary + recovery pointer in the active session
→ immutable exact pre-compaction session in the encrypted archive
→ search and recovery after vault reopen
```

Compaction is rejected while a turn or pending model output is active. The cutoff is
resolved to the end of a complete turn, so a Tool call/result or Expert delegation is
not split accidentally. The archive stores the exact source session before pruning
old messages and per-turn execution journals from the active projection. Cumulative
usage and the terminal outcome remain on the active session.

Every recovery pointer records the archive ID, source revision, cutoff turn and exact
archived message count. Recovery validates those fields against the encrypted archive
row before returning the source session. Stale compaction revisions use the same
conflict boundary as foreground session writes.

Search covers current sessions and immutable pre-compaction snapshots. Query text and
the derived search projection stay within `sessions.db`, which already uses the
Person-bound vault key and fails closed when that key is unavailable. Search input and
result count are bounded, and excerpts contain conversation content rather than hidden
model reasoning.

## Automated evidence

- `cargo test -p floe-agent`: 78 tests pass across library and integration targets.
- `cargo test -p floe-core --test agent_vault`: 17 tests pass.
- `cargo test --workspace`: passes across all Rust workspace targets.
- `cargo check --workspace`: passes.
- `cargo fmt --all -- --check`: passes after formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: blocked by existing
  warnings in the Agent/Core runtime and journal; none originate in this increment.

The new encrypted-vault regression covers live search, compaction, exact recovery,
stale-revision rejection, reopen persistence, archived search and loading the compacted
active session.

## Remaining gates

- Search is a bounded conjunctive lexical projection, not ranked FTS or semantic
  retrieval. Ranking, typed filters and corpus precision targets remain S5 work.
- The compaction summary is caller-supplied. There is no isolated summarizer, automatic
  threshold, context-budget trigger or model-output evaluation yet.
- Search, inspection and recovery have no protocol/FFI or Flutter surface yet.
- Source deletion propagation, tombstones, branch lineage and compaction manifests are
  not implemented.
- Persona/User Model, confirmed Personal Memory, Review-backed candidates, durable
  Playbooks, Learner and Curator remain unimplemented.
- S4 is not Accepted, and P0-D corpus gates are still open. This checkpoint therefore
  does not advance S5 from Planned or satisfy an S5 acceptance criterion.
