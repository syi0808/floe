# Vertical Slice Delivery

> Status: Accepted delivery approach; slices not yet implemented
>
> Date: 2026-09-04
>
> Decision: [ADR 0006](../../decisions/0006-slice-driven-delivery.md)

## 목적과 문서 역할

Phase는 제품 범위 지도, slice는 구현·검증·인수 단위다. 모든 Phase를 조금씩
구현하는 대신 하나의 사용자 시나리오를 실제 경계 전체에 연결한다.

- 이 문서: slice 범위, 의존성, 인수 조건, 테스트와 운영 규칙.
- [PROGRESS.md](../../../PROGRESS.md): 현재 상태와 검증 근거의 단일 기록.
- [Roadmap](../00-overview/roadmap.md): 장기 기능 범위와 미검증 영역.
- ADR: 진행 방식과 아키텍처 결정의 변경 이유.

## 중심 시나리오

> 오늘 일정을 읽고 내 선호를 고려해 집중 시간을 제안하며, 승인하면 외부
> 캘린더에 반영하고 Day Canvas에서 결과를 확인한다.

```text
Calendar Connector → 정규화·출처·Person-scoped store
→ Schedule Expert + 최소 Personal Memory → Manager
→ Action Proposal → App 근거 표시·명시적 승인
→ Policy → Validation → Permission Check → Deterministic Executor
→ Calendar Connector → 재수집 → Day Canvas 갱신
```

첫 루프는 macOS, 한 Person, Calendar connector 하나의 전체 캘린더, built-in Expert 하나,
집중 일정 생성 action 하나로 제한한다. Flutter는 화면과 승인 입력을 담당하고
canonical 변경은 Rust typed command를 거친다. Expert는 제한된 view와
capability를 사용하며 connector 자격증명이나 DB에 직접 접근하지 않는다.

Manager/Schedule Expert와 OS lifecycle을 담당하는 Device Agent를 구분한다.
S1–S3은 앱 실행 중 동작해도 되며, resident Device Agent 경계는 S5에서 검증한다.

## S1 — Connected Calendar Read

**사용자 결과:** macOS Calendar에 연결된 전체 캘린더의 일정이 하나의 Day Canvas에
통합 표시된다. 캘린더를 하나 고르거나 전환하지 않으며 각 일정의 출처는 유지한다.

2026-09-04 사용자 피드백에 따라 단일 캘린더 선택 범위를 변경했다.
계정·캘린더 목록은 연결 관리에서 확인하고, 날짜별 읽기는 전체 연결 범위에 적용한다.
새 캘린더는 다음 명시적 새로고침에 포함한다. 일부 캘린더 읽기 실패는 다른 캘린더의
성공 결과를 막지 않으며 실패한 출처의 캐시는 보존한다. 자세한 계약은
[ADR 0008](../../decisions/0008-unified-calendar-read.md)을 따른다.
현재 native 구현은 단일 선택 방식이므로 이 변경은 후속 구현·검증이 필요하다.

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

## S2 — Contextual Suggestion

**사용자 결과:** “오늘 언제 집중하면 좋을까?”에 실제 일정과 선호를 근거로 답한다.

**의존성:** S1 Verified 이상, 실제 모델 provider 선택.

built-in Schedule Expert 하나가 제한된 일정 view를 판단하고 Manager가
구조화된 제안을 전달한다. 최소 memory는 사용자가 직접 입력한 집중 시간
선호 한 개로 시작한다. 자동 memory 추출이나 identity resolution은 포함하지 않는다.

### Acceptance criteria

- **S2-A1:** 앱에서 선호를 저장·조회·수정·삭제할 수 있고 출처와 Person 범위를
  유지한다. 삭제한 선호는 이후 제안 context에서 제외된다.
- **S2-A2:** 실제 모델을 사용해 일정·선호를 참조하는 구조화된 집중 시간 제안을
  만들고 앱에 시간, 이유, 근거 출처를 표시한다.
- **S2-A3:** 제안 출력의 schema와 시간 범위를 검증하며, 제안만으로 일정이나
  task를 변경하지 않는다. Expert는 host-mediated view/capability만 사용한다.
- **S2-A4:** 모델 timeout, malformed output, 빈 일정, 삭제된 선호의 평가 사례와
  앱 오류 상태를 검증한다. 실제 모델 평가 결과와 알려진 품질 한계를 기록한다.

외부 모델에는 필요한 최소 context만 전달하고 전송 범위를 사용자에게
설명한다. 자격증명은 context·domain record·일반 로그에 포함하지 않는다.

**제외:** 자동 행동, 다중 Expert 협업, SDK, 외부 package 실행, 장기 기억 추출.

## S3 — Approved Calendar Action

**사용자 결과:** 제안의 상세 내용을 승인하면 집중 일정 하나가 외부에 생성된다.

**의존성:** S2 Verified 이상, 선택 provider의 생성 capability 검증.

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

## S4 — Same Loop Across Devices and Server

**사용자 결과:** 한 클라이언트에서 수행한 흐름을 다른 클라이언트에서 확인한다.

**의존성:** S3 Accepted, sync·identity·storage 보안 PoC.

최소 Go 서버와 재현 가능한 self-host 실행 경로, 두 클라이언트만 다룬다.
실제 기기/플랫폼 조합, provider 실행 위치, sync topology는 착수 전에 결정한다.
두 로컬 인스턴스 데모만으로 cross-device 검증을 완료하지 않는다.

### Acceptance criteria

- **S4-A1:** clean environment에서 문서화된 절차로 최소 서버를 실행하고 실제
  두 기기를 같은 Person에 인증·연결할 수 있다.
- **S4-A2:** 일정과 실행 결과가 두 기기에 수렴한다. offline 재연결, 충돌,
  중복 전달로 인한 중복 실행을 검증한다.
- **S4-A3:** 다른 Person의 접근과 철회된 기기의 신규 접근을 차단한다.
  전송/저장 보호, credential 보관, 삭제 전파 정책을 명시하고 검증한다.
- **S4-A4:** device-native Calendar의 실행 기기가 offline이면 실행을 보류하거나
  명시적으로 실패시킨다. 서버가 로컬 OS capability를 가진 것으로 가정하지 않는다.

**제외:** 전 플랫폼 parity, multi-user 관리 UI, OAuth broker, hosted 운영 완성.

## S5 — Event-driven Intervention

**사용자 결과:** 일정 변경으로 제안이 무효해지면 적절한 시점에 재제안을 받는다.

**의존성:** S4 Accepted; local lifecycle PoC는 필요할 때 앞당길 수 있다.

### Acceptance criteria

- **S5-A1:** macOS UI가 닫힌 동안에도 resident Device Agent가 일정 변경을
  감지하고 재제안한다. 프로세스 재시작 후에도 동작한다.
- **S5-A2:** quiet hours, opt-out, 중복 억제와 두 기기 사이 알림 중재를 검증한다.
- **S5-A3:** 알림에서 앱의 근거·승인 화면으로 이어지며 외부 쓰기는 S3의
  동일한 승인·검증 경계를 통과한다.
- **S5-A4:** 구조화된 dogfood 기록으로 유용한 개입, 불필요한 개입, 누락을
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
| 0 — PoCs | S1 connector, S2 model/expert, S3 executor, S4 sync/security | Health, 음성 및 나머지 PoC |
| 1 — Personal Day | S1 Day Canvas, S2 Manager, S3 승인 UI | 편집·folding·음성·MVP dogfood |
| 2 — Connected | S1 Calendar, S2 Schedule Expert, S3 생성 | Gmail, Contacts, Health, 추가 Expert |
| 3 — Memory | S2 명시적 선호와 출처·수정·삭제 | People, Episode, 자동 추출, identity resolution |
| 3.5 — Experts | S2 built-in contract와 capability 경계 | 외부 package, sandbox, SDK, marketplace |
| 4 — Cross-device | S4 두 기기 sync, S5 resident lifecycle | iOS/Android/Windows 전체 경험 |
| 5 — Ambient | S5 변경 감지와 개입 | wake word, 전사, speaker recognition, 음성 handoff |
| 6 — Hosted/Self-host | S4 최소 Go 서버와 배포 | admin, 다중 사용자 운영, broker, hosted 완성 |

이 표는 계획된 coverage다. 실제 검증 여부는 `PROGRESS.md`에서만 갱신한다.
