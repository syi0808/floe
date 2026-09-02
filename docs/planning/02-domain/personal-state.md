# Personal State

> Status: Core domain

## 질문

> 사용자는 지금 어떤 상태인가?

## 목적

Raw data를 매번 LLM context로 보내지 않고 사용자 현재 상태를 압축하여 유지한다.

## 예시

```text
CurrentState {
  temporal: {
    nextEvent,
    freeTime,
    scheduleDensity
  },

  physical: {
    recovery,
    sleepDebt,
    activityLoad
  },

  cognitive: {
    workload,
    overdueTasks,
    contextSwitching
  },

  environment: {
    device,
    locationContext,
    movement
  }
}
```

## Derived State

가능한 한 raw signal보다 derived state를 상위 intelligence에 전달한다.

예:

```text
Raw Health Samples
    ↓
local/statistical processing
    ↓
recovery = low
sleepDebt = moderate
```

## 특성

State는 Memory와 다르다.

- State는 현재성에 강하다.
- 빠르게 변할 수 있다.
- 오래 보존할 필요가 없는 값도 많다.
- 특정 source에서 다시 계산 가능할 수 있다.

## State Producers

후보:

- Health engine
- Calendar/Schedule reducer
- Task/workload reducer
- Location/device context
- User self-report
- Expert-derived state

## Open Questions

- State persistence 전략
- stale state 판정
- source disagreement
- cross-device aggregation
