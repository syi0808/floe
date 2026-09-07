# Skills, Actions & Authority

> Status: Core safety boundary

## Skill

Skill은 행동 capability다.

예:

```text
calendar.search
calendar.create
calendar.move

mail.search
mail.draft

task.create

health.query
```

## Playbook과 구분

Hermes Agent가 `Skill`이라 부르는 agent-authored procedural knowledge는 Floe에서
**Playbook**이라 부른다. Floe의 Skill capability와 혼동하지 않는다.

```text
Skill = 무엇을 실행할 수 있는가
Playbook = 반복 작업을 어떤 순서와 검증으로 수행할 것인가
Expert = 특정 domain에서 무엇이 적절한가
Manager = 무엇을 사용자에게 말하고 제안할 것인가
```

Playbook은 Skill을 조합할 수 있지만 capability나 authority를 새로 만들지 않는다.
S5의 self-improvement는 versioned Playbook change candidate를 만들며 Review와
rollback을 거친다. 자세한 계약은
[Agent Runtime and Governed Learning](agent-runtime-and-learning.md)을 따른다.

## LLM이 직접 외부 상태를 변경하지 않는다

```text
LLM / Manager
      ↓
Action Intent
      ↓
Policy Engine
  ┌───┴──────────────┐
  ↓                  ↓
Review Request   automatic authority
  └───┬──────────────┘
      ↓
Validation
      ↓
Permission Check
      ↓
Deterministic Executor
      ↓
Connector / OS API
```

원칙:

> **Intelligence proposes. Deterministic executor acts.**

## Action Authority

단계:

```text
Observe
  ↓
Analyze
  ↓
Suggest
  ↓
Prepare
  ↓
Act
```

사용자는 domain별로 autonomy를 다르게 설정할 수 있다.

예:

```text
Calendar
Automatic rescheduling: Allowed

Email
Draft automatically: Allowed
Send automatically: Never
```

`allow`는 사람의 반복 결정을 생략할 뿐 validation, provider permission,
idempotency와 audit을 생략하지 않는다. `ask`인 행동은 공통 Review Request로
전달되고 완료된 결과는 별도 Activity에 남는다. Calendar를 비롯한 각 domain이
독립적인 proposal 기능이나 inbox를 만들지 않는다.

제품 계약은 [Review, Action Authority & Activity](../01-experience/review-authority-and-activity.md)를 따른다.

## 민감 행동

다음은 높은 확인 정책을 가질 수 있다.

- 일정 삭제
- 이메일/메시지 전송
- 중요한 데이터 삭제
- 외부에 민감 데이터 공유
- 결제
- 계정/권한 변경

## 정확한 시간 실행

Expert가 정확한 시각에 다시 깨어날 것을 기대하지 않는다.

```text
AI decides
↓
OS schedules deterministic reminder/action
```

## Expert Capability Access

Third-party Experts do not receive direct Connector or credential access.

They are granted semantic capability handles such as:

```text
calendar.read
mail.search
task.create.propose
```

Every invocation is mediated by host policy.

Expert permission to *propose* an action does not bypass Person-level Action Authority.
