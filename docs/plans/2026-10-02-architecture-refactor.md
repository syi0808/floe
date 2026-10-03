# Floe architecture refactor: target and execution contract

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

- **Status:** reviewed specification; product/data decisions accepted at 13:46 UTC and ordered execution authorized at 13:49 UTC. All 1,172 tracked baseline files are accounted for by full semantic reads or appropriate asset inspection. Cross-contract/symbol-map reconciliation and independent static review R1–R8 are complete; see [review verdict](2026-10-02-plan-review.md). No implementation or test deletion was performed in preparation; execution now proceeds only through the coordinated next-stage handoff and recorded prerequisites.
- **Baseline:** `3f4b407f8079d611224cd7adbef121f9e7e75e8e`, branch `prep/architecture-test-ledger-20261002`.
- **Current scope:** inspect every tracked baseline file; inventory tests and classify inspected legacy mismatches; specify the future complete behavior ledger; produce an execution-ready architecture, symbol/file cutover map, dependency-correct sequence and verification/deletion gates. Stop for review.
- **Required order:** P0 is plan/read only. After separate authorization, T0 first documents every legacy test behavior before deleting the old tests. S1 is one dependency-closed Conversation vertical: trust-integrated pairing and private credentials, source-processing review and mandatory local Health transform, then Gateway Primary/common selector and complete caller cutover; G1 formats and compiles only the completed slice. S2 finishes all remaining structural owners and the final production compilation/build (G2). Only then does S3 design/write new tests and perform final behavioral/build qualification. No tiny-step compile/format/test loops or unnecessary full builds.
- **Non-goals:** compatibility with obsolete internal APIs/local schemas; preserving existing behavior merely because code or a test asserts it; product implementation during preparation; automatic profile/key reset; external account changes; push/deployment; Android expansion; marketplace, sandbox ecosystem or remote A2A distribution; a Go Actions domain.

## 1. Authority, evidence and plan hierarchy

This is the proposed single execution-order authority for the requested repository-wide structural refactor. The existing [ADR0034 convergence plan](../development/plans/reasoning-source-processing-convergence.md) records earlier scoped sequencing and evidence; its accepted semantic requirements are incorporated here, not silently cancelled. Once this plan is accepted, its stage order replaces overlapping implementation/checkpoint instructions, and the old plan must link here and identify its historical status. The older [Agent environment/grounding plan](../development/plans/agent-execution-environment-grounding.md) remains historical evidence. Do not run two overlapping active migration sequences.

Current source/manifests establish what exists. Product requirements, [architecture invariants](../architecture/invariants.md), and the amended ADRs establish required meaning. Legacy tests establish what was asserted, not that the implementation works or that the assertion should survive. During preparation, the user's plan-only hold superseded the earlier immediate test-deletion request. The later 13:49 UTC instruction authorizes ordered execution while preserving the complete behavior-ledger prerequisite before deletion.

Read the domain appendices alongside this file. They refine exact source symbols, line anchors, destination files, callers and wire changes; they do not create competing architecture or stage order:

- [Client cutover](2026-10-02-client-cutover.md), [presentation preservation](2026-10-02-client-presentation-plan.md), and [client test-removal plan](2026-10-02-client-tests-plan.md);
- [Canonical wire/product/dependency contracts](2026-10-02-canonical-contracts.md), [Rust contracts and cutover](2026-10-02-rust-cutover.md), [Vault/storage cutover](2026-10-02-vault-cutover.md), and [bindings cutover](2026-10-02-bindings-cutover.md);
- [Server contracts/cutover](2026-10-02-server-cutover.md), [per-file/symbol map](2026-10-02-server-symbol-map.md), and [machine-readable symbol map](file-read-ledger/server-symbol-map.json);
- [App/owner cutover](2026-10-02-app-cutover.md) and [root/tools cutover](2026-10-02-root-tools-cutover.md), including exact Actions storage/effect proposal and production-tool callers;
- [file-read ledger](file-read-ledger/) for exact baseline-file accounting.

**Preparation gate satisfied:** independent review re-read the final reconciled contracts/maps and verified explicit dispositions for all 1,172 baseline paths. The plan passed review; the three decisions and ordered execution are now accepted, while specific destructive/security action gates remain in force. The canonical appendix fixes exact inference/product/dependency signatures; other appendices refine implementation ownership without creating alternate contracts. These are preparation deliverables, not deferred implementation-design decisions. The one-encrypted-store Actions model and resulting locked-Vault availability change in App §8 are user-accepted decisions as of 2026-10-02 13:46 UTC; ordered implementation was authorized at 13:49 UTC, subject to the recorded prerequisites.

### 1.1 Checkout facts: CP07/CP08 are not landed in this frozen snapshot

The checkpoint namespace matters. The old Agent environment plan's CP07 is a Foundation grounding evaluation; its proposed CP08 was deferred by ADR0034. The newer reasoning/source-processing plan's CP07 is Gateway inference-wire cleanup, and its CP08 is the shared Primary/Fallback selector. They are different work.

The frozen newer-plan document reports CP00–01 complete and CP02 not started; conversation context reports CP07/CP08 work underway. Source independently confirms only that those cutovers are absent from this frozen main snapshot, without deciding the state of unseen local/unpushed work:

| Baseline evidence | Observed implementation | Required target |
|---|---|---|
| `apps/client/lib/app/runtime/floe_client.dart` and `features/conversation/application/agent_conversation_gateway.dart` | `profileId` flows into explicit profile product intent | Product purpose only; no profile selector |
| `crates/app/src/turn_request.rs` | `ConversationTurnRequest.profile: ProfileSelection` | Conversation-owned intent without concrete model selection |
| `crates/adapters/providers/src/models/server.rs` | inventory decodes `requires_external_consent`, `placement`, `recipient` | Gateway purpose capability, explicit absence/error distinction |
| `server/internal/inference/gateway.go` | request has `AllowExternal`, `ExpectedRecipient`; response exposes routing placement/external transfer | purpose/input/output contract; downstream routing internal |
| `crates/modules/inference/src/application/service.rs` | `rank_profile` ranks Device first; automatic candidate loop continues after transport failure | Gateway Primary; Fallback only for valid planning-time Primary absence |
| `crates/modules/inference/src/application/router.rs` | another route/recipient selector exists beside service selection | One Inference selector |
| `crates/adapters/providers/src/models/root.rs` | root-named provider assembly; shared saved connection already exists | Role-neutral Gateway/device composition |
| `crates/modules/connections/src/ports/remote_control.rs` | pairing port only confirms/observes; Flutter starts/cancels | Rust owns complete pairing lifecycle |

Conversation context reports CP07/CP08 work underway. The frozen main checkout does not contain those cutovers; local/unpushed work may exist elsewhere and has not been inspected here. This is a source-snapshot difference, not evidence that the reported work failed or that anyone claimed it complete. Before later implementation, reconcile any supplied work against the frozen baseline and refresh the map; never overwrite unseen work or copy historical validation claims into new evidence.

### 1.2 Inspected defects that must not become target requirements

These are static findings, not newly executed failing tests. Each is fixed in the mapped owning slice rather than preserved behind an interface:

- Flutter `LocalServerPanel` owns raw pairing proof, HTTP start/cancel and credential persistence; disposal can cancel the attempt. Rust owns the operation and commit/readback; disposal only detaches.
- `ConnectorScreen._synchronizeServerCalendar`, `_bindServerCalendar`, `_catalogChangedExplicitly` mutate bindings while reading catalog; `_macOSAttentionConnection` fabricates source/permission state. Catalog observation becomes strictly read-only, and Rust owns source summaries and explicit configuration.
- `NativeDayGateway` implements source fanout, reconciliation, import and failure policy; Flutter publication/refresh helpers hard-code freshness timers. Move those semantics to Connections/Context/Day, keeping display time and OS request pumping separate.
- `CalendarActionController` locally interprets standing Allow, autoapproves, executes, collects and re-derives allowed actions/provider capability. One Actions intent owns the complete workflow; Flutter renders `allowed_actions` and exact review handles.
- `AgentRegistryController.configureCapability` fans a toggle into sequential installation/assignment mutations and assumes revision increments. One Experts command commits the intended configuration atomically; candidate selection remains configuration, not permission.
- `AgentController.closeView` can call Stop through lock behavior. Closing presentation detaches; explicit Run cancellation and explicit Vault lock are separate owner intents.
- FFI `context_wire.rs::validate_calendar_completion` compares acquisition deadline allowance with the queried Calendar range start; that can reject future-date ranges. Context must distinguish observation/deadline from requested time interval. Binding structural validation cannot become source/freshness policy.
- FFI `conversion/owners.rs` infers safe actions/retry/session sealing from stage strings, and `app_wire.rs::interaction_snapshot` chooses expiry/approval/navigation/Continue. Move semantic failure and interaction projections to owners; FFI only serializes typed owner outcomes.
- Native Calendar `LOCAL_PERSON` equality is hard-coded in App/provider paths. Constructors receive verified host Person/device and retain exact current source/subject checks; no replacement magic constant or client identity override.
- Go `microsoftauth/identity.go::requiresProviderIdentity` compares a mutable credential name with a fixed profile name, even though `BindCredential` installs a scoped key name. Target identity requirements derive from immutable provider profile/namespace, independently of credential storage key. Calendar timestamp parsing must reject malformed short values before indexing `DateTime[10:]`.
- Legacy App test `disable_still_pauses_and_disconnects_without_expectations` preserves Paused grants on disconnect. Target revokes every non-revoked grant before source/credential cleanup. No remaining grant may later re-enable a removed source.
- Review projection can omit provider identity while a test allows subject change without source-authority change. Target provider/native subject replacement advances `SourceAuthority` atomically; stale reviewed evidence is rejected even when product DTOs intentionally omit the private identity.
- App's `first_party_observe` reads builtin manifests to decide policy. Access owns that policy, receiving a validated trusted package capability catalog at construction; business modules never import `floe-experts-builtin`.
- Learner has both App's Foundation-only host and Knowledge's `LearnerModel::placement` non-device rejection. Remove both in the same Inference caller cutover, preserving candidate-only staging, independent evidence limits and foreground preemption.


## 2. Final topology and semantic owners

```text
Flutter presentation / debug CLI / native shell
    -> AppWire intent and safe projections
    -> FFI conversion + verified AppHost admission
    -> Rust semantic owner service
         Conversation -> role-neutral Agent Runtime
         Experts      -> Package -> Installation -> Directory -> Task -> Runtime
         Knowledge    -> reviewed Memory/Playbook and bounded Learner
         Connections  -> source/Gateway/integration lifecycle
         Access       -> source permission + processing admission
         Context      -> authorized bounded evidence/provenance
         Inference    -> purpose, one Primary/Fallback plan, attempts/usage
         Actions      -> approval, exact authority, durable effect/recovery
         Day          -> local domain + calendar mirror/projection
    -> owner-defined external port
    -> Rust Gateway adapter or native/storage adapter
    -> Go Gateway transport
         trust / pairing / authority / integrations / views / inference
    -> concrete provider adapter or secret store
```

### 2.1 Boundary rules

1. Flutter owns presentation, draft/input state, navigation, accessibility, display formatting and mechanical correlation. A refresh click is product intent. Flutter does not choose source authority, provider routes, bearer storage, pairing proof verification, source-reconciliation/freshness policy, permission consumers or inference eligibility.
2. AppWire carries owner intents and safe snapshots. Person/device/runtime identity comes from admitted host state, never caller fields. No saved bearer, arbitrary model endpoint, provider DTO, OAuth refresh token, model profile, exact downstream recipient or internal route crosses it.
3. A user-entered initial Gateway setup address is a bounded setup input, not a stored endpoint authority. Validate loopback under the current deployment contract in Rust; keep transport addressing private afterward. This refactor does not enable arbitrary remote URLs/TLS/self-host deployment.
4. `floe-app` constructs concrete owners/adapters, establishes verified host identity/lifetime, and forwards typed requests. It does not own domain command validation, policy digest calculation, first-party consumer selection, source reconciliation, job semantics, Expert dispatch decisions, interaction resolution or Action repository choice. Do not move these into a differently named App facade.
5. Rust owners define their ports; adapters depend inward. Inference and Connections remain independent with no dependency path between them. A shared physical credential adapter can implement both owners' distinct non-secret capability ports.
6. Go owns remote model/provider I/O, provider API keys and OAuth refresh credentials, source fetch/normalization, and enforcement of the Rust-authorized signed remote protocol. Rust retains user/domain policy. Go does not acquire a second Conversation, Memory, Experts or Actions policy engine.
7. Provider/model/effort/route configuration is internal to Go operator surfaces. A route change cannot mutate source grants, require model approval or relax source processing policy.
8. No permanent legacy aliases, dual decoders, parallel old/new buses or migration-only optional state. A bounded compile break inside a slice is preferable to a compatibility layer.

### 2.2 Keep authority distinct

- Connections: stable Person/source identity, account/device/execution owner, physical resources, local CAS and `SourceAuthority`.
- Access: logical View permission, consumer/purpose/category/processing policy and `GrantAuthority`.
- Context: exact observed physical resources, source/grant dependencies, freshness and contribution coverage.
- Experts: installation/assignment/binding and immutable Run/Task selection; configuration never grants source access.
- Conversation: Session/Run, journal, review lifecycle, durable decision and automatic fresh linked resume.
- Actions: review/approval, current provider preconditions, stable execution/idempotency identity, intent committed before I/O and uncertain-outcome reconciliation.
- Day: observations/mirror revision, not source authority. A failed refresh does not advance source authority or erase healthy cache.
- Vault: encrypted repository implementations and short atomic transactions, not cross-owner policy. Never hold a global Vault transaction across model/provider I/O.

## 3. Canonical Rust boundary contracts

These are target interfaces, not claims that the symbols already exist. The canonical-contracts appendix is the exact signature/wire/manifest authority; the Rust/client/server appendices reference it and must agree before approval. Futures/deadlines/cancellation use existing execution conventions; an opaque handle is not an authority token.

### 3.1 Gateway credentials: `GatewayCredentialStore`

**Placement:** `crates/adapters/providers/src/gateway/credentials.rs`, implemented through `crates/platform/native/src/keychain.rs` or a narrow native secure-storage driver. Secrets are adapter/platform-private. Connections defines only non-secret enrollment persistence semantics; Inference obtains prepared transport through its own port, never through Connections.

Adapter-private interface:

```rust
trait GatewayCredentialStore {
    fn read(&self, slot: &GatewayCredentialSlot)
        -> Result<Option<StoredGatewayCredential>, CredentialStoreError>;
    fn commit(&self, expected: CredentialExpectation,
              enrollment: VerifiedEnrollment, credential: SecretGatewayCredential)
        -> Result<CommittedGatewayCredential, CredentialStoreError>;
    fn remove(&self, slot: &GatewayCredentialSlot,
              expected: GatewayCredentialRevision)
        -> Result<(), CredentialStoreError>;
}
```

`GatewayCredentialSlot` is an exact host-owned slot; credential payload includes the Gateway address, bearer, client/Person/device identity and pinned producer/enrollment identity. `SecretGatewayCredential` and `StoredGatewayCredential` have no product serialization and redacted diagnostic rendering; raw fields are private. `commit` performs exact replacement, secure write, exact readback and identity comparison before returning success. Unavailable/locked, malformed, identity mismatch, conflict and write/readback failure are distinct errors; none becomes “no configured Gateway.” No new key or database is created on read/open error.

The Connections-owned `GatewayCredentialCommit::commit(staged: StagedCredentialRef, expected: &VerifiedEnrollment) -> Result<GatewaySummary, PairingError>` accepts only an opaque staged credential reference plus verified enrollment metadata; its adapter redeems the secret internally. `readback(operation_id) -> Result<Option<GatewaySummary>, PairingError>` rejoins exact acknowledged commits after response loss. Keep the existing exact `app.floe.local-server` / `connection-v1` secure-storage namespace unless a separately reviewed namespace change is necessary; removing the Flutter bridge does not authorize broad deletion or changing unrelated credential slots. Domain services never receive bearer bytes. A staged reference binds one operation, exact host principal, producer/issuer bundle and runtime/storage generation; it cannot be redirected or replayed for another pairing.

Remote approval, issuer activation and local credential persistence are not one cross-process transaction. Model the recoverable sequence explicitly: `AwaitingApproval -> Verifying -> Committing -> Paired` or typed `RepairRequired`. Go atomically commits issuer activation and client credential issuance; Rust verifies that exact result and commits/readbacks locally before publishing `Paired`. Lost acknowledgement rejoins the same pairing ID/operation. A partially committed local state is unusable, and recovery reconciles the exact original bundle rather than silently accepting another key.

### 3.2 Connections: `GatewayPairingPort`

**Owner placement:** `crates/modules/connections/src/ports/gateway_pairing.rs`; implementation `crates/adapters/providers/src/gateway/pairing.rs`.

```rust
trait GatewayPairingPort {
    async fn start(&self, request: PairingStartRequest, scope: &OperationScope)
        -> Result<PairingChallenge, PairingError>;
    async fn confirm(&self, request: PairingConfirmation, scope: &OperationScope)
        -> Result<PairingObservation, PairingError>;
    async fn observe(&self, pairing: &PairingHandle, scope: &OperationScope)
        -> Result<PairingObservation, PairingError>;
    async fn cancel(&self, pairing: &PairingHandle, scope: &OperationScope)
        -> Result<PairingObservation, PairingError>;
}
```

Inputs bind verified host Person/device, stable operation ID, owner public-key reference, exact validated setup target and attempt. Adapter-private state retains poll proof and raw wire challenge; Connections/Access key owner verifies the signed producer/issuer bundle and signs only the approved exact challenge. No private signing key is exported. Observation includes status, verified identity metadata and an opaque staged credential reference only when remotely approved.

Accepted deployment scope (2026-10-02 13:46 UTC): retain the existing loopback-only deployment and explicit bounded setup rather than adding network discovery or remote TLS in this structural refactor. `connections.gateway.prepare_setup { address_text } -> GatewaySetup { target_ref, display_address, expires_at }` normalizes `localhost` to `127.0.0.1`, validates one numeric nonzero loopback port and rejects path/userinfo/query/fragment/other hosts. `target_ref` binds the verified caller/host generation and expires; it carries no credential and is not trust approval. `connections.pairing.start` consumes that reference, never an arbitrary later endpoint.

Product commands: `connections.pairing.start`, `confirm`, `cancel`; queries: `connections.pairing.get`, `connections.gateway.get`. Start returns an operation ID/display code/expiry. Exact domain/product state mapping is canonical §3.3: Pending→starting, AwaitingLocalConfirmation→awaiting_local_confirmation, AwaitingApproval→awaiting_gateway_approval, Verifying→verifying, Committing→committing, Paired→connected, and Rejected/Expired/Cancelled/RepairRequired→snake_case, with owner allowed actions. Ordinary observation does not finalize by Flutter side effect. Caller identity and proof are absent from product DTOs. Explicit cancellation is separate from observer disposal; disposing a screen or timing out a query never cancels durable work.

Pairing includes ADR0028 authority enrollment. Delete routine `RemoteAccess` enrollment/inspection product paths; advanced safe identity details may be part of Gateway summary. Pairing creates zero source grants.

### 3.3 Connections: `RemoteIntegrationPort`

**Owner placement:** `crates/modules/connections/src/ports/remote_integration.rs`; implementation `crates/adapters/providers/src/gateway/integrations.rs`.

The exact `RemoteIntegrationPort: Send + Sync` boxed-future signature is canonical-contracts §3.2: `list`, `begin`, `observe`, `cancel(CancelIntegration)`, `configure`, `disconnect`, and `management_launch(ManagementLaunchRequest) -> ValidatedManagementLaunch`, all with the owner OperationScope and typed IntegrationError. No alternative async-trait declaration or omitted cancellation branch is normative here.

`IntegrationSnapshot` is normalized, typed, non-secret: stable connector/connection/execution-owner IDs, safe account label, lifecycle, catalog capabilities, provider identity continuity evidence, source revision/incarnation/epoch, typed resource inventory and configured selection. Unknown provider-specific fields never enter the domain as `Map<String,Object>` or `serde_json::Value`. Product selection uses opaque catalog-issued resource IDs and an expected revision; Rust resolves/revalidates them against current metadata. Distinguish local configuration CAS from remote revision/epoch.

`BeginIntegration` chooses a supported connector and trusted operation ID; result may carry a bounded validated browser-launch action and display code. Provider secrets are entered only at the Go-owned authorized setup surface, never Flutter. `observe` returns status/metadata, never access/refresh tokens. Setup observation is read-only and does not silently grant Observe. Explicit completion flows through Connections source admission and Access review.

`configure` uses remote expected revision plus local expected revision when reconciling source metadata; source drift yields conflict, not auto-acceptance. `disconnect` revokes every non-revoked Access grant for the exact source, including Paused grants, before durably removing source/remote credentials under exact operation identity. Failure remains an observable resumable operation; it cannot restore permissions or erase uncertain Actions records.

Gateway-forget is an explicit local binding removal with exact expected credential generation; it also invalidates local source use and reports any pending remote revocation separately. It cannot claim the remote credential was revoked after a local-only delete.

Errors: `Unavailable`, `NotPaired`, `RepairRequired`, `IdentityMismatch`, `PermissionDenied`, `Conflict`, `UnsupportedConnector`, `InvalidSelection`, `ExpiredOperation`, `Cancelled`, `DeadlineExceeded`, `CredentialUnavailable`. Preserve absence versus error. Raw provider response bodies are redacted.

### 3.4 Inference and source-review contracts

Inference plans from `Purpose`, consumer, admitted input classes/coverage and genuine generic execution constraint. Role-specific defaults do not select provider/profile.

```text
observe_primary -> Result<PrimaryObservation<Prepared>, ModelObservationError>
PrimaryObservation = Available(PreparedModelProfile<Prepared>) | Absent(PrimaryAbsence)
PrimaryAbsence = NoGatewayConfigured | PurposeNotConfigured | PurposeDisabled
```

The exact boxed-future signatures, closed error enum, strict schema-2 HTTP inventory/request/response/usage/error contract and Rust↔Go decoder mapping are canonical-contracts §1–2. No conditional PurposeUnavailable or Failed enum variant remains; every observation failure is Result::Err.

A valid missing primary permits one local capability plan. Every credential/identity/inventory/transport error is `Err(ModelObservationError)`; a chosen Primary failure never triggers fallback. Availability and execution use this same selector. Remove `RootModelProvider` naming and the independent router; adapters prepare transport, Inference decides the plan.

The Agent contract defines `ModelPort::prepare(ModelPlanRequest, &ExecutionScope) -> Result<Box<dyn PreparedModelCall>, AgentFailure>`. Inference implements it; the role-neutral runtime never imports Inference. `PreparedModelCall::plan()` exposes only immutable `PreparedModelPlan` (operation ID, admitted principal/device, purpose/consumer/capabilities, Device/Gateway boundary, opaque non-secret authority-binding digest). The returned object privately owns the prepared transport. `PreparedModelCall::generate(ModelRequest, &ExecutionScope) -> Result<ModelResponse, AgentFailure>` consumes projection bound to that exact plan and never reselects. No process-global plan-ID stash, duplicate planning DTO or serialized credentials are introduced.

Engine prepares before projection, creates a projection operation, obtains exact plan-bound authorized evidence, commits ModelIntent, then generates. A live Gateway binding drift fails, never reroutes. Bounded correction attempts reuse the same selected plan with new attempt identity/live fences. Crash/reopen keeps existing interrupted/unknown-charge semantics; it does not reissue an already handed-off or unsettled model call. A fresh linked review resume plans new work and reprojects live evidence. The exact Rust signatures and owning files are in the Rust appendix.

The prepared-call result no longer returns a model-recipient review. Remove `ModelCallOutcome::NeedsUserAction(ProcessingRequirement)` and consent-only recipient lineage. Context history projection distinguishes revoked/unreadable evidence (exclude it) from still-readable evidence requiring expanded Gateway processing (typed source review), rather than flattening AccessReviewRequired and PolicyDenied into one retain/exclude boolean. Source policy mismatch is returned by Context/Access projection as `ModelProjectionOutcome::NeedsSourceReview(SourceProjectionReview { projection_operation_id, target_digest, blockers: SourceAccessBlockers })` before allocating a model attempt. Engine preserves already-settled work and returns a source-review outcome; Conversation creates durable owner-bound interaction, no fake attempt/usage or generated answer.

A pre-model blockage needs a durable projection/review origin (Run + projection operation ID + exact target digest), not a forged model attempt. Tool/Task blockers retain real admitted journal origins. Review binds expected source/grant including absence, consumer/purpose/categories, physical-resource evidence and compare-only policy digest. Access alone widens source processing after exact review. Actions approval is never inferred.

Health is a separate source-local operation before any reasoning: bounded acquisition/minimization -> mandatory Apple local typed semantic transform -> validated, still-HighlySensitive Wellbeing View with host-owned evidence. No Agent runtime, Persona, tools, remote sanitizer or deterministic semantic fallback. Fail closed on unavailable/invalid/expired transform. This precondition lands before enabling Gateway-first callers.


### 3.5 Product review and owner-operation DTOs

Canonical service names used by App forwarding are `ConversationService`, `ConnectionsService`, `AccessService`, `ContextService`, `DayService`, `ExpertsService`, `KnowledgeService` and `ActionsService`. Their full methods and baseline callers are fixed in the App/domain appendices. Connections may orchestrate a connection-level review intent but Access alone stores/revalidates the policy descriptor and mutates grants.

A product review returns `ReviewRef { id, revision, digest }` plus bounded display members, scope/processing disclosure, expiry and safe allowed actions. The authoritative descriptor is persisted by its owner and includes expected source/grant state (including absence), private native/provider identity, physical resources, consumer/purpose/categories and policy digest. AppWire echoes only the review reference and decision, never raw authority fields. Owner resolution reloads the descriptor and current facts; changing selection or facts requires a new review, not substitution under a previous ID.

Mutations carry stable `CommandId`, expected aggregate revision where applicable and product payload. The admitted host supplies principal/device/runtime epoch. Owner service returns its typed, state-tagged operation snapshot with operation reference, revision, valid state payload and owner-produced allowed actions as fixed in canonical-contracts §3.3; allowed actions are a projection, never permission at dispatch. Query/observation can rejoin the exact operation; transport release only releases observation state and cannot undo/cancel durable owner work.

- Connections source configure takes a current `ReviewRef` and opaque resource choices; it does not accept provider DTOs or imported Calendar events.
- Connections `prepareObserveReview(commandId,sourceRef,expectedRevision,requestedProcessing)` persists the Access-owned review; `inspectObserveReview(reviewRef)` is pure. `setObserve` encodes tagged enable with exact review or pause without a review, through canonical §3.2. Preparing source/integration/binding reviews is likewise an explicit replay-safe command; no query allocates durable meaning.
- Actions `submit(ActionIntent)`, `decide(action_ref, review_ref, decision)`, `reconcile(action_ref)` each own the complete approved lifecycle. No client `approve -> execute -> collect` orchestration. A timeout after native/provider dispatch remains uncertain until lookup establishes outcome.
- Experts `setInstallationEnabled(installation_ref, enabled, expected_revision)` atomically applies installation/assignment configuration. `prepareBindingReview(commandId,assignmentRef,requirementRef,expectedRevision)` persists an Experts-owned descriptor; pure `inspectBindingReview` returns it. `replaceBinding(commandId,reviewRef,selectedCandidateRefs,expectedRevision)` resolves the exact package/definition/requirement from that descriptor, validates the fresh catalog/CAS and changes configuration only. `CandidateCatalog` is Experts-owned and supplied through a Context-backed adapter; Experts does not import Context/provider implementation.
- Day `refreshDay(DayQuery)` owns mirror CAS, per-source failure and result projection through a Day-defined `CalendarAcquisitionPort` in `crates/modules/day/src/ports/calendar_acquisition.rs`. Context implements that port using current source admission/acquisition; App injects it. Day never imports Context (Context already depends on Day). Display-date intent does not supply authoritative source IDs or mirror batches.
- Knowledge review DTOs/interfaces belong to Knowledge application/domain, never a presentation widget imported inward.

First-party policy construction takes Access-owned `TrustedConsumerCatalog::registrations()` returning pure `TrustedConsumerRegistration { package_identity, manifest_revision, declared_view_capabilities, consumer_identity }` as validated immutable configuration. App supplies trusted shipped registrations; Access derives the exact policy/digest. Registry enabled/binding state and untrusted installations cannot add consumers. Source editors and Observe controls stay separate under ADR0031.

### 3.6 Health prerequisite, fixed contract

Source contract: `apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift`. Input has only optional `sleep_hours`, `steps`, `exercise_minutes`; reject all-absent, nonfinite/negative, sleep >36h, steps >1,000,000 or exercise >2160min. Do not clamp. Output has only closed capacity (`reduced|typical|strong|unknown`) and recovery (`needs_recovery|typical|recovered|unknown`) enums. No free text, identity, timestamps, confidence, provenance, tools or authority.

`HealthPrivacyTransforming::transform(input)` uses fixed Health-owned instructions, a 10-second deadline and 64 response tokens. A separate `HealthPrivacyTransformHost` owns independent availability/start/poll/cancel/release state and strict `floe_health_privacy_transform`/`free` ABI; it shares physical Foundation capability but never the Agent job slot, prompt grammar or tool catalog. Native availability must query actual model readiness, not merely dylib symbol presence. Failure codes distinguish unsupported/disabled/not-ready/model-unavailable/invalid-input/invalid-output/deadline/cancelled without raw native error text.

The mandatory product constructor injects this transformer. Host projection constructs timestamps, 30-minute expiry, confidence and opaque evidence handles only after valid typed success; no aggregate numbers enter Wellbeing output. Both categories unknown yields zero confidence/no evidence handles. Failure never refreshes a cached View. All direct `LocalModel.swift` compile sites, including macOS/iOS build scripts, Xcode input paths and retained CLI/smoke packaging, compile the one Health contract source. Dormant Android Health cannot enter reasoning without required evidence; no Android transformer/build work is added.

## 4. Go packages, ports and DTOs

### 4.1 Package graph

```text
cmd/floe-server -> node (assembly, lifecycle, shutdown)
transport/http -> trust, pairing, integrations, authority, inference
pairing -> trust
integrations -> authority-owned source ports, views
connector adapters -> integrations ports, views contracts, private provider auth
                     (never the reverse)
authority -> trust, views
inference -> owner-defined provider/credential ports
provider/auth/storage adapters -> owner contracts
```

`node` may import all constructors to wire them. No owner imports `node` or `application`. `views` has no concrete connector import; `trust` has no source-domain/provider import. Transport owns strict JSON/header/session/HTTP mapping, not authorization or source selection. External provider DTOs remain inside their adapters. Go package-level imports enforce most of this; supplement with a structured dependency check after the architecture stabilizes, not one-off source-shape regexes.

### 4.2 `trust`: authentication and enrollment

Move producer identity, issuer registry/tombstones, authenticated client principal, administrator-session authority and pairing settlement from `authorization`/`application` into `internal/trust`. HTTP retains cookie/header encoding, Host/Origin/CSRF transport checks, not a second session authority. Exact-byte proof primitives live in `trust/proof.go`; authority and pairing own their separate challenge semantic fields/encoders. Trust never imports authority or views. Define immutable `Principal` constructed only by successful authentication, `ProducerIdentity`, `IssuerBinding`, `PairingBundle` and exact `PairingCommit`.

Required owner methods:

- `AuthenticateBearer(ctx, bearer) (Principal, error)`: credential verification, current client/Person/device/revocation; transport-only bearer input never forwarded into source DTOs.
- `ActivatePairing(ctx, activation: PairingActivation) (PairingCommit, error)`: one durable transaction for client credential identity and issuer activation; reject revoked-key resurrection and bundle drift.
- `VerifyIssuer(principal, proof, challenge) (VerifiedIssuer, error)`: cryptographic proof bound to operation/audience/issuer/current principal. `WithActiveIssuer(principal, keyID, consume)` fences every runtime claim/release against current trust state; authority keeps no second mutable issuer map.
- `RevokeClient` and `RevokeIssuer`: durable exact revocation/tombstone semantics; return only after commit.
- producer metadata/signing behind typed methods, private keys owned by secure storage, no public raw-state maps.

Split runtime admission challenges from enrollment challenges even when they share cryptographic primitives. Trust establishes who/which issuer; it does not decide source permission, processing policy or provider capability.

### 4.3 `views`: normalized read contract

`internal/views` owns provider-neutral query/result value contracts and the read port:

```go
type Reader interface {
    Read(context.Context, ReadRequest) (Result, error)
}
```

`ReadRequest` contains one immutable `SourceSnapshot`, a closed typed query (`CalendarQuery`, `MailQuery`, `WorkQuery`, `LogisticsQuery`) and `Bounds{MaxItems, MaxBytes, Deadline}`. Queries express domain limits, not arbitrary HTTP path/body. `SourceReference` binds connector/connection/execution owner and incarnation/epoch. `SourceSnapshot` adds Person/device, provider identity, revision and exact physical resources. Neither contains token, refresh credential, grant authorization, route or secret-store object.

`Result` is a closed normalized payload union plus observed/expiry times, exact coverage, item/byte count, cursor and source continuity metadata. Calendar results preserve per-source failures and recurrence/interval identity; unavailable is distinct from successful empty. Communication/Work/Logistics keep their current bounded domain shapes after moving out of `connectors/common`. Provider-native wire structs remain private to each connector.

Reader errors are typed (`Unavailable`, `PermissionDenied`, `IdentityChanged`, `RateLimited`, `InvalidQuery`, `BoundExceeded`, `Cancelled`, `DeadlineExceeded`) and never carry provider response bodies or secrets. Reader success is evidence awaiting authority release, not an authorization decision.

### 4.4 `authority`: runtime source enforcement

`internal/authority` receives authenticated principal and owner-produced source facts. It owns runtime signed preview, admission, bounded-read state, staging and release; it never chooses a concrete connector or switches over View IDs.

```go
type SourceFence interface {
    WithCurrentSource(trust.Principal, views.SourceSnapshot,
        func(views.SourceSnapshot) error) error
}
type SourceResolver interface {
    ResolveSource(context.Context, trust.Principal, views.SourceTarget)
        (ResolvedSource, error)
    PreflightSource(context.Context, trust.Principal, views.SourceSnapshot) error
}
```

`views.SourceTarget` is exact connector/connection/logical View with requested revision, not a path. `ResolvedSource { Snapshot views.SourceSnapshot, Reader views.Reader, Limits views.Bounds }` binds the immutable normalized source snapshot, its canonical logical resource/exact physical resources and registered Reader. The final fence compares the complete snapshot including revision, provider identity and credential generation, not only source incarnation/epoch. `integrations` implements these authority-owned ports, so `authority` imports only `trust` and `views`, never `integrations` or connectors.

Public methods are typed `Preview`, `Admit`, `Read`, `Release`, `CancelAdmission`, `CancelRelease`. Their DTOs preserve schema, challenge/operation identity, source and grant references, purpose/consumer, query digest, bounds, expiry and signed proof. Runtime proof cannot activate an issuer. Preview cannot grant access. Read returns a staged release challenge; only successful release exposes bounded payload.

Provider network identity preflight occurs outside owner locks. Every consume/release rechecks cached credential/provider identity plus current source/grant facts; continuity failure discards payload. Lock order is trust principal fence -> integration record fence -> provider identity fence -> authority consume. The authority mutex is never held while invoking outward callbacks or performing provider I/O. Duplicate/lost acknowledgements rejoin an exact immutable admission; altered requests with the same identity conflict.

### 4.5 `integrations`: remote account/source lifecycle

Rename Go `connections` to `integrations` to avoid conflating its remote provider account/source implementation with Rust's full product Connections domain. Move lifecycle/use-case decisions currently in `application` here. Define owner ports for setup/auth flow, source inventory/configuration, provider-identity preflight/fence and credential reference binding. Replace broad `Action(context,string)(any,error)`, `ConnectionSnapshot() any`, domain-reader mixtures and exported mutable maps with typed operations and immutable snapshots.

Concrete adapters implement the server appendix’s typed `Setup`, `IdentityVerifier`, `RuntimeFactory` and `Runtime` operations for supported source setup, status, resource listing, scope configuration and disconnect. Credential references stay internal; provider keys/access/refresh tokens live in Go secure storage. Authentication refresh must preserve exact Person/connection/provider identity and handle rotation/uncertain persistence fail-closed. Trust persists CleanupTicket before integration cleanup effects; integrations persists per-step source/cache/provider/credential cleanup before exact AcknowledgeCleanup. Do not delete a credential first and then infer cache cleanup succeeded. A late OAuth callback compares initiating client and binding generation before publication. No owner map lock spans logout/Keychain/network I/O.

Integrations selects an exact reader by registered connector + connection + View capability; unrelated connections are not scanned as candidates. It supplies signed current-source facts through authority ports. Adding a connector requires adapter registration in `node`, not editing authority or a domain switch in HTTP. Move `connectors/common` domain DTOs to `views`; keep provider transport helpers adapter-local rather than creating a new universal connector framework.

### 4.6 Inference and excluded Actions

Go inference takes product purpose and canonical model input, selects configured provider/model/effort internally, normalizes typed result/nullable observed usage and reports bounded failures. Canonical-contracts §2 fixes routes, schema version 2, fields/enums/status/errors, absence and caller decoder cutover; provider-native replay is removed from the paired wire while Engine validated-batch replay remains. Remove downstream-recipient approval flags from paired protocol in the same cutover as Rust callers. Keep strict request size/schema, prepared capability/account-generation fences and privacy-safe audit; no provider-native replay object crosses the paired boundary. Go OAuth source implementations move under `connectors/{googleauth,microsoftauth,workoauth}`; model OAuth moves under `inference/codex`. Credential storage remains `credentials`; shared atomic-file mechanics alone move to `storage/private_file.go`, with domain stores retaining schema/transaction/recovery decisions. Synthetic operator connection probes remain production behavior; a symbol named `TestTarget` or use of `httptest` is not sufficient reason to delete it.

Do not create `server/internal/actions` or add provider writes. Current Go connectors remain read-only. Any future write executor must accept a Rust Actions-authorized exact operation with stable idempotency/intent identity, enforce provider preconditions, return receipt or uncertain outcome, and expose lookup/reconciliation without blind redispatch. That future capability needs its own authorization/design; this refactor does not implement it.

## 5. File/symbol disposition summary

Exact line-level mappings and all callers belong in the domain appendices. This table fixes the cross-domain destination, preventing file moves that merely rename an incorrect owner. `KEEP` preserves a responsibility, not a guarantee that every existing assertion or implementation is correct.

| Action | Current files/symbol families | Target owner/files and caller cutover |
|---|---|---|
| KEEP | `crates/contracts/{kernel,context,agent}`, `crates/runtime/{execution,agent}` | Pure values and role-neutral runtime; narrow recipient-only contracts and parallel Expert execution vocabulary |
| SPLIT | `crates/app/src/services.rs`, `turn_request.rs`, `session_services.rs`, `composition.rs`, `worker.rs` | Conversation commands/query/error/lifecycle into Conversation; host admission/wiring stays App; update FFI/CLI/Dart together |
| MOVE | `crates/app/src/vault_host/conversation_turn.rs` and `conversation_turn/{engine_ports,interaction_publication}.rs` | Conversation application/runtime composition ports; no App Run policy |
| SPLIT | `crates/app/src/vault_host/conversation_turn/{expert_dispatch,expert_host}.rs`, `expert_services.rs`, `vault_host/expert_setup.rs`, `expert_binding_settings.rs` | Experts Task/registration/host services plus Context source-reader and Inference ports; App injects shipped registrations only |
| MOVE/REWRITE | `crates/app/src/first_party_observe.rs`, `connection_observe.rs`, `vault_host/{calendar_access,personal_access,personal_grants,remote_observe,review_snapshot}.rs` | Access-owned trusted-reader policy/review; Connections current source lifecycle; typed immutable owner outcomes |
| MOVE/REWRITE | `crates/app/src/connection_services.rs`, `calendar_facade.rs`, `local_context.rs`, `personal_source_spec.rs` | Connections source setup/reconciliation, Context acquisition, Day mirror semantics at their owners |
| MOVE | `crates/app/src/vault_host/{interaction_owners,interaction_resolution}.rs` | Conversation interaction orchestration using owner ports; Access/Connections/Experts still decide their mutations |
| SPLIT | `crates/app/src/{action_facade,action_services}.rs`, `vault_host/product_actions.rs` | Actions service/repository authority and result mapping; App constructs one authoritative repository |
| MOVE | `crates/app/src/{knowledge_services.rs,vault_host/learner_worker.rs}` | Knowledge learner job/review lifecycle with common Inference |
| REWRITE | `crates/modules/connections/src/ports/remote_control.rs`, `application/{pairing,remote_pairing}.rs` | Full `GatewayPairingPort`, owner operation/recovery and secret-free product snapshots |
| SPLIT/REWRITE | `crates/adapters/providers/src/control/server_connection.rs` | Private Gateway credential persistence plus prepared capability adapters; remove raw `SavedServerConnection` from domain API |
| SPLIT/REWRITE | provider `control/authorization.rs`, `control/recipient_authority.rs`, `models/{root,server,wire}.rs`, `sources/server.rs` | Gateway-private trust/control/inference/View adapters; delete recipient-consent-only adapter logic |
| REWRITE/DELETE | Inference `application/{service,router,route_selection,saved_connection}.rs` and `api.rs` | One purpose-based selector, explicit Primary absence/error; delete parallel router and concrete product-profile intent |
| REWRITE | `crates/bindings/protocol/src/dto/{commands,queries,connections,conversation,interactions}.rs`; FFI `app_wire.rs`, `remote_wire.rs`, ABI/header | One owner-prefixed AppWire path; remove separate remote Access/pairing ABI only with Dart/native/CLI callers |
| DELETE | `apps/client/lib/features/connections/application/local_server_client.dart`, `remote_access_gateway.dart` | Typed AppWire Connections gateway; no direct HTTP/provider DTO/credential storage |
| REWRITE | `remote_pairing_gateway.dart`, `remote_owner_operation.dart`, `remote_owner_models.dart`, `app/runtime/app_runtime.dart`, `native_transport.dart` | Ordinary owner gateway and safe owner operation snapshots; remove separate remote dispatch/correlation surface |
| REWRITE | `local_server_panel.dart`, `server_connector_panel.dart`, `connector_screen.dart`, `main.dart`, settings/Day constructors | UI intents and safe projections; remove `_synchronizeServerCalendar`, bearer/proof/secret state, automatic disposal cancellation |
| SPLIT/DELETE | `features/day/application/native_day_gateway.dart`, `calendar_observation_publisher.dart`, `calendar_observation_refresh.dart` | Rust Day/Connections/Context owns sync, freshness, fanout and reconciliation; retain only mechanical native acquisition bridge |
| DELETE | `floe/local-server` credential handlers in `macos/Runner/MainFlutterWindow.swift`, `ios/Runner/AppDelegate.swift` | Rust native secure-store driver; keep independent browser-launch/platform functions if still used |
| SPLIT | `server/internal/authorization/{authorization,producer_identity,admissions,source_authority,source_service}.go` | `trust` identity/enrollment; `authority` runtime protocol; `views` query/payload; `integrations` reader resolution/source facts |
| MOVE/REWRITE | `server/internal/connections/*.go` | `integrations` lifecycle and owner-defined typed ports; update imports/HTTP DTO conversion and concrete registrations |
| SPLIT | `server/internal/application/{console,state,person,trust_store,producer_identity,pairing_commit,source_authority,remote_view_admission,calendar_admission}.go` | Trust/integrations/authority/storage owner implementations; no exported shared Console state |
| SPLIT | `application/{connectors,connectors_operations,connector_runtimes,management,provider_profile,config,http,bootstrap}.go` | Typed owner services/adapters/HTTP; `node` constructor/lifecycle only |
| MOVE/REWRITE | `server/internal/connectors/common/{contract,calendar,work,logistics}.go`; each connector `contract.go`, `service.go`, `client.go` | Normalize via `views`; provider-private wire/pager helpers stay adapters; dependency inversion |
| SPLIT | `server/internal/inference/{gateway,agent,provider,synthetic}.go`; `cmd/floe-server/main.go` | Transport versus inference/provider boundaries; one node assembly, remove `legacyGateway` entry after paired canonical path replaces callers |
| KEEP | `server/internal/{credentials,googleauth,microsoftauth,workoauth,codexauth,envfile}` responsibilities | Go-only provider secrets/OAuth/deployment; exact interfaces narrowed, no client token readback |
| KEEP/REWRITE | root manifests, dependency policy, native packaging, debug CLI, validation scripts, fixtures | Update only demonstrated callers/targets; production probes and reused fixtures survive test deletion |

## 6. Ordered execution, prerequisites and temporary incompleteness

### P0: review preparation only (current authorization)

1. Freeze tracked-file inventory and SHA/line/byte identity at baseline. Read full text and inspect binary assets using a meaningful file-kind method; no sampled file counts disguised as full reading.
2. For every file record KEEP or concrete split/move/rewrite/delete mapping; for affected executable symbols record current path/symbol/line -> target path/module/domain, callers, imports, manifests, wire and error changes.
3. Inventory test files/registration/helper structure and classify inspected mismatches as durable safety, obsolete design, disputed product quality or incidental implementation. Specify the complete behavior-extraction method for T0; do not claim a per-behavior ledger already exists. Full semantic reading now is not equivalent to having extracted every assertion/case into that future ledger.
4. Complete this plan and appendices with matching interface names and dependency order. Identify all real user-owned decisions; none is fabricated merely to postpone a technical choice.
5. Static audit only: file coverage, symbol anchors, caller closure, same-snapshot wire pairing, dependency DAG, planned-deletion reachability and stage prerequisites. No tests, build, compiler, analyzer, formatter or executable architecture checker now.
6. Present the plan for review. No test/source/manifests deletion, code implementation, credential changes, data reset, push or deployment.

Exit: complete full-read ledger; no unknown unassigned file; all affected symbols/callers have a stage and destination; contracts closed; discrepancy register resolved or explicitly user-owned; plan approved separately before execution.

### T0: document-then-remove old tests (future authorization required)

Prerequisite: P0 accepted and explicit authorization to document/remove tests. First extract the complete per-behavior natural-language ledger in §7 and independently reconcile every test/table/helper-derived case; only then delete. This is not a product refactor, not proof of passing behavior, and not a request to rebuild old tests in new wrappers. Remove only independently identified test-only code/support/targets. Keep production declarations embedded beside tests, shared fixtures with live consumers and real diagnostics/connection probes. Make test-only manifest/build-target removals narrowly and together. No tests/build/format at this preparation/removal stage unless the later user instruction changes that gate.

Exit: exact old suite deletion ledger and residual audit; baseline product code retained except removal of test-only blocks/registration; no orphaned test target; no broad filename deletion; safety behaviors still documented.

### S1: first complete architecture slice, Conversation through Gateway

**Prerequisite:** authorized implementation after T0. The expanded safety/identity prerequisites below are a proposed dependency closure for review, not approval to implement them now. Treat S1.1–S1.7 as one dependency-correct implementation unit; no repeated format/compile loops between tiny steps.

1. **S1.1 contracts and construction:** establish owner command/result/error types, Gateway credential/pairing ports, new inference capability/absence contract, source processing/review values, Go trust/authority/views/integration interfaces and necessary repository ports. Define destination modules/manifests and visibility first. No runtime switches yet. A temporarily uncompilable same-slice caller set is explicit and resolved in S1.7.
2. **S1.2 identity and pairing foundation:** implement Rust credential adapter/secure platform store, Go trust+pairing transaction, Rust pairing start/confirm/observe/commit recovery. Replace the Flutter HTTP/keychain proof/token path and remote pairing ABI together. This provides the one canonical private Gateway capability needed by inference. Remove separate RemoteAccess enrollment after source/grant consumers use Access-owned paths.
3. **S1.3 source safety prerequisite:** implement source DeviceOnly/GatewayAllowed policy, owner-derived review and Context/Access projection admission, retaining exact source/grant/physical-resource fences. Implement mandatory Health-local transform and provenance before enabling changed reasoning. Readiness must not silently accept old data as transformed/authorized. Update storage/DTO/native contracts in the same snapshot; old local profiles fail explicitly, never reset automatically.
4. **S1.4 model path:** cut Rust/Go inference inventory/request/result wire and role-neutral provider composition together; implement one Primary/Fallback selector. Migrate Manager, shipped Expert model callers and Learner selection call sites together so none retains hidden local-only/recipient policy. Remove product `ProfileSelection/profileId`, recipient flags and old selector. Source readers may still be structurally in their baseline files until S2, but must use the new authority contract now.
5. **S1.5 Conversation ownership:** move command validation, Run/session lifecycle, model projection orchestration, durable review/resume and scheduling from App to Conversation. Preserve exact validated-batch/environment takeover, journal ordering, usage and cancellation direction. Durable auto-resume is implemented before removing routine resolved-card Continue; blocked projection records no fabricated answer.
6. **S1.6 vertical caller cutover:** update App thin forwarding, FFI DTO conversion, C header/exports, Flutter Conversation/Connections intent gateways, CLI and native host wiring. All same-snapshot callers consume the canonical contract. Migrate exact owner operation/recovery semantics, not a generic worker bus facade.
7. **S1.7 delete and close:** remove obsolete Conversation/model consent/pairing/credential routes, imports, fields, commands, ABI and private owner branches. Run concept-level residual audit and update current architecture/affected ADR rationale together. Format once and compile the complete changed slice (§8 G1); defer full artifact/application builds to G2. Do not add new tests yet.

**S1 exit:** a real Flutter/CLI intent has one admitted Conversation->Runtime->Inference->Gateway->Go path; pairing is Rust-owned and secret-free at product boundary; source processing/Health prerequisites enforce correct denial/review; one selector and no recipient consent; durable source review resumes exactly once; all obsolete in-scope paths gone; G1 recorded. This is the first architecture milestone, not completion of the remaining owner cleanup or behavioral qualification.

**Bounded remaining S1 state:** Go source Reader implementations/normalization and non-Conversation App semantic jobs may still reside in their old physical packages where S1 did not own them. Trust-coupled integration lifecycle and cleanup are already moved in S1.2. Their final ports are already established. S2 removes these exact residual owners; no duplicate adapter/runtime path is introduced to preserve them. Existing shipped Expert package loop structure may remain until S2 common-runtime migration, but inference policy is already shared.

### S2: remaining structural refactor, one coherent closure

Prerequisite: S1 residual and G1 gates complete. Execute S2.1–S2.6 in dependency order as one remaining-architecture phase; format/compile at its completed boundary, not after every file move.

1. **S2.1 Go source owners:** finish integration runtime/source lifecycle (trust-coupled state/cleanup already moved in S1.2), normalized DTOs/query validation to views, source resolution to integrations and runtime enforcement to authority. Implement concrete adapters against inward ports, then migrate HTTP handlers and exact source clients. Delete `authorization`/`connections` superseded packages only after every caller moves. Preserve signed challenge/producer/issuer protocol meaning unless deliberately coordinated.
2. **S2.2 Connections/Access/Context/Day:** move source setup, inventory reconcile, permission policy/digests, review, acquisition/freshness/fanout/mirror orchestration out of Flutter/App. Direct product Day refresh and Expert request-scoped reads remain separate intents using common owner source facts. Remove Flutter calendar sync and App policy facades after callers migrate; retain mechanical OS brokers only where platform callback ownership requires them.
3. **S2.3 Experts common runtime:** move registration/admission/binding/Directory/Task/settlement/host composition to Experts. Every shipped package follows Package -> Installation -> Directory -> Task -> role-neutral Engine using package-owned role/tools/output. Remove parallel `ExpertModel/ExpertReasoner/ExpertStep` vocabulary and package-name dispatch branches. Same invocation/report path for builtins and explicitly supplied packages; no marketplace or remote A2A work.
4. **S2.4 Knowledge and Actions:** move Learner job and memory review/use policy to Knowledge; Actions owns service and authoritative repository selection, exact approval/precondition/idempotency/uncertainty. Wire Context/Access/Connections through owner contracts. No Go Actions writes. Preserve new-system uncertain external-operation records during ordinary lifecycle/schema handling. The accepted clean-profile cutover may discard legacy Floe development data after a separately confirmed reset and narrow external-effect hazard check, without migration or automatic replay.
5. **S2.5 assembly and transport:** shrink `floe-app` and Go `node` to verified admission/constructor/lifetime/wiring. Move Go disk state adapters out of composition into owner repository implementations. Remove cross-domain WorkerAction/optional result matrix once owner services have exact typed operations; a scheduling utility may remain in runtime/execution only for demonstrated role-neutral mechanics.
6. **S2.6 closure:** migrate all remaining CLI/native/product/operator callers; delete obsolete paths and test-only seams left by T0; narrow public exports/dependencies; update same-snapshot packaging, reset-slot documentation and developer docs. Static audit every baseline file/mapping against final disposition. Format once and run full compile/build gate G2 before any new tests.

**S2 exit:** no semantic policy in App/Flutter/node, no forbidden dependencies, one owner/path per concept, no obsolete production route or placeholder port, all current architecture documents match final source, no unresolved structural mapping, all required production targets compile/build (or concrete platform blocker is recorded, not passed).

### S3: new behavioral proof after structural closure

Prerequisite: S2 complete. Derive new owner API and real-boundary tests from accepted requirements and the reviewed safety ledger, not copied source shapes. Do not restore obsolete model-consent/local-first/Flutter-credential tests. Rebuild focused tests for identity/authority, source review, Health privacy, cancellation, CAS/recovery, exactly-once linked resume, Task pinning, approved effects/uncertainty and wire strictness. Add end-to-end smoke and capability grounding evaluation only on the final topology. Run G3 final test/build qualification. Report unavailable real Apple/provider/device gates explicitly.

## 7. Safe future test removal

### 7.1 Inventory and behavior ledger before deletion

Freeze each test artifact's baseline path, hash and exact test symbols/line spans. For each behavior capture setup/input, assertion/outcome, failure/race/crash scenario, current owner and intended target disposition. Count tests only with the language's actual registration mechanism; raw `test` substrings are not coverage. Classify:

- durable safety/property to re-prove after S2;
- product hypothesis requiring reassessment because current product quality is untrusted;
- obsolete representation/compatibility test to retire;
- test harness/support only;
- production diagnostic/fixture/tool despite a test-like name.

Tests may be removed after their behavior is documented even when the old implementation is wrong. Removal never asserts the behavior passed. New tests wait until full structural closure.

### 7.2 Language-specific removal method

- **Rust:** independently classify `tests/` integration targets, named `[[test]]`, unit `#[cfg(test)]` modules, external `#[path]` test modules, test-only helpers/features/dev-dependencies and doctest examples. Remove complete syntactic test items, not regex ranges that can delete adjacent production code. Preserve useful production docs/examples; classify executable doctests separately. Reconcile harness imports and explicit target entries. A fixture-producing example may have cross-language consumers.
- **Dart/Flutter:** classify `test/`, `integration_test/`, package/native tests and catalog tests separately from `lib/preview`, product fake implementations, generated localization and design-system demonstration entry points. Remove only test-only imports/targets/dependencies with no production/tooling use. Preserve native acquisition code and UI assets.
- **Go:** remove identified `*_test.go` and exclusively test support only after behavior capture. Preserve `internal/inference/synthetic.go`, `internal/application/test_target.go` and production `httptest` usage if part of actual connection probes. Do not delete connector `testdata` fixtures while Rust/Dart/tool consumers remain.
- **Swift/Kotlin/C/Python/shell:** classify XCTest/Swift package test targets, native assertion scripts and tool unit tests explicitly. Preserve diagnostic hosts such as `tools/validation/calendar/core-check.c`, production packaging/build helpers, signed smoke host metadata and launch/run scripts. A shell file running a test is not necessarily exclusively test infrastructure.
- **Shared fixtures/assets:** build a consumer graph across Rust `include_str!/include_bytes!`, Dart asset/file reads, Go embed/testdata, Swift package resources, Python/shell paths and docs. Keep contract corpus/rubric and live fixtures needed by product tools until consumers move. Delete only orphaned test-only artifacts, documenting why.

### 7.3 Deletion gates

1. User authorizes removal after review; immutable behavior inventory is present.
2. Every candidate path/item is classified; mixed production/test files use exact AST/item spans, manually inspected after edit.
3. Shared-consumer graph has no live production consumer of a proposed removed fixture/helper.
4. Manifest/build/package target references are removed in the same deletion change; no dependency removed merely because its name is test-like.
5. Static residual scan covers test registration, imports, include/resource paths, scripts and instructions. Remaining matches are classified, not mass-deleted.
6. Inspect complete diff and `git diff --check` only; no compiler/test/formatter loop at T0. Record exactly what was deleted and not run.
7. Never delete source safety enforcement, signed fixture/security protocol data with live consumers, private profiles/credentials, uncertain operation records or unrelated files.

## 8. Verification policy and final evidence

The repository's normal fast-test policy is intentionally overridden for this task's phase order. Current P0 is documentation/static inspection only. T0 removes tests only after separate approval. Future formatting/compilation happens at completed implementation-slice boundaries; new tests are written only after S2 structural completion. Avoid many small-step formatting/compilation loops.

### G1: completed Conversation/Gateway slice, formatting and compilation only

At the completed S1.1–S1.7 boundary, format the changed languages once and compile the changed production owners and complete same-snapshot callers. No tests, test harnesses, release/package/application builds, full workspace build, clean/cache churn or executable architecture checker runs here. Commands below are planned, not executed now:

- Rust: `cargo check --workspace --lib --bins`; include explicitly affected retained production examples through `cargo check -p <owning-package> --example <exact-retained-target>` from the root/tools caller map. Do not run `cargo build -p floe-ffi` at G1.
- Go: from `server`, `go build ./internal/...` is the package-compilation-only gate (no `cmd` executable linking/output). Defer `go build ./...` and executable/package distribution to G2.
- Dart/Flutter and Apple: run only the changed production source compiler/type-check phase with the owning SDK and the target's existing defines/import/SDK inputs. Do not invoke `flutter build`, Xcode application/archive, codesigning or bundle packaging. Where the available platform tooling cannot run this compiler-only phase independently, record that G1 platform compilation is blocked until the appropriate authorized environment is available; do not substitute a full build or claim a pass. Exact environment-specific command must be recorded with its compiler invocation before running, not inferred from a Linux host.
- Inspect static dependency/visibility, same-snapshot C ABI/export/binding, secret/obsolete-route residuals and the complete diff. The executable architecture checker remains G2.

G1 proves completed-slice compilation closure only, not runtime behavior or final release/build qualification. Necessary platform compilation must be completed or explicitly blocked; Linux success cannot stand in for Apple compilation. The full final production compilation/build gate is mandatory at S2/G2 before S3 tests.

### G2: all structural work complete

Format changed Rust/Go/Dart/Swift surfaces once at closure. Compile/build the whole Rust workspace production targets, Go executable/packages, Flutter client and affected Apple native packages/FFI/debug host. Run `python3 tools/architecture/check_boundaries.py` only now after policy/manifests reflect the final graph; extend dependency enforcement only for stable owner rules, not deleted-symbol regex fixtures. Run `git diff --check`. Inspect bundle/library/ABI source-snapshot consistency. Required final compilation/build cannot be waived because old tests were removed.

### G3: final qualification after new tests

Use the final target-based suite, then the repository full gates: `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast`, `go test -race ./...`, `go vet ./...`, `flutter analyze`, `flutter test`, affected Apple/native tests and final Apple/FFI builds. Restore test manifests only for the new suite. Keep one consistent build target/cache/configuration; no automatic clean or flags churn. Final build results are required even when tests pass; reuse covered successful builds only if their inputs/configuration are unchanged and recorded.

Required behavioral evidence includes:

- wrong Person/device/producer/issuer/account, revoked/tombstoned identity and stale credential rejection;
- pairing approval lost acknowledgement and secure-write/readback failure never showing usable Paired;
- zero source grants from pairing; no model/provider review; explicit source-processing review without fallback bypass;
- Primary absence versus failure and identical availability/execution selector;
- Health transform invalid/missing/expired/cancelled behavior, no raw/pre-transform payload export, HighlySensitive retained;
- stale source/grant/native/provider state at admission/handoff/post-I/O/release; successful empty distinct from unavailable/partial;
- immutable Run Expert environment and exact Task replay/settlement under binding changes;
- source review decision crash/reopen/race and exactly one fresh linked child, no duplicate user text;
- cancellation direction and observer timeout/disposal detachment;
- Action intent before effect, exact approval/idempotency/preconditions, uncertain result lookup-only recovery;
- Memory candidate/review/CAS/tombstone/provenance and Learner no direct mutation;
- no secret leakage through wire, error/debug/trace, clipboard, process arguments or UI state;
- fixed Manager grounding corpus on Gateway Primary with strict rubric, separately reported local Fallback safety floor. Existing Foundation failure is historical, not Gateway evidence. No rubric weakening or prompt-case patching to pass.

## 9. Residual audit and documentation convergence

Search by concept and exact symbol, then classify every residual. A deleted-symbol grep is one-time execution evidence, not permanent architecture tooling.

- Flutter: `LocalServerClient`, `ServerCredentialStore`, `KeychainServerCredentialStore`, `ServerConnection.token`, `RemoteAccessGateway`, direct `HttpClient`, `floe/local-server`, `profileId`, `processing_recipient`, `_synchronizeServerCalendar`, calendar freshness timers/fanout/import orchestration.
- App/FFI: `RemoteAccessCommand`, `RemotePairingCommand`, remote ABI exports/header/bindings, raw `PairingTarget.base_url`, `ProfileSelection`, `WorkerAction` cross-domain dispatch, optional `WorkerResult`, App grant/digest/consumer/source/Action decisions.
- Rust inference/authority: `ProcessingRequirement`, `ProcessingRecipient`, recipient-only `RecipientLineage`, `allow_external`, `expected_recipient`, `RootModelProvider`, separate router, local-first ranking/transport fallback and role-specific hidden selectors. Do not remove unrelated recipient concepts or independently versioned provider/security fields.
- Experts: alternate `ExpertModel`/`ExpertReasoner` loop contracts, built-in-name/package-ID dispatch switches, Registry mutation in Conversation activation/turn path, direct source fallback by Manager.
- Go: imports of old `application`/`connections`/`authorization`, owner imports of concrete connectors/common, View-ID switch in authority, `any` DTO/`map[string]any` at owner seams, shared mutable Console maps, `legacyGateway`, credential readback/provider DTO leakage to paired client.
- Resources/build/docs: dead tests/harnesses, fixture consumers, package/manifest/target entries, scripts with removed ABI/profile/recipient options, architecture current-state descriptions and links to superseded execution sequence.

Update current architecture only when the owning code lands; do not falsely document the target as already implemented. Preserve ADR history and amend only changed durable rationale. Keep this file's stage evidence temporary and bounded. Current docs needing updates include `docs/architecture/{README,modules,runtime,authority-recovery}.md`, the dependency policy, client/server guides, debug CLI and OAuth/credential setup instructions where contracts change. Design mockups remain historical references, not acceptance requirements.

## 10. Preparation evidence and remaining approval gates

The independent static review resolved R1–R8 against the saved contracts and maps. [Read coverage](file-read-ledger/coverage-audit.json) and [map-path coverage](file-read-ledger/mapping-coverage-audit.json) each reconcile all 1,172 frozen baseline paths with zero missing, duplicate or incomplete records. Each area supplies semantic changes and exact declaration/target/caller evidence; lexical references remain explicitly distinct from compiler-resolved calls. No runtime correctness is claimed.

No tests, builds, compilers, analyzers or formatters were run. Source/document reads, Git inventory/status, symbol/import searches, appropriate image/font inspection and static document/JSON/link reconciliation are preparation evidence only. Tracked source/test/configuration/manifest diff is empty; only this planning bundle is added.

The three previously open product/data choices were accepted by the user on 2026-10-02 at 13:46 UTC. No further implementation-scope question is required for this revision. Ordered execution was authorized at 13:49 UTC; this preparation task performs documentation changes only. See §10.1 and the [Korean summary](2026-10-02-review-summary.ko.md).

The recorded 13:49 UTC instruction authorizes ordered T0/S1/S2/S3 work, not automatic data reset. Use the coordinated task handoff, preserve the document-before-delete gate, implement one complete S1 slice before G1 compilation, finish all structure and G2 builds before new S3 tests, and record actual evidence at those future boundaries.

## 10.1 Accepted user decisions and clean-profile cutover

Accepted user decisions (2026-10-02 13:46 UTC): one encrypted ActionsRepository with external Actions unavailable while Vault is locked; current loopback Gateway deployment scope; existing Floe development data may be discarded for a clean new profile/schema. No legacy-data migration or old-runtime retention is required solely for compatibility. This is a plan-only revision, not an instruction to execute reset/deletion now. Any later permanent deletion needs exact local Floe targets and action-time confirmation; unrelated provider/calendar/mail data is outside scope. Before any approved reset, check narrowly for in-flight/uncertain external effects, stop old dispatch and prevent replay into the new profile; do not impose preservation of the old database/keys/runtime as a product requirement. New-system durable intent, uncertainty and recovery invariants remain unchanged.

The clean-profile cutover is the implementation target. Do not implement old-schema conversion, backward-compatible decoders, migration-only ownership branches, or an old-runtime retention requirement. Runtime open/read/identity errors still fail closed; they are not an implicit reset trigger. Creating or materially changing persistent credentials remains a separate action with its own required authorization. A pre-reset hazard check is limited to preventing orphaned/in-flight external actions from being unknowingly repeated; it does not require migrating legacy databases. No provider-side cancellation/deletion/cleanup is inferred from permission to discard local Floe development data.

## 10.2 Ordered execution authorization

Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.

## T0 registration-inventory clarification

T0 handoff clarification: the execution inventory subsequently found 31 Conversation test registrations absent from the preparation test-symbol index. Full-file read coverage remains a separate claim; the planning test index is not an exhaustive registration/case inventory or behavior ledger. The execution coordinator owns the authoritative source-registration reconciliation and behavior-by-behavior T0 coverage before deletion. No test deletion may rely solely on the preparation index.

### Execution checkpoint: T0 complete; S1 beginning (2026-10-02 15:25 UTC)

Ordered execution is active on `refactor/architecture-20261002`. Planning was checkpointed at `1b128525`; initial complete behavior partitions at `f9b97f8e`. Full source-registration reconciliation found 1,298 Rust registrations, not a count inferred from the preparation symbol map. Domain T0 ledgers record actual assertions, missing/overclaimed coverage and obsolete target semantics before any removal. Go and Context imports receive independent semantic repair; declaration counts alone do not pass the deletion gate. That initial checkpoint was documentation only. At `c44c57c4`, the complete semantic and fixture gate closed: Context150 has475 source-linked assertion sites and zero file/span hash mismatches; Go266 has807 explicit cases; every other partition and shared fixture has source/hash/consumer evidence. Recoverable test-only removal is now active in nonoverlapping batches, with exact balanced item spans and same-batch target/dependency cleanup. Vault and Go source preservation is independently compared against baseline; active test-tool instructions are being reconciled. Product architecture implementation has not started. No formatter/compiler/build/test or architecture checker has run.

Delegation workflow update (2026-10-02 15:40–15:48 UTC): the user now authorizes bounded code implementation by GPT6Luna with Max effort and Fast service, preferring saved Codex Cloud execution; local Claude is reserved for adversarial review. Design, scope decomposition, integration and substantive review remain with the coordinator/native reasoning workers. This supersedes the earlier mechanical-only Luna restriction for this refactor. All patch inputs/outputs must be exact source snapshots, nonoverlapping ownership and reviewed before integration. The main stage order and verification gates remain unchanged.

T0 closure: all 1,298 Rust, 266 Go, 326 direct Dart sites (370 outer-loop instances), 33 Swift framework registrations, 20 Python registrations and two standalone Swift programs have been removed with their covered exclusive support and targets. Eight t0-removal-*.json audits bind exact spans, hashes and preservation results. Whole-diff whitespace and static registration/target scans pass; retained runtime source bytes are unchanged outside audited test-only spans. JSON corpora and live diagnostics remain. Old development-plan commands are historical evidence. No compiler, formatter, build, tests, dependency resolution or architecture checker ran. S1.1 now begins; no new tests before full S2/G2 closure.

### S1 execution clarification: dependency closure and current state (2026-10-02 16:35 UTC)

S1 owner/provider/native/Go implementations are present in the shared worktree, but Conversation atomic storage, owner service integration and App/FFI caller cutover remain incomplete. No G1 formatter/compiler/build/test/checker has run and no passing runtime behavior is claimed. Immutable source-review and model-accounting identity clarifications are recorded at their canonical contract sections.

S1 sequencing clarification (2026-10-02 17:07 UTC): restricted native-host callbacks replace arbitrary Flutter trusted-publication routes, but complete Day refresh, multi-source mirror reconciliation and acquisition transport retain their original S2.2 stage. S1 exposes cached Day reads/CRUD; refresh controls remain absent until the real owner capability is connected. Prepared DTOs are contract preparation only, with no runtime success or availability claim. G1 qualifies the complete Conversation vertical; S2/G2 must close all Day features. No placeholder refresh, fabricated source, first-source selection, or compatibility publication route is accepted. The eventual Day operation persists command/operation identity, acquires outside storage transactions, commits mirror CAS, and exposes pure observation.


## 11. Approved Transform and DeviceModel closure refinement (2026-10-03)

The user approved the revised structure at 10:52 UTC after clarifying the common Transform interface and DeviceModel peer-backend boundary at 09:50, and reaffirming common remote Primary/device Fallback for Learner, Manager and Experts at 10:19. This reopens S2 structural closure before a complete G2 rerun. The detailed design and scope are in [Health and DeviceModel separation](2026-10-03-health-separation-gap-proposal.md); that document is the bounded refinement of this sequence, not an alternative global plan.

1. Freeze one cross-language DeviceModel request/result/capability/schema contract and shared typed Transform interface. Verify the installed Apple SDK structured-generation API by read-only declaration inspection before assigning backend implementation.
2. Implement the SDK-neutral contracts and concrete Health transform; extract FoundationModels as the first peer DeviceModel backend. No second local backend or Email/Android implementation is included.
3. Lift Agent/Learner domain schemas and request/result codecs above the backend; route all admitted local reasoning through DeviceModel while preserving Engine, ModelPort, Inference and Access responsibilities. Correct Learner purpose to deep_work. No role/purpose-based business dispatch remains inside the backend.
4. Separate Health acquisition, Apple mapping, concrete transformation, execution and source-owned receipt/envelope publication. Preserve mandatory local operation, exact provenance consumption, independent lifetimes and one authoritative receipt registry.
5. Complete same-snapshot Swift/Rust/native build wiring, remove obsolete coupled paths, review residual concepts and update current architecture. No interim formatting, compile or test loops.
6. At coherent structural closure, rerun the applicable whole G2 build gate; record external iOS prerequisite limits honestly. Then jointly verify isolated real behaviors and fix structure before reconstructing automated tests. No live provider writes, data reset or credentials are authorized by this sequence.

Earlier handoff RemoteOnly/background-recipient-grant requirements are superseded by the current explicit routing decision and ADR0034. The prior CP07 Foundation quality failure remains historical evidence; interface separation is not a quality pass. Primary and Fallback qualification retain their distinct factual-safety requirements.

## 12. Root-cause closure after live startup and pairing review (2026-10-03)

The user explicitly required design-level correction rather than successive local workarounds. This amendment is the authoritative continuation of S2; it does not start another migration plan. The unpublished Connections-card/combined-source-review checkpoint remains held until these contracts and callers converge. The already built diagnostic checkpoint may collect fixed-field OSStatus evidence; it is not a credential-access fix.

### 12.1 Evidence and causal boundaries

- Pinned Turso reconstructs CREATE SQL, so whitespace-only schema comparison rejected valid fresh plain and encrypted stores. The shared token comparator fixes that proven defect. Creation, validation and preflight still repeat layout facts, and some encrypted open paths recreate absent mandatory families.
- Rust VaultBridge already owns one activation queue and ReadyGeneration. The remaining client coupling routes Settings through Conversation.load, thereby opening storage and creating/resuming a Session. The recent button gate prevents a race but does not remove that feature dependency.
- Prepare currently writes OS staging before its durable setup receipt. Another Prepare can replace unstarted staging; replay may change proof/expiry or return a descriptor no longer represented by the slot. The singleton pre-start stage lacks mandatory Person/device/operation identity. Generic failure projection can discard the command identity despite an uncertain mutation.
- There is no active per-model-recipient approval. The new absent-grant policy belongs to Access and is substantive. However mixed per-view processing is summarized inconsistently with any/all reductions, so the disclosure can hide scope expansion.

The subsequent diagnostic build proved the live credential failure: Gateway SecItemUpdate selected Data Protection Keychain and returned OSStatus -34018 (missing entitlement). The user clarified that macOS must use the existing login/file Keychain to hold the encryption key, with private data in the encrypted local store; paid developer provisioning and a DP requirement are not part of the product. This supersedes the earlier assumed macOS ThisDeviceOnly contract. The existing Vault key provider already implements the intended macOS boundary; iOS retains its separate protected Keychain root-key backend. File Keychain does not promise DP ThisDeviceOnly semantics. No backend fallback, signing-account setup, real key migration or deletion is authorized by this source cutover.

### 12.2 Canonical schema lifecycle: Vault adapter

Keep the existing plain and encrypted databases. Introduce only private static schema declarations and small concrete create/inspect helpers inside floe-vault; no schema registry, ORM, new crate, migration engine or dynamic plugin mechanism. One declaration of each object/marker drives creation, token-preserving definition validation and preflight. Domain record, journal and authority semantics stay with their current owners.

Fresh creation creates all current tables/indexes and seed markers in one database transaction, validates that declaration, commits/checkpoints/syncs, and publishes ready only afterward. Registry and interaction optional business state is represented by absent rows; their tables are not created by installation/review commands. Encrypted layout version 3 deliberately replaces identity versions 1/2, which currently encode Registry installation. It is one current format, with no old decoder or migration ladder. Plain layout version 1 remains because its stored meaning does not change.

Existing open and preflight share the same inspect-only layout contract. They do not CREATE missing mandatory objects, alter versions or rebuild search as a side effect of schema admission. Derived search maintenance remains an explicit existing repository operation. Remove duplicated DDL strings, preflight object/version inventories, create-if-missing branches reachable from open, lazy Registry/interaction DDL and version-as-business-state checks. Preserve every PK/UNIQUE/CHECK/FK/index predicate and semantic validation.

Filesystem, key insertion and SQL commit are not one transaction. Preserve exclusive creation, stable identity, key readback and partial artifacts on failure. Typed admission distinguishes unsupported/corrupt stored data from permission, locked/busy storage, generic I/O and invalid compiled-in schema. Only proven stored-state evidence may enter the already approved debug-only recoverable archival policy. Old keys, journals, uncertain external-operation records and diagnostics remain preserved; stable builds never silently reset or replay.

### 12.3 Shared readiness: existing App/VaultBridge and app-lifetime controller

Retain the Rust VaultBridge queue, ReadyGeneration publication and retirement semantics. Move the existing client VaultController ownership to AppRuntime; coalesce the existing open operation after native callbacks are available. Features observe that same typed readiness/failure/safe-action state. No second lifecycle owner or feature-specific Vault generation is introduced.

Conversation owns Session/Run work only after readiness. Settings and Connections never create a Session. Remove Conversation's construction/disposal of the shared VaultController, implicit physical opening in load, and Day's Connections preparation/mirrored-readiness relay. Feature disposal/navigation does not lock, cancel, reset or reopen shared storage. Explicit lock/recovery uses existing owner commands and allowed actions; a Never-retry failure must not gain an unconditional Retry button. Pending lifecycle and feature command identities survive observer timeout. Day's local product display remains independently available where its owner capability allows it.

### 12.4 Pairing ownership: one encrypted credential transaction boundary

Connections remains the sole workflow owner. Prepare performs bounded pure target validation and one encrypted transaction storing exact command/actor, immutable safe descriptor and expiry. It performs no OS credential read/write or remote request. Replay returns the original descriptor/expiry. Remove the adapter's mutating prepare_setup and replaceable pre-start staging (`request=None`, independently regenerated proof/expiry).

Gateway proof and bearer state live in private encrypted Vault rows under the existing Keychain-held Vault root key. They are never included in owner product snapshots, wire output, diagnostics or plaintext storage. The macOS canonical path is login/file Keychain root key → encrypted Vault; there is no direct Gateway DP item or error-triggered alternate backend. Existing keys and obsolete items remain untouched. The current encrypted layout 3 is still unpublished, so these declarations join that same fixed layout without another version ladder.

The Vault admission transaction generates the fixed 32-byte polling proof and atomically stores it with PairingRecord and Pending expectation. Exact command replay reads the existing operation before creating new proof. The provider receives private material through a read-only storage capability; no opaque private-payload mutation port exists. Stage creation is no longer a DB/OS dual write. Commit Dispatched before HTTP; no database transaction spans remote I/O. Pre-dispatch cancellation atomically settles the operation, retires its exact active stage and restores the captured prior expectation. After possible remote handoff, never invoke Start solely to manufacture a cancellation handle. Preserve activation/response-loss evidence and report RepairRequired when an operation-bound remote readback cannot establish the outcome. Accepting the remote Start response atomically stores its safe challenge and typed enrollment-signing command. Activation re-reads the stored confirmation, signing receipt and private stage; it validates exact Person/device/client/issuer/producer/challenge and pin CAS, derives the binding from those stored facts, then atomically commits the bounded secret bearer, pin receipt/pin, Committed expectation and Paired record. The bearer is authenticated transport material, not a claim that its bytes encode identity. The removed OS boundary no longer requires a separate Committing/credential-readback phase. Local Forget fences use atomically and retains the exact private revocation material while the remote outcome is unresolved.

Delete Gateway OS-slot serialization, physical-root credential namespaces, blocking Keychain workers and staging/readback phases that existed solely to bridge the removed OS boundary. Preserve the normal installation/Vault lifetime lease, current actor identity, revision CAS, trusted Gateway identity and remote-effect recovery. There is no credential migration, automatic external cleanup or fallback to an obsolete slot.

All mutating Connections methods return an owner-classified command failure: NotAdmitted, Admitted or Indeterminate. Proof comes from the actual admission/transaction boundary, never an error name. Queries keep their ordinary error contract. The single product response envelope gains a command_error variant with required disposition; exact success still returns its receipt. Parse/validation rejection before owner invocation may establish NotAdmitted, while commit/transport/decode uncertainty cannot. The app-lifetime pending command retains sticky uncertainty: a later NotAdmitted retry cannot erase an earlier possible admission. Only an exact successful receipt or identity-bound terminal owner observation resolves that earlier attempt. No route disposal, timeout or generic Retry creates a replacement command or automatically redispatches an external effect.

### 12.5 Connector permission and truthful disclosure

Keep the approved new-grant default in Access: genuinely absent source grants include Gateway processing for trusted View categories in the first immutable connector review, before explicit Allow. Existing DeviceOnly restrictions remain unchanged until an explicit source review. Settings preselection is a requested intent, never authority. Preserve Health's raw-local boundary, mandatory local transform and live receipt, exact source/category/consumer checks, Gateway identity, OS connector permissions and external Action confirmation.

Replace inconsistent any/all summaries and fabricated connector-derived sensitivity labels with immutable per-view disclosure. Each concrete Context View declares its DataClass once; evidence builders and the trusted Access capability use that declaration, and ReviewedView snapshots it in the review digest. Product rows separately expose this sensitivity, actual reviewed/current-grant Metadata/Content/Derived categories, and current/requested processing. Do not hide expansion of one view behind another view's Gateway permission. Product Observe requires the actual nonzero stored source revision; no absent-source revision is fabricated as 1. Native setup already creates the real Pending source before configuration. Generic Access absence semantics do not create a second product setup path. Remove obsolete model-approval copy, not source guards or restriction semantics. Restore only the requested owner-backed Connections cards; Read-only/source-detail removal and current Calendar/Conversation flows otherwise remain.

### 12.6 Ordered cutover and nonoverlapping ownership

1. Freeze the concrete Vault schema declarations/admission result, shared-controller injection, and Pairing encrypted admission/recovery contracts. Review schema version 3 and cancellation truthfulness explicitly.
2. Schema owner extracts declarations and closes create/open/preflight. Release shared gateway-authority source sections before pairing owner edits them; no concurrent whole-file rewrites.
3. Pairing owner makes Prepare DB-only and closes atomic encrypted staging/credential transitions, remote dispatch and replay/cancellation. Use the existing durable operation rather than introducing another Prepare state machine.
4. Client owner moves the existing VaultController to AppRuntime and cuts every feature caller over, removing the temporary Day/Conversation readiness path. Connections keeps only its own operation/presentation state.
5. Complete mixed-view source disclosure and requested cards/defaults against Access-owned immutable review data. Remove old aggregation and lifecycle routes in the same source boundary.
6. Perform conceptual residual searches and update current architecture/ADR facts. Only then run one coherent formatting/compile/analysis batch, followed by affected Apple builds.

### 12.7 Bounded verification evidence

Use isolated stores and fake ports/keys; no real user data, Keychain, source grants or external effects. This remains focused behavior validation before S3 test reconstruction.

- Plain and fake-key encrypted create/close/inspect/reopen agree for the current declaration. Remove a mandatory family or alter CHECK/UNIQUE/FK/index predicate/marker while keeping columns: open and preflight both reject, perform no repair and preserve bytes.
- Registry/interaction empty-row states work without DDL or identity-version changes. Inject schema creation/commit/checkpoint and key-insert/readback failures: no false Ready, replacement key or destructive cleanup.
- Shared readiness activates once; Settings creates no Session; navigation/disposal cannot affect physical Vault lifetime; typed safe actions and retained command IDs govern recovery.
- Prepare uses zero credential/HTTP calls and exactly replays descriptor/expiry. Start atomically stores operation/private proof before remote effects; response loss, restart, foreign identity and changed intent cannot regenerate the admitted proof or duplicate activation. Commit failures preserve uncertainty and replay resolves the same transaction identity.
- Cancellation before/after handoff remains distinguishable; no missing handle causes a new Start. Paired requires exact generation/binding readback. Unknown outcomes preserve evidence and remain explicitly repairable.
- New source review combines approved scope once; existing DeviceOnly remains restricted; mixed-view expansion is visible; raw Health cannot bypass local transformation.
- Diagnostics expose only closed stages/kinds, opaque correlation IDs and numeric OSStatus. The confirmed Gateway DP path is removed; actual root-key operations or user-state migration remain separately authorized. No test may log proof or bearer bytes.


### Direct-owner implementation checkpoint (2026-10-04)

At the user's instruction, delegated writers stopped and the primary assistant took over research and implementation directly. The unfinished tree was preserved before further edits. Gateway's OS credential record and write workers are removed; the provider now reads private Vault snapshots while Pairing admission, activation and Forget use encrypted transactions. The original proof and retired credential material are preserved as evidence. A private `creation.pending` marker records exact Person/Vault/layout identity before key insertion; a surviving valid marker produces `IncompleteCreation` and is not automatically reset or resumed. Empty databases without such proof stay unavailable rather than being classified as unsupported schema. These source changes still await the coherent compiler, isolated-failure and adversarial-review gates; no user data or credential operation has been executed.


### Adversarial-review closure refinement (2026-10-04)

The initial root-owned source snapshot passed the full Rust production build, DAG and isolated fake-key owner/storage probes. It is held on a review branch, not main. Independent source review identified additional failure interleavings; those findings reopen closure before another coherent qualification batch.

- Cancellation after a recoverable failure needs an explicit nonterminal Cancelling phase. This is a workflow state, not a second credential authority; remote Approved still produces historical evidence only.
- A delayed exact Start reply may add its handle/enrollment after local Cancel or Forget. It must preserve those decisions and never restore current authority. Reconciliation is single-flight per operation within the existing owner instance; no database transaction spans HTTP.
- Any exact late Approved after a terminal observation retains historical revocation evidence without changing a newer expectation or pin.
- Pairing admission consumes a monotonically increasing durable generation. Restoring a previous expectation does not restore the high-water mark.
- Assistant and product authorization signing share a transaction-level current-credential fence; a surviving historical producer pin is not active Gateway authority.
- Existing open must not create a missing host lock. Unclassified partial creation remains unavailable and never authorizes key replacement. A failed durability barrier is not a successful creation merely because bytes are visible.
- Command rejection requires a causal storage outcome and an identity-bound resolution. A generic Conflict name is not evidence of rollback; conversely a proven rollback must not fabricate commit uncertainty. Pairing Start/Confirm/Cancel now commit an immutable exact-intent success or rejection receipt in the same transaction as their state change. The NotApplied response is a terminal proof for the whole command, distinct from attempt-local NotAdmitted, and can resolve sticky client uncertainty. Commit/read failure still cannot produce that proof. Other mutation paths remain under audit; aggregate qualification is pending.

Real Apple runtime, Flutter aggregate analysis, the expanded failure probes and reviewed final source qualification remain pending. These refinements do not authorize user data/key cleanup or reconstruction of the full S3 test suites.


Readiness review clarification: Registry and Memory already fence callbacks with their operation generation and reset it on readiness transitions; their stale-report premise did not include those source files. A failure revision now additionally prevents an in-flight Vault presentation update from overwriting a newer failure report. No second physical Vault generation is introduced.

### Follow-up review closure (2026-10-04)

The review accepted exact pairing rejection receipts, monotonic admission, late Cancel/Forget response retention and current-credential signing fences. Its remaining concrete driver/projection findings are being closed in one source batch:

- Pairing owner jobs retain their lifecycle until terminalization or owner shutdown, with bounded per-round deadlines and capped retry backoff for transport uncertainty. Product error flattening no longer decides driver liveness. No-op observations do not increment revisions.
- Revocation evidence and Forgotten are explicit terminal pairing states. They cannot advertise connection authority or automatic reconciliation. A cancellation already recorded is not offered again.
- Local private/issuer/pin reads are prepared before durable Start dispatch intent. The final intent remains conservative across the unavoidable database/network crash window; no failure causes automatic secret replacement or loss of evidence. The Go Gateway's exact operation/proof replay was inspected in `server/internal/pairing/pairing.go` and `receipts.go`; it reuses the saved challenge and rejects changed identity/proof.
- Enrollment receipt readback returns metadata only and cannot mint a new signature. Undecodable remote mutation responses remain Indeterminate.
- Product-record commands use an atomic negative receipt fenced against their first business commit. A resolver checks for exact existing admission before recording NotApplied; late first inserts and Forget must honor the same rejection fence. Preflight errors are not interpreted as proof by name. Native Pending source materialization moves after durable NativeSetup admission. Preparatory Access review artifacts are not an applied grant or remote effect and remain preserved.
- Source-operation-only commands and their existing owner journals remain under separate semantic audit; a product-record absence proof must never be used to erase an uncertain source operation.

All changes in this follow-up batch still require a coherent Rust/Dart gate and targeted behavior probes. No permanent S3 suite, real user credential operation or data cleanup is included.


Source-journal audit found the same pre-admission uncertainty issue at its own reserve boundary. The closure is storage-local: source apply/pause/disconnect resolve failed admission through a plain source-command rejection fence serialized with `source_operations` reservation. This adds a mandatory plain-layout declaration but no migration or fallback. It does not consult product-record absence or clear an existing source fence. The shared command identity value is reused, while proof remains with the corresponding storage owner.


The next lifecycle review exposed a query/receipt mismatch: even successful `vault.status` had been passed through a worker-result release route. Status now bypasses the mutation observer; unknown lifecycle admission rejoins the exact command after an absent readback. Sealed-generation recovery is owned by Unlock's queue, not by query-driven activation. Pre-enqueue failures carry explicit NotAdmitted evidence. Released outcomes move to an immutable storage archive rather than being discarded or counted forever against the active receipt cap; an old archived error must not retire a newer generation. These lifecycle changes are not yet qualified and still need dedicated gateway/queue probes.


### Lifecycle/source-command closure qualification (2026-10-04)

Root directly implemented the latest review findings. The production Rust workspace build passed after typed lifecycle-error caller updates and source cancellation admission. Seven isolated Flutter gateway/controller probes passed: pure status; retry after query timeout; exact-ID resubmission after absent admission; lost release acknowledgement without command re-execution; shared readiness failure revision; per-target pending recovery; and whole-command NotApplied resolution. Flutter analysis reports zero errors/warnings and 138 informational lints, not a clean default analyzer exit.

A fresh plain-store AppHost probe processed 80 real Lock-only queue commands, released/archived outcomes, replayed the oldest command and rejected a changed intent under that ID. It did not create/unlock an encrypted Vault or access a real key provider. Fake-key storage probes additionally passed late Start-after-Forget retention, terminal authority fences and separate source/product negative receipts. These disposable probes remain outside the committed application/test suite. Actual sealed-generation key-provider recovery and Apple runtime qualification remain pending; no existing user profile or private UI has been operated.

The cancellation worker and per-target recovery changes require final same-snapshot qualification and another narrowly scoped Opus review. Main remains held at the last diagnostic-qualified snapshot. Mac command-only validation is waiting for the user's explicit exception to the no-delegation instruction.
