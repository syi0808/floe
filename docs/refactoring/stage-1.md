# Stage 1 — Physical Ownership

**Status: complete**

Stage 1 answers one question: **which module owns each piece of Floe's business state and policy?**

The purpose was to establish the modular monolith physically before attempting every caller cutover. Stage 1 moved business semantics out of App and mixed runtime surfaces into the module that owns them.

See [Stage 2](stage-2.md) for the active internal-runtime cutover and [Stage 3](stage-3.md) for product-boundary and final-composition work.

## Target ownership

| Owner | Responsibility |
|---|---|
| Conversation | Session, transcript, root Run, replay/continuation semantics |
| Experts | Expert registry, Task, assignment and A2A delegation semantics |
| Access | Grants, recipients, authority, revocation, leases and release/dispatch admission |
| Context | Source acquisition, projections, provenance, coverage and freshness |
| Inference | Model profile/route, model attempt lifecycle and model usage |
| Execution | Budget scopes, cancellation and execution limits |
| Connections | Connection, OAuth and pairing lifecycle |
| Actions | Proposal, approval, external effect and uncertainty |
| Knowledge | Memory, playbooks and learning |
| Day | Task, note and calendar domain state |
| Built-in Experts | Domain reasoning owned by each Expert package |
| Protocol / FFI | DTO, ABI and wire conversion only |
| Adapters / Platform | Storage, OS integration and transport |
| App | Composition only |

The machine-enforced target dependency policy remains `tools/architecture/module-dependencies.json`.

## Rules established in Stage 1

- App does not become the owner of business policy merely because it wires concrete implementations.
- A stateful concept has one semantic owner.
- Moving a type without moving the policy that gives it meaning does not complete ownership transfer.
- Do not create permanent parallel `v2`, `v3`, `next` or legacy/new architectures.
- Backward compatibility for old local development data and old internal APIs is not a refactor goal.
- Security invariants are never weakened to keep transitional tests green.

Non-negotiable invariants include exact recipient and grant authority, revocation fencing, lease/producer/key identity, provenance/coverage/freshness, durable pre-dispatch intent, uncertain external-write recovery, idempotency/CAS, parent-child cancellation, A2A Task lifecycle and pairing/OIDC/server authorization.

## Ownership moves completed

Major completed ownership moves include:

- Calendar setup/access → Experts, Access and Context.
- Grant authority and subject preview → Context/Access.
- Remote pairing → Connections.
- Remote authority → Access.
- Memory review → Knowledge.
- Conversation admission (`prepare_turn` / `PreparedTurn`) → Conversation.
- Built-in Expert registry → Experts.
- Schedule context preparation → Context.
- History projection → Context/Conversation.
- Action policy/approval → Actions.
- People grant selection → Access.

This records semantic ownership, not a frozen file layout.

## What Stage 1 intentionally did not finish

Stage 1 did not require every production caller to use the new owner path. Transitional callers could remain when immediate deletion would mix ownership placement with a much larger runtime cutover.

General Conversation runtime convergence belongs to Stage 2. Outer product callers such as `run_calendar_agent_turn`, `CalendarTurn`, AppHost, FFI and Flutter belong to Stage 3 where appropriate.

## Exit criteria

Stage 1 is complete because:

- business ownership is under the intended modules;
- App no longer defines the moved domain semantics;
- dependencies obey the approved target policy;
- remaining compatibility is explicitly assigned to later stages;
- test/compile ownership can be restored without reversing production dependencies.

Stage 1 is **not reopened** for normal caller cutover, API cleanup or product-boundary work.
