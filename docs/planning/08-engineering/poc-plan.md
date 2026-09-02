# PoC Plan

> Status: Recommended validation order

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

- 대화
- transcript
- 이메일

출력:

- Person identity candidate
- Episode
- Claim
- Commitment
- source/provenance
- confidence

특히 false merge와 false memory를 측정한다.

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
