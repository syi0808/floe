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

Experts are agents with identity and Task lifecycle, not provider-native Tools. Stable Task identity, assignment/eligibility, cancellation and A2A semantics belong to Experts. App holds no run-id endpoint authority: the root turn serves `TaskCoordinator` as its `DelegationPort` directly, and every endpoint invocation is self-sufficient — session, device, AgentContext, and output bound arrive in the explicit execution context, and the Manager delegation message is the Expert assignment. One canonical delegation request digest covers principal/parent linkage, selected agent + revision, message, context refs, and execution context; TaskId and InvocationKey stay stable identity fields checked exactly alongside it. Endpoint composition uses a constructor-injected saved-connection store (host keychain in production, fixed fixture in tests), admitted per execution against the invocation principal and context device id. Model execution uses `ExpertModelHost` and the shared `InferenceExecutor` under the Task's bounded child `ExecutionScope`; Experts express execution constraints, not provider routes.

Schedule has one production model-execution path: `ScheduleEndpoint` → `run_calendar_expert_endpoint` → `ExpertModelHost` → `InferenceExecutor`. Calendar source acquisition, exact dependency coverage, grant revalidation and atomic Task/Expert settlement remain with their existing owners. There is no parallel Schedule Session/agent-turn model runtime. Test-only persisted Delegation/ExpertResult fixtures support proposal inspection and publication tests without another model runtime. The separate live AgentFixture compatibility runtime remains product-boundary debt for Stage 3-D/3-E.

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
  -> App composition
  -> owner service
  -> canonical internal runtime
  -> adapters
```

Outer callers may express user intent, including an explicit user-selected model profile where the product exposes one. They must not carry raw bearer tokens, arbitrary model endpoints, resolved internal route bundles or Access policy flags.

That outer cutover is active Stage 3 work; it must not be pulled back into the frozen Stage 2 as compatibility ownership.
