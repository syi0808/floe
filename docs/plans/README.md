# Floe refactor planning bundle

Current execution (2026-10-03): T0 removal and S1/G1 are complete. S2 source closure is complete and awaits the [G2 build gate](s2-g2-command-sheet.md); reconstructed tests start only after G2. See [execution evidence](t0-execution-status.json) and [adversarial review adjudication](2026-10-03-s2-adversarial-adjudication.md). Earlier preparation text below is historical, not a request to repeat T0.

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

**Decision revision, 2026-10-02 13:46 UTC:** the three product/data choices are accepted; see [current accepted decisions](2026-10-02-architecture-refactor.md#101-accepted-user-decisions-and-clean-profile-cutover). Existing Floe development data may be discarded for a clean profile/schema without migration. No actual reset/deletion or implementation is authorized by this documentation update; new-system recovery safety remains mandatory.

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e` (main snapshot), preparation branch `prep/architecture-test-ledger-20261002`.

This is a proposed architecture and execution specification. The user separately authorized ordered execution at 13:49 UTC; this document revision itself performs no code changes or test deletion. The independent review record is the readiness authority; a full read count alone does not establish execution readiness. No formatter, compiler, build, tests or architecture checker were executed during preparation.

## Read first

1. [한국어 검토 요약](2026-10-02-review-summary.ko.md): proposed structure/order, user-review decisions and evidence limits.
2. [Architecture and execution plan](2026-10-02-architecture-refactor.md): one stage/order/gate authority.
3. [Canonical contracts](2026-10-02-canonical-contracts.md): exact inference wire, public owner commands/results, repository/port construction and manifest DAG.
4. [Independent review](2026-10-02-plan-review.md): issues, evidence and final static readiness verdict.

## Domain execution appendices

- [App composition and semantic extraction](2026-10-02-app-cutover.md), including accepted single Actions repository and recovery contract.
- [Rust modules/contracts/providers](2026-10-02-rust-cutover.md).
- [Vault persistence/transactions](2026-10-02-vault-cutover.md).
- [Protocol/FFI/native bindings](2026-10-02-bindings-cutover.md).
- [Go owners/adapters](2026-10-02-server-cutover.md) and [source-symbol map](2026-10-02-server-symbol-map.md).
- [Flutter client cutover](2026-10-02-client-cutover.md), [feature/UI partitions](2026-10-02-client-feature-ui-plan.md), [presentation preservation](2026-10-02-client-presentation-plan.md), and [test-removal method/inventory](2026-10-02-client-tests-plan.md).
- [Root/tooling/fixture cutover](2026-10-02-root-tools-cutover.md).

## Read and change-mapping evidence

[file-read-ledger](file-read-ledger/) contains the frozen baseline manifest, per-file full-read evidence and exact source-symbol target maps. The JSON maps are navigable by current file path and baseline line; their method notes distinguish semantic decisions, lexical reference candidates and compiler-resolved facts (no compiler resolution was run).

[Coverage reconciliation](file-read-ledger/coverage-audit.json) accounts for all 1,172 tracked paths exactly once: root/tools45, App75, Rust331, Vault40 (36 plus4 pure suites), bindings61, client375, Go171 and docs74. Text1,132 and binary assets40 have appropriate completed inspections. Missing, duplicate and incomplete paths are zero. Font inspection did not render every glyph.

[Map-path coverage reconciliation](file-read-ledger/mapping-coverage-audit.json) separately matches all 1,172 baseline paths to explicit file dispositions with no gaps or duplicate ownership. It does not replace semantic review.

Primary target maps: [App](file-read-ledger/app-symbol-map.json), [Rust](file-read-ledger/rust-symbol-map.json), [Vault](file-read-ledger/vault-symbol-map.json), [bindings](file-read-ledger/bindings-symbol-map.json), [client](file-read-ledger/client-symbol-map.json), [Go](file-read-ledger/server-symbol-map.json), [root/tools](file-read-ledger/root-tools-symbol-map.json). The docs ledger itself carries explicit KEEP/REWRITE destinations and real central stage IDs.

The recorded test symbols and selected legacy-behavior findings are **not** the future exhaustive per-behavior ledger. Execution authorization is recorded; T0 must still finish that ledger before deleting each corresponding test/helper/case. Shared fixtures, product code beside inline tests and real opt-in diagnostics require the recorded consumer classification. Existing assertions never automatically become accepted target requirements.

## Safety and qualification limits

No tracked source/test/configuration/manifest file was modified. No credentials, provider accounts, external data or remote branches were changed. New documents are the only working-tree addition. The user permits discarding existing Floe development data for a clean new profile/schema, without migration. No reset or cleanup is executed by this plan revision; exact-target action-time confirmation is required for later permanent deletion.

The user accepted the encrypted single Actions store, unavailable-while-locked external Action behavior, and loopback deployment scope, and permits discarding existing Floe development data for a clean profile/schema. Full structural compilation/build is a future G2 gate before writing the new S3 suite, not a claim established by this planning bundle.

Use this repository-native bundle beside the exact baseline checkout; relative links into pre-existing architecture/product/ADR documents refer to that checkout. The bundle does not embed Git metadata or credentials.

The ZIP also includes `floe-refactor-plan-docs.patch.gz` as a docs-only patch. It adds this planning bundle to the exact baseline checkout and contains no product implementation.

## T0 registration-inventory clarification

T0 handoff clarification: the execution inventory subsequently found 31 Conversation test registrations absent from the preparation test-symbol index. Full-file read coverage remains a separate claim; the planning test index is not an exhaustive registration/case inventory or behavior ledger. The execution coordinator owns the authoritative source-registration reconciliation and behavior-by-behavior T0 coverage before deletion. No test deletion may rely solely on the preparation index.
