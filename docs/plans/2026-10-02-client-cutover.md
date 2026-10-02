# Flutter and native client cutover plan

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

**Decision revision, 2026-10-02 13:46 UTC:** the three product/data choices are accepted; see [current accepted decisions](2026-10-02-architecture-refactor.md#101-accepted-user-decisions-and-clean-profile-cutover). Existing Floe development data may be discarded for a clean profile/schema without migration. No actual reset/deletion or implementation is authorized by this documentation update; new-system recovery safety remains mandatory.

Status: plan-first review draft; all 375 frozen apps files have been read/inspected (340 text, 35 binary). No product, test, build, manifest or generated-source changes are authorized by this document. Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Read progress and exact per-file observations are in [client read ledger](file-read-ledger/client.json); the final ledger contains zero pending rows; font inspection parses all tables but does not visually inspect every glyph. Tests were not run and the baseline is not claimed to compile.

This appendix refines the [architecture plan](2026-10-02-architecture-refactor.md). The end state is Flutter product intent → ordinary AppWire → Rust semantic owner → real provider/Gateway adapter → Go or Apple OS. Flutter may render a review, collect a product choice, carry opaque references and mirror owner state. It may not select credentials, serialize provider protocols, infer permission from successful transport, manufacture source authority, choose fallback, or sequence approval/execution as a local policy engine.

## Decisions and boundaries

1. Connections owns pairing and enrollment as one operation. Remote approval alone is not completion: verified enrollment, exact secure credential commit, readback and protected readiness must complete before `connected` is displayed. No bearer, polling proof, signing challenge, public-key envelope or provider token crosses the product DTO boundary.
2. Flutter has no direct Gateway HTTP client. Provider OAuth and API-key entry live in the Go management/authorization surface. Flutter opens an owner-validated, bounded launch action and observes an opaque operation. The launch URL cannot contain Floe bearer credentials. Provider-secret input fields are deleted from Flutter.
3. Connections owns connector lifecycle and source selection. Loading/observing a catalog cannot bind or disconnect a source. Resource changes are explicit product commands with expected owner revision and a reviewed selection reference.
4. Rust Context owns freshness, acquisition admission and publication/revocation. Rust Connections owns source inventory reconciliation; Day owns its imported mirror. Flutter retains only the necessary app-lifetime native request/response pump and OS prompts. The native boundary preserves request/host/device/source identity, before/after native subject, deadline, size and permission fences.
5. Rust Actions owns propose/approve/execute/recover/collect ordering, standing authority interpretation and uncertain-write recovery. UI only displays owner `allowed_actions` and submits an explicit decision or operation.
6. Rust Experts owns atomic package/installation/assignment lifecycle. One capability toggle becomes one revision-checked owner command, not a loop of installation and assignment mutations in Dart. Source binding is configuration and does not grant Access permission.
7. Conversation is the first vertical. Preserve stable command IDs, explicit Run cancellation, durable result reconciliation, runtime epoch/cursor validation and stale-render suppression. Screen dismissal and observer timeout detach only. Vault lock and Run cancellation remain separate explicit intents with owner-defined outcomes.
8. Keep existing design components, preview assets, checked-in generated localization, Apple build/signing settings and dormant Android/Windows scaffolding unless an exact affected symbol or reference is mapped below. Do not retain legacy architecture to preserve untrusted tests.

## Exact proposed client interfaces

All interfaces below are client projections/adapters. The exact command/query, owner-method, DTO, failure and replay contract is [canonical contracts](2026-10-02-canonical-contracts.md); this appendix fixes the matching client names and caller destinations. Their authoritative implementation is in the Rust owner from the main plan. Client methods return `Future<T>`. Every command, including preparation that allocates a persisted descriptor or launch action, carries a stable `commandId`; every change to an existing mutable aggregate carries `expectedRevision`. Read-only inspection never allocates a review, refreshes its meaning or advances a revision. Outputs carry their exact aggregate revision and opaque identity; a setup result carries its bounded target identity and expiry rather than an invented source revision. AppWire schema remains 2 / protocol 1 under the canonical contract. No `_v3`, `next`, compatibility facade or parallel backend path.

### Connections product adapter

New files:
- `apps/client/lib/features/connections/application/connections_gateway.dart`: `ConnectionsGateway` interface.
- `apps/client/lib/features/connections/domain/connection_models.dart`: product DTOs only.
- `apps/client/lib/features/connections/infrastructure/app_wire_connections_gateway.dart`: intent encoding, result decoding and transport correlation only.
- `apps/client/lib/features/connections/presentation/connections_controller.dart`: screen loading, current selection, pending rendering and stale-result suppression only.

Methods:
- `prepareGatewaySetup(commandId, addressText)` → `GatewaySetup{targetRef, displayAddress, expiresAt}`. Bounded initial loopback input only; Rust validates and binds it.
- `overview()` → `ConnectionsOverview{revision, gateways, integrations, sources}`.
- `getGateway(gatewayRef)` → `GatewaySummary`; pure readback of the exact binding.
- `startPairing(commandId, gatewayTargetRef)` → `PairingSnapshot`. `gatewayTargetRef` is issued by Rust local Gateway setup/discovery, never an arbitrary request URL. Initial setup is `prepareGatewaySetup(commandId, addressText)` → `GatewaySetup{targetRef, displayAddress, expiresAt}` through `connections.gateway.prepare_setup`; Rust normalizes localhost to 127.0.0.1 and rejects non-loopback host, userinfo, path, query or fragment. The opaque target is bound to host principal and generation; later operations never echo endpoints.
- `confirmPairing(commandId, operationRef, expectedRevision)` → `PairingSnapshot`. This expresses the user's displayed-code comparison decision. It cannot submit or choose a cryptographic challenge.
- `observePairing(operationRef)` → `PairingSnapshot`.
- `cancelPairing(commandId, operationRef, expectedRevision)` → `PairingSnapshot`; explicit user intent only.
- `forgetGateway(commandId, gatewayRef, expectedRevision)` → `GatewaySummary`. It forgets the specified local binding/credential, returns whether remote revocation remains necessary, and never deletes unrelated/shared credentials after an error.
- `prepareIntegrationReview(commandId, integrationRef, expectedRevision)` → `IntegrationReview`; Connections persists the exact connector/catalog/Gateway revision and bounded setup/initial-selection descriptor. It grants no source access.
- `inspectIntegrationReview(reviewRef)` → `IntegrationReview`; pure inspection.
- `startIntegration(commandId, integrationRef, reviewedSelectionRef, expectedRevision)` → `ConnectionOperationSnapshot`.
- `observeOperation(operationRef)` → `ConnectionOperationSnapshot`.
- `cancelOperation(commandId, operationRef, expectedRevision)` → `ConnectionOperationSnapshot`; ordinary AppWire routes to `ConnectionsService::cancel_operation`, which invokes `RemoteIntegrationPort::cancel(CancelIntegration, &OperationScope)`. Owner cancellation is explicit and revision-checked; closing a view only detaches. A timeout or uncertain remote cancellation preserves the operation for observation/reconciliation.
- `prepareSourceReview(commandId, sourceRef, expectedRevision)` → `SourceReview`. Connections persists the immutable source/resource selection descriptor and the command receipt; replay returns the same review reference.
- `inspectSourceReview(reviewRef)` → `SourceReview`; pure retrieval of that descriptor, including current usability/staleness, without replacing its reviewed meaning.
- `configureSource(commandId, sourceRef, reviewRef, selectedResourceRefs, expectedRevision)` → `SourceSummary`.
- `disconnectSource(commandId, sourceRef, expectedRevision)` → `ConnectionOperationSnapshot`.
- `prepareObserveReview(commandId, sourceRef, expectedRevision, requestedProcessing)` → `ObserveReview`. Connections delegates the exact verified source facts to Access, which persists the immutable processing/grant review and its reviewed expectations. The Connections command receipt retains that exact Access review reference.
- `inspectObserveReview(reviewRef)` → `ObserveReview`; pure Access-backed inspection. It does not create a replacement review or grant.
- `setObserve(commandId, sourceRef, enabled, reviewRef?, expectedRevision)` → `SourceSummary`; `reviewRef` required for enable, absent for pause, and validated by Access against the exact source authority, policy, native subject and reviewed members.
- `requestManagementLaunch(commandId, gatewayRef, expectedRevision)` → `LaunchAction{actionRef, purpose, validatedUrl, expiresAt}`. Native URL opening remains an OS effect; no credential channel remains. The owner validates URL scheme, target, expiry and absence of credential data before exposing the bounded launch action.

`GatewaySummary`: `gatewayRef`, `displayName`, `state`, `revision`, `allowedActions`, `remoteRevocationPending` and optional bounded failure. Exact state is `unpaired|paired|repair_required|forgotten`. No endpoint/token/client credential/key envelope.

`PairingSnapshot`: `operationRef`, `revision`, `state`, optional `displayCode`, optional `expiresAt`, optional `gateway`, `allowedActions`, optional bounded failure, optional owner observation delay. The exact Rust-owner → product-wire mapping is `Pending` → `starting`, `AwaitingLocalConfirmation` → `awaiting_local_confirmation`, `AwaitingApproval` → `awaiting_gateway_approval`, `Verifying` → `verifying`, `Committing` → `committing`, `Paired` → `connected`, `Rejected` → `rejected`, `Expired` → `expired`, `Cancelled` → `cancelled`, `RepairRequired` → `repair_required`. Unknown states fail decoding; no client-local transition can manufacture `connected`. A transport timeout leaves operation identity intact; it does not become `cancelled` or a new pairing. `displayCode` is for comparison only and cannot authorize provider transport.

`IntegrationSummary`: `integrationRef`, `displayName`, `category`, `state`, `revision`, `capabilities`, optional source summary. Capabilities are product action affordances, not provider OAuth scopes or arbitrary scope-field maps.

`ConnectionOperationSnapshot`: `operationRef`, `revision`, `state`, optional `launchAction`, optional `displayCode`, optional source summary, `allowedActions`, optional bounded failure and owner observation delay. OAuth state, polling proofs, producer attempt payloads, access/refresh tokens and secrets stay inside Rust/Gateway adapters.

`SourceSummary`: opaque `sourceRef`, revision, display labels, availability, last observation display timestamp, selected resource labels/references, Observe state and allowed actions. Native permission prompts are explicit bounded OS actions supplied by the owner; never inferred from provider name in a widget.

### Review, launch and replay identity

The canonical DTO definitions are authoritative; client models use the corresponding lower-camel field names, not an independent payload. `SourceReview` identifies the reviewed source and source revision plus the selected-resource candidates issued by Connections. `ObserveReview` identifies the immutable Access review and the exact source/expected-grant/processing choice it represents. `BindingReview` identifies the Experts-owned review, assignment, requirement and binding revision; its opaque candidate references belong only to that review. Connections resource IDs and Experts candidate IDs are distinct typed opaque UUID strings. Every review reference uses the unchanged canonical `ReviewRefDto {id, revision, digest}` with its owner-specific type; it is not a bare ID or a client-reconstructed descriptor. Never coerce an ID from one namespace into another.

Preparation is a command because it durably allocates a bounded immutable descriptor. Same principal + command ID + identical input returns the same descriptor; changed input under that command ID returns the canonical owner `Conflict`. Stale expected revision returns a typed conflict and reload instruction. Inspection is a query: it can report expired/stale/consumed/unavailable but cannot overwrite a descriptor, issue another reference or apply a decision. Applying a review rechecks the owner's current facts and the exact persisted expectations. A review reference is not itself authority, and expiration never becomes silent re-approval.

Gateway setup preparation and management launch are likewise explicit commands with owner-persisted command identity and expiry. Setup creation has no existing aggregate revision; management launch binds the requested Gateway revision. `startIntegration` consumes the Connections-owned `IntegrationReviewRef` returned by `prepareIntegrationReview`, never an Experts binding candidate. Launch URLs are bounded by the owner; URL opening does not report successful authorization. Readback after response loss retrieves the original setup, launch or operation receipt; the UI does not create a fresh command ID merely to escape uncertainty.

`setObserve(..., enabled: true, reviewRef, ...)` routes to `ConnectionsService::apply_observe`, with Access owning review validation and grant CAS and Connections owning the cross-store source fence. `enabled: false` routes to `pause_observe`, requires no review reference and cannot rewrite the policy of any grant. The persisted reservation/receipt and lock/recovery protocol is the [Vault appendix](2026-10-02-vault-cutover.md); Flutter does not infer availability from a locked Vault or reconstruct a successful grant receipt from a source summary.

The user accepted the unified encrypted Actions repository and unavailable-while-locked external Action behavior on 2026-10-02 13:46 UTC. UI states expose that owner outcome. The user also permits discarding existing Floe development data for a clean profile/schema; no migration/old-data preservation is required. This revision executes no reset, and any later permanent deletion requires exact Floe-only targets and action-time confirmation. Preserve the new system’s durable uncertainty semantics and never replay an old external effect after reset.

Failure contract: preserve the admitted owner failure envelope's domain/category/reason/incident/correlation/reload/seal/recovery/safe-actions. Display-only localized messages may map bounded codes. Do not copy the current Dart fallback retry allowlist into the target; owner decides whether retry/review/reconcile is safe. Unknown, malformed or mismatched output is a transport/contract failure and cannot enable a mutation.

### Day, Actions and Experts product changes

- `DayGateway.refreshDay(commandId, DayQuery)` → `DayRefreshSnapshot{operationRef, revision, state, day, failure?}`. Day query is display date/timezone context, not Calendar batch/source authority. Connections/Context resolve current sources, acquire authorized evidence, and Day imports the mirror. Observation is read-only; refresh cannot silently establish a source or expand its resources.
- `ActionsGateway.listActions(cursor?, limit)` → bounded owner page; pure query, no competing-store probing.
- `ActionsGateway.inspectAuthority()` → standing Calendar Create authority snapshot; pure query.
- `ActionsGateway.setCalendarCreateAuthority(commandId, mode, expectedRevision)` → authority snapshot; `mode` is exactly `allow|ask|deny` for Calendar Create only. An explicit CAS command, never derived from Observe or Expert selection.
- `ActionsGateway.inspect(actionRef)` → `ActionSummary{actionRef, revision, status, displayProposal, allowedActions, reviewRef?, outcome, recovery}`. No raw `mutation.original`, external EventKit identifiers or provider-specific status guards.
- `ActionsGateway.submit(commandId, ActionIntent{targetRef, editedProductFields, expectedRevision})` → operation snapshot. Rust owns standing approval, durable pre-dispatch intent and dispatch.
- `ActionsGateway.decide(commandId, actionRef, reviewRef, decision, expectedRevision)` → operation snapshot. The UI does not follow approval with a second local execute command; owner runs the approved lifecycle.
- `ActionsGateway.reconcile(commandId, actionRef, expectedRevision)` → operation snapshot; unknown outcomes require lookup/reconciliation, never blind duplicate writes.
- `ExpertsGateway.directory()` → `ExpertDirectorySnapshot`; pure query.
- `ExpertsGateway.setInstallationEnabled(commandId, installationRef, enabled, expectedRevision)` → `ExpertDirectorySnapshot`. Rust makes the intended installation/assignment change atomically.
- `ExpertsGateway.prepareBindingReview(commandId, assignmentRef, requirementRef, expectedRevision)` → `BindingReview`; `inspectBindingReview(reviewRef)` → `BindingReview` is a pure query; `replaceBinding(commandId, reviewRef, selectedCandidateRefs, expectedRevision)` → `ExpertDirectorySnapshot`. Experts persists and resolves the immutable descriptor, including the exact assignment/package/definition/requirement/binding expectations and the candidate IDs it issued. The client never reconstructs package authority from display fields, and FFI never creates a parallel review store. Candidate selection grants no Access permission.
- `KnowledgeGateway` retains read/review/decide product methods; review DTO/interface moves from `presentation/agent_memory_review.dart` into `application/memory_gateway.dart` and `domain/memory_review.dart`, with presentation callers importing those contracts; the current file contains no widget and is deleted after all imports migrate.

### Conversation first vertical

Keep `commandId` distinct from request ID. On response loss, query the same admitted command/operation; a retry resubmits the identical command only when owner recovery permits. Owner mutation receipt means admitted, not complete. Preserve event epoch, contiguous cursor and monotonic aggregate revision checks. Reopen/resync uses owner durable snapshots. The exact blocked mapping is `RunState::Blocked` → `AppRunStateDto::Blocked` → `state: blocked`, `AppTurnExecutionDto::Blocked` → `execution: blocked`, and `AppReplyStatusDto::NotProduced` → `reply: not_produced`. It preserves real settled attempts/coverage and has no fabricated answer or generic Finished alias.

Replace `AgentConversationTurnRequest.profileId` and client continuation eligibility with product-only intent: session reference/revision, text, optional opaque continuation/retry reference returned by Rust. Inference chooses Gateway-primary/local-fallback at planning time; unavailable or denied in-flight attempts cannot cause a client-side fallback. Source processing review belongs to Connections/Access; delete `processing_recipient` prompts with synchronized owner DTO migration. Once Rust durable review resolution auto-resumes the waiting Run exactly once, remove the routine resolved-card `continueRequest`/`continueInteraction` path. Explicit user continuation of a bounded finished turn remains a distinct owner-admitted product intent.

Split `AgentController` cross-owner composition: Conversation controller handles session/send/observe/explicit stop; Vault controller handles status/create/unlock/explicit lock; Connections, Knowledge and Experts retain their presentation controllers. `LocalOwnerGateways` becomes a non-null app composition dependency bundle where product capabilities exist; preview supplies its own explicit preview bundle rather than runtime `is SomeGateway` fallback probing.

## Source-to-target mapping rules

The exact symbol/line/import inventory is [client-symbol-map.json](file-read-ledger/client-symbol-map.json). A file-level KEEP row includes all its unlisted private helpers. A SPLIT/REWRITE row enumerates exact declaration partitions under `semantic_reconciliation`; `targets` lists retained/moved files and each symbol names its destination. A DELETE symbol has null `target`; its `replacement` identifies the replacement caller/responsibility, never a same-name move. Groups enumerate actual affected call/import/part edges. The narrower feature/presentation lexical indices refer to this one disposition authority. No omission implies deletion. These are planned dispositions only.

- `features/connections/application/local_server_client.dart`: DELETE the entire file. Move transport address normalization/redirect rejection/DIRECT proxy/response budgets/identity checks into Rust Gateway adapter, not a new Dart HTTP helper. Its `ServerPairingStart`, `ServerConnection`, `ServerCredentialStore`, `KeychainServerCredentialStore`, connector and inference DTOs are not target product DTOs. Replace all callers with Connections product methods above.
- `remote_access_gateway.dart`: DELETE `RemoteAccessGateway` and `NativeRemoteAccessGateway`; separate producer enrollment UI/API disappears into Connections pairing.
- `remote_pairing_gateway.dart`: DELETE challenge-bearing `PairingTarget`, `RemotePairingGateway`, `NativeRemotePairingGateway`; replacement adapter is ordinary `AppWireConnectionsGateway` with opaque operation results.
- `remote_owner_operation.dart`: DELETE duplicate remote job/observer/failure decoder only after shared operation observer preserves retained-result/release reconciliation. No mutation result is released before Rust credential persistence completes.
- `domain/remote_owner_models.dart`: DELETE all signing/enrollment/token-bearing DTOs; replace display-only models in `connection_models.dart`.
- `connector_authorization_gateway.dart`: REWRITE directive/interface into owner operation/launch product contract; remove `ServerConnection` and `ServerConnectorAttempt` parameters. Polling cadence and terminal transition remain owner decisions.
- `local_server_panel.dart`: REWRITE as `gateway_connection_panel.dart`. Remove persisted address/proof/challenge/token fields and `_pair`/`_drive`/`_finishPairing` policy. A bounded initial address text draft is allowed only for `prepareGatewaySetup`. Keep display/confirmation/cancel/retry controls backed by a controller. `dispose` only detaches.
- `server_connector_panel.dart`: REWRITE as `integration_detail_panel.dart`; delete secret controller, arbitrary provider scope map, direct provider HTTP calls and provider attempt polling. Keep confirmation and Observe review display. Authentication launches trusted management flow.
- `connector_screen.dart`: SPLIT small overview/detail widgets and product controller. DELETE `_synchronizeServerCalendar`, `_bindServerCalendar`, `_catalogChangedExplicitly` source mutation on read, `_macOSAttentionConnection` fabricated descriptor/permission state, provider-specific policy getters and catalog transport. The screen renders owner summaries; explicit choices call product intents.
- `source_connection.dart`, `agent_connections.dart`, `connection_observe.dart`: REWRITE DTOs as opaque source/review summaries; Rust remains authority. Product labels/allowed actions replace native provider lists, grant/policy/private subject details and duplicated status interpretation.
- `app_wire_calendar_source_gateway.dart`, `calendar_source_gateway.dart`, `native_personal_source_gateway.dart`, `connection_observe_gateway.dart`: SPLIT product source/Observe methods into new Connections adapter; delete inventory reconciliation and raw remote binding requests from UI surface. Keep exact review/CAS semantics at Rust owner.
- `calendar_connection_view.dart`, personal source cards and connection settings: REWRITE to compose/render product summaries and owner review handles; retain UI selection/confirmation/accessibility, remove provider authority derivation.
- `app/runtime/local_owner_gateways.dart`: SPLIT `NativeConversationSessionGateway`, `NativeMemoryGateway`, `NativeRegistryGateway`, `NativeProposalGateway`, `NativeConnectionsGateway`, `NativeVaultLifecycleGateway` into respective feature infrastructure adapter files. No forwarding-only facade remains. DTO validation is boundary decoding; policy is owner-owned.
- `app/runtime/app_runtime.dart`: REWRITE assembly imports/injection only; remove `remotePairingV2`, `remoteAccessV2`, `remoteAccess` and special pairing gateway. Add Connections adapter. Runtime owns transport/read-model shutdown, features own no core lifetime.
- `app/runtime/native_transport.dart`: KEEP isolate, close, envelope/correlation ownership; DELETE special remote methods and worker dispatch branches. SPLIT `LocalContextTransport` into internal native-host infrastructure so feature product gateways cannot publish arbitrary local context.
- `features/day/infrastructure/floe_native_bindings.dart`: MOVE to `infrastructure/native/floe_native_bindings.dart`; DELETE only `remotePairingV2`/`remoteAccessV2` lookups after ordinary owner intents are live; keep allocation/free/open/version and command/query/events symbols.
- `app/runtime/floe_client.dart`: SPLIT Conversation DTOs/codec into `features/conversation/infrastructure/app_wire_conversation_client.dart`; generic transport/correlation belongs in runtime. Preserve prepare/submit/lookup stable identities, typed error and event validation; delete explicit model profile client control.
- `app/runtime/app_read_model.dart`: REWRITE in place only for moved DTO imports and exact blocked projection, retaining cursor/epoch/revision fences. It is never durable truth; physical relocation adds no semantic value to this cutover.
- `app/runtime/owner_operation.dart`: REWRITE shared transport observer only for ambiguous submission reconciliation and canonical failure fields; no owner business policy. `_failureEnvelope` decodes owner safe actions without manufacturing retry authority.
- `features/day/application/native_day_gateway.dart`: SPLIT to `features/day/infrastructure/app_wire_day_gateway.dart`; KEEP product capture/classify/task/delete/query codec; DELETE `_syncCalendar`, `_runCalendarOperation`, `_updateCalendarObservation`, `_revokeCalendarObservation` and `_calendarFailure` after Rust owner replacement. `drain` then disappears from main.
- `calendar_observation_refresh.dart` and `calendar_observation_publisher.dart`: DELETE entire files after Context owns freshness/acquisition/observations and Day consumes those owner facts for mirror refresh. No replacement Flutter timer carrying calendar policy.
- `calendar_gateway.dart`: SPLIT product refresh/query interface from `infrastructure/native/eventkit_calendar_host.dart` OS reader. Keep native permission/settings and bounded host request forwarding, not source selection/sync rules.
- `infrastructure/native/local_context_publication.dart`: SPLIT `LocalDeviceIdentity` into `infrastructure/native/local_device_identity.dart`; DELETE publishing decorators and their bind/revoke/freshness logic after Rust Context cutover. Within an active profile preserve valid device identity; invalid identity fails closed rather than silently replacing authority. The accepted clean new profile may initialize its own explicitly admitted identity; old device-marker migration is not required.
- `main.dart`: REWRITE pure app/native host assembly, remove LocalServerClient, publication wrappers, macOS refresh timer and Day drain. Keep diagnostics and explicit host start/dispose ordering.
- `calendar_action_controller.dart`: REWRITE UI state only; remove `canApprove` semantic conditions, `canModify` provider parsing, standing `allow` autoapproval and propose/execute/collect sequences. Product Actions operation decides these.
- `calendar_action_facade.dart`: DELETE forwarding-only facade after direct adapter injection. `native_calendar_action_gateway.dart` and action DTOs rewrite to product Actions operation contract; presentation callers update imports and render owner actions.
- `agent_registry_controller.dart`: REWRITE `configureCapability` to one owner command; keep pending UI and stale selection suppression. `agent_registry.dart` splits package/installation/directory/task presentation types according to Rust Experts owner without marketplace or A2A-distribution expansion.
- `agent_controller.dart`: SPLIT as described above; remove runtime capability probing, panel-implied cancellation and model/provider choices. `agent_session.dart` and interaction DTOs remove obsolete fixture-calendar scope and recipient consent representation in the same snapshot as Rust.
- `macos/Runner/MainFlutterWindow.swift`: DELETE only `LocalServerBridge` and local-server channel registration/property. KEEP window, CalendarBridge, attention bridge, design feedback and all-day normalization. A bounded URL launcher may use normal URL-launcher plugin instead of a credential channel.
- `ios/Runner/AppDelegate.swift`: DELETE only `IOSLocalServerBridge`, local-server channel registration/properties and no-longer-used credential imports. KEEP engine/plugin/context/calendar fail-closed initialization.
- `macos/CalendarActions/EventKitActions.swift`: KEEP actual EventKit provider-side permission/fingerprint/revision/marker/recovery fences. Adapt only exact Rust-native DTO cutover. No weakening because old test source is removed.
- `macos/LocalModel/LocalModel.swift`: KEEP FoundationModels SDK and bounded native job/cancellation ABI. SPLIT prompt/learner semantic adaptation to Rust Inference/Knowledge; native provider translates an admitted typed request and SDK output, does not choose domain authority or fallback.
- Apple Contacts/Health/ScreenTime and platform acquisition brokers: KEEP actual OS adapters, privacy reduction, identity/deadline/size fences. Remove Dart-owned publication policy only after source-owned Rust replacement. ScreenTime stays unavailable without approved entitlement/report extension; no signing changes.
- Android/Windows scaffolding: KEEP dormant code and resources; no parity work or platform validation expansion. Android Vault key store is independent from Gateway credentials and remains intact.

## Exact client semantic partitions

These partitions close mechanically wrong destinations in the original lexical map. The JSON lists the individual declarations and concrete affected callers; this table supplies the readable owner boundary.

| Baseline family | One target responsibility and caller replacement |
|---|---|
| `NativeTransport`, errors, `_nativeWorkerMain` | Remain `app/runtime/native_transport.dart`; only special remote dispatch branches are deleted. `LocalContextTransport`/`NativeLocalContextGateway` become a restricted `infrastructure/native/native_context_host_transport.dart` capability; arbitrary publish/read/revoke methods are deleted. |
| `AgentController` | Conversation members → `application/conversation_controller.dart`; registry → Experts controller; source display → Connections controller; memory → Knowledge controller; proposals → Actions controller; explicit unlock/lock → Vault controller. Old cross-feature forwarding members disappear. `closeView` → Conversation `detachView`, never Vault lock or Stop. |
| `ConnectorScreen` | Cards/detail/build helpers remain widgets in the existing screen. Only loading/pending/explicit intent plumbing moves to `connections_controller.dart`. Fabricated source/native permission and catalog-read mutation branches are deleted. |
| Pairing/integration widgets | Pure display moves to `gateway_connection_panel.dart` / `integration_detail_panel.dart`; `_drive`, `_finishPairing`, provider-scope parsing and local polling policy are deleted. Canonical owner snapshots drive states/actions. |
| Calendar mixed gateway | `DayGateway.refreshDay` is product refresh; source selection uses Connections. `CalendarAdapter`, `EventKitCalendarAdapter`, OS access/settings and native `CalendarChoice` move to the restricted native host; no Day product gateway exposes OS/source authority. |
| Calendar/People/Wellbeing/Attention publishing | Flutter decorators/timers are deleted. Context `observations.rs`, `native_acquisition.rs` and `personal_sources.rs` own evidence/freshness; Day owns only refresh/mirror consumption. Valid local device identity alone moves to `local_device_identity.dart`. |
| Recipient consent/manual resume | `AgentConsentScope`, `AgentRecipientConsentTarget`, `PreparedInteractionResume` and routine `resumeInteraction` are deleted, not renamed. Safe source-processing review projection and durable linked-child observation replace them. |
| Actions | Interface/adapter → `actions_gateway.dart` / `app_wire_actions_gateway.dart`; safe DTO → `action_models.dart`. Public execute-after-approve/global writes-enabled and local collection workflow disappear; render `allowedActions` and observe owner settlement. `FloeApp` → `PersonalDayScreen` passes explicit ActionsGateway. |
| Experts | Interface/adapter → `experts_gateway.dart` / `app_wire_experts_gateway.dart`; DTO → `expert_models.dart`. The binding review resolves only in Experts; no package/definition authority or source permission is rebuilt by Dart. |
| Knowledge review | DTOs → `domain/memory_review.dart`; interface → `application/memory_gateway.dart`. The baseline presentation file has no widget and is deleted after all imports move. |

## Ordered execution after separate approval

There is no independent client implementation sequence. The following is the client projection of the main plan's exact stage IDs. P0 is documentation/static reading only; no source, test, manifest, generated-source or config changes, deletion, formatter, compiler, build, test, architecture checker, Keychain/provider operation, commit, push or remote write is authorized now.

| Central stage | Client obligations and prerequisite |
|---|---|
| **P0** | Finish/reconcile the read ledger, behavior inventory, exact contracts and semantic caller map. Record static baseline compile-shape issues without invoking a compiler. Private named constructor formals versus public caller names are existing evidence, not a claim that the starting suite compiles. Stop for approval. |
| **T0** | Only after separate approval and completed behavior-by-behavior documentation, remove confirmed legacy test-only files/ranges and exact mixed manifest references. Keep product previews/diagnostics and every fixture with a retained consumer. |
| **S1.1** | Establish canonical product DTOs, command/query identities, owner review storage, cancellation and failure contracts plus constructor/dependency plan. No reasoning-route switch. |
| **S1.2** | Complete trust-integrated Rust pairing, private credential readiness and commit/readback recovery; then switch Flutter pairing/management to ordinary Connections intents and remove HTTP/keychain/proof/token/special ABI paths together. No duplicate credential owner. |
| **S1.3** | Complete source-processing reviews, exact source/grant/native subject fences and mandatory Health-local transform with the exact Apple host/build source references. Only then is changed reasoning eligible. |
| **S1.4** | Cut common Gateway-primary/local-fallback planning and the Rust↔Go wire together. All Manager/Expert/Learner selection callers migrate; remove product profile/recipient choices. Flutter cannot trigger fallback. |
| **S1.5** | Complete Conversation-owned durable session/Run/Blocked/review/resume semantics, including exactly-once fresh linked child and no fabricated answer. Preserve actual settled attempts/coverage and explicit cancellation. |
| **S1.6** | Switch the complete Conversation/Connections Flutter, FFI, CLI and native host caller set to those completed owner contracts. Remove routine resolved-card Continue only after durable automatic resume exists. |
| **S1.7 / G1** | Close same-snapshot callers and obsolete aliases, inspect residual concepts, format the completed S1 unit once and compile the affected production boundaries. G1 is formatting plus compilation, not new tests or a full-build loop between smaller steps. |
| **S2.1** | Finish Go source/integration structural owners behind the already established product contracts. Flutter keeps only the ordinary owner adapter. |
| **S2.2** | Complete Connections/Access/Context/Day extraction: replace source reconciliation/acquisition/freshness/publication/mirror import with owner intents, remove Dart timers/publishers/sync queue, retain only the app-lifetime bounded OS request pump. |
| **S2.3** | Replace Experts toggle/binding fanout with atomic owner commands and persisted review references; Directory/Task/common Runtime remains the sole package path. |
| **S2.4** | Complete Knowledge and Actions owner extraction, explicit Actions dependency injection and one repository lookup contract; remove UI approval/execute/collect sequencing. Preserve new-system uncertain records and expose the accepted locked-Vault availability. Legacy development data may be discarded under the separately confirmed clean-profile cutover; never replay old external effects. |
| **S2.5** | Finish thin composition/transport and split feature/Vault contracts without inward presentation imports or forwarding facades. |
| **S2.6 / G2** | Migrate remaining callers and native build references, reconcile localization only when strings changed, and close all structure. Format at this completed boundary and run the required whole-structure compilation/build gate before any new tests. No unrelated Xcode/signing/Android expansion. |
| **S3 / G3** | Only after G2, design replacement tests against the accepted owners/contracts, then run the final behavioral and build qualification. Report static reasoning, compilation, builds and behavior separately. |

S1.1–S1.7 is one dependency-correct unit. A temporary compile break inside it is resolved at its boundary; it does not authorize a compatibility path, early insecure route switch or repeated tiny-step verification loops.

## Static exit evidence required

- Every frozen apps file has a complete read or explicit binary-inspection limit, and an explicit KEEP or affected mapping.
- Product Dart has no LocalServerClient, ServerCredentialStore, ServerConnection token, RemoteAccessGateway, signed pairing challenge/proof, direct Gateway HTTP or provider-secret form.
- Native transport/bindings contain only approved ordinary AppWire routes; no remote pairing/access escape hatch.
- Connection overview/load cannot establish/disconnect sources; only explicit mutation intents can.
- Calendar refresh policy/publication and Actions/Experts execution policy have exactly one Rust owner.
- No presentation interface imports flow inward from widget files.
- Observer timeout/disposal does not cancel owner Run/operation; explicit cancel still works; credential/result release follows confirmed owner commit/readback.
- No build/test execution is claimed in this planning pass. Existing test assertions are not acceptance criteria by default.

## Exact Rust owner destinations

The following paths are the agreed same-repository targets, not new crates:

| Removed Flutter semantic responsibility | Target file / owner |
|---|---|
| Pairing start/confirm/observe/commit/recovery | `crates/modules/connections/src/application/gateway_pairing.rs` |
| Pairing port / adapter | `crates/modules/connections/src/ports/gateway_pairing.rs` → `crates/adapters/providers/src/gateway/pairing.rs` |
| Gateway credential staging, secure commit and readback | `crates/adapters/providers/src/gateway/credentials.rs` |
| Integration authorization/lifecycle | `crates/modules/connections/src/application/integration_lifecycle.rs` through `ports/remote_integration.rs` |
| Source inventory/selection and exact revisions | `crates/modules/connections/src/application/source_connections.rs` |
| Source processing/Observe review and grant policy | `crates/modules/access/src/application/connection_review.rs`, `first_party_policy.rs` |
| Calendar refresh/mirror orchestration | `crates/modules/day/src/application/calendar_refresh.rs` plus existing Day mirror repository |
| Actions prepare/approve/execute/reconcile | `crates/modules/actions/src/application/operation.rs` |
| Atomic Expert installation/configuration and candidate selection | `crates/modules/experts/src/configuration.rs`, `ports/candidate_catalog.rs` |

All remaining native provider SDK code stays behind real adapter boundaries. `floe-app` only constructs and forwards the verified product intent to those owners. Canonical-contract appendix signatures are authoritative across Rust/FFI/client; the client methods above are the corresponding product-facing projection and have no independent revision/replay semantics.

## Mandatory Health privacy prerequisite within S1

This supersedes any interpretation that preserving the existing deterministic Health reducer is sufficient. The new mandatory source-owned transform lands before changed reasoning is enabled. It is neither an Agent role nor an optional S2 enhancement.

Create `apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift` as the single contract and provider-host source for the independent Health local transform:

- `HealthPrivacyTransformInput`: exactly optional finite `sleep_hours`, `steps`, `exercise_minutes`. At least one non-null signal. Reject nonfinite/negative values and sleep above 36 hours, steps above 1,000,000 or exercise above 2,160 minutes. Reject, never clamp or substitute missing data.
- `HealthPrivacyTransformOutput`: exactly `capacity` (`reduced|typical|strong|unknown`) and `recovery` (`needs_recovery|typical|recovered|unknown`). No prose, raw aggregate, sample, identifier, confidence, timing, evidence or model reasoning in model output. Source-owned deterministic envelope code adds its own permitted metadata after strict output validation; classification remains HighlySensitive.
- `HealthPrivacyTransformHost`: independent bounded job slot and FoundationModels session with `tools: []`, fixed 10-second deadline and 64-token output budget, with an 8 KiB native input envelope and 4 KiB response envelope hard cap. Do not log raw input/output. It cannot share `LocalModelHost.job`, call `nativeActionTools`, parse Conversation `run_frame`, produce Learner proposals or use Gateway fallback.
- `floe_health_privacy_transform(bytes: UnsafePointer<UInt8>?, length: Int) -> UnsafeMutablePointer<CChar>?` and `floe_health_privacy_transform_free(output: UnsafeMutablePointer<CChar>?)`: strict bounded C envelope with operation/request correlation, input only for start, typed completion/availability/failure and explicit cancellation/release semantics. Null/invalid/mismatched output fails closed; host unavailability, timeout, model refusal or malformed output cannot admit an old deterministic projection as replacement.
- Owner-facing `transform(input, scope)` returns only the strict coarse output or typed `unavailable|invalid_input|deadline_exceeded|cancelled|invalid_output|policy_denied` failure. Raw aggregate never enters Dart product state, general model prompts, journal or exported diagnostics. Provider SDK error text is not a product message.

Rewrite `HealthKitWellbeingProvider.readDerivedWellbeing` so its bounded native aggregate is submitted to the source-owned transform; preserve HealthKit read permission ambiguity and native subject continuity. `AppleWellbeingReducer.reduce` no longer defines capacity/recovery by hardcoded thresholds. Its envelope/timestamp/evidence work becomes source-owned post-transform code. `AppleContextChannel.readWellbeing` returns only the transformed source view; a successful authorization-request call alone never reports confirmed read authorization. Delete/disable the direct Dart `PublishingAppleContextGateway.readWellbeing` publication path in the same cutover.

Required caller/build wiring, all within S1 prerequisite:
1. Inject the concrete transform into the Rust Health source path; construction without a transform is not a valid production configuration.
2. Update macOS `build_native.sh` and iOS `build_native.sh` to compile the exact new source alongside the intended native dylib sources. Preserve weak-linked FoundationModels and platform target/deployment flags. The transform symbols may share the bundled library artifact, but runtime state and semantics remain independent.
3. Add the exact source to both Xcode `Build Swift Libraries.inputPaths`; preserve current output paths/signing/caching and make the native fingerprint include the new file content.
4. Update every direct `LocalModel.swift` compiler invocation outside apps (CLI, smoke, test fixture builder, native host validation) to include the exact contract source through the same-source builder. Parent tooling owner handles these references. No independently copied transform implementation.
5. At the approved completed-S1 compilation gate, verify transform source/ABI compilation; inspect full bundle/export/source-snapshot consistency at S2/G2 build closure. New semantic tests still wait for S3 as the main plan requires.

## Additional verified caller and disclosure defects

These are static source findings, not reproduced runtime failures:

- `FloeApp.calendarActions` is accepted/stored but not passed to `PersonalDayScreen`; that screen constructs an action controller only by testing whether its Day gateway implements `CalendarActionGateway`. Replace this duck-typed hidden dependency with explicit `ActionsGateway` injection through all three layers.
- `PersonalDayScreen` omits `AgentPanel.onOpenConnections`; interaction `openConnection` affordance can call an absent optional callback and then refresh. Route the owner-supplied source-review navigation action through explicit app navigation wiring.
- `macos/Runner/Info.plist` says Floe never edits/deletes existing events, while the native Actions adapter supports update/delete. Make OS disclosure and localized UI descriptions match the approved final product authority; do not weaken native checks or silently retain misleading copy.
- `AppDiagnostics.clear` is marked visibleForTesting but is called by product `deleteJournal`; it is not test-only. Debug feedback captures rendered text and PNG crops to temp storage/clipboard and is real product tooling. Preserve these sources and their privacy boundaries during suite removal.

## Native test and fixture removal scope for T0

Alongside the separately inventoried 87 Dart test/integration files, apps contains five native test source files with 33 declarations: Apple Contacts 11, Apple Health 8, ScreenTimeGate 5, macOS Runner 8, and one empty iOS template. Counts are source declarations, not executed or expanded parameterized case counts. Contacts has authorization-state/selection/budget matrices; ScreenTime has eight capability boundary cases; those cases must be individually documented before removal. Native provider model/permission/privacy/uncertainty assertions remain legacy evidence, not requirements by fiat.

Keep all five fixture JSONs until the complete cross-language consumer graph is closed: Apple Health `Tests/.../Fixtures/wellbeing_view.json` is also included by `crates/experts/builtin/tests/apple_wellbeing_projection.rs`; ScreenTime supported/unknown fixtures are included by `crates/experts/builtin/tests/personal_context.rs`. Android fixtures have Rust consumers as well. Package.swift test targets and Xcode RunnerTests graph/scheme references are mixed product files; future deletion removes exact test-only entries without touching product settings. `tool/mobile_vault_smoke.dart` is an operator diagnostic with real private-profile/Keychain effects and remains KEEP pending explicit tool classification, not a filename-based test deletion.

Final static evidence for this appendix: every one of 375 tracked apps files matches its baseline bytes; full-read ledger has 340 text complete and 35 binary-inspection complete with no pending entry; all 375 have explicit file disposition and symbol map reference. Binary inspection limitations and lexical-symbol limitations remain explicit. No test/build/compiler/formatter or live provider operation ran.
