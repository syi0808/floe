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
  -> Provider model transport
```

Responsibilities do not collapse across this chain:

- Conversation owns Run/transcript/continuation.
- Context owns source-backed projection, coverage, provenance and freshness.
- Inference owns profile/route/attempt/usage and transport retry/fallback policy.
- Access owns exact-recipient processing/dispatch/release authority.
- Provider adapters resolve private credentials and execute transport.

Product Run snapshots project model-attempt and delegated-Task references from Conversation's
durable intent journal. The refs remain derived read data: FFI does not manufacture them, and
Inference and Experts retain ownership of attempt and Task semantics.

The canonical `InferenceService : ModelPort` production cutover is complete and is the current General Conversation reality. App and Provider composition do not own model semantics.

### Host composition

`ConversationTurnRequest` carries only session/revision, text, profile intent, continuation/retry intent and the verified device identity. It has no credential source or lookup semantics.

The Vault worker owns one shared `CurrentSavedConnectionStore`, cloned into the open Vault and its root, built-in Expert and Schedule compositions. Production binds the host keychain; tests inject a fixed or mutable store at worker construction. Provider-owned `RootModelProvider::from_current_connection[_scoped]` and `ServerSourceClient::from_current_connection` load and admit the current connection against the verified person/device, returning opaque capabilities. App neither materializes saved credentials nor selects model placement. Recipient authority shares this store and reloads it at every fence rather than trusting the prepared transport's snapshot.

`InferenceAvailability` observes execution classes for a purpose/consumer through the same candidate rules used for execution. Experts owns card eligibility against that non-secret observation. Availability is not dispatch authorization: actual execution still runs the canonical Inference/Access checks. Remote source capability is observed independently of model availability; source catalogs remain lazy and never participate in profile discovery or selection.

### Tool path

Canonical ownership:

```text
Agent Runtime Engine
  -> ToolPort
  -> ContextToolService
  -> Context + Access
  -> authorized source adapter
  -> ToolResult { coverage, artifacts, issue }
```

Tool availability is a source/authority property, not a model-route property. The Tool result carries its own evidence/coverage rather than relying on an App-side side channel.

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

Schedule has one production model-execution path: `ScheduleEndpoint` → `run_calendar_expert_endpoint` → `ExpertModelHost` → `InferenceExecutor`. Calendar source acquisition, exact dependency coverage, grant revalidation and atomic Task/Expert settlement remain with their existing owners. There is no parallel Schedule Session/agent-turn model runtime. Test-only persisted Delegation/ExpertResult fixtures support proposal inspection and publication tests without another model runtime. The synthetic App AgentFixture runtime and its ABI are deleted. Test-only owner fixtures do not keep a second Conversation runtime alive.

Context exposes a signed remote Calendar View read and dependency reauthorization under a selected DataAccessGrant resource, with Access checking the Calendar-specific local-only scope and Vault resolving its consumer-policy binding. The common built-in Expert host injects a Calendar reader that uses that Context read for each currently selected remote resource and records its exact dependency. Native Calendar acquisition and the separate Schedule endpoint still use their existing App paths pending convergence.

Schedule planning receives remote **source** availability from the composed capability, not a model route. Google/Microsoft enumeration remains lazy and provider-specific; Access owns Calendar authorization, while Schedule owns the requirement that freshly remote-acquired Calendar data uses DeviceOnly reasoning.

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

The legacy Conversation AgentRuntime/ModelRunner/TransportModelRunner branch and its envelope/recovery adapters are deleted after caller-zero proof. Canonical ConversationService, Engine, Inference, provider transports and Session storage remain. Live shared Expert host composition is named `expert_host`, with no compatibility re-export. Flutter tests use pure product-interface fakes; native smoke uses direct provider transport or canonical Inference, never a synthetic production Conversation runtime. App wire 2, protocol 1 and Conversation storage 7 remain unchanged; old local profiles/binaries are not implied compatible.

## Local Go server

`server/internal/application` composes the local Console, durable disk/Vault adapters and concrete connector runtimes. Authorization owns the authority Engine, producer signing/validation, source admission/read/release semantics and bounded admission records. Its source operations receive current-source fencing and provider-identity preflight capabilities from Application. Connections owns domain records, scope validation, listing and detached registry snapshots; Application owns durable connector cleanup and concrete runtime construction. Pairing owns the pending operation, proof/identity checks, confirmation and approval sequencing, calling Application only for durable client/issuer settlement.

`transport/http` owns routing, strict JSON decoding, headers, management sessions and response mapping. Application injects typed management, connector, pairing and source capabilities; HTTP neither reaches Console state nor decides authority. The transport-independent `operation` result values carry only outcomes and failure categories. No owner imports Application, and no Console/state alias crosses this boundary. Synthetic management model probes also run through Inference's existing exact-recipient fence; they do not exempt requests from consent. Rust Access remains the local authority owner—the Go server is its remote producer, not a replacement authority policy engine for the client.
