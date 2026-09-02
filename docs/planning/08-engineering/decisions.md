# Architecture & Product Decisions

> Status: Decision log

결정은 구현 과정에서 바뀔 수 있다. 바뀌면 기존 항목을 삭제하기보다 superseded 상태를 기록하는 것을 권장한다.

---

## D-001 — Floe의 중심은 Agent Framework가 아니다

**Status:** Accepted

Floe의 제품 abstraction은 Personal Timeline, State, Memory, Integration에 둔다.

---

## D-002 — One Manager, Many Experts

**Status:** Accepted

사용자가 만나는 비서는 하나이며 Expert는 내부 advisor 역할을 한다.

---

## D-003 — Calendar + Todo + Notes를 Day Canvas로 통합

**Status:** Accepted

UI projection은 통합하되 underlying domain model을 억지로 하나로 합치지 않는다.

---

## D-004 — Memory는 개인사와 관계를 1급으로 다룬다

**Status:** Accepted

People, Relationship, Episode, Commitment가 핵심 memory domain이다.

---

## D-005 — macOS 우선, Android/Windows/iOS 모두 1급

**Status:** Accepted

플랫폼별 feature parity가 아니라 experience parity를 목표로 한다.

---

## D-006 — iOS ambient custom wake를 핵심 전제로 두지 않는다

**Status:** Accepted

Back Tap/App Intent 기반 빠른 invocation을 우선한다.

---

## D-007 — Health raw data는 local-first

**Status:** Accepted

휴리스틱/통계/작은 local model로 derived state를 만든 뒤 최소한의 context만 상위로 전달한다.

---

## D-008 — Floe Connector Contract를 직접 소유

**Status:** Accepted

Activepieces/n8n 등은 connector source/adapter로 활용하고 workflow engine은 Floe 핵심 abstraction으로 채택하지 않는다.

---

## D-009 — Activepieces를 우선 connector source로 검토

**Status:** Accepted direction

라이선스와 실제 adapter 비용은 connector별로 계속 검증한다.

---

## D-010 — Floe는 오픈소스이며 서버는 self-host 가능

**Status:** Accepted

Hosted Floe는 managed distribution으로 본다.

---

## D-011 — Account ≠ Person

**Status:** Accepted

Account는 인증 principal, Person은 실제 Floe가 보좌하는 사용자다.

Membership으로 연결한다.

---

## D-012 — Self-host Admin에서 multi-user 관리 가능

**Status:** Accepted direction

Account/Person/Membership/connector 상태를 관리하는 dashboard를 장기적으로 제공한다.

---

## D-013 — Managed OAuth는 optional

**Status:** Accepted direction

장기적으로 Floe Cloud OAuth broker를 제공할 수 있으나 self-host의 필수 dependency가 되어서는 안 된다.

---

## D-014 — Local / Subscription / API / Self-hosted AI resource 지원

**Status:** Accepted

사용자는 기존 구독형 AI 서비스의 공식 integration path가 있을 경우 이를 inference resource로 활용할 수 있다.

---

## D-015 — 모델 선택은 도메인 로직의 책임

**Status:** Accepted

범용 `InferenceRequest`가 privacy/model size/task 의미를 중앙에서 판단하는 구조를 피한다.

도메인별 service/model이 적합한 heuristic/model/provider를 명시적으로 선택한다.

---

## D-016 — Intelligence ≠ Authority

**Status:** Accepted

LLM/Agent가 직접 외부 상태를 변경하지 않고 Action Proposal → Policy → deterministic execution 경계를 둔다.

---

## D-017 — Flutter를 Main Cross-platform UI의 기본 후보로 사용

**Status:** Recommended baseline

macOS/Windows/iOS/Android main visual surface는 Flutter를 우선한다.

OS lifecycle/security가 지배하는 system surface는 native implementation을 사용한다.

---

## D-018 — Shared Performance Core는 Rust 우선

**Status:** Recommended baseline

sync, policy, crypto, desktop device agent, 고성능 local processing 등 공통 core는 Rust를 우선한다.

---

## D-019 — Desktop UI와 Resident Device Agent 분리

**Status:** Recommended baseline

macOS/Windows에서는 ambient voice/background 기능을 Flutter UI process와 분리한 Rust/native Device Agent에 둔다.

---

## D-020 — Mobile에서는 Rust Core를 앱에 Embed

**Status:** Recommended baseline

iOS/Android에서 permanent daemon을 가정하지 않고 Flutter app 안에 Rust Core를 native library로 포함하며 OS 기능은 Swift/Kotlin adapter로 연결한다.

---

## D-021 — Activepieces 호환 Connector Host는 TypeScript/Node로 격리

**Status:** Recommended baseline

Floe Server의 핵심 runtime과 분리하여 Activepieces connector ecosystem의 호환성을 우선한다.

---

## D-022 — Server는 Rust + PostgreSQL을 기본 후보로 사용

**Status:** Superseded by D-024 / D-025

초기 recommendation이었으나 Floe Server의 workload가 I/O 중심 control plane이라는 점과 Turso 채택을 반영해 변경한다.

---

## D-023 — Hot path는 Dart/Platform Channel을 경유하지 않는다

**Status:** Recommended baseline

Audio, wake word, 대형 health batch, local model 등 high-frequency path는 native/Rust에서 처리하고 Flutter에는 coarse event/state만 전달한다.



---

## D-024 — Server Control Plane은 Go를 기본 후보로 사용

**Status:** Recommended baseline

Floe Server의 주요 workload는 HTTP/RPC, device session, auth, connector coordination, database/model provider 호출 등 I/O 중심 control-plane이다.

성능 hot path는 Device/Rust 쪽에 유지하고 Server application layer는 Go의 단순한 concurrency와 운영성을 우선한다.

---

## D-025 — Persistence foundation은 Turso Database를 사용

**Status:** Recommended baseline

plain SQLite 대신 Turso Database를 Device local persistence와 Server/Hosted database의 기본 후보로 사용한다.

Hosted에서는 Turso Cloud, Self-host에서는 self-host 가능한 Turso 구성을 우선 검토한다.

Turso Sync를 Floe application authorization과 동일시하지 않으며 실제 replication topology는 PoC 후 결정한다.
