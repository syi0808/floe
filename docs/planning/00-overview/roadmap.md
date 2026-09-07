# Roadmap

> Status: Capability roadmap; delivery sequenced by vertical slices

## Delivery model

Phase는 장기 제품 범위 지도이며 순차 구현 gate가 아니다.
[ADR 0006](../../decisions/0006-slice-driven-delivery.md)에 따라 실제 구현은
[S1–S9 vertical slices](../08-engineering/vertical-slice-delivery.md)로 진행한다.
S1–S3에서 connector → app → 승인 실행 기반을 검증하고, S4에서 Calendar/Gmail과
device-local Screen Time/Apple Health context를 사용하는 Chat 기반 Manager Agent →
Expert/Tool → 승인 action 루프, Codex authentication gate와 local Foundation
Model/sLLM 기반 sensitive routing을 먼저 검증한다. S5는 그 대화와
결과를 source-backed Memory와 procedural Playbook으로 학습하는 통제된 self-improvement를
추가한다. S6 voice mode와 S7 local wake-up이 같은 AgentSession을 재사용한 뒤,
S8–S9에서 서버·기기·개입으로 확장한다. 이 순서는 Floe의 핵심 제품 가설을 network
topology보다 먼저 실패시켜 보기 위한 것이다. Phase 0 PoC는 필요한 경계의 구현 전에
수행하며 보안·권한 검증을 생략하지 않는다.

Slice가 일부 경계를 검증해도 해당 Phase 전체가 완료되는 것은 아니다.
현재 상태와 검증 근거는 [PROGRESS.md](../../../PROGRESS.md)에서 관리한다.

## Phase 0 — Architecture PoCs

목표: 제품을 만들기 전에 실패 가능성이 큰 기술 경계를 검증한다.

- macOS wake word
- streaming transcription
- Health local processing
- Personal Memory compiler
- Floe Connector contract
- Activepieces adapter feasibility
- LLM → Action Proposal → deterministic executor
- encrypted personal store
- basic multi-device sync assumptions
- Expert contract + permission/sandbox PoC

## Phase 1 — Personal Day

- macOS client 우선
- Day Canvas
- Calendar
- Todo
- Notes
- Universal Capture
- 기본 Manager
- 기본 voice capture
- 최소 Personal Memory

검증 질문:

> Calendar + Todo + Notes를 Floe 방식으로 합친 Daily UX가 실제로 더 좋은가?

## Phase 2 — Connected Floe

- Connector framework
- Gmail
- Google Calendar
- Contacts
- OS calendar integration
- current location / travel ETA / Weather
- Screen Time public-API feasibility
- HealthKit
- Health Connect
- Health Expert
- Schedule Expert
- Communication Expert의 최소 버전

검증 질문:

> 연결된 데이터로 기존 생산성 앱이 못하던 판단을 실제로 할 수 있는가?

S4는 모든 connector breadth가 아니라 Today briefing에 필요한 Time,
Commitments, People, Feasibility, Capacity cohort를 먼저 검증한다. 선정과 제외 근거는
[Assistant Context Portfolio](../05-integrations/assistant-context-portfolio.md)를 따른다.

## Phase 3 — Personal Memory

- People
- Relationship
- Episode
- Commitment
- provenance
- memory inspection
- edit/delete
- sensitivity
- identity resolution

검증 질문:

> Floe를 오래 사용할수록 비서의 품질이 실제로 누적되는가?

S5는 이 Phase 전체가 아니라 conversation/outcome 기반 Preference/Commitment와
procedural Playbook의 evidence, review, retrieval, edit/rollback/delete lifecycle을
먼저 검증한다.

## Phase 3.5 — Expert Ecosystem

- public Expert contract
- declarative Expert format
- Expert assignment per Person
- capability permissions
- private Expert state
- local package install
- Expert SDK/testing harness
- sandboxed code Expert PoC

Marketplace discovery/commerce itself can come later; the runtime contract should stabilize earlier.

S4는 local Schedule Expert와 declarative fixture를 같은 계약으로 실행해 Manager,
permission, assignment, structured output 경계를 먼저 검증한다. Wasm과 Marketplace는
이 선행 slice의 완료 조건이 아니다.

## Phase 4 — Cross-device

- iOS
- Android
- Windows
- Device Agent protocol
- synchronized Day Canvas
- platform-specific invocation
- local inference/provider abstraction

## Phase 5 — Ambient Floe

- macOS wake word 안정화
- Windows ambient invocation
- improved speaker recognition
- meeting transcription
- device handoff/arbitration
- proactive intervention tuning

## Phase 6 — Hosted / Self-host Ecosystem

- polished Docker deployment
- admin dashboard
- multi-user instance
- Account / Person / Membership management
- optional Floe-managed OAuth broker
- family/delegated administration

## 원칙

Roadmap 순서는 고정 계약이 아니다.

특히 P0 PoC 결과에 따라 플랫폼 기능이나 데이터 경계는 앞 단계에서 수정할 수 있다.
