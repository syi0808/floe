# App, host, CLI and repository-tool cutover specification

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

**Decision revision, 2026-10-02 13:46 UTC:** the three product/data choices are accepted; see [current accepted decisions](2026-10-02-architecture-refactor.md#101-accepted-user-decisions-and-clean-profile-cutover). Existing Floe development data may be discarded for a clean profile/schema without migration. No actual reset/deletion or implementation is authorized by this documentation update; new-system recovery safety remains mandatory.

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. **Planning only, awaiting user review.** No source/test/manifest deletion or modification has occurred. No formatter, compiler, build or test has run. This appendix follows the stage order in [the central plan](2026-10-02-architecture-refactor.md); it does not authorize implementation.

The 50 App production/mixed-source files, five example files and two standalone test/fixture files have been read in full. The eighteen `vault_host/tests/**` files are fully read and merged into the same frozen 75-file coverage. Every legacy assertion remains evidence rather than a target requirement. [App read ledger](file-read-ledger/app.json), [App declaration cutover index](file-read-ledger/app-symbol-map.json), and [root/tool read ledger](file-read-ledger/root-tools.json) distinguish reviewed content from pending work. Declaration extraction is lexical evidence, not compiler name resolution; grouping and the exceptions below are the semantic plan.

## 1. The actual ownership problem

`floe-app`'s crate comment says composition only. Actual code implements source/grant policy, native subject matching, first-party consumer policy, model host logic, background Learner selection, Expert registry defaults, Task endpoint execution, review/approval reconciliation, source-read classification, Day ownership checks, and Action repository/executor selection. Renaming `VaultBridge` or moving it whole would preserve the problem.

The final App contains:

- verified local identity/profile admission, host request guards, request correlation and host lifetime;
- construction of repositories, native/Gateway adapters, owner services and shared runtime;
- typed forwarding from FFI/CLI to those owner services;
- shutdown ordering and safe host diagnostic correlation.

It does not contain a business `WorkerAction`, an optional-field `WorkerResult`, `OpenVault` methods interpreting requests, source-specific dispatch switches, or owner failure/recovery policy. A generic task handle may schedule an already-selected closure and report mechanical completion, but must not inspect business variants or substitute for a durable owner operation.

## 2. Dependency-correct construction and lifetime

### 2.1 Composition order

1. Resolve the selected existing profile's local Person/device identity. Fail explicitly on unreadable or malformed identity. Never auto-create a profile/key to conceal failure.
2. Construct the configured runtime and platform key access. Construct the unencrypted Day repository and encrypted Vault factory independently; neither receives model/provider I/O.
3. Construct exact Gateway credential slot/store and native OS drivers. Raw bearer/endpoint/proof are private to the Gateway adapter. The credential store is not defined by Inference.
4. Construct owner repository implementations and Access using the pure trusted-consumer catalog. Construct Connections using its source operation repository and Access API; construct Inference independently using its provider/Access ports. Inference and Connections have no dependency path to each other.
5. Construct shared immutable ContextCore over source/read repositories, Access, Connections, native/Gateway readers and clock. Construct independent ContextCalendarAcquisition, ContextLearnerProjection and ContextCandidateCatalog adapters over that core; none calls an optional whole ContextService.
6. Construct Day with its own CalendarAcquisitionPort; Knowledge with its own LearnerProjectionPort; Experts with its own CandidateCatalog and common ModelPort, repositories and supplied package catalog. Context implements those inward ports. Construct Actions with its sole repository and executor/projection ports. No business owner imports floe-experts-builtin.
7. Construct ContextService from the core and explicit Day/Knowledge read interfaces plus immutable Expert candidate values. No ServiceLocator, late Option initialization or Arc cycle is allowed.
8. Construct Conversation with repository, role-neutral Engine, Context projection, common model port, event/cancellation services and direct public Access/Connections/Experts/Actions handles. Canonical §4.1 fixes its typed resolution calls; do not introduce a reverse Access→Conversation interaction workflow port.
9. Bind AppHost's verified caller and publish the typed handles to FFI/CLI. Explicit Vault unlock constructs the encrypted owner handles, activates each owner's recovery through its own API, and atomically exposes the ready generation only after required activation succeeds.

### 2.2 Lifecycle and scheduling decisions

Retain `host.rs::AppHost::{request,shutdown}`, `HostRequest` guards and the Open/Closing/Closed admission barrier. Host shutdown stops new requests, detaches observers, requests cooperative cancellation through owner shutdown APIs, drains admitted host calls, and drops runtime resources in a documented order. Observer disposal and timeout never issue `cancel_run`.

Replace the one cross-domain worker with owner services. Conversation owns its command/run map and same-ID equality; Connections owns pairing/integration operation generation; Knowledge owns learner queue/backoff; Actions owns effect/recovery state. Runtime execution may provide bounded generic admission handles and cancellation/budget primitives only. App must not preserve `is_concurrent_host` as a list of business variants.

Vault lock is an explicit lifecycle command. Before closing a Vault generation, each owner stops admission and durably interrupts or records uncertainty according to its own semantics. Never drop a committed-but-unacknowledged Action intent. Key access failure keeps the generation unavailable; no plaintext fallback or replacement key.

## 3. Exact owner API contracts replacing App semantics

The method/input/result contracts below use the repository's `BoxFuture` convention; concrete generic/lifetime spelling is implementation syntax, not a deferred ownership decision. They are **proposed technical choices**, not claims of user-approved new product policy. `actor` means the existing typed Person/device derived by `HostRequest::caller`; it is not deserializable AppWire authority. Request correlation ID remains distinct from the durable command/operation ID. Owner APIs recheck person/resource ownership; a known identifier alone is not authority.

### 3.1 Conversation

Place input/output types in `crates/modules/conversation/src/api.rs`; orchestration in `application/coordinator.rs`, session operations in `application/session.rs`, interaction resolution lifecycle in `application/interaction_resolution.rs` (publication helpers in `application/interactions.rs`), and events in new `application/events.rs`.

| Method | Input | Result / failure | Required semantics |
|---|---|---|---|
| `start_session(actor, command_id)` | Verified Person/device and non-nil command ID | `SessionReceipt { session_id, revision }` | Creates one encrypted personal session; no source/model discovery or registry mutation |
| `resume_session(actor)` / `get_session(actor, session_id)` | Exact owner/session | `SessionSnapshot` or NotFound | Query/resume existing session ownership; no implicit Run cancellation |
| `recover_session(actor, command_id, session_id, expected_revision)` | Exact reviewed CAS | `SessionSnapshot` / Conflict / Unavailable | Owner recovery, not recreate/repair by App |
| `start_turn(actor, StartTurn)` | `command_id, session_id, expected_revision, text, TurnMode, retry_of` and meaningful product purpose/boundary only | `CommandReceipt { command_id, run_id, session_revision }` | Canonical text normalization and intent digest, durable admission before model/source work; no `ProfileSelection` |
| `cancel_run(actor, command_id, run_id)` | Stable cancellation command | `CancelRunReceipt` | Direction is explicit owner cancellation; NotFound remains distinct from inactive accepted cancellation |
| `resolve_interaction(actor, ResolveInteraction)` | `command_id, interaction_id, session_id, expected_revision, decision, reviewed_digest` | Typed `Resolved / Resolving / Denied / Dismissed / Superseded / Expired / Stale / WrongDevice` + current snapshot and optional linked receipt | Claim decision durably, call exact owner review operation, record semantic result; no raw grant fields from UI |
| `refresh_interaction(actor, interaction_id, expected_revision, command_id)` | Explicit reconciliation intent | Typed current state and linked receipt | Rejoin existing operation or inspect already-satisfied owner state; never create unrelated source permission |
| `resume_interaction(actor, origin_run_id, expected_revision, command_id)` | Origin linkage, no caller-supplied original text | `CommandReceipt` | Derive stored original intent, one child claim per origin; keep Run/Task lineage while removing recipient-consent lineage |
| `read_command/read_run/read_interaction/list_interactions` | Exact owner IDs, bounded list | Typed owner snapshots | Pure queries; no reconciliation, approval, dispatch or automatic child |
| `read_events(actor, epoch, cursor, limit)` | Paired optional epoch/cursor; bounded limit | `Events { next_cursor, events } / ResyncRequired { snapshot_cursor }` | Principal filtering, retention and cursor semantics owned Conversation; durable query resolves gaps |

Keep domain failures distinct: InvalidInput, NotFound, Conflict, AccessDenied with safe typed reason, Unavailable, Interrupted, Cancelled, DeadlineExceeded. Preserve deterministic command replay and stable receipt identity. A lost admission response is not permission to submit a fresh command; lookup the same command, then rejoin only that exact intent.

Move manager `FINALIZATION_TOKENS`, cost reserve, maximum iterations/output/duration and `NoManagerTools`/`ManagerPayloadValidator` into Conversation's configuration/manager policy. App injects the configuration; it does not derive it from user text. Manager cannot acquire domain evidence through direct tool fallbacks when no Expert is suitable.

### 3.2 Access review and Connections orchestration

Access owns `ObserveReviewService::{review_observe,apply_reviewed_observe,inspect_review}` and its immutable descriptor. Connections owns user connection intent, source lifecycle, operation progress and delegates grant meaning to Access.

`ReviewRef { id, revision, digest }` is the product-facing reference. The stored Access descriptor binds Person/device, logical source/view bundle, intended consumers/purpose/categories/processing boundary, source authority, grant authority or reviewed absence, source physical resource snapshot, native subject or pinned producer, creation/expiry, and the owning operation. `apply_reviewed_observe(actor, command_id, review_ref, decision)` loads and validates all of that; UI cannot supply a replacement authority snapshot.

- `ConnectionsService::prepare_observe_review(actor, command_id, source_ref, expected_revision, requested_processing)` is an explicit idempotent command: obtains source facts and asks Access to persist the immutable review descriptor. `ConnectionsService::inspect_observe_review(actor, review_ref)` is a pure query delegating Access `inspect_review`; it allocates nothing. Native subject refresh, where necessary, is a named Connections operation and advances SourceAuthority under CAS. Do not hide that mutation in a method advertised as a pure query.
- `ConnectionsService::apply_observe(actor, command_id, review_ref, decision)` forwards the exact reviewed operation. Access compares fresh owner evidence, rejects drift, atomically commits the whole logical grant bundle, and returns an owner-produced safe status.
- `configure_source(actor, command_id, source_ref, review_ref, selected_resource_refs, expected_revision)` uses the canonical persisted SourceReview. Internal native setup resolves supported connector and subject from that stored descriptor, never a separate public authority-bearing setup DTO. A native subject port observes before/after identity under deadline/cancellation. Resources are sorted/deduplicated and bounds enforced by Connections, not Flutter.
- `reconcile_inventory` is an owner command; inventory results cannot silently widen selected scope. Current native or remote provider account drift advances SourceAuthority separately from grant authority.
- `disconnect(actor, command_id, source_ref, expected_revision)` first invalidates every non-revoked grant for that exact source, records a cleanup operation, then removes source/remote credentials by exact generation. Paused grants must not remain re-enableable after disconnect. This deliberately rejects the current legacy assertion that disconnect ignores paused grants.

A provider subject/identity change cannot be treated as an inconsequential display update. Even though current review DTOs omit provider identity, target producer attestation/source incarnation must change when the provider account changes. Store failures and unknown source state are errors, not “no source configured.” Source permission, processing permission, pairing trust and Action approval remain independent.

Pairing API and credential contracts are central-plan §3. Replace `RemotePairingCommand::{Prepare,Confirm,Status,Finalize}` plus `PairingTarget` with one Connections operation whose private Gateway transport owns raw challenges/proofs. Delete `RemoteAccessCommand`, its result/query and every wire/worker branch. Pairing never grants Observe.

### 3.3 Experts

New `crates/modules/experts/src/api.rs`, `binding_service.rs`, `environment_service.rs`, and `execution.rs` hold the extracted owner work. Existing `registry.rs`, `directory.rs`, `task.rs`, `dispatch.rs`, `settlement.rs` remain canonical semantic owners.

- `ensure_shipped_installation(actor, operation_id, catalog_revision)` installs supplied manifests through the same Package→Installation path as other supplied packages. Stable operation/digest rejoin; no install during a read-only registry query or per-turn fallback.
- `set_installation_enabled(actor, command_id, installation_id, expected_revision, enabled)` changes installation and all linked assignment visibility atomically, then republishes Directory from committed state. No UI fanout.
- `inspect_binding(actor, assignment_id, requirement_key)` resolves current package/definition/binding and obtains normalized candidates through **Experts-defined `CandidateCatalog`**. Result includes candidate ID, safe label, availability, selected flag and binding revision. Missing configured candidates remain visible as unavailable, not silently removed.
- `prepare_binding_review(actor, command_id, assignment_id, requirement_key, expected_binding_revision)` persists an Experts-owned immutable descriptor containing package/definition, exact current binding and the candidate set, returning an opaque review reference. `inspect_binding_review(actor, review_ref)` is pure. `replace_binding(actor, command_id, review_ref, expected_binding_revision, candidate_ids)` resolves package/definition/requirement from that descriptor and re-resolves exact current metadata, rejects duplicates/unknown candidates and stale CAS, commits selected source references and publishes Directory after commit. Same command with changed selection conflicts; replay does not widen.
- `invoke(EndpointInvocation, ExecutionScope)` validates installed admission, immutable selection, principal/device, exact package definition and Task identity. Context source reads and Inference attempts are supplied through owner ports. All shipped packages execute the same role-neutral Engine; delete the App `A2ASendMessageRequest -> InProcessAgent -> A2ATask` roundtrip.
- `settle(task_id, admission, exact_selection, ExpertReport)` checks Task/package/evidence/coverage and coordinates repository settlement. Calendar draft sealing is Actions-owned; an Expert artifact bearing an Actions media type cannot manufacture approval/effect authority.

Context implements `CandidateCatalog` and source-read ports without a reverse Experts→Context concrete dependency. Composition supplies the trusted package catalog. Package ID identifies a registry entry, not a switch that grants privileges.

### 3.4 Context, Day, Knowledge and Actions

`ContextService::read_selected` receives exact admitted source selection, consumer/purpose, typed query, budget/deadline/cancellation, and returns `Ready { view, dependencies, held_leases } / Unavailable(reason) / NeedsSourceReview(blockers)`. Transport/identity/policy integrity errors remain errors. Successful empty is not unavailable. Provider/native values are converted behind the boundary; no arbitrary UI-provided JSON becomes trusted observation.

Move `SelectedCalendarContextReader`, personal readers, `CompositeDependencyResolver`, dependency routing and review classification into Context. Native driver construction remains adapter-owned. `ResultRecorder` must converge with canonical `TaskCoverageRecorder`: no two authoritative dependency journals, and dependent content cannot default to Independent because a recorder was absent.

Day defines:

`CalendarAcquisitionPort::acquire(request: CalendarRefreshRequest, scope) -> Result<CalendarAcquisition, CalendarRefreshError>`.

The exact request is CalendarRefreshRequest { actor: OwnerActor, query: DayQuery, expected_mirror_revision: Revision }. Context resolves current configured sources; no product-supplied source authority enters this request. Context implements the port, revalidates current Connections/Access source facts, acquires exact bounded native/remote evidence and returns source identity/revision, coverage, per-resource batches/failures and observation time. Day's `refresh_day` applies the result under mirror CAS and projects the snapshot. Day never imports Context. Remove AppWire `ImportCalendar`, `ImportCalendarSources`, `CalendarFailed` mutations; native callback completions remain a mechanically correlated OS bridge only where platform execution requires it.

Knowledge owns the entire background `LearnerModelHost`/queue/review meaning. S1 switches the model to the common planner; S2 moves the host into Knowledge and shares Context projection. Preserve one budget/journal, no tools/delegation or direct memory activation, structured bounded proposal, evidence/expiry, review/CAS and foreground preemption. Delete `LearnerRecipientAuthority`; do not replace it with permissive unconditional remote authority.

Actions exposes `submit`, `decide`, `reconcile`, `inspect`, `list`, `inspect_authority`, and `set_calendar_create_authority`, with canonical command/revision/scope fields. `submit` differentiates direct explicit product instruction from agent proposal, checks current standing policy, writes durable intent, and continues eligible execution/collection itself. `decide` binds exact reviewed action/version and continues owner workflow; Flutter must not issue a second execute step. `reconcile` resolves uncertain outcomes by exact marker/idempotency lookup, never blind replay. The accepted decision in §8 consolidates current plaintext/direct and encrypted/Expert storage behind one encrypted ActionsRepository; external Action availability requires an unlocked Vault. App does not infer approval from `agent_origin` or choose `LOCAL_PERSON`. Go Actions/writes remain out of scope.

## 4. File/symbol cutover and caller rules

The machine-readable [App declaration map](file-read-ledger/app-symbol-map.json) enumerates baseline file, line, declaration kind, symbol, target owner/module, operation and stage. A `MOVE` includes methods and private helpers of that declaration; a `SPLIT` follows the concrete group rules below. Test symbols are future T0 ledger-before-delete candidates only. No test can be removed on the strength of this plan's lexical span inventory alone.

### 4.1 Top-level App files

| Baseline file / primary symbols | Exact extraction |
|---|---|
| `api.rs`, `bootstrap.rs`, `host.rs`, `diagnostics.rs` | KEEP verified host/lifetime and safe diagnostics. Narrow exported owner context conversion; no semantic policy added |
| `services.rs:11 StartTurn`, `:61 TurnMode`, `ContinuationRef`, resolve/refresh/resume commands/results, read/event types | MOVE to Conversation API; canonical intent and validation have one definition. Remove profile selector. `:500 CalendarActionsResult` moves Actions API |
| `turn_request.rs:12 ConversationTurnRequest`, `ConversationResumeRequest` | MERGE into Conversation canonical admitted intent/resume types; delete App duplicates after call sites move |
| `events.rs:15 AppEventBuffer` and all methods | MOVE to `conversation/application/events.rs`; owner principal/cursor/resync semantics preserved |
| `session_services.rs`, `knowledge_services.rs`, `expert_services.rs`, `action_services.rs` | MOVE domain DTO/validation to named owner APIs. REWRITE App impls as direct typed forwarding; delete `local_request` result-slot extraction |
| `connection_observe.rs` full production section | SPLIT safe Connections projection from Access stored descriptor/match policy. Replace raw expected-authority product DTO with `ReviewRef` |
| `connection_services.rs` production source mutation/probe/setup helpers | MOVE orchestration to Connections; native subject implementation behind port; no `FloeCore.store` or `vault_host` reference |
| `first_party_observe.rs:9 FirstPartyObservePolicy`, digest and policy constructors | MOVE Access; replace direct builtin manifests lookup with injected trusted package catalog; source-processing target replaces `LocalOnly` default/recipient mutation |
| `personal_source_spec.rs:6 PersonalSourceSpec` and methods | MOVE Connections native source descriptor service. Dormant Android entries are not expanded |
| `context_services.rs` / `local_context.rs` | SPLIT Context trusted observation/acquisition owner from platform mechanical broker. Raw FFI completions validate against outstanding Rust-owned request. No product arbitrary publish/import |
| `day_services.rs`, `calendar_facade.rs` | MOVE Day commands/person/CAS; calendar refresh via Day acquisition port implemented Context. Remove Flutter-origin mirror ingest API |
| `action_facade.rs` | SPLIT Action policy and execution→Actions; `calendar_connector_snapshot`→Context; concrete native driver construction→provider adapter; pure error conversion→binding |
| `remote_services.rs` | REWRITE Pairing into Connections operation; raw address/proof/challenge→Gateway private staging; DELETE RemoteAccess; Observe expected values→Access |
| `composition.rs` | KEEP `open`, runtime construction, shutdown, injected handles. MOVE all prechecks/lifecycle/auto-resume/outcome interpretation to owners, then forward. DELETE `continuation_mode` duplicate and old `access_result`/`pairing_result` optional-slot projection |
| `core.rs`, `error.rs` | Retire `FloeCore` pass-through domain facade; keep repository construction in composition. Day owner errors retain metadata; binding serializes safe failure, not an App parallel error domain |
| `worker.rs`, `local_operations.rs` | DELETE tagged cross-domain bus/result matrix after last owner migrates. Move VaultState to host lifecycle; Calendar types Actions; Session types Conversation. Test-only WorkerOperation/events have T0 gate |
| `vault_services.rs` | KEEP explicit Vault lifecycle boundary, REWRITE typed result/generation and owner activation/shutdown; no cross-domain worker |
| `prompts.rs` | DELETE empty comment-only module after lib declaration removal |
| `lib.rs` | Narrow modules/reexports to composition/typed owner surface; remove native provider DTO and consent/profile/RemoteAccess exposure |

### 4.2 Deep `vault_host` groups

- `vault_host.rs::VaultBridge`, `Worker`, `WorkerMessage`, `Job`, `Progress`, `VaultExecutionResult`: delete after replacing typed owners. Do not move all to `runtime` with business enum intact.
- `conversation_command_identity`, `resume_command_identity`, conversation query/precheck/cancel/resolve/refresh/get/list jobs, `claim_auto_child`, `resume_lineage`, `execute_conversation_*`: Conversation coordinator. Stable ID replay compares complete canonical intent; existing fallback accepting unnameable intent must be replaced by explicit validation before job admission.
- `OpenVault::{activate,prepare_root_agent_environment,publish_expert_directory}`, validation of registrations, `ensure_expert_bundle`: split storage open/lifetime in composition from Experts installation/environment service. `Deref<EncryptedAgentVault>` must not give every owner implicit table access.
- `execute_action` is not moved wholesale. Vault variants→host lifecycle; Registry/binding→Experts; Observe→Connections/Access; CalendarAction→Actions; Session/Turn/Resume/Interactions→Conversation; Memory→Knowledge; Pairing→Connections. Standalone RemoteAccess variants deleted. Every branch receives typed result instead of setting another optional slot.
- `expert_setup.rs::VaultExpertBundle`: Vault's implementation of Experts install repository/port, injected manifests. `remote_authority.rs::VaultPairingKeys`: Vault key-sign/settle adapter; separate enrollment facade removed.
- `calendar_access.rs`: Context owns dependency authorization and view read orchestration; native Calendar subject transport→provider adapter; Connections owns current source/subject update; Access owns logical grant review/activation. No concrete Vault imports in owners.
- `personal_access.rs`, `remote_observe.rs`: Access review/grant service and store ports. Connections coordinates source lifecycle. `product_overview` is typed safe read model, not authority. `disable_bundle` revokes paused as well as active on disconnect.
- `personal_grants.rs`, `remote_views.rs`: Context authorization and exact read binding retained; private gateway transport/repository adapters injected. Remove App concrete source clients.
- `review_snapshot.rs`: Access descriptor capture, Conversation opaque reference. Preserve blocked member's observed expected grant/absence rather than substituting current authority. For drift, create a new review descriptor and expire/supersede old reference, never overwrite meaning.
- `interaction_resolution.rs`: Conversation decision CAS and replay/expiry/supersede lifecycle; Access compare/mutate/reconcile owner receipt; Experts exact binding readiness. Delete recipient consent branch. `Resolving` stays in-progress, never mapped to terminal merely because refresh did not prove completion.
- `interaction_owners.rs`: eliminate cross-owner host object. Access/Connections expose review APIs; Experts verifies Task-selection validity; Conversation calls typed ports. Remote readiness must prove active usable grants, not simply count non-revoked rows.
- `conversation_turn.rs`: Conversation prepare/run/resume/recover/automatic gate and manager config. Context owns composite dependency validation. Inject dependencies once rather than construct RootModelProvider and exact-recipient authority on each host turn. The canonical constructors are CompositeModelProvider::new(gateway, device), GatewayModelProvider::new(credential_store), and FoundationModelProvider::new(SessionProtection). They store no purpose/consumer defaults; observation receives the admitted ModelPlanRequest. Conversation owns CONVERSATION_PURPOSE/CONVERSATION_CONSUMER in domain/purpose.rs; Expert package role and Knowledge learner supply their own purpose at prepare. CLI/smoke callers use the same injected provider constructor and explicit owner intent, never root-default helpers.
- `conversation_turn/engine_ports.rs`: Conversation manager policy. `interaction_publication.rs`: Conversation trusted source/binding review publication; delete model-recipient blocker path. Model-produced JSON never creates authority even with a matching media type.
- `expert_binding_settings.rs`: Experts owner lifecycle with CandidateCatalog port; exact package/definition/CAS/replay and Directory publication. Initial default binding is explicit installation operation policy, not model inference.
- `expert_dispatch.rs`: Experts canonical Task endpoint/environment + package runner adapter. Remove `ConversationExperts`/`DelegatedMessageExperts` optional-service matrix and in-process A2A conversion. Context source execution supplied via declared capability port; trusted blockers returned as typed report outcomes.
- `expert_host.rs`: common Engine model calls replace `ExpertModel`/`ExpertReasoner` duplication. Context owns source readers and live classification. Canonical Task coverage records exact dependencies; no unrecorded dependent data and no separate finalization authority side channel.
- `expert_dispatch/stateful_settlement.rs`: Experts validates admitted registry/selection/Task result; Actions seals calendar proposal using exact contributing evidence and its own policy. Vault persists the resulting domain transaction. Stateful draft does not execute an external write.
- `learner_worker.rs`: Knowledge learner runner with common Inference, Context projection and bounded background scope; no local-only hidden override.

### 4.3 Caller/import/wire conversion procedure

For each mapped group: define owner DTO/port and error first; add repository/transport implementations; instantiate in App composition; migrate every internal caller/import; migrate AppWire/FFI and CLI/Flutter callers from the same snapshot; remove obsolete App symbol/export/module; statically search all references and inherited doc/script paths. Do not keep an App reexport facade merely to avoid updating imports.

FFI stays a boundary: decode shape, invoke admitted owner method, serialize exact owner-produced result. FFI declares explicit dependencies on the owner public API crates it converts; App reexports do not conceal those edges. FFI imports no concrete Vault/provider adapter, and Protocol remains pure DTO/contracts. Native acquisition callbacks use the restricted `native_host.*` internal protocol with host-issued Person/device/runtime/host-epoch/outstanding-operation correlation; feature gateways cannot publish arbitrary trusted views. Failure classification, retry/action eligibility, expiry, required reload/seal and automatic continuation are owner semantics. Protocol `serde_json::Value` wrappers must be replaced with typed safe product projections, not simply serialized domain snapshots.

## 5. CLI, examples, fixtures and tools

- `examples/floe_cli.rs`: keep real debug CLI. Remove `Options.profile`/`--profile`/`profile_selection` and explicit profile persistence; use product intent. Forward owner APIs. Keep absolute existing-profile preflight, lock check, no Vault creation/reset, explicit cancel, reviewed digest, same-command uncertainty lookup, read-only inspection and payload-safe traces. Its eleven inline unit tests are future T0 candidates, not the CLI itself.
- `examples/local_model_smoke.rs`: retain opt-in synthetic diagnostic entrypoint. Use canonical Inference planner and role-neutral device transport; an explicitly local transport diagnostic is not a production fallback policy.
- `examples/local_model_smoke/learner.rs`: remove duplicated production learner host; call Knowledge-owned runner with synthetic repository/input. Keep exact disposable encrypted scope and proposal-review behavior.
- `examples/local_model_smoke/manager_guidance.rs`: remove `MemoryConsents`, exact recipient env gate, `RecipientLineage`, concrete profile and exported credential-file model. Use an existing authorized Gateway capability through private adapter configuration; no credential discovery/export. Retain complete corpus, typed rubric/phase metadata, fixed repetitions, model identity attribution and shape-versus-truth distinction. A valid `choice_accepted` never proves source-grounded truth.
- `examples/vault_keyring_smoke.rs`: retain opt-in probe/exercise/cleanup, exact private disposable root/marker ownership/mode checks and verified key absence. Its three cleanup tests may be removed after ledger; real tool still requires explicit run authorization.
- `tests/connected_calendar.rs`, `src/vault_host/tests/**`, inline test blocks and `tests/fixtures/LocalModelFixture.swift`: future T0 classification, documentation, then recoverable deletion. Fixture consumer graph first. No blanket folder deletion can decide which real example/tool survives.
- `fixtures/expert-report/delegation-v1.json`: shared Rust-produced/Dart-consumed contract fixture. Preserve through T0 if retained tooling needs it. Future S3 regenerate from canonical Task product projection after deleting in-process A2A conversion; no reconstructed Dart authority artifacts.
- `fixtures/manager-guidance/{corpus.json,README.md}`: retain evidence/rubric while future runner changes. Obsolete recipient instructions updated only with the new runnable path.
- `fixtures/remote-authorization/*`: keep signed fixture meanings until verified producer/issuer/runtime protocol change is deliberately coordinated. These known synthetic seeds are not user credentials.
- `tools/architecture/check_boundaries.py`: keep structural dependency checker; update policy with exact new owner-port edges and preserve Inference/Connections separation. Run only at authorized later gate.
- `tools/validation/calendar/core-check.c`: diagnostic ABI host, not assertion suite. Keep and update ABI exports with binding cutover. `ResponseLoss.swift`/`native-tests.swift` are test-only fault/assertion helpers requiring behavior ledger before retirement; `inspect.swift` is an exact opt-in recovery tool and must not be mass-deleted.
- Python native-build/fixture tests and `LocalModelHostTests.swift`: document every behavior/parameter case, then remove only test code. Keep production packaging. The full consumer audit in [root/tool cutover](2026-10-02-root-tools-cutover.md) confirms build_test_fixtures.py and its fixture inputs/test-host plists are exclusively consumed by retiring tests; remove that closed group in T0 only after the corresponding behavior ledger.
- `scripts/reset-local-data.sh`: never run in this task. Later credential namespace changes update exact documented Floe slots; no broad key deletion or automatic reset. No data is deleted in this planning task. A later exact-target confirmed Floe development reset may discard old data after a narrow external-effect hazard check; do not replay old operations or touch unrelated provider data.
- root `tests/composition/main.rs`: five comments, zero executable test behavior; record as obsolete empty placeholder before future deletion.
- `.agents`, root manifests, license, design system and brand assets: full reviewed. Keep safety/design/license/toolchain posture. `Cargo.lock` is generated resolved state including dev dependencies, not production dependency policy; regenerate only at authorized future slice boundary if dependencies change.

## 6. Ordered closure and temporary incompleteness

S1.1 establishes owner command/result/error and all referenced ports first. S1.2 implements paired private Gateway capability plus trust-coupled integration cleanup. S1.3 establishes Access stored review/processing and mandatory Health transform, Context projection outcomes, and typed source state. S1.4 switches Manager/Expert/Learner inference together. S1.5 moves Conversation coordinator/interactions/events. S1.6 migrates App/FFI/Flutter/CLI callers atomically. S1.7 deletes in-scope obsolete fields/branches and performs one completed-slice format/compile gate.

Remaining source-reader/personal/Day/Expert/Knowledge/Actions structural extraction can remain physically in old files inside S1 only when it already uses the canonical S1 authority/model contracts and has one runtime path. It is explicitly removed in S2.2–S2.5; no legacy alias/dual decoder is added. S2.3 uses pre-established common Engine and source ports; S2.4 closes Actions/Knowledge; S2.5 deletes the final cross-domain worker and App semantic facade. Only after all S2 structural mappings resolve are new tests written in S3.

Current preparation remains static reads/docs. T0 is a future approved phase: complete per-behavior ledger, shared-consumer audit, exact syntactic test-item deletion and recoverable diff. No tests/builds/formatters at T0. Final compilation/build is mandatory later; Linux cannot prove changed Apple native binaries compile, so required macOS/iOS gates remain explicitly unavailable until an authorized matching environment exists.

## 7. Static review findings to resolve in the plan

1. App model/Expert/Learner logic still uses concrete profiles and exact recipient consent; the frozen main has not landed newer CP07/08. Conversation reporting work underway may refer to unpushed/other-checkout work; no contradiction is inferred.
2. `remote_observe::disable_bundle` ignores paused grants during disconnect, and tests assert that. Target revoke all applicable non-revoked grants deliberately differs.
3. `interaction_owners::remote_scope_satisfied` can count paused non-revoked rows without proving active authorization. Target exact owner recheck replaces this heuristic; a resolved UI card never serves as dispatch authority.
4. Native Calendar snapshot capture mutates subject revision despite broad read-only wording. Make source observation update explicit; keep query/preview/approve meanings separate.
5. `refresh_interaction` can label a still-Resolving state terminal. Owner result must preserve pending/retry/uncertain state rather than let presentation decide.
6. `WorkerAction::Connections` fabricates `local-{OS}` instead of forwarding verified caller device. Typed service must use admitted identity end-to-end.
7. App native actions use `LOCAL_PERSON`; remove hardcoded principal and bind validated host/person through executor capability.
8. Current shared fixture/evaluation tests contain assertions about obsolete shapes. Preserve evidence and safety motivation, not the old representation or test verdict.

All 75 App files are fully read. This appendix remains a review draft until complete symbol disposition/caller edges, cross-owner signature reconciliation and baseline-wide independent static audit are closed.

## 8. Accepted Actions storage and effect contract (R8 closure)

**User-accepted decision, 2026-10-02 13:46 UTC:** every external Action, whether directly requested or Expert-proposed, has one authoritative encrypted record in the Person's Vault. Remove the plaintext/direct versus encrypted/Expert lookup split. Local Day task/note/capture CRUD remains in its Day repository and is not an external Action. All Action detail, review, dispatch and recovery operations require a ready Vault; a locked Vault returns `VaultLocked` rather than a partial list, a plaintext fallback or implicit unlock. A safe non-content lock status may still be shown. This intentionally changes any legacy direct-action availability while locked.

### 8.1 Canonical record and repository

Declare `ActionsRepository` in `crates/modules/actions/src/ports/repository.rs`, and implement it as `VaultActionsRepository` in `crates/adapters/vault/src/repositories/actions.rs`. The sole authoritative key is `(PersonId, ActionId)`; the exact external operation key is `ExecutionId`. No operation probes competing stores or infers physical location from origin. Day holds only a derived receipt/mirror projection, never an approval or dispatch ledger.

Replace `CalendarAction.agent_origin: Option<_>` plus `direct: bool` with a validated sum type:

- `ActionOrigin::Direct { command_id, actor_device_id }` binds the exact user command, including the complete effect digest.
- `ActionOrigin::Expert { task_id, invocation_id, package, installation_id, assignment_id, definition_revision, evidence_ref }` binds the exact admitted Task and proposal evidence.

`ActionRecord` contains `id`, `person_id`, `revision`, typed `origin`, immutable canonical `CalendarEffect`, `effect_digest`, `execution_id`, current `ActionState`, source/executor preconditions, reviewed approval reference, creation/expiry, and settled receipt/collection state. `CalendarEffect` has explicit Create/Update/Delete variants, with exact calendar/source IDs, title/schedule for writes, and expected external/original identity for mutations. Invalid combinations are not represented through optional booleans. Provider-specific transport fields and credentials never enter the record.

`ActionAuthorization` is either an exact direct instruction, a stored reviewed decision, or a current standing policy receipt. Each binds the immutable effect digest, Person, relevant authority revision and expiry. An Expert result, a UI boolean or a prior source processing grant cannot construct it. Store origin/evidence and authorization separately; direct instruction still undergoes current source/executor/precondition validation.

Owner port methods (all asynchronous, using existing `BoxFuture`, typed validated inputs and `Result<_, ActionStoreError>`):

| Method | Exact input and result | Atomic storage meaning |
|---|---|---|
| `get` | `(person_id, action_id) -> Option<ActionRecord>` | One exact record; foreign Person indistinguishable from absent |
| `list` | `(person_id, cursor, limit) -> ActionPage` | Bounded authoritative records from one store; no second-list merge |
| `admit` | `ActionAdmission { command_id, request_digest, record } -> AdmittedAction { record, replayed }` | Insert record and command receipt together; same command/digest rejoins, changed digest conflicts |
| `record_decision` | `ActionDecision { command_id, action_id, expected_revision, review_ref, decision, now } -> ActionRecord` | Decision and exact reviewed effect/authority CAS in one transaction; no provider I/O |
| `prepare_dispatch` | `DispatchIntent { action_id, expected_revision, execution_id, effect_digest, authorization, current_source_fence, executor_generation, now } -> DispatchAdmission` | Validate expected action revision/digest and owner-authorized state, persist durable intent and `Executing` state together before returning; identical execution rejoins |
| `load_execution` | `(person_id, execution_id) -> Option<DispatchAdmission>` | Recovery reads the one immutable operation identity; no new execution key |
| `settle_execution` | `ExecutionSettlement { execution_id, effect_digest, expected_revision, outcome } -> ActionRecord` | Exact receipt/failure and state CAS; success also creates a durable collection ticket in the same transaction |
| `pending_recovery` | `(person_id, cursor, limit) -> RecoveryPage` | Enumerate Executing/Unknown/collection-pending records without mutation |
| `ack_collection` | `CollectionAck { execution_id, receipt_digest, expected_ticket_revision, day_projection_ref } -> CollectionTicket` | Acknowledge only the exact idempotent Day projection after it is persisted; never changes approval/dispatch meaning |
| `read_authority` / `compare_and_set_authority` | Person and expected policy revision, exact change command -> typed authority snapshot | One authoritative Actions standing policy; no duplicate plain/encrypted policy write |

`ActionStoreError` is closed: `VaultLocked`, `Unavailable`, `NotFound`, `Conflict`, `InvalidRecord`, `CorruptRecord`. Domain policy denials stay Actions outcomes and are not rewritten as storage errors. No storage error permits a second store, a replacement key, or a recreated record.

### 8.2 Exact state and crash ordering

Use one owner state machine: `PendingReview`, `Approved`, `Rejected`, `Cancelled`, `Expired`, `Executing { execution_id }`, `Blocked { reason }`, `Failed { reason, not_applied_proof }`, `Unknown { reason }`, `Succeeded { receipt, collection }`. `collection` is `Pending { ticket_id }` or `Collected { day_projection_ref }`; it is not a second execution state. Direct or standing-authorized commands may skip PendingReview only when Actions records the precise authorization. The final model does not expose a public UI `execute` command after approve.

`Rejected` means a recorded denial. `Cancelled` and `Expired` are terminal only before any dispatch intent is consumed. `Blocked` means preflight/admission cannot currently proceed (`PermissionDenied`, `PolicyDenied`, `SourceChanged`, `ExecutorUnavailable`, `ScheduleConflict`); it never claims an uncertain external effect failed. `Failed` requires authoritative provider evidence that the exact operation was not applied, with reason `PermissionDenied`, `ProviderRejected` or `ProviderUnavailable` and a stored `NotAppliedProof`. Timeout, response loss, invalid/mismatched receipt, cancellation after handoff, and inconclusive lookup become `Unknown`, never `Failed`. A plain not-found lookup is not automatically a NotAppliedProof. Terminal non-success retains its immutable execution record; a new user request is a new reviewed action, not an automatic retry.

1. `submit` validates the actor, immutable effect, exact source/executor and current permission. It admits one Action and command receipt. A proposal needing approval remains PendingReview; a directly/standing-authorized effect proceeds under the stored authorization.
2. `decide` CASes the reviewed record/authorization. Actions immediately owns further progression; closing the UI does not stop it and rereading Approved does not dispatch again.
3. Actions performs external preflight outside a Vault transaction. It rechecks live source/approval/expiry/cancellation immediately before `prepare_dispatch`. That transaction commits the durable intent and Executing state. No external write can occur before it.
4. The adapter consumes the dispatch admission for that exact execution/effect once. No global Vault lock spans provider I/O. If acknowledgement is lost or process exits after the intent, recovery sees Executing/Unknown and performs exact marker/idempotency lookup. It does not create a new operation or blindly repeat the write.
5. `settle_execution` verifies exact execution, Person, provider/calendar/external identity and canonical effect receipt; ambiguous/mismatched results remain Unknown. A successful effect receipt and collection ticket commit together.
6. Context/Day acquires the exact resulting observation and Day applies it idempotently keyed by `(execution_id, receipt_digest)`. Only after Day's commit does `ack_collection` mark the ticket collected. A crash before acknowledgement repeats collection only, never the external effect. The UI differentiates “external change succeeded; collection pending” from a failed or unknown write.
7. Lock or key failure after dispatch preserves the intent and external uncertainty. Subsequent operations return VaultLocked/Unavailable until explicit unlock/recovery; no plaintext recovery shortcut. Denial, cancellation and expiry before dispatch stop it; cancellation after uncertain dispatch does not assert that no effect occurred.

### 8.3 Current-symbol/caller replacement

- `actions/ports/mod.rs::ActionRepository` and `ExpertActionStore`: merge storage concerns into the above owner port. `CalendarSourceReader` remains a distinct read port; `CalendarActionProvider` remains the real external executor boundary.
- `app/action_facade.rs::{actions,expert_actions}`: DELETE constructor facades. `calendar_action_command`, `execute_expert_calendar_action_with_cancellation`, `decide_expert_calendar_action`, `recover_expert_calendar_action` become calls to the single ActionsService; FFI and CLI use its typed public API.
- `app/vault_host/product_actions.rs::execute`: DELETE two-list/dual-lookup/origin-dependent dispatch logic. Every ID resolves by ActionsRepository before domain authorization. The standalone `schedule` parser moves to typed binding input conversion; temporal/domain validity is Actions-owned.
- `app/vault_host.rs::execute_agent_calendar_action`: DELETE alternate execution path after ActionsService handles both origin variants. Do not preserve separate `agent_action_policy` and Day-store authority writes.
- Vault's current `agent_calendar_action`, admission, decide, dispatch-with-fence and settlement methods implement the new repository transaction primitives; policy/decision callbacks move to Actions. Plain Turso's action table methods cease to be an active write/read path. Its Day/calendar mirror methods remain.
- Flutter's approval/execute chaining becomes a single `decide` intent followed by observation; direct edits submit the exact action once. Bindings serialize owner-produced status/allowed actions and never infer origin, standing approval, retry or recovery policy.

### 8.4 Existing data and uncertain records

Accepted user decisions (2026-10-02 13:46 UTC): one encrypted ActionsRepository with external Actions unavailable while Vault is locked; current loopback Gateway deployment scope; existing Floe development data may be discarded for a clean new profile/schema. No legacy-data migration or old-runtime retention is required solely for compatibility. This is a plan-only revision, not an instruction to execute reset/deletion now. Any later permanent deletion needs exact local Floe targets and action-time confirmation; unrelated provider/calendar/mail data is outside scope. Before any approved reset, check narrowly for in-flight/uncertain external effects, stop old dispatch and prevent replay into the new profile; do not impose preservation of the old database/keys/runtime as a product requirement. New-system durable intent, uncertainty and recovery invariants remain unchanged.

