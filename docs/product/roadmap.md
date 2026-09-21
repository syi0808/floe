# Capability roadmap

This is a **capability direction**, not an implementation progress board or a sequential refactoring gate. Active engineering order belongs to [Stage 3](../refactoring/stage-3.md); [Stage 2](../refactoring/stage-2.md) is complete.

## Current product foundation

Floe's repository already contains substantial foundations for:

- Personal Day / Day Canvas data;
- connected Calendar and other source adapters;
- one Manager with bounded Expert delegation;
- governed action review/execution;
- governed Memory/Playbook concepts;
- local and remote model adapters;
- device/server connection and authority primitives.

Whether a specific production path is fully canonical or live-validated is **not** asserted by this roadmap.

## Capability directions

### Calm personal day

Continue to make Timeline, Tasks, Notes, commitments and relevant interventions feel like one day rather than separate productivity dashboards.

### Connected context

Expand provider and device coverage through common Views and authority semantics rather than provider-specific Agent prompts. Prioritize context that materially helps time, commitments, people, feasibility, capacity and safe execution.

### Durable personal memory

Improve evidence-backed long-term Memory, people/relationship context, correction/forgetting, retrieval and user inspection without turning inferred context into unquestioned fact.

### Voice and presence

Build voice sessions over the same Manager/Session/Expert/Action core. Add platform-appropriate invocation and, where feasible, local wake detection without making always-on recording a product assumption.

### Cross-device

Add authenticated device capability/presence, bounded context queries, selective synchronization, revocation and handoff while respecting source ownership and sensitive-local processing.

### Hosted and self-hosted operation

Evolve the Go control plane, managed/self-host OAuth, sync/relay and administration without giving the server implicit authority over device-private data.

### Expert ecosystem

Stabilize public Expert contracts, packaging, permissions, testing and sandboxing before marketplace breadth. Extensions must preserve one Manager experience and host-governed data/action authority.

### Proactive assistance

Background source changes may produce bounded situations. Manager decides whether to stay silent, defer, notify, speak or escalate to UI. Intervention quality and interruption cost matter more than notification volume.

## Roadmap rule

A future capability does not become accepted architecture merely by appearing here. Durable boundary changes require an ADR; current implementation structure is documented under [Architecture](../architecture/README.md).
