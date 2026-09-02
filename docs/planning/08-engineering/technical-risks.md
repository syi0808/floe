# Technical Risks

> Status: Living risk register

## R1 — Ambient Voice

### 위험

모바일 OS에서 Siri와 동일한 항상-on custom wake 경험을 만들기 어렵다.

### 현재 방향

- macOS/Windows: ambient wake 우선
- iOS: Back Tap/App Intent 기반 explicit fast invocation
- Android: assistant/system surface 활용
- Watch: 핵심 voice surface 아님

## R2 — Health Privacy

### 위험

Health raw data를 cloud/server intelligence가 직접 처리하면 privacy와 보안 부담이 급격히 증가한다.

### 완화

- local feature extraction
- personal baseline
- heuristic/tiny model
- derived state
- privacy projection

## R3 — Integration Coverage

### 위험

각 서비스 API/권한/라이선스가 다르고 일부 OS app은 원하는 수준의 API를 제공하지 않을 수 있다.

### 완화

- Floe Connector Contract 소유
- native connector + OSS adapter 혼합
- capability-driven design

## R4 — Background Execution

### 위험

모바일에서 Expert를 daemon처럼 항상 실행할 수 없다.

### 완화

event-driven Expert + deterministic OS scheduler.

## R5 — Memory Corruption

### 위험

STT 오류, entity mismatch, 잘못된 inference가 영구 personal memory가 될 수 있다.

### 완화

- Evidence
- confidence
- fact/inference 분리
- memory candidate pipeline
- inspect/edit/delete

## R6 — Memory Poisoning / Prompt Injection

### 위험

메일/문서의 악성 텍스트가 장기 instruction으로 승격될 수 있다.

### 완화

external content = untrusted data.
typed extraction과 memory policy를 사용한다.

## R7 — Person Identity Merge

### 위험

동명이인/alias/email/contact를 잘못 연결하면 개인사 오염.

### 완화

uncertain identity link를 유지하고 irreversible auto-merge를 제한한다.

## R8 — Multi-device Sync

### 위험

offline/concurrent edit, provider changes, recurrence, deletion이 복잡하다.

### 완화

source identity, revision, idempotency, tombstone, mutation log 설계 검증.

## R9 — Unified UI / Domain Collapse

### 위험

Calendar/Todo/Notes를 UI와 DB 모두 하나로 합치면 semantics가 망가진다.

### 완화

UI projection만 통합하고 domain model은 분리한다.

## R10 — Subscription Provider Stability

### 위험

Codex 등 구독 인증형 integration은 provider 정책/공식 지원 범위가 변할 수 있다.

### 완화

- 공식 integration path만 사용
- provider interface로 격리
- API/local fallback
- credential을 가능한 한 Device Agent에 유지

## R11 — Self-host OAuth UX

### 위험

self-host 사용자가 provider별 OAuth app을 직접 만드는 경험은 어렵다.

### 완화

초기 BYO credentials, 장기 optional managed OAuth broker.

## R12 — Intervention Fatigue

### 위험

틀린 proactive notification이 반복되면 사용자가 Floe 자체를 끌 수 있다.

### 완화

intervention budget, confidence threshold, feedback personalization.
