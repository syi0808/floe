# T0 retained Rust behavior ledger

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`.

Scope: all Rust crates except App, Vault and bindings, whose ledgers have separate owners. Root tests/tools and Go are separately owned. This ledger documents legacy assertions before deletion; it never turns an assertion into an accepted target requirement or claims that a test passed.

## Coverage and review status

- 751 actual Rust test registrations have 751 natural-language entries, with zero missing, extra or duplicate path/symbol mappings.
- All 331 retained baseline paths retain their original SHA256. No test, production source, manifest, fixture, lockfile or runtime data was changed by this extraction.
- No formatter, compiler, build, test runner or architecture checker ran. Static source inventory, source reads and documentation reconciliation only.
- Context's 150 entries are mapped, but parent/Mac semantic correction and acceptance remain a separate gate. Do not equate the registration count with approval to remove them.
- New replacement tests wait for structural closure. The parent must review each covered deletion batch and preserve neighboring production code and live fixture consumers.

The [coverage audit](t0-rust-behavior-ledger-coverage-audit.json) owns exact path/symbol reconciliation and all 331 path dispositions. The [registration inventory](t0-rust-behavior-ledger-inventory.json) freezes per-test paths, symbols, lines, SHA256s and registration attributes. The prior `file-read-ledger/rust.json` is baseline reading provenance, not this behavior ledger.

## Natural-language partitions

- [t0-rust-behavior-ledger-access.json](t0-rust-behavior-ledger-access.json): 41 registered cases
- [t0-rust-behavior-ledger-actions.json](t0-rust-behavior-ledger-actions.json): 14 registered cases
- [t0-rust-behavior-ledger-agent-contract.json](t0-rust-behavior-ledger-agent-contract.json): 24 registered cases
- [t0-rust-behavior-ledger-agent-runtime.json](t0-rust-behavior-ledger-agent-runtime.json): 44 registered cases
- [t0-rust-behavior-ledger-builtin-inline.json](t0-rust-behavior-ledger-builtin-inline.json): 14 registered cases
- [t0-rust-behavior-ledger-builtin-integration-context.json](t0-rust-behavior-ledger-builtin-integration-context.json): 27 registered cases
- [t0-rust-behavior-ledger-builtin-integration-experts.json](t0-rust-behavior-ledger-builtin-integration-experts.json): 15 registered cases
- [t0-rust-behavior-ledger-connections.json](t0-rust-behavior-ledger-connections.json): 35 registered cases
- [t0-rust-behavior-ledger-context-contract.json](t0-rust-behavior-ledger-context-contract.json): 27 registered cases
- [t0-rust-behavior-ledger-context.json](t0-rust-behavior-ledger-context.json): 150 registered cases
- [t0-rust-behavior-ledger-conversation-application-tests.json](t0-rust-behavior-ledger-conversation-application-tests.json): 31 registered cases
- [t0-rust-behavior-ledger-conversation-domain-turn.json](t0-rust-behavior-ledger-conversation-domain-turn.json): 25 registered cases
- [t0-rust-behavior-ledger-conversation-interactions.json](t0-rust-behavior-ledger-conversation-interactions.json): 10 registered cases
- [t0-rust-behavior-ledger-conversation-projection-coordinator.json](t0-rust-behavior-ledger-conversation-projection-coordinator.json): 25 registered cases
- [t0-rust-behavior-ledger-conversation-recovery.json](t0-rust-behavior-ledger-conversation-recovery.json): 40 registered cases
- [t0-rust-behavior-ledger-day.json](t0-rust-behavior-ledger-day.json): 13 registered cases
- [t0-rust-behavior-ledger-execution-runtime.json](t0-rust-behavior-ledger-execution-runtime.json): 24 registered cases
- [t0-rust-behavior-ledger-experts.json](t0-rust-behavior-ledger-experts.json): 35 registered cases
- [t0-rust-behavior-ledger-inference.json](t0-rust-behavior-ledger-inference.json): 37 registered cases
- [t0-rust-behavior-ledger-knowledge.json](t0-rust-behavior-ledger-knowledge.json): 24 registered cases
- [t0-rust-behavior-ledger-providers-control.json](t0-rust-behavior-ledger-providers-control.json): 28 registered cases
- [t0-rust-behavior-ledger-providers-models.json](t0-rust-behavior-ledger-providers-models.json): 36 registered cases
- [t0-rust-behavior-ledger-providers-native-diagnostics.json](t0-rust-behavior-ledger-providers-native-diagnostics.json): 10 registered cases
- [t0-rust-behavior-ledger-providers-sources-live.json](t0-rust-behavior-ledger-providers-sources-live.json): 22 registered cases

## Reading, helper and removal evidence

- [Directly owned audit](t0-rust-behavior-ledger-owned-audit.json): 332 cases; 116 full-source reads totaling 32,288 lines; 424 helper declaration anchors; exact whole-test/support and cfg(test) item candidates; dependency/target candidates. Helpers' natural-language behavior remains with their cases, not inferred from the anchor list.
- [Provider/native audit](t0-rust-behavior-ledger-providers-audit.json): 96 cases; platform-gated/ignored/live harnesses and shared Swift fixture consumers.
- [Conversation audit](t0-rust-behavior-ledger-conversation-audit.json): 131 cases, including externally registered application/tests.rs (31); test-only turn/journal.rs and in-memory-repository evidence limits.
- [Builtin integration audit](t0-rust-behavior-ledger-builtin-integration-audit.json): 42 cases, corpus rows and external JSON/Swift fixture consumers. Its named integration target must be reconciled with all ten module registrations.
- [Context ledger](t0-rust-behavior-ledger-context.json) and its companion review record remain under parent/Mac acceptance.

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
