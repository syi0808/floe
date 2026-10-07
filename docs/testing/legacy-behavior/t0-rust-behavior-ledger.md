> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 retained Rust behavior ledger

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`.

Scope: all Rust crates except App, Vault and bindings, whose ledgers have separate owners. Root tests/tools and Go are separately owned. This ledger documents legacy assertions before deletion; it never turns an assertion into an accepted target requirement or claims that a test passed.

## Coverage and review status

- 751 actual Rust test registrations have 751 natural-language entries, with zero missing, extra or duplicate path/symbol mappings.
- All 331 retained baseline paths retain their original SHA256. No test, production source, manifest, fixture, lockfile or runtime data was changed by this extraction.
- No formatter, compiler, build, test runner or architecture checker ran. Static source inventory, source reads and documentation reconciliation only.
- Context's 150 entries are mapped, but parent/Mac semantic correction and acceptance remain a separate gate. Do not equate the registration count with approval to remove them.
- New replacement tests wait for structural closure. The parent must review each covered deletion batch and preserve neighboring production code and live fixture consumers.

The [coverage audit](artifact-index.md#unpublished-artifacts) owns exact path/symbol reconciliation and all 331 path dispositions. The [registration inventory](artifact-index.md#unpublished-artifacts) freezes per-test paths, symbols, lines, SHA256s and registration attributes. The prior `file-read-ledger/rust.json` is baseline reading provenance, not this behavior ledger.

## Natural-language partitions

- [t0-rust-behavior-ledger-access.json](artifact-index.md#unpublished-artifacts): 41 registered cases
- [t0-rust-behavior-ledger-actions.json](artifact-index.md#unpublished-artifacts): 14 registered cases
- [t0-rust-behavior-ledger-agent-contract.json](artifact-index.md#unpublished-artifacts): 24 registered cases
- [t0-rust-behavior-ledger-agent-runtime.json](artifact-index.md#unpublished-artifacts): 44 registered cases
- [t0-rust-behavior-ledger-builtin-inline.json](artifact-index.md#unpublished-artifacts): 14 registered cases
- [t0-rust-behavior-ledger-builtin-integration-context.json](artifact-index.md#unpublished-artifacts): 27 registered cases
- [t0-rust-behavior-ledger-builtin-integration-experts.json](artifact-index.md#unpublished-artifacts): 15 registered cases
- [t0-rust-behavior-ledger-connections.json](artifact-index.md#unpublished-artifacts): 35 registered cases
- [t0-rust-behavior-ledger-context-contract.json](artifact-index.md#unpublished-artifacts): 27 registered cases
- [t0-rust-behavior-ledger-context.json](artifact-index.md#unpublished-artifacts): 150 registered cases
- [t0-rust-behavior-ledger-conversation-application-tests.json](artifact-index.md#unpublished-artifacts): 31 registered cases
- [t0-rust-behavior-ledger-conversation-domain-turn.json](artifact-index.md#unpublished-artifacts): 25 registered cases
- [t0-rust-behavior-ledger-conversation-interactions.json](artifact-index.md#unpublished-artifacts): 10 registered cases
- [t0-rust-behavior-ledger-conversation-projection-coordinator.json](artifact-index.md#unpublished-artifacts): 25 registered cases
- [t0-rust-behavior-ledger-conversation-recovery.json](artifact-index.md#unpublished-artifacts): 40 registered cases
- [t0-rust-behavior-ledger-day.json](artifact-index.md#unpublished-artifacts): 13 registered cases
- [t0-rust-behavior-ledger-execution-runtime.json](artifact-index.md#unpublished-artifacts): 24 registered cases
- [t0-rust-behavior-ledger-experts.json](artifact-index.md#unpublished-artifacts): 35 registered cases
- [t0-rust-behavior-ledger-inference.json](artifact-index.md#unpublished-artifacts): 37 registered cases
- [t0-rust-behavior-ledger-knowledge.json](artifact-index.md#unpublished-artifacts): 24 registered cases
- [t0-rust-behavior-ledger-providers-control.json](artifact-index.md#unpublished-artifacts): 28 registered cases
- [t0-rust-behavior-ledger-providers-models.json](artifact-index.md#unpublished-artifacts): 36 registered cases
- [t0-rust-behavior-ledger-providers-native-diagnostics.json](artifact-index.md#unpublished-artifacts): 10 registered cases
- [t0-rust-behavior-ledger-providers-sources-live.json](artifact-index.md#unpublished-artifacts): 22 registered cases

## Reading, helper and removal evidence

- [Directly owned audit](artifact-index.md#unpublished-artifacts): 332 cases; 116 full-source reads totaling 32,288 lines; 424 helper declaration anchors; exact whole-test/support and cfg(test) item candidates; dependency/target candidates. Helpers' natural-language behavior remains with their cases, not inferred from the anchor list.
- [Provider/native audit](artifact-index.md#unpublished-artifacts): 96 cases; platform-gated/ignored/live harnesses and shared Swift fixture consumers.
- [Conversation audit](artifact-index.md#unpublished-artifacts): 131 cases, including externally registered application/tests.rs (31); test-only turn/journal.rs and in-memory-repository evidence limits.
- [Builtin integration audit](artifact-index.md#unpublished-artifacts): 42 cases, corpus rows and external JSON/Swift fixture consumers. Its named integration target must be reconciled with all ten module registrations.
- [Context ledger](artifact-index.md#unpublished-artifacts) and its companion review record remain under parent/Mac acceptance.

Important mixed-file boundaries include Access remote_view.rs, Connections pairing.rs, agent-contract delegation.rs, context-contract views/calendar.rs and builtin schedule/mod.rs. Each has production definitions after an inline test module. Never remove a tail range or whole mixed file.

## Semantic distinctions that must survive review

- Exact-recipient model consent, root-only model admission, local-first selection, role-specific DeviceOnly/RemoteOnly overrides and transport-failure fallback are obsolete assertions. They are not requirements for the accepted common Gateway Primary/local fallback planner.
- Keep exact source/grant/physical-resource authority, verified host/Gateway identity, CAS, cancellation direction, acknowledged pre-dispatch intent and truthful unknown-usage/external-write recovery.
- Old blocked-model tests allocate fictional attempt IDs and zero-usage results. Accepted source review occurs before ModelIntent with a projection-operation origin; preserve settled prior work without retaining that old representation.
- Tests named “restart” or “durable” often recreate a service over in-memory maps. Their behavior is captured without claiming disk crash, encryption or database atomicity qualification.
- A few names/comments overstate exercised behavior: some “no journal” checks exclude only batch/delegation intents; some digest mutations change more than one input; some secrecy checks inspect safe fixture substrings or hardcoded field lists. Entries state the actual assertions and limitations.
- Fake-model output corpus tests are contract/result validation evidence, not demonstrations of model judgment quality.

## Preservation and dependency rules

Keep production prompt assets and real smoke/diagnostic helpers. In particular,provider CurrentSavedConnectionStore::fixed is used by App diagnostics. NativeCalendarFixture.swift is shared with App/provider/Flutter consumers. Go ready snapshots,Android JSON,Apple Swift resources and the mail expert corpus cannot be deleted without their complete consumer closure.

Manifest candidates are exact and conditional in owner audits. Experts' old dev-only agent-runtime edge is planned for promotion at S2; Context's old dev-only kernel edge similarly has an accepted production destination. Do not delete a dependency based on its name or disable doctests as a shortcut. No Cargo.lock resolution is part of this extraction.
