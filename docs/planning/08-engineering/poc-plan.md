# PoC Plan

> Status: Recommended validation order

## Near-term gates for S4 and S5

S3 acceptance 뒤 구현 순서는 P0-D Memory Compiler 평가와 P0-F local vault/key
boundary → S4 Reviewable Memory → P0-I Expert Contract harness → S5 Manager/Expert
Advice다. P0-F의 self-host key model과 아직 선택하지 않은 sync/server는 S4나
S5의 실행 prerequisite로 만들지 않는다. P1-B Sync Chaos는 S6 착수 전에 수행한다.

## P0-A — macOS Ambient Voice

검증:

- wake word accuracy
- CPU/battery cost
- streaming STT latency
- accidental activation
- local-only pre-wake audio boundary

성공 기준은 제품 개발 전에 별도 정의.

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

## P0-D — Memory Compiler

입력:

- S4의 Floe Note corpus
- 이후 경계를 확인할 대화, transcript, 이메일의 adversarial fixture

출력:

- Preference
- Commitment
- source/provenance
- confidence

S4 착수 전에 corpus와 target threshold를 versioned fixture로 고정한다. 모든
candidate의 typed schema/provenance 보존, 같은 evidence 재처리의 idempotency,
rejected/tombstoned memory의 비부활은 100% 통과해야 한다. Candidate precision,
recall, false memory와 false merge는 별도로 측정하며 threshold 변경은 평가 결과와
함께 기록한다. S4가 다루지 않는 Person merge, Episode/Claim은 연구 결과만 남기고
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

S4 gate는 local at-rest protection, OS-backed key access, key-unavailable
fail-closed, 삭제 후 파생 데이터 잔존 여부까지다. 이 경계가 통과하기 전에는 synthetic
fixture만 사용한다. self-host key ownership과 cross-device key delivery는 결과를
기록하되 S6 전까지 미룰 수 있다.

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

## P0-I — Local Expert Contract

입력:

- manual trigger
- bounded TimelineView
- confirmed MemoryView
- Person assignment와 granted permissions

출력:

- InsightCandidate
- ActionProposal
- private state update
- diagnostics

검증:

- native Schedule Expert와 declarative fixture의 동일 contract 실행
- permission denial과 assignment state isolation
- malformed output, timeout, budget 초과의 failure isolation
- Expert의 DB/credential/direct mutation 접근 불가
- Manager 합성과 S3 action gate까지의 trace/replay

고정 scenario에서 grounding, stale-context rejection과 불필요한 제안 비율을
기록한다. S5 착수 전에 scenario set, 허용할 stale/ungrounded output 0건과 유용성
target을 고정한다. 이 PoC는 arbitrary code/Wasm이나 server placement를 결정하지
않는다.

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
