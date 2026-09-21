# Stage 3 — Product Boundary and Final Composition

**Status: active**

Stage 3 answers: **do product callers and wire boundaries also follow the canonical architecture built in Stage 2?**

Stage 2's internal exit gate is complete: the canonical internal runtime is frozen
and 2-F closed it with zero code changes. Stage 3 is now the active refactor source
of truth.

Detailed work is split into the execution plans linked below. This overview owns the Stage 3 progress checkboxes.

## Current checkpoint

**Active: 3-A — Remaining Root and Domain Caller Convergence.**

Use [the 3-A execution plan](stage-3/3-a.md) as the authoritative task document.

## Progress

- [ ] **3-A — Remaining root/domain caller convergence** — [execution plan](stage-3/3-a.md)
- [ ] **3-B — AppHost composition closure** — [execution plan](stage-3/3-b.md)
- [ ] **3-C — Protocol and FFI contract cutover** — [execution plan](stage-3/3-c.md)
- [ ] **3-D — Flutter, native and server caller cutover** — [execution plan](stage-3/3-d.md)
- [ ] **3-E — Outer compatibility deletion** — [execution plan](stage-3/3-e.md)
- [ ] **3-F — End-to-end product validation** — [execution plan](stage-3/3-f.md)

Stage 3 started after Stage 2's internal exit gate went green in 2-F.

## Product-boundary rule

Product callers submit **user intent**, not execution topology.

Allowed outer intent includes session/run identity, command text, continuation/retry intent and explicit user-selected model profile where applicable.

Do not expose raw model endpoint/base URL, bearer/token, resolved provider route bundles, Foundation-vs-Server selection, source connector catalog as model routing input, or internal Access policy flags.

## Target product path

~~~text
Flutter
  → FFI / protocol conversion
  → AppHost composition
  → owner services
  → canonical internal runtime from Stage 2
  → provider/native/server adapters
~~~

The outer layers translate intent and present results. They do not regain business ownership.

## Stage 3 exit gate

- General Conversation runs end-to-end through canonical owners.
- Expert execution runs end-to-end through canonical owners.
- Calendar/Schedule remaining roots are canonicalized.
- AppHost is composition-only.
- FFI exposes intent contracts rather than execution topology.
- Flutter performs no internal model routing.
- raw model credentials never cross product protocol boundaries.
- outer route compatibility is deleted.
- remaining `CalendarTurn` / `run_calendar_agent_turn` compatibility is deleted or canonicalized.
- Apple-focused end-to-end, revocation and recovery scenarios are validated.
