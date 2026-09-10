# Expert Runtime

> Status: Recommended architecture direction

> Manager–Expert transport follows
> [ADR 0018](../../decisions/0018-manager-expert-a2a-delegation.md). The S4 typed
> invocation is an implemented migration baseline, not the target Manager API.
> Product-domain boundaries and ambient delivery follow
> [ADR 0020](../../decisions/0020-ambient-assistant-expert-connector-model.md).

## S4 implementation baseline

S4는 이 문서의 모든 execution class를 한 번에 구현하지 않는다. local host에서
다음 최소 runtime을 먼저 제공한다.

- package/version, installation, Person assignment와 enable/revoke registry
- bounded `ExpertInvocation` / `ExpertResult`와 per-assignment private state
- read-only Timeline/Memory view handles와 deny-by-default capability dispatch
- native `floe.schedule` Schedule & Feasibility Calendar increment와 deterministic
  declarative fixture adapter
- timeout/output/budget validation, trace와 failure isolation

Sandboxed Component와 server embedding topology는 S4 제외 범위다. 다만 S4의 host
contract에 direct database, credential, UI 또는 authoritative Memory write를 넣어
미래 sandbox를 우회해서는 안 된다.

## Target runtime topology

Manager와 Expert는 기본적으로 같은 process에서 실행한다. 서로 직접 함수를 호출하지
않고 A2A-aligned operation port를 사용하며, 초기 binding만 in-process다.

```text
Manager Agent
  → A2ARouter
      → InProcessA2ATransport
          → ExpertRuntime
```

`InProcessA2ATransport`는 canonical Agent Card, Message, Task, Artifact와 Part 객체를
직접 전달하므로 loopback HTTP나 JSON serialization cost가 없다. 향후
`RemoteA2ATransport`가 공식 JSON-RPC, gRPC 또는 HTTP+JSON binding을 구현해도 Router와
Manager contract는 유지된다.

초기 operation surface는 다음과 같다.

```text
send_message(agent_ref, message, configuration) -> Message | Task
get_task(task_id)                                -> Task
cancel_task(task_id)                             -> Task
get_agent_card(agent_ref)                        -> AgentCard
```

stream/resubscribe와 push notification은 같은 process의 첫 구현에는 필요하지 않지만
canonical task lifecycle과 persisted event는 이후 추가를 막지 않도록 설계한다.

## Execution Classes

Floe supports multiple Expert implementation classes behind the same semantic contract.

```text
Expert Contract
├─ Native Built-in
├─ Declarative
└─ Sandboxed Component
```

---

# 1. Native Built-in Expert

Used by first-party Experts that need deep integration or maximum performance.

Examples:

- Schedule & Feasibility Expert
- Commitments / Communication Experts
- Relationships / Focus & Attention / Wellbeing Experts
- Work Context / Life Logistics Experts

Implementation may live in Go server or Rust Device Core depending on execution placement.

Even trusted Experts should consume domain Views and emit Artifacts with typed Data
Parts where practical.

---

# 2. Declarative Expert

The default user-created extension mechanism.

Package contains:

- manifest
- trigger definitions
- view requirements
- rules
- prompt/model references
- output schemas
- configuration schema

Example concept:

```yaml
id: dev.floe.community.job-search
api_version: v1

triggers:
  - mail.received
  - schedule.daily

permissions:
  - mail.read.content
  - timeline.read.range
  - task.create.propose

logic:
  pipeline:
    - extract_job_context
    - detect_followup
    - propose_task
```

The host implements pipeline primitives.

Benefits:

- portable across server/device where supported
- no arbitrary native code
- easy permission analysis
- marketplace review easier
- user-generated Experts feasible

---

# 3. Sandboxed Code Expert

Used when declarative primitives are insufficient.

Recommended format direction:

```text
WebAssembly Component
+ WIT Expert API
```

The WebAssembly Component Model defines typed component interfaces via WIT, which fits a capability-based Expert host.

Candidate host:

```text
Desktop Rust Device Agent → Wasmtime
Server → dedicated Rust expert worker OR Go control plane calling a Rust/Wasmtime worker
```

The exact server embedding topology requires a PoC; Go remains the Floe control plane regardless.

---

# Expert Host API

Conceptual host interface:

```text
context.current_state()
timeline.query(...)
people.resolve(...)
memory.query_projection(...)
expert_state.get/set(...)
capability.invoke(...)
model.run(alias, input)
output.emit(...)
```

Only manifest-approved interfaces are linked into the invocation context.

이 API는 Expert가 사용하는 host-side Tool/View boundary다. Manager가 Expert를
capability처럼 호출하는 API가 아니다.

---

# Model Usage

The global model architecture principle still applies: model choice is not delegated to a generic smart router.

For first-party Experts, domain implementation owns model selection.

For marketplace/declarative Experts, a package may declare **model requirements or aliases**, for example:

```text
model: local.small.language
```

or:

```text
model: remote.reasoning.standard
```

The Person/Instance maps allowed aliases to actual providers.

Third-party Experts do not receive provider credentials.

Sensitive model aliases can be prohibited by permission policy.

---

# Expert State

State is namespaced by:

```text
(instance, person, expert-package, assignment)
```

Schema version belongs to the ExpertPackage.

Updates may provide state migrations.

An Expert cannot read another Expert's private state unless an explicit future shared-state API is created.

---

# A2A Semantics

Manager의 assignment와 Expert의 clarification은 자연어 Message Part다. 실행 제어와
결과 lifecycle은 canonical A2A 객체와 Floe extension metadata로 구조화한다.

```text
Message {
  messageId
  contextId
  taskId?
  role
  parts[]
}

Task {
  id
  contextId
  status
  history[]
  artifacts[]
}
```

Artifact의 Text Part는 Manager가 종합할 자연어 결과를 담는다. evidence,
ActionProposal, MemoryCandidate와 state update는 versioned Data Part 또는 reference로
담아 typed validation과 Review를 유지한다. deadline, Person, assignment, grant, budget,
trace와 context projection은 model-authored text가 아니라 host-owned Floe extension
metadata다.

Agent Card의 description과 coarse skills는 discovery용이다. 이는 Expert가 수행할 수
있는 명령의 폐쇄적인 목록이나 권한 선언이 아니다. 실제 활성 여부와 Tool/View grant는
Installation, Assignment와 invocation policy로 결정한다.

---

# Resource Budgets

Marketplace Expert invocation receives limits.

Candidate dimensions:

- wall-clock deadline
- WASM memory
- host capability call count
- model token/cost budget
- output count/size
- scheduled invocation frequency

An Expert cannot schedule itself more frequently than host policy permits.
