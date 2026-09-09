# ADR 0018: Manager–Expert collaboration as A2A delegation

- **Status:** accepted
- **Date:** 2026-09-09
- **Amends:** ADR 0016 Expert invocation semantics and ADR 0017 context assembly

## Context

The S4 implementation advertises each Expert to the Manager as a provider-native
tool and routes the resulting capability call into an isolated Expert model loop.
This proves isolation and bounded execution, but gives the wrong abstraction:
an Expert appears to be a function with a predefined input/output schema rather
than another agent that applies domain judgment in its own context.

Floe needs Experts whose usefulness is not limited to an enumerated task list.
The Manager should know which domain advisors are active, decide when their
perspective is useful, delegate a goal in natural language and receive a focused
natural-language result. The transport must still carry identity, grants, budgets
and trace data without asking either model to express those controls in prose.

## Decision

### Expert is an agent, not a capability

Manager and Expert communicate through a first-class delegation boundary:

```text
User request
→ Manager context + Active Expert Index
→ A2A SendMessage(expertRef, natural-language Message)
→ isolated Expert context and agent loop
   → Expert-selected Tools and Playbooks
→ Task lifecycle + Artifact
→ Manager synthesis
```

Experts are removed from the Tool/Capability registry exposed to the Manager.
The internal model protocol gains a distinct delegation step. A provider adapter
may encode that step using a provider function-call primitive when no native agent
handoff exists, but it must normalize it as `delegate`, not register
`expert.<id>` as a Tool or route it through `CapabilityHost`.

Tools remain executable capabilities with argument/result schemas. Experts are
addressable agents with identity, description, an isolated context and their own
granted Tool/Playbook set.

### One semantic model, multiple bindings

Floe adopts the A2A 1.0 layering rather than requiring a separate server process:

```text
A2A-aligned data model     AgentCard, Message, Task, Artifact, Part
A2A operations             SendMessage, GetTask, CancelTask
A2A transport binding      InProcess now; HTTP/JSON-RPC, gRPC or HTTP+JSON later
```

The initial `InProcessA2ATransport` passes the canonical domain objects directly to
an Expert runtime in the same process. It does not serialize through loopback HTTP.
`RemoteA2ATransport` may later map the same operations to an official A2A binding.
The Manager and semantic operation do not change with the Expert's process location;
the router selects the configured binding.

Until Floe passes the relevant A2A conformance suite and exposes a standard remote
binding, this is described as **A2A-aligned internal runtime**, not an A2A-compliant
server. Protocol version and Floe extension versions are explicit so changes never
silently reinterpret persisted tasks.

### Expert discovery

Every Expert package supplies Manager-facing discovery metadata aligned with an
A2A Agent Card:

```text
FloeAgentCard {
  id
  packageVersion
  protocolVersion
  name
  description
  domainTags[]
  supportedInterfaces[]
  capabilities
  skills[]
}
```

`description` is a short natural-language explanation of the Expert's domain,
perspective and situations in which consultation is useful. Agent Card `skills` are
coarse discovery examples, not an exhaustive command list, workflow, Tool schema or
permission grant. Cards are versioned package content and are size-limited and
reviewed like other instructions.

Core never copies raw third-party card text directly into the prompt. Registry
validation enforces size, allowed fields and control-character rules, and the
assembler renders a quoted discovery-data projection at lower precedence than the
Manager Role. Card text cannot grant permissions or introduce executable policy.

At each Manager model call, Core resolves enabled installations, the current
Person's active assignments and invocation eligibility. The Context Assembler adds
only those entries to a bounded **Active Expert Index** under scoped instructions.
Disabled, unavailable, incompatible or unauthorized Experts are omitted. The
manifest records the descriptor and assignment revisions used.

The Manager Role contains stable delegation principles only: consult an Expert when
its independent domain judgment can materially improve the result; send sufficient
goal, context, constraints and desired outcome; do not delegate mechanically; and
retain ownership of the user-facing answer. Expert-specific descriptions never get
copied into the static Manager Role.

### Natural-language A2A messages

The request and clarification bodies are A2A Messages whose primary Part is natural
language. Final work products are Artifacts; transport controls remain typed and
host-owned:

```text
A2ADelegation {
  recipientExpertRef,
  message: Message { messageId, contextId, taskId?, parts[] },
  configuration,
  metadata: FloeDelegationMetadata {
    parentTurnId, contextProjectionRef,
    deadline, budgetRef, traceRef
  }
}

Task {
  id, contextId, status,
  history[],
  artifacts[]
}
```

Text Parts are not parsed as command enums such as `find_focus_time` and do not have
a domain-specific response schema. An Artifact normally contains the Expert's
natural-language result plus typed Data Parts that reference evidence or mutation
candidates. Floe-specific fields use a versioned A2A extension URI rather than
changing core A2A objects. Failure, cancellation and budget exhaustion are Task
statuses, not fabricated Expert replies.

The Expert receives the Manager's delegation message as its current assignment,
not the full Manager conversation. Core separately assembles the least-privilege
User Model, Memory, View and session projections granted to that Expert. Expert
Tools are resolved for the child context and are never inherited merely because
the Manager can use them.

### Conversation and lifecycle

A delegation creates an A2A Task and child conversation scope linked to its parent
Manager turn by `contextId` and Floe metadata.
Expert Tool calls, Playbook loads and intermediate messages remain in that scope.
Only terminal Artifacts and permitted typed references return to the Manager
conversation. Messages remain available for clarification and status communication;
they are not used to disguise final results. Traces preserve correlation without
exposing private reasoning.

The initial runtime implements `SendMessage`, `GetTask` and `CancelTask`, with one
request and one terminal Artifact in the common path. The contract leaves room for
multi-message clarification, streaming, resubscription and remote agents. Recursive
delegation is deny-by-default until depth, cycle, authority and cost policies are
defined.

## Context assembly amendment

The Manager scoped instruction layer becomes:

```text
scoped_instructions
├─ turn purpose and response contract
├─ Active Expert Index
├─ guidance for Tools actually granted
├─ eligible Playbook index
└─ loaded Playbook bodies
```

Expert context uses its own Role and package revision, the natural-language
delegation assignment, its own granted Tools and Playbooks, and only authorized
data projections. An Expert does not receive the Active Expert Index unless future
policy explicitly permits further delegation.

## Consequences

- The Manager chooses domain collaborators by meaning rather than matching a
  function schema.
- Expert packages must provide concise, trustworthy discovery descriptions.
- Natural-language flexibility increases, while typed out-of-band references retain
  evidence, permission and action safety.
- The current `expert.schedule` capability path becomes a migration implementation,
  not the target contract.
- Provider function calling may remain an adapter detail, but cannot define the
  product's Agent-to-Agent semantics.

## Initial migration

1. Add A2A-aligned Agent Card metadata and assemble the Active Expert Index.
2. Add canonical Message, Task, Artifact and Part records plus a distinct model step.
3. Add an A2A router and `InProcessA2ATransport` over the existing Expert runtime.
4. Replace domain command/result payloads with natural-language Parts plus typed
   evidence and proposal Data Parts.
5. Remove Manager-visible `expert.*` Tool descriptors after compatibility tests pass.

## References

- [A2A Protocol 1.0 specification](https://a2a-protocol.org/v1.0.0/specification/)
- [A2A Protocol repository](https://github.com/a2aproject/A2A)
