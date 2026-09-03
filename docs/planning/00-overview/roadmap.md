# Roadmap

> Status: Draft sequencing

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

Phase 0 PoC는 Phase 1 제품 작업과 일부 병렬로 진행할 수 있다. 모든 PoC를 끝낼 때까지 Day Canvas dogfood를 막지 않는다.

## Phase 1 — Personal Day

- macOS client 우선
- **calendar-first Day Canvas**
  - day time grid
  - all-day region
  - current-time line
  - timed Event geometry
  - dense-day baseline
- Floe-native Todo
  - Today unscheduled tasks
  - scheduled-task UX hypothesis
- Floe-native Notes
  - Today notes
  - contextual-note boundary
- Universal Capture
  - explicit classification PoC
  - Pending Capture recovery
- **첫 read-only real Calendar integration**
  - 실제 사용자 일정으로 Day Canvas 검증
  - timezone/source provenance
- 기본 Manager의 최소 suggestion
- 기본 voice capture
- 최소 Personal Memory foundation

검증 질문:

> **익숙한 Calendar를 중심으로 Todo와 Notes가 조용히 결합된 Floe Day Canvas가 실제 사용자의 하루를 더 잘 이해하고 운영하게 하는가?**

Phase 1의 Calendar integration은 connector breadth를 증명하기 위한 것이 아니라 **제품 mental model을 실제 일정 데이터로 검증하기 위한 최소 integration**이다.

## Phase 2 — Connected Floe

- Connector framework 고도화
- Gmail
- Google Calendar direct connector
- Microsoft Calendar/Mail 방향
- Contacts
- 추가 OS calendar route
- calendar source-route deduplication
- HealthKit
- Health Connect
- Health Expert
- Schedule Expert
- Communication Expert의 최소 버전

검증 질문:

> 연결된 데이터를 이용해 기존 생산성 앱이 못하던 판단과 조정 제안을 실제로 할 수 있는가?

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

> Floe를 오래 사용할수록 개인 비서로서 품질이 실제로 누적되는가?

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

Expert extensibility가 Day Canvas를 widget/plugin dashboard로 바꾸지 않도록 Floe-owned structured output surface를 유지한다.

## Phase 4 — Cross-device

- iOS
- Android
- Windows
- Device Agent protocol
- synchronized Day Canvas
- platform-specific invocation
- local inference/provider abstraction

Calendar-first mental model은 유지하되 플랫폼별 composition은 native UX에 맞게 조정한다.

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

특히 dogfood와 PoC 결과에 따라 플랫폼 기능, Calendar composition, 데이터 경계는 앞 단계에서 수정할 수 있다.
