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

## LLM이 직접 외부 상태를 변경하지 않는다

```text
LLM / Manager
      ↓
Action Proposal
      ↓
Policy Engine
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
