# Stage 3 — Product Boundary and Final Composition

**Status: future**

Stage 3 answers: **do product callers and wire boundaries also follow the canonical architecture built in Stage 2?**

~~~text
Stage 1: Who owns it?
Stage 2: Does the internal runtime use that owner?
Stage 3: Do all product callers and boundaries follow the same architecture?
~~~

Stage 3 pushes the canonical architecture through AppHost, FFI, Flutter, native and server callers, removes outer compatibility and validates the actual product flow.

## Product-boundary rule

Product callers submit **user intent**, not execution topology.

Outer requests may contain session/run identity, user text/command, continuation/retry intent and an explicit user-selected model profile when applicable.

They must not carry internal execution decisions such as raw model endpoint/base URL, bearer/token, resolved route bundles, Foundation-vs-Server selection, source connector catalog as model routing input, or internal Access policy flags.

## 3-A — Remaining root/domain caller convergence

Canonicalize paths intentionally left outside the Stage 2 General Conversation cutover.

Known targets include `run_calendar_agent_turn`, `CalendarTurn` and Schedule/Calendar model paths that still bypass canonical Inference/Access ownership.

Do not force every domain through General Conversation. Require the same owner semantics:

~~~text
Expert/domain request
  → canonical Context/Access
  → canonical Inference
  → Provider transport
~~~

Exit:

- old Calendar-specific model route path: 0;
- old Calendar model-attempt accounting: 0;
- remaining domain callers use canonical owner contracts.

## 3-B — AppHost composition closure

Target:

~~~text
AppHost
  ├─ ConversationService
  ├─ Expert services
  ├─ Context services
  ├─ Access services
  ├─ InferenceService
  ├─ Connections services
  ├─ Actions services
  ├─ Knowledge services
  └─ Day services
~~~

AppHost may construct/inject services, translate outer requests and project results.

It must not own model route calculation, domain policy, grant/recipient decisions, model retry/attempt semantics, credential interpretation or source capability business dispatch.

## 3-C — Protocol and FFI contract cutover

Protocol describes product intent, not internal routing.

Remove or redesign outer DTOs exposing implementation details, including remaining `AgentRemoteRouteDto`, raw model-route DTOs, provider endpoint/base URL fields, bearer/token fields and duplicated worker/protocol requests tied to old App routing.

A Conversation turn DTO should trend toward:

~~~text
session identity
expected revision
text / command
turn mode / continuation
explicit profile selection (optional)
retry identity (optional)
~~~

FFI remains ABI/wire conversion only.

## 3-D — Flutter, native and server caller cutover

### Flutter

Flutter owns presentation and explicit user choices: commands, optional explicit profile selection, consent/review/recovery UI, connection UX and result rendering.

Flutter must not select model endpoints, carry bearer credentials, build route bundles or decide local-vs-remote inference policy.

### Native

Native/platform code owns OS data acquisition, secure credential/key storage and native transport where applicable. It does not own product authorization policy.

### Server

Server/provider services may own capability observation, remote transport, pairing/OIDC/server identity and remote source transport. They do not replace local Access policy or become source-authorization owners.

## 3-E — Outer compatibility deletion

After callers are cut over, delete dormant product-boundary compatibility.

Candidates:

- `AgentRemoteRouteDto`;
- model-execution uses of `RemoteTurnRoute`;
- legacy FFI route conversions;
- Flutter route/provider models;
- old worker variants and temporary protocol fields;
- old AppHost constructors;
- transitional execution-profile compatibility;
- CalendarTurn/run-calendar compatibility replaced by canonical callers.

## 3-F — End-to-end product validation

Validate actual product flows.

### General Conversation

~~~text
Flutter
  → FFI
  → AppHost
  → Conversation
  → Engine
  → Inference
  → Access
  → Provider
  → reply
~~~

### Tool use

~~~text
Manager ToolCall
  → ContextToolService
  → authorized source
  → ToolResult
  → next model call
  → answer
~~~

### Expert delegation

~~~text
Manager
  → Expert Task
  → Expert reasoning/model/tool execution
  → TaskReceipt
  → Manager
  → answer
~~~

Also validate remote model dispatch, revocation during execution and crash/recovery around validated batches and continuation boundaries.

Required recovery behavior:

- no duplicate external effect;
- no lost pending validated batch;
- no re-planning of already validated pending work.

## Security and privacy exit invariants

Secrets stay inside their owners.

The following are absent from product wire/public caller contracts:

- Flutter bearer;
- FFI bearer;
- Conversation bearer;
- Inference public bearer;
- arbitrary caller-provided model endpoint.

Authorization remains Access-owned; Context remains source/projection-owned; Inference remains model-route/attempt-owned; Provider remains transport-owned.

## Stage 3 exit gate

Stage 3 is complete when:

- General Conversation works end-to-end through canonical owners;
- Expert execution uses canonical owners end-to-end;
- Calendar/Schedule remaining callers are canonicalized;
- AppHost is composition-only;
- FFI exposes user-intent contracts rather than execution topology;
- Flutter performs no internal model routing;
- raw model credentials never cross protocol boundaries;
- outer route compatibility is deleted;
- old CalendarTurn/run-calendar compatibility is deleted or canonicalized;
- end-to-end product, revocation and recovery scenarios are validated on the supported Apple-focused product path.

At that point the entire product, not only the Rust interior, follows one architecture.
