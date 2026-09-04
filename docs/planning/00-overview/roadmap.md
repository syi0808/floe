# Roadmap

> Status: Capability roadmap; delivery sequenced by vertical slices

## Delivery model

Phase는 장기 제품 범위 지도이며 순차 구현 gate가 아니다.
[ADR 0006](../../decisions/0006-slice-driven-delivery.md)에 따라 실제 구현은
[S1–S5 vertical slices](../08-engineering/vertical-slice-delivery.md)로 진행한다.
S1–S3에서 connector → agent → app → 승인 실행 흐름을 먼저 검증하고,
S4–S5에서 서버·기기·개입으로 확장한다. Phase 0 PoC는 필요한 경계의 구현 전에
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
- HealthKit
- Health Connect
- Health Expert
- Schedule Expert
- Communication Expert의 최소 버전

검증 질문:

> 연결된 데이터로 기존 생산성 앱이 못하던 판단을 실제로 할 수 있는가?

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
