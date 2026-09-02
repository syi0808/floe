# Sensitive Local Compute

> Status: Core architectural direction

## 목적

민감한 raw data를 가능한 한 사용자 기기 안에서 처리하고 상위 intelligence에는 필요한 최소 context만 전달한다.

## 기본 pipeline

```text
Sensitive Source
      ↓
Local Processing
      ↓
Derived State / Structured Claim
      ↓
Privacy Projection
      ↓
Manager / Remote Model
```

## Local 처리 후보

- Health raw data
- wake word
- voiceprint
- PII detection/redaction
- 사소한 intent classification
- memory candidate extraction 일부
- simple entity extraction
- notification relevance
- sensitive transcript pre-processing

## Health 예시

```text
Raw HealthKit / Health Connect
       ↓
Feature Extraction
       ↓
Personal Baseline
       ↓
Heuristic / Tiny Model
       ↓
Derived Health State
```

## Model 크기와 privacy

"Local sLLM"을 하나의 범용 모델로 고정하지 않는다.

도메인에 따라:

- heuristic
- statistical estimator
- tiny classifier
- small LLM
- larger local model

중 적합한 방법을 선택한다.

## Privacy Projection

Remote reasoning에 전체 원문을 보내지 않고 문제 해결에 필요한 최소 표현을 만든다.

예:

원문:

```text
어제 여자친구와 A 문제로 크게 다퉜고 새벽 3시까지 잠을 못 잤다.
```

일정 재계획에 필요한 projection이:

```text
recent_interpersonal_stress = high
sleep_deficit = high
```

라면 원문 전체를 remote model에 보낼 필요가 없다.

## 중요한 경계

Privacy Projection 정책은 중앙 모델 Router가 암묵적으로 정하는 것이 아니라 domain/business logic에 가깝게 둔다.
