# S5 Confirmed Memory Context Retrieval

> Date: 2026-09-10  
> Scope: active confirmed Personal Memory projection into Manager model context  
> Acceptance status: integration foundation only; S5-A1–A6 remain pending

## Delivered path

```text
approved encrypted Memory revision
→ active/non-tombstoned vault query
→ temporal eligibility filter
→ bounded typed ContextMemory projection
→ Personal inference-policy validation
→ ContextEnvelope.contextual_data.memories
→ source/revision ContextManifest entry
→ local or remote model adapter
```

General Personal conversations now assemble confirmed Memory immediately before the
Agent runtime starts. Calendar Experts and synthetic fixture sessions receive no
ambient Memory. Memory remains structured contextual data below the current user
request; it is never copied into the stable prompt or capability guidance.

Each item carries its target ID, revision, Memory/epistemic type, confidence, temporal
bounds and source session/turn references. The manifest repeats the stable identity,
revision and sources needed for inspection. Policy rejects Memory outside Personal
scope, duplicate targets, missing provenance, invalid confidence, future/not-yet-valid
or expired records, and projections over 32 items or 16 KiB.

The vault projection returns only active revisions and filters temporal records before
dispatch. It fails rather than silently truncating an over-budget active set until the
ranked retrieval and degradation policy is implemented.

## Automated evidence

- `cargo test -p floe-agent --test policy --test runtime`: 44 tests pass.
- `cargo test -p floe-core --test agent_vault`: 19 tests pass.
- `cargo test --workspace`: passes across all Rust workspace targets.
- `cargo check --workspace`: passes.
- `cargo fmt --all -- --check`: passes.

Focused coverage proves that confirmed Memory remains outside instructions, appears in
typed contextual data and the manifest, requires encrypted Personal policy, rejects
stale or source-free projections, and preserves the approved revision/source after a
Memory correction.

## Remaining gates

- Retrieval currently projects the bounded active set; query relevance, typed filters,
  ranking, deduplication and per-section token allocation are not implemented.
- Context manifests are assembled for dispatch but are not yet durably attached to
  each model-attempt journal record.
- Conversation extraction, background Learner and shared Review protocol/Flutter UI
  remain disconnected.
- Per-chat Memory use/generation controls, usage/outcome evaluation, rollback, curation
  and deletion propagation remain open.
