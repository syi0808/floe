# Runtime architecture

This document describes the canonical internal runtime and the remaining transition boundaries. Progress is not tracked here; see [Stage 3](../refactoring/stage-3.md).

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

The canonical `InferenceService : ModelPort` production cutover is complete and is the current General Conversation reality. A legacy caller does not make App or Provider the semantic owner.

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

The production Context Tool cutover completed in Stage 2-B.3 and is the current Tool reality, alongside the canonical Delegation path through `TaskCoordinator : DelegationPort`.

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

Schedule has one production model-execution path: `ScheduleEndpoint` → `run_calendar_expert_endpoint` → `ExpertModelHost` → `InferenceExecutor`. Calendar source acquisition, exact dependency coverage, grant revalidation and atomic Task/Expert settlement remain with their existing owners. There is no parallel Schedule Session/agent-turn model runtime. Test-only persisted Delegation/ExpertResult fixtures support proposal inspection and publication tests without another model runtime. The separate live AgentFixture compatibility runtime remains product-boundary debt for Stage 3-D/3-E.

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

Canonical AppWire commands, queries and events pass through `HostRequest` and the typed `ConversationCommands`, `ConversationQueries` and `ConversationEvents` services. Identity and runtime epoch come from `CallerContext`; read requests cannot override person/device. AppComposition delegates to private worker/query and bounded event-buffer machinery, and event payloads are filtered to the caller principal. FFI only converts service/owner values to unchanged wire DTOs. Concrete AppComposition getters and `legacy_services()` remain only for live old ABI callers, not canonical AppWire.

Pairing and remote Access have separate owner-oriented protocol envelopes and typed `RemotePairingCommands` / `RemoteAccessCommands` services. Their FFI entry points admit each request through `AppHost::request(request_id)`; FFI validates/converts structure and never supplies caller identity or decides authority. Pairing derives Person/device from `CallerContext` and client identity from the exact pending pairing ID. Connections and the key holder validate challenge, issuer and report identity.

After pairing, producer inspection/enrollment and Calendar/View grant operations receive only review/source/grant intent. Provider-owned `RemoteAuthorityEndpoint::from_current_connection` and `ServerSourceClient::from_current_connection` reload the shared current store and admit the verified Person/device before preparing transport. App sees non-secret admitted client identity, not raw saved credentials. Access retains producer pinning and exact source/revision/provider/recipient/grant checks; source catalogs stay lazy and separate from model discovery.

Remote operations use bounded worker jobs. Their result observations and completed-job release are bound to the originating Person, device, runtime epoch and owner domain; they do not cancel a Run or Task. The generic AgentVault polling/stop/release path cannot access these jobs. While a job is retained, a changed command cannot reuse its operation ID. Approved credentials are held only in bounded results until release/eviction and secure persistence by the caller.

The old AgentVault ConversationTurn wire and route-shaped pairing/authority/grant inputs are not accepted. Internal `WorkerAction::ConversationTurn` remains the canonical runtime machinery; unrelated AgentVault/AgentFixture callers and ConversationSession remain live compatibility. Flutter's old remote senders are deliberately not a second Rust contract; their cutover belongs to Stage 3. That outer cutover must not be pulled back into frozen Stage 2 as compatibility ownership.
