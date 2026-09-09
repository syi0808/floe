# S4 Agent context and Schedule Expert generalization

Date: 2026-09-09. Implementation and automated evidence only; S4 remains **0/14**.

## Implemented

- Replaced monolithic Manager and Schedule Expert prompts with typed, versioned
  `PromptAssembly` components: Behavior Kernel, Role, optional Persona and the base
  capability protocol. Prompt identity is now structural rather than inferred by
  comparing instruction text.
- Added a bounded `PersonaProfile`. Manager calls compose the selected Persona while
  Schedule Expert calls omit it. Persona text is validated, classified as Personal
  context and excluded from contextual evidence serialization.
- Added a typed `ContextEnvelope` and manifest for stable prompt components,
  invocation-scoped capability descriptors, contextual evidence, conversation and
  runtime output bounds. Local and remote adapters serialize the same Core assembly.
- Generalized `ExpertInput` with `analyze`. Free-text Calendar turns no longer become
  an implicit 60-minute focus request.
- Schedule Expert can choose `calendar.read`, `calendar.search` and, only for a request
  with an explicit focus duration, `schedule.find_free_windows`. It may answer without
  a mandatory Tool call; host budgets and schema validation remain authoritative.
- Added a bounded `PlaybookRegistry` and per-invocation discovery session. Only
  eligible roots are initially visible; loading a parent reveals direct child
  summaries. Missing ancestry, cycles, repeated loads, depth and content budgets fail
  closed.

## Automated evidence

```text
cargo fmt --all -- --check
Passed.

cargo check --workspace
Passed.

cargo test --workspace
All Rust workspace tests passed.
```

Coverage includes prompt component validation, custom Manager Persona composition,
context manifest serialization, generic Schedule analysis without focus capabilities,
general Calendar read/search descriptors, optional direct Expert answers, nested
Playbook visibility and hierarchy rejection. The existing Calendar proposal,
authority, encrypted-session, model-adapter and replay suites remain green.

## Remaining boundary

This increment establishes runtime contracts; it does not add durable Persona/User
Model storage, `SOUL.md` file import/export UI, Memory/Archive retrieval, Playbook
persistence/Review, or model-visible `playbook.load` dispatch. Those remain S5 work.
No live EventKit, Foundation Models or remote-provider acceptance gate is claimed.
