# Model Layer & Inference Resources

> Status: Accepted architectural direction

## 핵심 원칙

Floe는 하나의 AI provider에 종속되지 않는다.

사용 가능한 inference resource:

```text
Local
├─ on-device sLLM
├─ OS-provided model
└─ self-hosted local runtime

Subscription
├─ officially supported subscription-authenticated AI
└─ Codex/App Server 같은 integration

API
├─ OpenAI API
├─ other model APIs
└─ hosted inference

Self-hosted
├─ Ollama
├─ vLLM
└─ custom endpoint
```

## 중앙 `InferenceRequest` Router를 비즈니스 핵심으로 만들지 않는다

어떤 모델 크기/특성/휴리스틱을 사용할지는 도메인 로직과 강하게 결합된다.

따라서:

```text
Business Domain
    ↓
Domain Model / Service
    ↓
Reusable AI Primitive
    ↓
Provider / Runtime
```

구조를 선호한다.

## Domain Service 예시

```text
Health
├─ RecoveryEstimator
├─ ConditionEstimator
└─ HealthInterventionEvaluator

Memory
├─ ClaimExtractor
├─ EntityResolver
└─ ConflictResolver

Schedule
└─ SchedulePlanner
```

`RecoveryEstimator`가 내부적으로 heuristic인지 tiny model인지 LLM인지가 구현 세부다.

## 재사용 가능한 primitive

후보:

```text
LanguageModel
EmbeddingModel
TranscriptionModel
ClassificationModel
VisionModel
```

Provider는 인증/transport/availability/model discovery 같은 실행 책임을 맡는다.

## Business Logic Owns Model Choice

예:

```text
Health pipeline
→ heuristic
→ tiny local classifier
→ 필요한 경우에만 local LLM
```

```text
Manager planning
→ 강한 subscription/API reasoning model
```

중앙 Router가 sensitivity나 model size를 추론해서 자동 선택하는 구조는 우선 피한다.

## Fallback

공통 infrastructure가 제공한다면 다음 정도로 제한한다.

- 지정 모델 unavailable 시 대체 모델
- provider availability
- retry
- quota/transport error handling

Privacy/업무 의미 판단은 상위 도메인에서 끝나 있어야 한다.

## Subscription Credential

Codex 등 공식 구독 인증 경로가 있는 provider는 사용자 기존 구독을 inference resource로 활용할 수 있다.

가능하면 subscription credential/token은 Device Agent의 secure credential vault에 두고 서버가 직접 소유하지 않는다.
