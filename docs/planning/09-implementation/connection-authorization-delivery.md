# Connection Authorization — Delivery Plan

> 2026-09-12. Baseline: `205107b`. Planning and review: primary agent.
> Implementation: explicitly requested `gpt-5.6-luna`, reasoning `high` subagent.
> Status: P1 accepted; P2a implementation in progress; P3/P5 integration audits in progress.

## 1. Scope and completion rule

Implement the remaining runtime in [Connection Authorization Runtime](connection-authorization-runtime.md),
not merely rename revisions or add unused common interfaces. The existing Calendar authority split,
lazy validation, subset projection and conservative history filtering remain protected regressions.
No backward compatibility, old-format grant conversion, or authorization fallback is required.
Do not delete/reset user databases, invoke live provider mutations, change OS permissions, create
branches, or install unrelated dependencies. Synthetic fixtures and disposable test stores only.

A milestone is complete only when its production callers, negative tests and failure mapping are
implemented and reviewed. P1 is explicitly a foundational store/API milestone, not runtime adoption.
An unsupported platform/provider remains closed and is recorded as an outstanding gate; disabling
all sources or returning unavailable from a stub does not complete that provider's integration.
Live OS/provider acceptance and cross-device owner handover are separately recorded, not inferred
from mocks. S8 relay/handover remains a separate project gate in the original design.

## 2. Invariants and ownership

1. Source identity includes Person, exact connection, connector/provider and execution owner.
   Account/tenant/device/owner replacement cannot inherit a grant by provider name.
2. Mirror row CAS, source epoch, grant access epoch, policy epoch and provider item version are
   different values. No numeric substitution or sync-driven consent mutation.
3. A grant authorizes an explicit bounded resource set, operations, purposes, consumers and
   processing policy. `all` means the reviewed finite set, not future discovered resources.
4. Authority lives in authenticated Core/vault or source-host boundaries, never model text or
   self-asserted wire stamps. Observation permission is not action approval.
5. Pause/revoke and admission/release are serialized at the owner. Durable deny precedes success;
   cleanup is retriable and cannot revive the grant. No network call inside a long-held DB lock.
6. General chat does not acquire all sources. A failed optional source is a typed tool failure.
7. Previously released bytes cannot be recalled. Newly released source-derived output must pass
   an authority fence. Cancellation alone is not a fence.
8. Unsupported schema, missing authority, ambiguous identity and unavailable owner verification
   fail closed. Never turn partial coverage into an empty successful observation.

## 3. Milestones and file ownership

### P1 — Typed grant authority and encrypted durable lifecycle

Implementation owner: first Luna/high assignment. Scope is limited to domain types, vault storage,
their exports/initialization, and adjacent tests. Do not edit Flutter, FFI routes, server, registry
mutation logic, or the planning documents in this assignment.

Read first:
- `crates/floe-domain/src/connection_authority.rs`, `calendar.rs`, `id.rs`.
- `crates/floe-core/src/agent_vault.rs`, `agent_vault/learning.rs`,
  `agent_vault/registry.rs`, and existing encrypted vault tests.
- Runtime design §§2–3, 7 and 10; semantic connection/access document and ADR 0027.

Deliverables:
1. Domain-owned grant model in a focused module. Use typed grant identity and a grant authority
   stamp with random non-nil incarnation and checked positive access epoch, distinct from
   `SourceAuthority`. Explicit source binding includes Person/connection/connector/execution owner
   and the reviewed source authority. Grant owner is the local vault identity, not a caller-selected
   remote issuer. Use existing ID types where their meaning matches; do not equate a grant ID with
   a registry revision or pretend device IDs are UUIDs.
2. Scope explicitly carries domain/data categories, nonempty bounded resource handles, operations,
   purposes, consumers and processing restriction. Document limits in exported constants/tests; canonicalize or reject
   duplicates deterministically. Initially support finite explicit resource sets only. Reject blank,
   overlong, unsupported/unknown, missing or zero-valued authority fields. No wildcard fallback.
   Keep provider raw identifiers in encrypted host data; no source payload or credential in grants.
   Revalidate nested identifiers after deserialization, not just in constructors. A reordered finite
   set is the same consent. Unsupported processing recipients must not become a bare allow-external flag.
3. States: paused/active/revoked. Creation is paused. Activation or reviewed replacement is an
   explicit host command against the expected grant stamp and the exact reviewed source binding.
   Pause/revoke cannot activate or expand scope. Revoked identity is terminal: renewed consent
   requires a new grant ID. Same-ID review cannot change Person/connection/connector/execution owner.
   Source mismatch produces review-required admission distinct from user pause; P2 must represent
   this explicitly rather than silently overwrite the paused state. Scope replacement/review and state changes advance access epoch with
   checked overflow. No-op commands do not gratuitously advance epochs.
4. Add an encrypted grant table and cleanup outbox using the same vault database and existing
   transaction/key-access patterns. Include a strict per-store schema marker. Never repurpose
   plaintext Core mirror storage or invent a second key store. Initializing an empty new table grants
   nothing and does not migrate existing registry receipts. Unsupported/corrupt stores return failure.
5. Host methods: create paused grant; get/list bounded grants; activate/review; pause; terminal revoke;
   enumerate pending cleanup; acknowledge a specific cleanup item. All methods bind to vault Person
   and owner and validate stored payload against indexed identity/state/stamp. No general upsert,
   resurrection helper, arbitrary caller owner, or mutable public authority bypass.
6. Serialize read-modify-write and expected-stamp CAS in an immediate transaction. Grant mutation
   invalidating prior authority and the corresponding cleanup outbox item commit together. Old pending
   cleanup remains distinguishable from a newer mutation; acknowledgement of an old job never deletes
   a newer job or modifies current authority. Successful grant reads must not repair invalid data.
7. Keep existing calendar registry as the currently active runtime path until P2; do not dual-write
   it best-effort from this foundation. Clearly report that the new APIs are not yet runtime authority.

Required tests:
- Create is paused; explicit activate; pause/reactivate; exact reviewed scope change; revoke terminal.
- Cross-Person/owner/source identity rejection; invalid/duplicate/oversized scope; nil/zero/overflow.
- Stale CAS fails without writes; two competing changes cannot both commit from the same stamp.
- No-op/retry behavior is deterministic and cannot revive a revoked grant.
- Reopen/WAL/checkpoint preserves authority, terminal revoke and pending cleanup.
- Injected failure before transaction completion leaves neither half of the grant/outbox update.
- Cleanup failure or stale acknowledgement leaves durable deny intact and newer work pending.
- Corrupt/unknown schema/identity mismatch fail closed; locked/unavailable key denies operations.
- Existing vault, registry and Calendar regressions still pass; fixture files contain no plaintext
  sentinel grant/source metadata after persistence/checkpoint.

Validation: `cargo test -p floe-domain`, focused new Core tests, `cargo test -p floe-core`,
`cargo fmt --all -- --check`, `git diff --check`. Primary agent reviews before committing.

### P2 — Make grants the actual native Calendar authority

Depends on P1 review. Owners of consent/registry projection must be updated in one milestone.
- Files: `agent_vault/registry.rs` and its Calendar setup module; agent registry Calendar bindings;
  `vault_host.rs`; `conversation_turn/expert_dispatch/schedule.rs`; Calendar DTOs/controllers.
- Store consent grant and its enabled registry execution projection in the same encrypted transaction.
  Registry bindings reference grant identity/stamp rather than independently authorize from a receipt.
- FFI resolves exact source metadata from Core before presenting/reviewing consent; a stale review
  cannot be silently stamped with changed source scope. Cosmetic/sync changes do not invalidate review.
- Native reads validate current source plus grant, consumer/purpose/processing restrictions, then
  effective scope intersection. Use per-grant/consumer policy versions, not unrelated global registry
  changes, to invalidate in-flight source work. Retain storage CAS for registry mutation conflicts.
- Pause/revoke/consumer disable invalidate admission; remove automatic grant repair paths. Setup
  retries return the original receipt; no default consent from connection availability.
- Tests: atomic grant/projection rollback, old receipt cannot grant access, grant expansion review,
  disable during read, unrelated expert install, sync 100 times, 2-of-11 resources, general chat.

### P3 — Headless-capable native acquisition and lease fences

Depends on P2. Resolve actual host threading/lifetime before adding callback ABI.
- Files: `calendar_view.rs`, FFI host/local context, protocol DTOs, Flutter native gateway,
  macOS EventKit bridge, iOS CalendarChannel, Android context channel and adjacent tests.
- Introduce bounded `AcquireView` and opaque immutable in-process lease. Bind invocation, grant,
  exact source, effective scope, query/timezone, consumer, purpose, processing audience and limits.
- Serve only a matching fresh cached observation or request a bounded native read from the host
  service, not `PersonalDayScreen`. No prompt or dialog is created by background acquisition.
- Producer checks OS permission/source identity before and after reading. Late callbacks carry
  invocation/source stamps and cannot publish after revoke/cancel/replacement. Callback ownership
  must survive neither disposed host nor reused handle. Unavailable platform returns typed failure.
- Per-Person quotas and expiry use monotonic deadlines within a process; persisted wall-clock values
  never mint restart-surviving leases. Single-flight only identical authorization/query keys.
- Pin first valid observation; one bounded reacquisition for eligible expiry/scope narrowing, never
  automatic re-consent or action replay. Enforce output/model-egress fences after source consumption.
- Tests use permission/provider fakes and controllable clocks/barriers: before/during/after revoke,
  expiry, clock rollback, late callback, process restart, host disposal, concurrent observations.
- Real OS permission testing requires explicit user consent and is a separate acceptance record.

### P4 — Durable dependencies across context and lifecycle

Depends on lease stamps and fence APIs from P3; design contracts can be prepared earlier.
- Files: agent contract/runtime, `calendar_history.rs`, Core model adapters, session archive,
  expert private state, `agent_vault/learning.rs`, proposal storage and model replay adapters.
- Record source/grant/policy stamps, observation and expiry on every consumed View. Inherit the union
  into generated messages, artifacts, compaction, replay, private state and Learner candidates.
- Model-input projection revalidates dependencies without changing durable display history. Expired
  or revoked source data cannot survive via summaries, encrypted provider replay or accepted memory.
- Resume restores dependencies then acquires/revalidates a fresh invocation lease before using them;
  it does not extend an old lease. Preserve independent conversation rather than global reload.
- Remove the conservative Calendar-name/history filter only after equivalent negative tests cover
  source-derived answers, failed tasks carrying data, compaction, recovery archives and replay.
- Cleanup outbox processes affected working context/cache idempotently; durable audit retention
  and user-visible archives follow retention policy rather than blanket deletion.
- Tests: source A revoked while B remains; multi-turn derivation; unseen old summary; memory review;
  resume after restart; cleanup crash/retry; unrelated data survives; no raw sensitive egress.

### P5 — Server owner verification and connector adoption

Depends on P2–P4 contracts. Primary review must settle issuer enrollment/proof before implementation.
- Files: Go `connectors/common`, console store/Person lifecycle/client connector handlers, source View
  routes, Rust protocol/infra remote adapters, paired-client connection setup, shared fixture corpus.
- Select exact Person-owned connection for Calendar/Mail, then Work/Logistics and remaining View routes.
  Provider name, runtime map iteration, global configured source or pairing alone cannot authorize reads.
- Local vault remains grant owner. Remote producer requires authenticated owner enrollment and fresh,
  audience/request-bound online authorization. A request's `allow`, epoch, grant ID or bearer pairing
  token is not proof. No invented shared secret, arbitrary callback URL, or offline permit fallback.
- Choose an existing authenticated enrollment/transport when it can prove the owner; otherwise build
  explicit enrollment + challenge verification as a separately reviewed substep. Grant activation for
  remote use requires explicit consent to scope, consumer and processing recipient.
- Source owner issues source epoch; token refresh/health update preserves it; identity/scope revoke
  changes it. Verify both admission and release, preserve pagination/partial coverage/provider limits.
- Rust/Go fixtures: missing/forged/replayed/wrong-audience proof, wrong Person/device/tenant, stale
  owner, partition, source replacement, normal token refresh and revoke races.
- Integrate each connector with a negative and positive provider fake; a deny-only stub is not done.

### P6 — Other personal context and action admission

Depends on P4, and P5 for remote sources. Split into reviewed context and action commits.
- Contacts/Wellbeing/Attention/Feasibility adapters use the shared authorization boundary while keeping
  domain retention, device presence and derived-only projection. Do not introduce a raw universal cache.
- Files: native context providers, Core connected-context views, FFI context publication, protocol
  descriptors and Flutter recovery UI. Provider-specific retention/consent constraints take precedence.
- Action path: exact Person/source/target, normalized payload digest, current action policy, approval,
  provider precondition and durable idempotency attempt checked before dispatch admission.
- Persist prepared/approved/admitted/succeeded/failed/unknown states and serialize cancel/revoke with
  admission. Timeout/crash after send stays unknown and reconciles the same attempt; never blindly retry.
- Tests: permission/presence expiry, projection/retention limits, target vs unrelated item change,
  revoke before/after dispatch, crash after provider success, reconciliation and no duplicate effects.

### P7 — Source-local recovery and final acceptance

Error UI evolves with P2–P6, not only at the end. Final pass verifies complete producer/consumer parity.
- Versioned failure envelope: kind, stage, opaque affected refs, retryability, recovery action and
  correlation. Source errors never mutate session/grants or display raw IDs/payload/credentials.
- Calendar unavailable/review-needed is separate from conversation CAS conflict and execution unknown.
  Retry only eligible reads; review/reauthentication is explicit; unknown actions show reconciliation.
- Run full Rust workspace, Go suites/race tests where supported, Flutter analyze/tests, native package
  tests, rebuilt C ABI tests and protocol corpus. Record unchanged baseline failures separately.
- Review all protected routes against the runtime conformance matrix; no untested enable gate.
  Record each item as implemented+tested, pending environment validation, or not implemented.
- Update ADR/runtime/this ledger to actual evidence; never mark the entire design done while a provider
  is stubbed, lineage is heuristic, or required owner verification is still only a document.

## 4. Delegation and review protocol

One Luna/high agent implements one bounded milestone at a time in the shared working tree. It must
read applicable AGENTS.md, use apply_patch, avoid unrelated edits/dependencies, and report changed files,
tests, remaining risks and incomplete criteria. It must not spawn more agents or modify planning docs.
The primary agent owns the plan, audits subsequent integration paths while implementation runs,
reviews diffs and negative tests, requests corrections, then creates a coherent commit. Never start
dependent milestone edits against unreviewed authority APIs. Update this ledger per accepted milestone.

### Initial integration audit

- `server/internal/console/console.go::serveInference` derives Person/device from a paired bearer
  client, but the inspected client record contains only token hash, Person and device. This is not
  enrolled grant-owner proof; do not silently reinterpret it as such in P5.
- The current Mail View route has no exact connection selector and may fall back from Gmail to
  Microsoft Mail after a read error. P5 must remove that fallback for a grant-bound request, not just
  add an optional selector that leaves the old branch accessible.
- Calendar View already takes exact connector/connection/revision, while Work/Logistics collect
  owned runtimes. Ownership filtering is not a substitute for per-grant effective scope. Aggregation
  needs a separately authorized source set and per-source dependencies/coverage.
- Native Calendar reads currently enter through Flutter channels owned by macOS MainFlutterWindow,
  iOS CalendarChannel and AndroidContextChannel. Moving a Dart timer is not headless acquisition.
  P3 must define a host-service lifetime and safe callback boundary, reusing the underlying providers.
- Android's native context read also has a four-calendar guard. Audit producer vs AI View budgets
  separately when adopting the 2-of-11 resource test; raising a single common limit is not the fix.
- Learning records already preserve evidence references, but those references are not source/grant/
  policy lease dependencies. P4 extends those paths rather than creating a parallel memory system.

| Milestone | State | Evidence |
| --- | --- | --- |
| P1 | Accepted foundation | Domain 9 tests, grant-store 8 tests, full Rust workspace and formatting passed; no runtime adoption |
| P2 | In progress: P2a | Luna/high: atomic native grant/projection and actual read admission; P2b review/policy work follows |
| P3 | Architecture audit | Separate Luna/high read-only native host/worker lifecycle audit; implementation not yet accepted |
| P4 | Pending | — |
| P5 | Architecture audit | Separate Luna/high read-only enrollment/verification audit; no paired-bearer authorization shortcut |
| P6 | Pending | — |
| P7 | Pending | Live environment gates remain explicit |

### P1 review evidence and limitations

The Luna/high implementation received primary review and correction passes for nested deserialization,
canonical recipient scope, indexed cleanup identity, terminal revoke, partial-schema rejection and
write-failure handling. A real SQL uniqueness failure after outbox insertion verifies rollback of both
authority and cleanup; a precommit key failure verifies the same boundary. True persistence failures
latch the vault unavailable in the current process; normal CAS/known write-lock contention does not.
Tests also exercise competing mutations, reopen, retained cleanup, schema/payload corruption,
capacity limits and absence of a resource sentinel in database/WAL files.

Validation on the reviewed tree: `cargo test --workspace`, domain 9 tests, focused Core grant tests 8,
`cargo fmt --all -- --check`, and `git diff --check` pass. No OS permission or real provider mutation
was performed. `serde_json` reuses the existing workspace version for scope-byte validation and tests.

The store is initialized empty in the existing encrypted vault and is not populated from Calendar
receipts. Runtime still uses the current registry authority path. Creation is paused/unreviewed;
reviewed activation clears that marker, and Active+unreviewed stored payloads are rejected. Dynamic
source mismatch/needs-review admission, consumer/policy epochs, native refresh, output fences,
cleanup execution and server issuer verification are not implemented by P1. The initial retained-grant
capacity is 128 records including terminal identities; P2 must account for that explicit quota in UI
and lifecycle integration, not delete tombstones to make room silently.

## 5. Continued execution toward P7

The user requested continuation through P7. P2 is split into reviewable substeps rather than treating
the foundation as completion. P2a replaces native Calendar's registry-only authorization with an
actual encrypted grant and atomic execution projection, preserving lazy acquisition. P2b finishes
authority-based consent review DTO/UI and per-consumer policy epochs (including disable/re-enable ABA)
so unrelated global registry changes do not invalidate source work. Registry CAS still protects writes.

Native and remote architecture audits run independently of P2a implementation. They cannot edit the
shared Core/FFI files or enable a protected path before its dependent contracts are reviewed. All
implementation agents retain the requested Luna/high setting; the primary owns decisions and review.

### P4 integration direction for review

Existing `LearningEvidenceRef` already references session/turn identities. Prefer a vault-owned,
transactional per-turn dependency sidecar over adding fields to every AgentMessage variant:
- Record explicit independent/source-dependent coverage for new turns. Missing coverage is unknown,
  not permission to reuse generated source history. Do not migrate old authority implicitly.
- Every source acquisition supplies immutable grant/source/policy stamps plus observation ID, effective
  scope handle and expiry. Record consumption before derived output can be committed.
- Inherit the union from all retained historical context and consumed Views into generated output for
  the current turn. Whole-turn conservative attribution is acceptable; model self-report is not.
- Session CAS/result/private-state commits and dependency records must be atomic in the encrypted
  vault. A detached post-save metadata write cannot establish lineage.
- General as well as Calendar model paths project/revalidate history and provider replay. A general
  follow-up that sees valid Calendar history inherits its dependencies even without a new tool call.
- Compaction unions covered dependencies into its summary turn. Archive restoration, proposal
  evidence, Learner discovery/review and accepted memory projection resolve the same sidecar.
- Scope/resource/recipient policy still limits allowed uses: Read/Suggestion must not silently become
  permission to persist a source-derived memory. Keep source learning closed without explicit learning
  consent and retention policy; ordinary source-independent learning remains separately governed.
- Observation expiry is not extended by restart or summary. Historical retention needs a distinct
  supported policy; do not claim durable lineage alone authorizes indefinite reuse.

This is an integration direction, not implemented behavior. Final P4 contracts must match the reviewed
P2 admission record and P3 lease/observation identities before implementation is assigned.
