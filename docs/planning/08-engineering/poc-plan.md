# PoC Plan

> Status: Recommended validation order

## Near-term slice gates

S3 acceptance 뒤 순서는 P0-I Agent/Expert Contract, P0-C Gmail subset, P0-K Apple
context, P0-L privacy-aware inference와 P0-F session vault/key → S4 Connected Agent → P0-D Memory/Learning
Compiler와 P0-F memory vault → S5 Governed Memory → P0-A
Streaming Voice → S6 Voice Mode → P0-J Local Wake → S7 Wake-up이다. P0-F의
self-host key model과 sync/server는 S5 prerequisite가 아니다. P1-B Sync Chaos는
S8 착수 전에 수행한다.

## P0-A — Streaming Voice Session

검증:

- streaming STT latency
- partial/final transcript correction
- foreground recording/file transcription의 timecoded segment와 restart recovery
- transcript → summary/Task/Commitment candidate provenance
- TTS first-audio latency와 barge-in
- text/voice turn ordering
- microphone permission, audio retention과 remote transfer boundary
- noise/echo, CPU/battery cost

S6 착수 전에 short-conversation/meeting fixture를 포함한 versioned audio corpus와
live 환경, latency/WER/resource threshold를 정의한다. 이 PoC는 wake word나
resident background lifecycle을 포함하지 않는다.

## P0-B — Health Local Engine

검증:

```text
HealthKit
↓
Feature Extraction
↓
Personal Baseline
↓
Heuristic/Tiny Model
↓
Derived State
```

- 실제로 유용한 derived state를 만들 수 있는지
- raw data를 cloud에 보내지 않고 충분한 판단이 가능한지

## P0-C — Connector Contract + Activepieces Adapter

대상 후보:

- Gmail
- Google Calendar
- Notion 혹은 단순 REST connector

검증:

- auth mapping
- action mapping
- sync semantics
- capability mapping
- upstream 업데이트 비용

S4 gate에서는 Activepieces breadth보다 공식 Gmail API를 우선한다. local Go
connector에서 OAuth scope, bounded initial import, `messages.list/get`, on-demand
body, restart-safe cursor/history, revoke/rate-limit/partial failure와 untrusted-content
경계를 검증한다. 범용 adapter 변환은 이 결과를 막지 않는다.

## P0-D — Memory Compiler

입력:

- S4의 conversation, outcome, correction corpus
- 이후 경계를 확인할 대화, transcript, 이메일의 adversarial fixture

출력:

- Preference
- Commitment
- source/provenance
- confidence

S5 착수 전에 corpus와 target threshold를 versioned fixture로 고정한다. 모든
candidate의 typed schema/provenance 보존, 같은 evidence 재처리의 idempotency,
rejected/tombstoned memory의 비부활은 100% 통과해야 한다. Candidate precision,
recall, false memory와 false merge는 별도로 측정하며 threshold 변경은 평가 결과와
함께 기록한다. S5가 다루지 않는 Person merge, Episode/Claim은 연구 결과만 남기고
slice 범위를 확장하지 않는다.

## P0-E — Action Gate

```text
Manager
↓
Action Proposal
↓
Policy
↓
Validation
↓
Calendar Mutation
```

- idempotency
- stale revision
- user confirmation
- rollback/error

검증.

## P0-F — Encrypted Personal Store

검증:

- Person별 vault separation
- local/device secrets
- deletion/provenance
- self-host key model

S4 gate는 session/message/tool result의 local at-rest protection, OS-backed key
access와 key-unavailable fail-closed다. S5는 여기에 Memory/Playbook/evidence와 삭제
후 파생 데이터 잔존 여부를 추가한다. 각 경계가 통과하기 전에는 synthetic fixture만
사용한다. self-host key ownership과 cross-device key delivery는 결과를 기록하되
S8 전까지 미룰 수 있다.

## P0-G — Turso Local / Sync

검증:

- Rust Core에서 embedded Turso 사용
- macOS/iOS/Android/Windows build feasibility
- Person별 DB topology
- encryption
- 2-device offline write
- push/pull
- conflict
- delete propagation
- self-host sync server/auth integration
- native vector retrieval

결과에 따라 Turso Sync를 Floe sync의 하위 primitive로 채택할지 결정한다.

## P0-H — Native ConnectorSpec / Activepieces Port

대상:

- API key 기반 단순 REST connector
- OAuth2 connector
- polling connector
- webhook connector

검증:

1. Floe ConnectorSpec로 표현
2. Go runtime에서 실행
3. 필요한 경우 Rust runtime에서 동일 spec 실행
4. Activepieces Piece 하나를 AST 기반으로 spec에 반자동 변환
5. unsupported arbitrary TypeScript를 명확히 감지

목표:

Node 없이도 핵심 connector 구현 비용을 충분히 낮출 수 있는지 확인한다.

## P0-I — Conversational Agent and Expert Contract

입력:

- multi-turn chat commands
- bounded TimelineView
- Person assignment와 granted permissions

출력:

- InsightCandidate
- ActionProposal
- private state update
- typed AgentEvent와 diagnostics

검증:

- fixture/model adapter의 동일 internal message/tool-call contract
- native Schedule Expert와 declarative fixture의 동일 Expert contract 실행
- session persist/resume와 ordered streaming event
- permission denial과 assignment state isolation
- malformed output, timeout, cancel, budget/stall의 failure isolation
- Expert의 DB/credential/direct mutation 접근 불가
- Manager 합성과 S3 action gate까지의 trace/replay

고정 scenario에서 grounding, stale-context rejection과 불필요한 제안 비율을
기록한다. S4 착수 전에 scenario set, 허용할 stale/ungrounded output 0건과 유용성
target을 고정한다. 이 PoC는 arbitrary code/Wasm이나 server placement를 결정하지
않는다.

## P0-J — Local Wake Word and Resident Lifecycle

검증:

- macOS on-device wake phrase false accept/reject와 latency
- local-only bounded pre-roll과 trigger 실패 시 즉시 폐기
- UI 종료/재시작 뒤 resident Device Agent lifecycle
- microphone single ownership과 S6 AgentSession handoff
- opt-in/pause/disable, visible listening state와 lock-screen policy
- idle CPU/battery와 hotkey/manual fallback threshold

S7 착수 전에 다양한 거리, 소음과 유사 발화 corpus를 고정한다. speaker recognition은
편의 신호로만 평가하며 action authorization을 대체하지 않는다.

## P0-K — Apple Assistant Context Sources

signed Apple builds와 physical iPhone/iPad에서 검증:

- Contacts limited/full/denied access와 identity-reference projection
- Core Location When In Use/reduced accuracy, revoke와 ephemeral retention
- MapKit ETA throttle/cancel/error와 next-event leave-by 계산
- WeatherKit entitlement, event-window query/cache와 attribution
- Family Controls/Device Activity의 individual authorization과 entitlement 상태
- app/website usage data의 OS/region/distribution 제한
- privacy-preserving attention aggregate를 host app이 실제 소비할 수 있는지
- HealthKit availability와 sleep/activity 최소 read authorization
- raw Health sample → local coarse HealthState derivation
- denial/revocation/no-data/stale/external deletion 상태

Screen Time public API가 필요한 signal을 합법적으로 제공하지 않으면 unsupported
capability와 대체 가능한 coarse signal을 ADR로 결정한다. private database, undocumented
API, entitlement 우회는 성공 조건이 아니다. HealthKit은 macOS에서 data access가
불가능하므로 macOS fixture와 iOS live evidence를 명확히 구분한다.

## P0-L — Privacy-aware Inference Routes

동일한 bounded `LanguageModel` contract로 검증:

- deterministic fixture adapter
- supported Apple device의 Foundation Models on-device profile 또는 native packaged
  sLLM 하나; Private Cloud Compute/server profile은 local gate에서 제외
- API key 등 공식 지원 remote adapter 하나
- Codex browser consent/refresh/revoke/logout와 subscription inference feasibility
- purpose, data class, allowed placement, performance class, projection version,
  external-transfer consent를 포함한 `InferencePolicyDecision`
- local-only unavailable, remote denied, credential expiry, quota, cancel과 retry 상태
- outbound capture를 통한 raw Health/Screen Time/location/credential/mail 경계 확인

Codex CLI/App credential은 복사하지 않는다. Codex OAuth wire compatibility만으로
Floe의 third-party integration이 공식 지원된다고 가정하지 않으며, supportable한 경계를
확립하지 못하면 unsupported 결과와 API-key route를 유지한다. local-only 요청은 절대
remote로 silent fallback하지 않는다.

## P1-A — Day Canvas Dogfood

macOS에서 실제 사용하며:

- 산만함
- Now/Next 효용
- Unified Capture
- Event/Task/Note projection

검증.

## P1-B — Sync Chaos

3개 device 또는 simulator에서:

- offline edit
- delete
- concurrent update
- provider update

를 고의로 충돌시킨다.

## P1-C — Intervention Dogfood

2주 이상 실제 사용하면서:

- 제안 발생 횟수
- accept
- ignore
- dismiss
- false positive

를 기록한다.
