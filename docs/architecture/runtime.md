# Runtime architecture

This document describes the canonical runtime and current product boundaries.

## General Conversation

Conversation owns the durable Session/root-Run lifecycle and projects the state required by the role-neutral Agent Runtime.

### Model path

Canonical ownership:

```text
Conversation
  -> Conversation/Context model projection
  -> AuthorizedModelProjection
  -> Agent Runtime Engine
  -> Inference ModelPort
  -> InferenceService
  -> Access dispatch admission/fence
  -> PreparedModelTransport with consumed dispatch target
```

Responsibilities do not collapse across this chain:

- Conversation owns Run/transcript/continuation and durable user interactions (origin, reviewed target, lifecycle, decision intent, resume linkage).
- Context owns source-backed projection, coverage, provenance and freshness.
- Inference owns profile/route/attempt/usage and transport retry/fallback policy.
- Access owns exact-recipient processing/dispatch/release authority, including the contextual recipient-consent store.
- Provider adapters resolve private credentials and execute transport.

A dispatch the fence cannot admit on a recoverable consent case blocks as a typed expected completion, never a forged answer or a silent reroute: Inference returns the exact requirement, the Engine journals the blocked attempt, and Conversation publishes the durable card under the attempted origin and completes with the deterministic limitation. Missing consent never triggers hidden fallback to another recipient; hard denials (prohibited classes, foreign identity, no lineage, transport failure) fail closed without a card. Approved consent unblocks only the exact reviewed dispatch: recipient, profile, purpose, consumer, classes, scopes, lineage, pairing and device all bind the consent id.

Product Run snapshots project model-attempt and delegated-Task references from Conversation's
durable intent journal. The refs remain derived read data: FFI does not manufacture them, and
Inference and Experts retain ownership of attempt and Task semantics.

`InferenceService : ModelPort` is the General Conversation model path. App and Provider composition do not own model semantics. The prepared provider receives a target derived only from the consumed Access fence; an external server request carries `allow_external=true` and the exact `expected_recipient`, while local requests carry neither external approval nor recipient.

### Host composition

`ConversationTurnRequest` carries only session/revision, text, profile intent, continuation/retry intent and the verified device identity. It has no credential source or lookup semantics.

The Vault worker owns one shared `CurrentSavedConnectionStore`, cloned into the open Vault and its root, built-in Expert and Schedule compositions. Production binds the host keychain; tests inject a fixed or mutable store at worker construction. Provider-owned `RootModelProvider::from_current_connection[_scoped]` and `ServerSourceClient::from_current_connection` load and admit the current connection against the verified person/device, returning opaque capabilities. App neither materializes saved credentials nor selects model placement. Recipient authority composes the Access-owned consent store with pairing admission over this store plus a clock, reloaded at every fence rather than trusting the prepared transport's snapshot. Saved pairing credentials contain no recipient approval.

`InferenceAvailability` observes execution classes for a purpose/consumer through the same candidate rules used for execution. Experts owns card eligibility against that non-secret observation. Availability is not dispatch authorization: actual execution still runs the canonical Inference/Access checks. Remote source capability is observed independently of model availability; source catalogs remain lazy and never participate in profile discovery or selection.

### Tool path

Canonical ownership:

```text
Agent Runtime Engine
  -> ToolPort (App trusted publication)
  -> ContextToolService : typed source outcomes
  -> Context + Access
  -> authorized source adapter
  -> ToolResult { coverage, artifacts, issue } + durable UserInteractionRef
```

Tool availability is a source/authority property, not a model-route property. The Tool result carries its own evidence/coverage rather than relying on an App-side side channel. Context returns owner-produced outcomes (ready, temporarily unavailable, or review-required blockers); only the App boundary publishes requirements as durable interactions under the admitted Tool origin and settles the blocked result with safe refs. Experts follow the same shape through the common delegation endpoint: host-captured blockers publish under the Task origin, and a deterministic blocked-domain report completes the Task with no conclusion and no model-proposed requirement. A blocked expert model dispatch follows the same shape: the delegation lineage binds the dispatch, the host-captured requirement publishes under the Task origin, and the expert reports a blocked-domain judgment.

The production Context Tool path is canonical, alongside the canonical Delegation path through `TaskCoordinator : DelegationPort`.

### Expert delegation

Canonical ownership:

```text
App run_general_turn
  -> DelegationExecutionContext (secret-free host context)
  -> Conversation TurnRequest (runtime field, never identity)
  -> Manager Engine
  -> Delegate step + batch-bound exact context
  -> TaskCoordinator : DelegationPort (direct; no bridge)
  -> Directory endpoint resolution
  -> EndpointInvocation { DelegationRequest, canonical request digest }
  -> isolated Expert runtime
  -> Task terminal result / Artifact
  -> Manager synthesis
```

Experts are agents with identity and Task lifecycle, not provider-native Tools. Stable Task identity, assignment/eligibility, cancellation and A2A semantics belong to Experts. App holds no run-id endpoint authority: the root turn serves `TaskCoordinator` as its `DelegationPort` directly, and every endpoint invocation is self-sufficient — session, device, AgentContext, and output bound arrive in the explicit execution context, and the Manager delegation message is the Expert assignment. One canonical delegation request digest covers principal/parent linkage, selected agent + revision, message, context refs, and execution context; TaskId and InvocationKey stay stable identity fields checked exactly alongside it. Endpoint composition shares the host-scoped current store described above; provider adapters admit it per execution against the invocation principal and context device id. Model execution uses `ExpertModelHost` and the shared `InferenceExecutor` under the Task's bounded child `ExecutionScope`; Experts express execution constraints, not provider routes.

Directory publication replaces one owner's complete endpoint set under one write lock, preserving unrelated registrations and rejecting cross-owner collisions before mutation. A Task executes the endpoint resolved before admission rather than resolving it again after the Working transition, so a later publication refresh cannot reroute that execution.

Schedule uses the same production model-execution path as every shipped Expert: `RegisteredExpertEndpoint` → generic bundle host → `ExpertModelHost` → `InferenceExecutor`. The shipped bundle owns its `ExpertRegistration` collection (manifest plus runner); App builds dispatch from that collection instead of listing individual Experts. The Experts-owned manifest defines the package and definition revision, bounded prompt/result identities and declared source requirements. Generic Registry installation creates only Expert installations and assignments, with an exact manifest-set digest in the durable install receipt; it never installs a synthetic Tool. The common runtime prompt role is `Expert`; each package's Role source, revision and content retain its distinct prompt identity. Delegated model inference uses the `experts.delegated` consumer for profile observation and exact-recipient consent, while source reads keep the exact admitted package consumer such as `floe.builtin.schedule`. Existing consent for `experts.builtin` does not authorize the new consumer.

The App-private built-in execution seam admits an exact Registry instance, assignment, installation, package and definition identity, pins the selected endpoint and card, captures source/model dependencies and blockers, and binds exact coverage to package-owned typed artifacts before producing `ExpertReport` directly. A bounded human-readable result and those covered artifacts settle into the canonical `TaskSnapshot`; neither the Registry nor A2A reconstructs domain results from JSON. Completed A2A and Conversation projections carry the direct Task result and safe artifacts without internal `ContextDependency` coverage. Vault Task schema 3 persists the exact internal admission identity and a unique invocation key, without an old-row decoder; an old development profile must be explicitly recreated. Settlement loads the current Registry under the Task transaction, verifies that exact admitted identity and private-state revision, and updates only that assignment's private state. Unrelated Registry configuration changes are preserved; a same-assignment state race conflicts. There is no Schedule-specific endpoint or settlement owner. The cross-language delegation fixture is generated through this report-to-Task-to-product path.

Context's declared-source resolver accepts a key only when it occurs in the admitted manifest, maps its capability to the current source reader, bounds the query and returns payload plus every contributor dependency. The common Expert host exposes that generic read, model, dependency capture and settlement surface; package code decodes its own typed views. Context owns remote and native Calendar View acquisition and dependency reauthorization. Remote reads bind a signed source preview to an exact DataAccessGrant resource. Native reads bind the current connection, EventKit subject stamp, local-only grant, bounded observation and post-read recheck, then retain dependency evidence for later model/history admission. Product composition derives first-party remote, Calendar and native personal Observe consumers from shipped bundle manifests, never from arbitrary installed packages. Context reads under each actual requesting package identity. Calendar reads distinguish ready, temporary unavailability and review-required outcomes; the current connection, resource selection and authority bind review requirements where available, without authorizing a read. Native OS permission denial or a grant that excludes the requesting consumer requires review. Optional Experts record missing Calendar context rather than treating it as empty evidence. Persistent selected-source binding is not implemented yet; the current source selection is replaced in checkpoint 04.

The Expert Registry is source-independent: card availability is Expert package/installation/assignment state only, and no Registry record authorizes a source read. Disabled assignments stay disabled across default startup ensure. Registry schema 2 is a direct cutover without an old-data decoder; an old development profile must be explicitly recreated rather than silently reset. Context/Access decide source reads at invocation time. Calendar grants select DataAccessGrant by current connection/source identity plus exact consumer, with CalendarGrantPolicy carrying consumer-policy authority and the reviewed native subject. Package artifact semantics belong to the package; a generic Task result is not a domain schema or an Action proposal. Schedule proposes a bounded interval draft, and trusted settlement binds exactly one captured Calendar dependency to an Actions-owned proposal artifact. The artifact is review evidence, not execution authority.

Schedule request planning is provider-independent. It selects only the bounded Calendar interval and domain intent; Context resolves native or remote acquisition, while captured dependencies and Access/Inference own processing and recipient admission for model execution.

### Durable interactions

Conversation durably records recoverable owner requirements as interactions: the journal-verified origin (Tool call, Delegation Task or Model attempt), the owner-produced requirement, and the immutable reviewed target (exact connection/device/source, resources, capability bundle, consumer/purpose, source revision, grant expectation including expected absence, policy authority and App-produced prospective policy fingerprint). Publication identity derives deterministically from origin plus canonical digests, so crash replay settles the same row; decisions bind the reviewed digest through compare-and-swap with identical-command rejoin. Lifecycle is Pending to Resolving to Resolved, with Denied, Cancelled, Superseded and Expired as the other terminal states; the original Run completes with its limitation instead of waiting. Session messages, transcripts and model-safe artifacts carry only the opaque interaction reference. Authority stays with Access, Connections and the provider/native owners, which re-verify current state and prospective policy when a decision resolves; a linked resume is a fresh Run, not budget continuation. It admits through one atomically bound per-origin slot, dispatches under origin-carried lineage so origin-reviewed grants still scope it, and re-scopes any fresh blockage review to the attempting Run. Flutter decides through versioned commands carrying only the decision and the reviewed digest; snapshots carry safe review identity plus backend-projected actions, never authority. See [ADR 0030](../decisions/0030-durable-interaction-and-linked-resume.md).

Budget Continue instead replays the exact validated batch and cursor without model recall. The batch's stored `projection_coverage` is its source provenance: Conversation reauthorizes every recorded dependency against the current resolver before a pending step executes and again before terminal output release. Stale or Unknown coverage blocks stored steps and answers; message shape, Expert identity and capability names never substitute for provenance.

## Consequential actions

Model or Expert output may produce an Action proposal. It never directly authorizes the external effect.

```text
Intelligence
  -> Action proposal
  -> Actions policy/review
  -> exact authority + provider preconditions
  -> durable execution intent
  -> adapter write
  -> receipt or uncertain outcome
  -> reconciliation
```

See [Authority and recovery](authority-recovery.md).

## Product boundary

The final outer path is:

```text
Flutter / native / server caller
  -> protocol / FFI intent conversion
  -> AppHost request admission / verified CallerContext
  -> typed App service / composition
  -> owner service
  -> canonical internal runtime
  -> adapters
```

Outer callers may express user intent, including an explicit user-selected model profile where the product exposes one. They must not carry saved bearer tokens, arbitrary model endpoints, resolved internal route bundles or Access policy flags. Before a connection is saved, pairing alone may supply a bounded loopback setup endpoint and signed challenge/polling evidence. Only an approved pairing result may return the newly issued token for secure persistence; its Debug/diagnostic representation is redacted.

Local AppWire commands and queries pass through `AppHost::request`, `HostRequest` and typed owner services for Vault lifecycle, Conversation sessions/turns, Experts, Access, Knowledge, Connections, Day, Actions and Context. Identity and runtime epoch come from `CallerContext`; neither requests nor nested native completion values can supply person/device authority. Product open requires the verified local profile and fails before database side effects when identity is unavailable. There is no unverified host fallback, `FloeHandle::services()`, `legacy_services()` or public concrete AppComposition getter.

The existing schema-2 `command_v2/query_v2` carry owner-prefixed intents, not worker action maps. `events_v2` remains Conversation event delivery with principal filtering. FFI strictly validates/converts DTOs, admits the host request, calls a typed service and serializes values; it does not load credentials or decide authorization, routing, eligibility or Action policy. App's private worker/job machinery remains where owners require non-blocking work.

Async local results bind the exact intent and originating Person/device/runtime epoch/owner. Observation and accepted-decode-before-release retain the same operation identity across uncertain acknowledgements, and do not imply cancellation. Flutter has separate owner gateways and mechanical correlation only, not a generic request bus. Day preserves Person ownership and repository CAS. Actions selects the authoritative encrypted/core repository inside App, rejects agent-origin mirrors, and preserves durable intent and lookup-only uncertain-write recovery without client fallback. Context injects admitted identity while retaining native request/epoch/fingerprint evidence; broker disposal is not Run cancellation.

App publishes a terminal Conversation Run event only after the owning turn job is marked complete.
The read-only Conversation Session get operation may also overlap the narrow interval after the
Run and Session commit but before that job returns; mutating Session operations remain serialized.

Pairing and remote Access have separate owner-oriented protocol envelopes and typed `RemotePairingCommands` / `RemoteAccessCommands` services. Their FFI entry points admit each request through `AppHost::request(request_id)`; FFI validates/converts structure and never supplies caller identity or decides authority. Pairing derives Person/device from `CallerContext` and client identity from the exact pending pairing ID. Connections and the key holder validate challenge, issuer and report identity.

After pairing, producer inspection/enrollment and Calendar/View grant operations receive only review/source/grant intent. Provider-owned `RemoteAuthorityEndpoint::from_current_connection` and `ServerSourceClient::from_current_connection` reload the shared current store and admit the verified Person/device before preparing transport. App sees non-secret admitted client identity, not raw saved credentials. Access retains producer pinning and exact source/revision/provider/recipient/grant checks; source catalogs stay lazy and separate from model discovery.

Remote operations use bounded worker jobs. Their result observations and completed-job release are bound to the originating Person, device, runtime epoch and owner domain; they do not cancel a Run or Task. There is no generic AgentVault product polling/stop/release path. While a job is retained, a changed command cannot reuse its operation ID. Approved credentials are held only in bounded results until release/eviction and secure persistence by the caller.

Flutter binds the two remote owner entry points through the app-lifetime `NativeTransport`, sharing schema-2 envelope correlation with Conversation. Its separate pairing and Access gateways centralize bounded submit/read/release without generic worker polling or implicit cancellation. Pairing alone supplies a setup target; Access requests contain no saved endpoint or bearer. Approved results remain retained until the client writes and re-reads the exact connection through the `app.floe.local-server` / `connection-v1` Keychain bridge. Uncertain acknowledgements retain the same operation ID rather than reissuing mutations. Native code only stores bytes and opens the loopback management page; it does not decide Access or model policy.

The local C ABI consists only of open, command/query/events v2, remote pairing/access v2, protocol version and string/handle free. Old Day/Actions/LocalContext/AgentVault/fixture entry points and their wire envelopes are deleted, not aliased. Conversation sessions use their canonical admitted owner service. Internal `WorkerAction::ConversationTurn` and other private owner machinery remain.

ConversationService, Engine, Inference, prepared provider transports and Session storage form one runtime path. Live shared Expert host composition is named `expert_host`. Flutter tests use pure product-interface fakes; native smoke uses canonical Inference. App wire 2, protocol 1 and Conversation storage 7 remain unchanged; old local profiles/binaries are not implied compatible.

## Local Go server

`server/internal/application` composes the local Console, durable disk/Vault adapters and concrete connector runtimes. Authorization owns the authority Engine, producer signing/validation, source admission/read/release semantics and bounded admission records. Its source operations receive current-source fencing and provider-identity preflight capabilities from Application. Connections owns domain records, scope validation, listing and detached registry snapshots; Application owns durable connector cleanup and concrete runtime construction. Pairing owns the pending operation, proof/identity checks, confirmation and approval sequencing, calling Application only for durable client/issuer settlement.

`transport/http` owns routing, strict JSON decoding, headers, management sessions and response mapping. Application injects typed management, connector, pairing and source capabilities; HTTP neither reaches Console state nor decides authority. The transport-independent `operation` result values carry only outcomes and failure categories. No owner imports Application, and no Console/state alias crosses this boundary. Synthetic management model probes also run through Inference's existing exact-recipient fence; they do not exempt requests from consent. Rust Access remains the local authority owner—the Go server is its remote producer, not a replacement authority policy engine for the client.
