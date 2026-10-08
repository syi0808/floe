# ADR 0035: Role-neutral Conversation Core and separate A2A boundary

- **Status:** accepted
- **Date:** 2026-10-08
- **Amends:** [ADR 0018](0018-manager-expert-a2a-delegation.md)
- **Extends:** [ADR 0033](0033-run-scoped-agent-environment-and-grounded-manager.md)

## Context

Floe needs explicit contracts for agent conversation identity, message admission,
replay and execution sequencing. The current `floe-conversation` module owns the
Manager-facing Session and root Run path and depends on `floe-experts`. Experts
already own their separate Task lifecycle and execution endpoint. A shared
conversation transition layer must not introduce an Experts-to-Conversation
cycle or imply that either existing production path has migrated.

A2A wire and peer identity are also separate from host conversation and Task
ownership. The existing `floe-agent-contract` AgentCard, AgentMessage, Task and
Artifact contracts remain unchanged. A2A peer identifiers cannot become local
Conversation, Message, Command, Run or Task identity by parsing or coincidence.

## Decision

### Conversation values and transitions

`floe-conversation-contract` contains role-neutral values only: person and agent
assignment identity, conversation and branch identity, message identity,
transcript references, admission targets and receipts, Run-to-Task links and
checkpoint values. It owns no repository, persistence port, product role,
learning behavior or authorization policy.

`floe-conversation-core` owns the pure admission and sequencing rules and the
port through which a future host store applies them. The accepted transitions
are:

- A new goal selects a fresh conversation and branch. Continuation names an
  existing reference explicitly. The target-selection helper continues only
  when the complete pinned agent identity, including definition revision,
  matches; an identity or definition revision change defaults to a new isolated
  conversation.
- A message ID is scoped to its conversation. The same message ID, body,
  authenticated origin, Task association and host evidence commitment replays
  its receipt. Reusing that ID with changed delivery content or evidence is a
  conflict. The role-neutral message stores only a content-addressed evidence
  reference; the owning host domain stores the evidence bytes. Command IDs
  remain a separate idempotency key.
- Each admitted message receives a transcript sequence. A message admitted
  while a writer is active joins the FIFO inbox. A conversation has at most one
  active Run writer; only that Run can complete its claim before the next queued
  message is claimed.
- A Run ID identifies one execution segment. A Task ID identifies host-owned
  Task lifecycle and may link more than one Run. These values reuse the existing
  `RunId`, `CommandId` and `TaskId` types. Existing Run journals and V1 Task
  receipts remain immutable.
- A checkpoint names an exact conversation branch and completed transcript
  prefix and carries that prefix's digest. It cannot cover the active writer or
  queued inbound messages. A completed prefix may be checkpointed while a
  later input is appended; applied checkpoints cannot regress. It is not an
  authorization proof or a portable source of context.

Core admission APIs schedule inbound work into the Run inbox. They do not
record generated assistant or Tool output; a separate output-recording port is
future work. A caller must not infer that every transcript append should start
a Run.

`MessageOrigin` records provenance supplied by a host-verified boundary. A model
role, role name, prompt or generated output does not authenticate an origin or
grant source, model-processing, Task or product authority. Identity values are
not grants.

### Separate A2A module and host ownership

`floe-a2a` owns the transport-neutral exchange envelope, internal contract
version and extension policy, peer-scoped external IDs, explicit local/remote
identity mapping, external Task observations, and ports for peer exchange and
the host Task owner, including bounded cancellation requests that return a
peer observation rather than proof of effect rollback. Its part-count,
aggregate artifact and serialized envelope limits are provisional framing
resource guards, not measured prompt, context or user-content defaults. Each
exchange port call carries an execution scope and host agent identity; a
binding must authorize that scope and verify peer/Task binding rather than
treating IDs as grants. It depends on neutral Conversation and Agent contracts; it
does not depend on Conversation Core, Manager Conversation, Experts, Vault or
the built-in Expert packages.

The host Task owner remains authoritative for Task admission, status, artifacts
and immutable receipts. A2A peer Task observations are external facts and never
create a second Task aggregate or prove a local effect. Directory/discovery
selects an eligible host agent; host Task lifecycle controls execution; A2A
mapping and bindings translate exchange identity. These responsibilities do not
replace one another.

No remote HTTP or standard-conformance claim is included. A future in-process
binding calls the same peer operation directly without loopback HTTP. A future
remote binding may add transport-specific encoding around these contracts. The
current AgentCard/AgentMessage definitions are not relocated by this decision.

### Learning boundary

Learning remains Manager-only under Knowledge's governed lifecycle. Generic
Runtime and Conversation Core receive no learning hooks. There is no
per-Expert Learner and no autonomous change to Expert prompt, role or tools.
Authorized knowledge reads and continuity of an Expert's own conversation are
separate concerns.

## Consequences and status

The R1 contracts and deterministic transition fixtures do not change the
production Manager Session path, connect Expert conversation resume, implement
durable storage, migrate stored sessions, or change Task receipts. Those caller,
storage and resume slices remain future work and are tracked in the single
active architecture-refactor plan. Passing contract fixtures is not production
integration or persistence evidence.
