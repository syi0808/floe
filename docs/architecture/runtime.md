# Runtime architecture

This document describes the canonical internal runtime and the remaining transition boundaries. Progress is not tracked here; see [Stage 2](../refactoring/stage-2.md).

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

During Stage 2, canonical `InferenceService : ModelPort` production cutover is still active work. A legacy caller does not make App or Provider the semantic owner.

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

The production Context Tool cutover belongs to Stage 2-B.3.

### Expert delegation

Canonical ownership:

```text
Manager Engine
  -> Delegate step
  -> Experts Task / endpoint dispatch
  -> isolated Expert runtime
  -> Task terminal result / Artifact
  -> Manager synthesis
```

Experts are agents with identity and Task lifecycle, not provider-native Tools. Stable Task identity, assignment/eligibility, cancellation and A2A semantics belong to Experts. Stage 2-C removes the remaining internal legacy delegation bridge and staged App endpoint context.

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

That outer cutover is Stage 3 work; it must not be pulled back into Stage 2 as compatibility ownership.
