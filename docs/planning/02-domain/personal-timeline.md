# Personal Timeline

> Status: Core domain

## 질문

> 사용자가 언제 무엇을 하는가?

## 포함 영역

- Events
- Tasks
- Notes
- Commitments
- Routines
- Interventions

## 중요한 원칙

UI에서 하나처럼 보여도 내부 domain semantics를 보존한다.

### Event

주요 속성 후보:

- start/end
- timezone
- recurrence
- attendees
- availability
- external calendar identity

### Task

- completion
- deadline
- priority
- recurrence
- source

### Note

- content
- attachments
- links
- creation context

### Commitment

사람 사이의 약속이나 해야 할 일을 명시적으로 표현한다.

예:

- "엄마 건강검진 같이 가기로 했다."
- "민수에게 면접 결과 나오면 연락하기로 했다."
- "다음에 이 자료를 보내기로 했다."

Commitment는 Event나 Task로 즉시 확정되지 않을 수도 있다.

## External Source Identity

외부 source에서 온 object는 최소 다음 정보를 유지하는 방향을 고려한다.

```text
provider
externalId
externalRevision
origin
lastSyncedAt
```

## Open Questions

- canonical timeline object와 provider mirror의 경계
- recurrence representation
- time-less item의 projection
- conflict resolution
