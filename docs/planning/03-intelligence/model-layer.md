# Model Layer & Inference Resources

> Status: Accepted architectural direction

## Inference 실행 경계 — 2026-09-05

[ADR 0011](../../decisions/0011-inference-performance-classes.md)에 따라 네트워크 기반
모델 호출은 Floe 소유의 Go inference gateway로 모은다. 도메인은 `fast`,
`balanced`, `high_effort` 성능 class와 전송 허용 범위를 명시한다. 서버 관리자가
class를 target·model·reasoning effort에 매핑하며, 앱 사용자는 이를 설정하지 않는다.

Apple Foundation Models 같은 기기 내 모델은 native executor 경계로 분리하며
원격 서버를 필수 경유하지 않는다. CLIProxyAPI 전체 채택은 보류한다.
[ADR 0010](../../decisions/0010-local-connection-console.md)은 로컬 관리 콘솔,
Keychain 기반 키 등록, 앱 주소 설정·페어링 및 서버 소유 Codex OAuth를 추가한다.
API/Ollama와 Codex 모두 bounded structured-output 호출이며 도구는 전달하지 않는다.
실제 Codex consent/refresh·native adapter·일반 streaming·사용량 집계는 검증된
것으로 보지 않는다.

일반 대화의 primary route는 Go gateway의 `everyday_assistance` purpose다. 연결되어
사용 가능한 route가 있으면 해당 route를 먼저 선택하고, 실행 실패 후 다른 모델로
자동 fallback하지 않는다. 서버 연결 또는 해당 purpose route가 없을 때만 client의
device-local model을 primary로 선택한다. macOS에서는 Foundation Models를 사용하고,
그 외 지원 client에서는 서명·해시 검증된 sLLM artifact를 명시적 다운로드한 뒤
device-local runtime으로 사용한다.

민감 입력의 정제·요약은 일반 대화를 대신 처리하는 global route가 아니다. Manager가
필요한 경우에만 제한된 local subagent task로 요청하고, 그 결과만 primary model의
context로 전달한다. 민감도 판정과 projection 범위는 Manager/domain이 소유하며
gateway나 공통 router가 prompt 내용을 보고 추측하지 않는다.

S4는 이 미검증 경계를 제품 flow에서 앞당겨 검증한다. fixture, 하나의 device-local
Foundation Model/sLLM과 하나의 supported remote adapter가 같은 bounded contract를
사용해야 한다. Codex browser authentication은 별도 feasibility gate이며, 공개적으로
supportable한 Floe integration을 확립하지 못하면 API-key route로 대체한다.

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
└─ Codex OAuth 같은 provider-specific integration

API
├─ OpenAI API
├─ other model APIs
└─ hosted inference

Self-hosted
├─ Ollama
├─ vLLM
└─ custom endpoint
```

## 성능 class router를 업무 의미 router로 만들지 않는다

도메인은 필요한 품질/지연 class를 명시하지만 구체 model/provider를 소유하지 않는다.
Gateway는 요청 내용을 읽어 업무 용도를 추측하지 않고 명시된 class만 해석한다.

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

## Business Logic Owns Performance Requirements

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

중앙 Router가 sensitivity나 업무 의미를 추론하는 구조는 피한다. 관리자가 각
performance class의 model과 reasoning effort를 바꿀 수 있다.

## Fallback

공통 infrastructure가 제공한다면 다음 정도로 제한한다.

- 지정 모델 unavailable 시 대체 모델
- provider availability
- retry
- quota/transport error handling

Privacy/업무 의미 판단은 상위 도메인에서 끝나 있어야 한다.

모든 remote 가능 호출은 domain이 만든 `InferencePolicyDecision`을 요구한다. 여기에는
purpose, data classes, allowed placements, performance class, projection version과
external-transfer consent state가 포함된다. local-only 실패를 remote fallback으로
바꾸는 것은 retry가 아니라 새로운 전송 결정이다.

## Subscription Credential

Codex 등 구독 인증형 provider는 Floe 같은 third-party client에 공식적으로 허용된
integration 경계가 확인된 경우에만 사용자 기존 구독을 inference resource로 활용한다.
Codex client용 browser sign-in wire compatibility만으로 이를 가정하지 않는다.

가능하면 subscription credential/token은 Device Agent의 secure credential vault에 두고 서버가 직접 소유하지 않는다.
