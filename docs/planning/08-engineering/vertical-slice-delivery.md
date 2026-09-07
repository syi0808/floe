# Vertical Slice Delivery

> Status: Accepted delivery approach; S1/S3 in progress, later slices planned
>
> Date: 2026-09-04
>
> Decision: [ADR 0006](../../decisions/0006-slice-driven-delivery.md), amended by
> [ADR 0012](../../decisions/0012-memory-and-expert-first-slices.md)

## 목적과 문서 역할

Phase는 제품 범위 지도, slice는 구현·검증·인수 단위다. 모든 Phase를 조금씩
구현하는 대신 하나의 사용자 시나리오를 실제 경계 전체에 연결한다.

- 이 문서: slice 범위, 의존성, 인수 조건, 테스트와 운영 규칙.
- [PROGRESS.md](../../../PROGRESS.md): 현재 상태와 검증 근거의 단일 기록.
- [Roadmap](../00-overview/roadmap.md): 장기 기능 범위와 미검증 영역.
- ADR: 진행 방식과 아키텍처 결정의 변경 이유.

## 중심 시나리오

> 오늘 일정을 읽고, Note에서 확정한 개인 맥락을 Schedule Expert가 참고해
> Manager에게 조언한다. 사용자는 근거를 확인하고 외부 캘린더 작업을 승인한 뒤
> Day Canvas에서 결과를 확인한다.

```text
Calendar Connector → 정규화·출처·Person-scoped store
                                      ┐
Note Evidence → Memory Candidate → Review → Confirmed Memory
                                      ┘
→ bounded Views → Schedule Expert → Manager → Action Proposal
→ App 근거 표시·명시적 승인
→ Policy → Validation → Permission Check → Deterministic Executor
→ Calendar Connector → 재수집 → Day Canvas 갱신
```

첫 루프는 macOS, 한 Person, Calendar connector 하나의 전체 캘린더와
일정 생성 action 하나로 제한한다. Flutter는 화면과 승인 입력을 담당하고
canonical 변경은 Rust typed command를 거친다. S4는 여기에 source-backed
Personal Memory를, S5는 같은 로컬 경계 안에 Manager와 Expert를 연결한다.
Expert는 제한된 view와 capability를 사용하며 connector 자격증명이나 DB에
직접 접근하지 않는다.

사용자와 대화하고 Expert 결과를 취합하는 Manager agent와 OS lifecycle을 담당하는
Device Agent를 구분한다. S5는 전자의 orchestration contract만 검증한다.
S1–S5는 앱 실행 중 동작해도 되며, resident Device Agent 경계는 S7에서 검증한다.

## S1 — Connected Calendar Read

**사용자 결과:** macOS Calendar에서 포함한 캘린더의 일정이 하나의 Day Canvas에
통합 표시된다. 타임라인에서 출처를 전환하지 않으며 각 일정의 출처는 유지한다.

2026-09-04 사용자 피드백에 따라 단일 캘린더 선택 범위를 변경했다.
2026-09-05 결정: 부분 선택은 명시한 목록을 유지하고, 전체 모드만 새 캘린더를
다음 새로고침/날짜 읽기에 자동 포함한다. 기존 연결은 부분 선택으로 유지한다.
계정·캘린더 목록과 모드는 연결 관리에서 확인한다. 일부 캘린더 읽기 실패는 다른 캘린더의
성공 결과를 막지 않으며 실패한 출처의 캐시는 보존한다. 자세한 계약은
[ADR 0008](../../decisions/0008-unified-calendar-read.md)을 따른다.
프로토타입 UI 이후 native 두 모드와 출처별 복구를 구현했으며 실기기 acceptance는 남아 있다.

**의존성:** 기존 Flutter ↔ Rust ↔ Turso 기반.

macOS EventKit/OS Calendar를 우선 검토하되 provider는 아직 확정하지 않는다.
실제 읽기와 S3의 생성 권한을 짧은 PoC로 확인하고 선택 이유를 기록한다.
다른 provider가 필요하면 native Rust/Go 실행 경계를 유지하며 계획을 갱신한다.
S1 자체는 읽기 전용으로 동작하며 쓰기 권한을 미리 요구하지 않는다.
단, 2026-09-04 사용자 승인에 따라 읽기에도 전체 접근이 필요한 EventKit의
OS 권한은 예외로 허용한다. 앱의 외부 쓰기 기능은 포함하지 않는다.
선택 근거와 미검증 PoC는 [ADR 0007](../../decisions/0007-eventkit-calendar-read.md)을 따른다.

### Acceptance criteria

- **S1-A1:** 실제 provider 권한 요청·전체 캘린더 연결·목록 확인·문제 상태 표시가 앱에서 가능하며,
  거절 또는 권한 철회 시 typed error와 재연결 경로를 제공한다.
- **S1-A2:** 전체 연결 캘린더의 선택한 날짜 범위 일정이 stable external ID, connection/Person,
  revision 또는 동등한 변경 식별자, provenance와 함께 저장·표시된다.
  calendar ID/account 출처를 구분하고 all-day와 timezone 경계의 표시를 검증한다.
- **S1-A3:** 재수집해도 중복되지 않고 외부 수정·삭제가 반영된다. 제한된
  조회 범위 밖이나 실패한 캘린더의 항목을 삭제된 것으로 잘못 처리하지 않는다.
  새 캘린더 포함, 일부 출처 실패, 사라진 캘린더의 캐시 보존도 검증한다.
- **S1-A4:** 앱/core 재시작 후 일정이 유지된다. 수집 실패 시 마지막 데이터와
  stale/error 상태를 표시하고 재시도할 수 있다.

**제외:** LLM, 외부 쓰기, 다중 connector, 범용 ConnectorSpec 엔진.

## S3 — Approved Calendar Action

**사용자 결과:** 제안의 상세 내용을 승인하면 집중 일정 하나가 외부에 생성된다.

**의존성:** S1 Verified 이상, 선택 provider의 생성 capability 검증.

### Acceptance criteria

- **S3-A1:** 앱에서 대상 캘린더·제목·시작/종료·timezone을 확인하고 승인 또는
  거절한다. 승인 전이나 거절 후에는 외부 변경이 없다.
- **S3-A2:** 실행 직전에 Policy, Person, capability, 권한, 현재 일정과 제안의
  유효성을 재검증한다. 오래된 제안이나 충돌은 차단하고 재제안/재승인한다.
- **S3-A3:** 중복 클릭·재시도·재시작으로 중복 일정이 생기지 않는다. durable
  실행 ID와 상태를 보존하고 결과가 불명확하면 조회·대조 후 복구하며 맹목적으로
  재실행하지 않는다. provider 제약으로 안전한 복구가 불가능하면 사용자 확인을 요구한다.
- **S3-A4:** 실제 provider에 생성된 일정이 재수집되어 Day Canvas에 나타나고
  proposal → 승인 → 실행 결과 → external ID를 추적할 수 있다.
- **S3-A5:** 쓰기 권한 거절/철회, provider timeout, 부분 실패 시 오류와 복구
  동작을 검증한다. 모델은 executor를 우회해 외부 API를 호출할 수 없다.

**제외:** 일정 이동·삭제, 메일 전송, 자동 승인, 범용 workflow 엔진.

## S4 — Reviewable Personal Memory

**사용자 결과:** 사용자가 Floe Note에 남긴 선호나 약속에서 만들어진 Memory
candidate의 근거를 확인하고 저장한 뒤, Memory 화면에서 수정하거나 완전히 잊게
할 수 있다.

**의존성:** S3 Accepted, Person-scoped local store, P0-F의 local vault/key boundary.
모델 기반 추출은 P0-D의 고정 평가 세트를 먼저 통과해야 하며, 모델 없이도 fixture
extractor로 전체 정책 경계를 재현할 수 있어야 한다.

첫 범위는 한 Person, Floe가 소유한 Note evidence, Preference와 Commitment 두
memory type으로 제한한다. 자동 수집이나 외부 이메일 ingestion보다 evidence →
candidate → review → authoritative memory → projection → edit/delete의 수명주기를
먼저 검증한다. 사용자 명시 입력과 모델 inference를 구분하며 candidate는 저장 전
권한을 갖지 않는다.

### Acceptance criteria

- **S4-A1:** Note evidence와 immutable source reference에서 typed MemoryCandidate를
  만들고 extraction version, observed time, confidence, fact/inference 구분을 보존한다.
  재처리해도 같은 candidate나 authoritative memory가 중복되지 않는다.
- **S4-A2:** inferred candidate는 공용 Review request에서 source excerpt와 변경될
  structured fields를 확인한 뒤 저장·거절할 수 있다. 거절·보류 상태는 장기 Memory
  view나 Manager context에 나타나지 않으며 민감도 정책 위반은 저장 전에 차단된다.
- **S4-A3:** 저장된 Memory의 source, 현재 값, 시간 유효성, confidence를 앱에서
  확인하고 수정·삭제할 수 있다. 수정은 provenance를 잃지 않는 새 revision으로
  남고, 삭제는 projection, 검색 index, embedding, cache에서 전파 여부를 검증한다.
- **S4-A4:** 앱/core 재시작과 extractor version 변경 후에도 evidence, decision,
  revision, tombstone이 일관된다. 고정 corpus에서 false memory와 false merge를
  기록하고 삭제된 Memory가 재컴파일로 되살아나지 않는다. 실제 개인 데이터는
  local at-rest protection과 key-unavailable fail-closed 동작을 검증한 build에서만
  dogfood한다.

### 구현 increment

1. fixture Note 하나를 evidence/candidate로 저장하고 재시작 후 같은 Review에 복구한다.
2. Review 결정이 confirmed Memory projection에만 반영되고 거절 시 나타나지 않게 한다.
3. Memory 화면에서 source 확인, revision edit, forget과 파생 데이터 정리를 연결한다.
4. versioned extractor corpus, 재컴파일, key failure와 삭제 회귀를 자동·live 검증한다.

**제외:** 이메일·transcript 자동 ingestion, 범용 knowledge graph, 관계 자동 병합,
cross-device memory sync, 무검토 inferred memory 저장, vector database 선정 확정.

## S5 — Manager and Expert Advice Loop

**사용자 결과:** Manager가 오늘 일정과 사용자가 확정한 Memory를 바탕으로
Schedule Expert의 조언을 받아 근거 있는 집중 시간 제안을 하나 보여준다. 사용자가
실행을 선택하면 기존 S3 승인·실행 경계를 그대로 통과한다.

**의존성:** S4 Accepted, S1 calendar view, S3 action gate. sync/account server나
resident Device Agent 없이 macOS 앱 실행 중인 local Expert host에서 먼저 검증한다.
실제 model runner가 기기 밖에 있으면 기존 inference class와 전송 동의 경계를 따른다.

첫 범위는 manual `plan my day` trigger, built-in Schedule Expert 하나, 동작을 비교할
deterministic declarative fixture Expert 하나로 제한한다. ExpertPackage,
Installation, Person별 Assignment를 분리하고 두 구현이 동일한 invocation/result
contract를 사용하게 한다. 실제 모델 평가는 고정 시나리오 세트로 별도 기록한다.

### Acceptance criteria

- **S5-A1:** registry가 package/version, installation enablement, Person assignment,
  trigger, granted permissions와 private state namespace를 보존한다. built-in과
  declarative fixture Expert가 동일한 bounded invocation과 structured result
  contract로 실행된다.
- **S5-A2:** Expert는 허용된 Timeline/Memory projection만 읽고 DB, source evidence,
  connector credential에는 접근하지 못한다. 미승인 view/capability 호출을 거부하고
  assignment 간 state 격리와 disable/revoke의 즉시 적용을 검증한다.
- **S5-A3:** Expert는 InsightCandidate 또는 ActionProposal만 반환한다. Manager가
  근거, 충돌, freshness를 검토해 사용자에게 하나의 응답으로 합성하며 Expert가
  직접 UI를 표시하거나 Calendar를 변경하지 못한다. 실행은 S3 policy, review,
  validation, idempotent executor를 재사용한다.
- **S5-A4:** trigger → granted views → Expert result → Manager decision → Review/Activity를
  민감 원문 없이 추적·재현할 수 있다. timeout, malformed output, budget 초과,
  Expert 하나의 실패가 다른 상태를 손상시키지 않으며 고정 시나리오에서 근거성,
  유용성, 불필요한 제안 비율을 기록한다.

### 구현 increment

1. fixture package를 install/assign하고 manual trigger 결과와 private state를 복구한다.
2. Timeline/Memory view handle과 deny-by-default capability를 연결해 거부 사례를 통과한다.
3. Schedule Expert 결과를 Manager의 한 응답과 S3 ActionProposal로 end-to-end 연결한다.
4. real-model scenario, revoke, timeout, malformed output, budget와 trace 회귀를 검증한다.

**제외:** Marketplace 배포·결제, arbitrary code/Wasm Expert, Health/Mail Expert,
background trigger, multi-agent 대화 UI, Expert의 직접 action/memory mutation,
동적 model router.

## S6 — Same Loop Across Devices and Server

**사용자 결과:** 한 기기에서 확정한 Memory와 수행한 제안·실행 결과를 다른
기기에서 확인한다.

**의존성:** S5 Accepted, sync·identity·storage 보안 PoC.

최소 Go 서버와 재현 가능한 self-host 실행 경로, 두 클라이언트만 다룬다.
실제 기기/플랫폼 조합, provider 실행 위치, sync topology는 착수 전에 결정한다.
두 로컬 인스턴스 데모만으로 cross-device 검증을 완료하지 않는다.

### Acceptance criteria

- **S6-A1:** clean environment에서 문서화된 절차로 최소 서버를 실행하고 실제
  두 기기를 같은 Person에 인증·연결할 수 있다.
- **S6-A2:** 일정, confirmed Memory revision/tombstone, active Expert assignment와
  Review/실행 결과가 두 기기에 수렴한다. offline 재연결, 충돌, 중복 전달로 인한
  Memory 부활이나 중복 실행이 없음을 검증한다.
- **S6-A3:** 다른 Person의 접근과 철회된 기기의 신규 접근을 차단한다.
  전송/저장 보호, credential 보관, 삭제 전파 정책을 명시하고 검증한다.
- **S6-A4:** device-native Calendar의 실행 기기가 offline이면 실행을 보류하거나
  명시적으로 실패시킨다. 서버가 로컬 OS capability를 가진 것으로 가정하지 않는다.

**제외:** 전 플랫폼 parity, multi-user 관리 UI, OAuth broker, hosted 운영 완성.

## S7 — Event-driven Intervention

**사용자 결과:** 일정 변경으로 제안이 무효해지면 적절한 시점에 재제안을 받는다.

**의존성:** S6 Accepted; local lifecycle PoC는 필요할 때 앞당길 수 있다.

### Acceptance criteria

- **S7-A1:** macOS UI가 닫힌 동안에도 resident Device Agent가 일정 변경을
  감지하고 재제안한다. 프로세스 재시작 후에도 동작한다.
- **S7-A2:** quiet hours, opt-out, 중복 억제와 두 기기 사이 알림 중재를 검증한다.
- **S7-A3:** 알림에서 앱의 근거·승인 화면으로 이어지며 외부 쓰기는 S3의
  동일한 승인·검증 경계를 통과한다.
- **S7-A4:** 구조화된 dogfood 기록으로 유용한 개입, 불필요한 개입, 누락을
  평가한다. 일정/알림은 deterministic scheduler가 담당한다.

**제외:** wake word, 상시 녹음, 회의 전사, speaker recognition, 음성 handoff.

## 구현 운영과 완료 상태

1. 착수 시 slice의 선행 조건, 실제 provider, 제외 범위, 인수 조건을 확정한다.
2. 작업을 계층 전체가 아니라 앱에서 확인 가능한 작은 end-to-end 변경으로 나눈다.
3. 같은 운영 계약의 fixture connector/model로 경계를 연결한 뒤 실제 의존성을
   하나씩 교체한다. 실제 provider PoC는 초기에 실행해 후반 통합 실패를 줄인다.
4. 각 변경은 관련 테스트와 진행 기록을 포함한 논리적 commit으로 남긴다.
5. 데모와 실패 사례를 검증한 뒤 dogfood를 거쳐 인수한다.

| 상태 | 진입 조건 |
| --- | --- |
| Planned | 범위와 인수 조건이 정의됨; 구현 완료를 뜻하지 않음 |
| Implementing | 선행 조건을 확인하고 구현 착수 |
| Integrated | 앱에서 전체 경로가 동작; fixture만 쓰면 그 사실을 표시 |
| Verified | 모든 인수 조건 통과, 실제 의존성 검증과 회귀 테스트 근거 기록 |
| Dogfooding | Verified build를 실사용하며 정해진 시나리오와 문제 기록 |
| Accepted | dogfood 결과 검토, 필수 조건 충족, 차단 결함 없음 |

진행 중인 구현 slice는 하나만 둔다. 다음 slice 착수 시 기존 로컬 slice의
비차단 작업은 Deferred로 명시하고 기존 완료 기록은 유지한다. Dogfooding은
다음 slice와 병행할 수 있다. blocker는 상태와 별도로 원인·해소 조건을 기록한다.
인수 조건 회귀가 발견되면 상태를 되돌리고 근거를 남긴다.

각 slice의 dogfood 기간과 관찰 질문은 착수 시 정하고 결과를 기록한다.
이는 Personal Day MVP의 별도 2주 dogfood 요건을 대체하지 않는다.

## 검증 근거 형식

각 Sx-Ay에 다음 정보를 연결한다. 통과 수는 구현률이 아니라 검증된 조건 수다.

```text
Criterion: S1-A1
Result: pending | pass | fail
Integration: fixture | sandbox | live (connector/model/server/device별 기록)
Evidence: test command + result 또는 수동 데모 절차 + 관찰 결과
Build: 검증한 commit SHA
Date / environment: 날짜, OS, provider, 모델 및 설정(해당 시)
Known limitations / blocker: 남은 제약과 해소 조건
```

- CI/자동 회귀: 고정 clock/timezone, fixture provider, deterministic model 대역.
- 경계 테스트: connector 정규화, Rust typed command, snapshot, Flutter UI,
  proposal validation, 실행 상태 머신과 재시작 복구.
- 실제 연동 smoke test: 권한, 수집, 생성·재수집을 전용 테스트 캘린더에서 검증.
  자격증명 없는 CI에 live 테스트 성공을 요구하거나 실패를 숨기지 않는다.
- 실제 모델 평가: 동일한 시나리오 세트로 근거·시간 유효성·실패 처리를 평가하며
  문장 완전 일치와 단일 성공 응답을 품질 기준으로 삼지 않는다.
- 재현 로그: source → context → proposal → execution → projection의 ID를
  연결하되 민감 원문과 자격증명은 기본적으로 남기지 않는다.

## Phase coverage — 완료 선언이 아닌 검증 지도

| Phase | 먼저 검증하는 slice 경계 | 여전히 별도 검증이 필요한 범위 |
| --- | --- | --- |
| 0 — PoCs | S1 connector, S3 executor, S4 memory compiler, S5 Expert contract, S6 sync/security | Health, 음성 및 나머지 PoC |
| 1 — Personal Day | S1 Day Canvas, S3 승인 UI | 편집·folding·음성·MVP dogfood |
| 2 — Connected | S1 Calendar, S3 생성, S5 Schedule Expert | Gmail, Contacts, Health, 추가 Expert |
| 3 — Memory | S4 Note 기반 Preference/Commitment lifecycle | People/Relationship/Episode, 외부 source, identity resolution |
| 3.5 — Experts | S5 local contract, assignment, permission, built-in/declarative 실행 | Wasm, SDK, marketplace, server placement |
| 4 — Cross-device | S6 두 기기 sync, S7 resident lifecycle | iOS/Android/Windows 전체 경험 |
| 5 — Ambient | S7 변경 감지와 개입 | wake word, 전사, speaker recognition, 음성 handoff |
| 6 — Hosted/Self-host | S6 최소 Go 서버와 배포 | admin, 다중 사용자 운영, broker, hosted 완성 |

이 표는 계획된 coverage다. 실제 검증 여부는 `PROGRESS.md`에서만 갱신한다.
