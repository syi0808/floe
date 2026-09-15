# 02 — Connections·Actions·Day·Knowledge·Go의 실제 owner

대응: 계획 §3.2 / P05·P06·P08·P10·P20. 01의 계약을 사용한다. 다음은 [03](03-admission-inference.md).

## 02.1 Connections의 설정·관측·Operation을 분리

**읽기:** [S13 PairingService:L1–118](../SOURCE_ANCHORS.md#s13), [S14 Widget catalog:L130–240](../SOURCE_ANCHORS.md#s14), [S15 OAuth:L45–250](../SOURCE_ANCHORS.md#s15). `crates/modules/connections/src/api.rs`, `ports/` 및 `LocalServerClient`의 connector 관련 method도 직접 읽는다.

**목표:** 기존 Connections crate의 `api.rs`, private `domain/{connection,operation}.rs`, `application/{service,operations,refresh}.rs`, `ports/{repository,remote_control}.rs`.

1. `ConnectionIntent`(사용자가 원한 연결/해제/범위), `ConnectionObservation`(제공자 상태·관측 시점), `RefreshIssue`, `ConnectionOperation`을 별도 값으로 만든다. `connected: bool` 하나로 네 사실을 대체하지 않는다.
2. Operation에는 ID·principal·command identity·kind·connection ID·expected generation·remote attempt ref·state·bounded issue를 둔다. 런타임 handle map에는 cancellation/join만 두고 두 번째 상태 원본을 만들지 않는다.
3. 상태는 `Accepted → Running → AwaitingUser | Reconciling → Succeeded/Failed/Cancelled/Indeterminate`를 구현한다. 사용자 취소 의도와 원격 취소 확정은 구별한다. 실제 provider가 조회를 지원하지 않는 불확실 write는 Indeterminate로 남긴다.
4. `get_connections/get_operation/preview_grant`는 저장된 snapshot을 읽는다. 새 관측이 필요하면 `refresh_connections` command를 수락한다. query 경로에서 OAuth 시작·reconcile mutation·Run cancel을 호출하지 않는다.
5. 저장 port `admit_operation`은 dedup과 초기 레코드를 같이 commit한다. `settle_operation(expected_revision, expected_connection_generation, outcome)`은 늦은 응답을 CAS로 거절한다. 새 연결 generation을 취소된 이전 작업이 덮지 못하게 한다.
6. 기존 PairingService의 UUID/proof/response status 검증과 child deadline 처리를 재사용한다. accepted 원격 token은 credential sink로 보내고 Operation snapshot/issue/Debug에는 넣지 않는다.

**A 완료:** 원격 실행을 소유하는 실제 async service와 port가 있다. repository 구현은 05, UI 연결은 07에서 끝낸다. 지금은 Widget 상태를 복제해 canonical로 선언하지 않는다.

## 02.2 Widget의 reconciliation·OAuth 루프를 이관

**원본:** S14 `_loadCatalog → _synchronizeServerCalendar`; S15 `_connect/_poll/_cancel/_waitForPoll`와 `pollGeneration/pollTimer/attempt`.

1. `_synchronizeServerCalendar`의 connection 선택·변경 로직은 Connections reconciliation use case로 옮긴다. Day mirror 갱신은 주입한 projection sink로 전달한다. Connections→Day 금지 의존을 추가하지 않는다.
2. catalog 읽기 실패는 observation의 stale/unknown + RefreshIssue다. local disconnect로 바꾸지 않는다. 명시 `Disconnect`만 intent와 generation을 변경한다.
3. `_poll`의 타이머와 5분 deadline은 앱 수명의 Operation runner로 이동한다. 매 wake 전에 generation/cancellation/deadline을 검사하고 모든 종료 경로에서 join/타이머를 정리한다.
4. UI는 검증된 authorization URL의 브라우저 열기만 중계한다. 응답은 operation ID + host epoch에 bind한다. 화면 dispose는 구독 해제이고 `_cancel`은 명시 command다.
5. `_connect` 성공 뒤 `onChanged` 실패를 같은 연결 실패로 만들지 않는다. durable success receipt는 그대로 두고 projection sync issue만 표시한다.
6. UI의 secret/scope text controller·선택 상태는 UI에 남긴다. secret은 전송 직후 지우고 일반 read model에 보관하지 않는다.
7. 실제 UI 교체는 07에서 수행한다. 이 단계에서 이미 전환 가능한 service caller는 바꾸되, source 원본 삭제 전 남은 `LocalServerClient` 직접 호출 목록을 원장에 남긴다. 이 목록은 현재 원장 일부이지 별도 배정표가 아니다.

**A 검사:** Operation 정책의 타입과 종료 경로 검토. 의미 변경이 있는 generation fence/중복 외부 효과만 좁게 확인한다. 실제 OAuth 로그인은 B까지 요구하지 않는다.

## 02.3 Actions를 Core에서 업무 모듈로 이동

**읽기:** [S16 L1–165](../SOURCE_ANCHORS.md#s16), [S17 L290–515](../SOURCE_ANCHORS.md#s17), [S18 exports](../SOURCE_ANCHORS.md#s18). `agent_action.rs`와 `agent_vault`의 action admission/approval 구현을 해당 export에서 따라간다.

**목표:** `crates/modules/actions/src/{api,domain,application,ports}`. 기존 approval/preflight/recovery를 이동하며 Action 프레임워크를 다시 만들지 않는다.

1. `CalendarAction`, `CalendarMutation`, 상태/정책/receipt·`AgentActionOrigin`·제안 참조와 validation을 Actions에 배치한다. `ActionAuthority`의 허가 결정은 Access 공개 API를 통해 공급받고 Actions가 grant를 직접 확장하지 않는다.
2. `propose/draft/direct/decide/execute/recover_calendar_action`, `action_block_reason`, `CalendarCreateReceipt::matches`의 판단은 Actions service로 옮긴다. `FloeCore` 전체나 raw store handle을 인자로 받지 않는다.
3. action repository는 expected revision/state 비교 + proposal/approval/dispatch/receipt 저장을 원자 업무 단위로 제공한다. 실제 Calendar write보다 durable intent commit이 앞서야 한다.
4. preflight의 Person/provider/calendar/permission/timezone/conflict 검증, source observation과 connection revision 재검증, expiry 재확인을 그대로 유지한다. approval 뒤 payload가 바뀌면 재승인한다.
5. `Unknown`은 이름을 Indeterminate로 정리할 수 있어도 의미는 유지한다. `recover`는 lookup으로 같은 execution identity를 관측할 뿐 create를 다시 호출하지 않는다.
6. Day에 필요한 event/mirror read port를 사용한다. agent-origin action과 direct user action의 승인 경로는 명시 분리한다. 둘을 `direct=true` 한 값으로 무조건 우회하지 않는다.
7. Expert는 proposal ref를 반환한다. Expert completion이 사용자 승인을 의미하지 않는다. Schedule가 action table을 직접 쓰지 못하게 한다.

**삭제:** Core 안의 이관 완료 업무 판단. 실저장 SQL은 05까지 남더라도 Actions 정책을 중복 정의하지 않는다.
**A 검사:** 승인→intent→effect 순서의 최소 통제 검증. 현재 사용자 계정의 Calendar write는 하지 않는다.

## 02.4 Day와 Knowledge의 잔여 공개 경계

**읽기:** [S19 Day lib:L1–15](../SOURCE_ANCHORS.md#s19), [S20 Knowledge lib:L1–58](../SOURCE_ANCHORS.md#s20), [S21 repository:L1–33](../SOURCE_ANCHORS.md#s21). Core exports(S18)에서 store·context_evidence·context_history의 직접 caller를 확인한다.

1. Day의 기존 `DayService`, domain, projection, classification, repository 정책을 재작성하지 않는다. Core의 남은 Day wrapper를 공개 API 직접 호출로 바꾼다. Calendar mirror provenance, capture classification, event/task/note revision은 유지한다.
2. Knowledge의 `pub mod application` 공개를 좁힌다. 실제 외부 소비자가 사용하는 `LearnerService`, `MemoryContextReader`, review/playbook API만 `api` 또는 crate root에서 노출한 뒤 private application을 만든다. public API를 잠그기 전에 모든 직접 import를 전환한다.
3. learner/review의 evidence membership·candidate revision·source hash·user-only decision·compaction placeholder 제외를 업무 API와 transaction-bound evidence port에 유지한다. Context/Conversation을 Knowledge가 역으로 import하게 만들지 않는다.
4. 남은 Memory review/stage 저장은 owner port `commit_review` 등으로 선언하고 05에서 실제 SQL과 연결한다. 검증과 mutation이 별개 transaction 사이에서 엇갈리지 않게 한다.
5. learner background handle은 Knowledge service가 소유하고 낮은 quota/독립 cancellation으로 실행한다. learner 실패가 root Run이나 UI 전체 busy를 변경하지 않는다. 모델 호출 정산은 04의 Inference service로 위임한다.
6. 실제로 사라진 기능의 타입/fixture만 삭제한다. 기존 policy 알고리즘을 단순화하거나 학습 품질 개선 연구를 추가하지 않는다.

## 02.5 Go Console을 transport와 업무 owner로 분리

**읽기:** [S22 console.go:L1–240](../SOURCE_ANCHORS.md#s22), `server/internal/console/trust_store.go`, `server/cmd/floe-server/main.go`, `server/internal/application/bootstrap.go`.

1. `Console` 필드를 분류한다. `sessions/loginAttempts/loginWindow` 등 웹 로그인·CSRF·rate limit은 HTTP/admin transport에 남긴다. `state/pair/connectorAttempts/connectorReservations/connectionLifecycles`의 업무 상태는 Connections/Pairing 서비스로 옮긴다.
2. trust store·producer identity·authorization admission은 기존 authorization engine과 주입한 store port가 소유한다. `consoleTrustStore{console:*Console}`를 실제 store adapter로 교체한다. application→console import를 만들지 않는다.
3. Console의 provider 객체 map은 composition에서 provider registry로 주입한다. facade가 전체 state lock을 잡은 채 네트워크를 기다리지 않게 한다. connection별 serialization과 identity fence는 보존한다.
4. 기존 `internal/inference.Gateway`와 optional bootstrap 실패 격리를 그대로 사용한다. exact-recipient 승인 검사나 signed challenge bytes를 앱 schema 정리와 함께 바꾸지 않는다.
5. `main.go`는 서비스·저장·HTTP handler 조립만 한다. 외부 HTTP path를 바꿀 필요가 없으면 그대로 둔다. provider OAuth의 실서버 credential 원본은 여전히 Go에 있다. Rust Operation은 사용자 의도·관측/remote ref를 소유하며 그 권한을 대체하지 않는다.
6. `console/web` embed 자산을 transport로 옮기면 embed 경로와 caller도 같이 갱신한다. 기존 concrete provider 파일까지 불필요하게 이름을 바꾸지 않는다.

**단계 종료:** 실제 업무 정책과 port가 정리되었고 남은 adapter/caller가 식별되어 있다. `go build`/관련 정적 검사는 가능하면 수행한다. live provider validation은 B다. 다음 작업은 03의 Conversation canonical 수락이다.
