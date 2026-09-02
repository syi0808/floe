# Health Intelligence

> Status: Core privacy-sensitive domain

## 목표

Health data를 dashboard로 보여주는 것이 아니라 사용자의 하루 계획과 컨디션 관리를 더 잘하기 위해 사용한다.

## Data Sources

```text
Apple
HealthKit

Android
Health Connect
```

## 기본 pipeline

```text
Raw Health Data
      ↓
Local Feature Engine
      ↓
Personal Baseline
      ↓
Heuristics / Statistical Analysis
      ↓
Optional Tiny/Local Model
      ↓
Derived Health State
      ↓
Health Expert / Manager
```

## 휴리스틱 우선

다음과 같은 계산을 LLM에게 맡기지 않는 방향을 선호한다.

- 개인 baseline 대비 변화
- sleep duration/debt
- resting heart rate deviation
- HRV trend
- activity load
- recent workout load

## Local Model의 역할

작은 local model은 다음에 적합할 수 있다.

- 복합 신호 분류
- semantic state classification
- intervention relevance 판단
- 사용자 자기 보고와 sensor signal 결합

## 상위로 넘기는 정보

원시 Health data 대신:

```text
recovery = low
sleepDebt = moderate
activityLoad = high
trend = declining
```

와 같은 정제된 state를 우선한다.

## 주의

Derived state도 건강 관련 민감 정보일 수 있다.

"정제됨 = 비민감"이라고 자동 가정하지 않는다.

Privacy policy가 별도로 결정해야 한다.

## Health Expert

Health Expert는 사용자의 현재 schedule/workload와 건강 state를 결합해:

- 일정 강도
- 휴식 필요
- 운동 강도
- 장기 변화

등의 판단 후보를 Manager에게 제공한다.
