# 03 — 불변 command·durable admission·비동기 모델 준비

대응: 계획 §3.3 / P07·P12·P13·P14. 선행 01의 command/repository 계약, 02의 Connections 관측 port. 다음은 [04](04-runtime-recovery.md).

## 03.1 환경과 무관한 기존 command replay

**읽기:** [S07 coordinator:L1–220](../SOURCE_ANCHORS.md#s07), [S08 저장 exact_admission:L1–165](../SOURCE_ANCHORS.md#s08), [S09 root:L360–490](../SOURCE_ANCHORS.md#s09), [S10 host:L35–180](../SOURCE_ANCHORS.md#s10). 같은 coordinator 파일의 `turn_digest/verify_existing`도 검색해 읽는다.

1. 01.2의 canonical intent를 Conversation entry에서 한 번 만든다. digest 함수는 입력 정규화·직렬화 함수와 같은 private module에 둔다. caller마다 해시를 구현하지 않는다.
2. `find_command`는 검증된 principal의 repository namespace 안에서 조회한다. 같은 ID의 저장 record에 principal·intent digest·kind를 대조한다. StartTurn과 CancelRun의 cross-kind 재사용은 conflict다.
3. 기존 command라면 **현재 Vault/caller 접근과 원래 의도 동일성만 확인한 후 receipt 반환**한다. 새 revision 검사·model profile 가용성·Keychain model credential 조회·catalog 요청보다 앞에 둔다. 답변 원문 조회의 공개 권한 검사는 따로 수행한다.
4. 기존 `LegacyComposition::start_turn`의 receipt 사전 조회 뒤 `inference_routes.resolve(...)` 재호출을 제거한다. 새 app facade는 Conversation의 수락 API만 호출한다. 반환 receipt를 만들기 위해 provider를 다시 조회하지 않는다.
5. `request_context_digest`와 저장 `model_placement`의 equality를 canonical intent 비교에서 제거한다. 과거 row mapper는 만들지 않는다. 저장 의미 변경은 명시적으로 새 개발 profile이 필요함을 원장에 적는다.
6. Continue의 원래 Run chain·generation·level·session을 validation한다. exact replay라면 이미 저장된 continuation snapshot을 반환하고 새 source와 모델 환경으로 동일성을 재계산하지 않는다. 실제로 재개할 때의 전송 권한은 별도 재검사한다.

**좁은 A 검사:** 동일 입력의 normalized digest 안정성, 다른 payload/다른 namespace/cross-kind conflict. provider 호출 여부를 증명하는 통제된 전체 흐름은 B에서 검사한다.

## 03.2 수락과 실행을 두 lifecycle로 분리

**읽기:** S07의 `admit_turn → cancellation register → on_admitted → engine.drive`; [S05 port](../SOURCE_ANCHORS.md#s05); [S27 Worker](../SOURCE_ANCHORS.md#s27).

**목표:** `modules/conversation/src/application/{admission,coordinator,cancellation}.rs`와 owner repository. 현재 `ConversationService`를 수정한다. 별도 NewConversationService를 만들지 않는다.

```text
StartTurn
  validate caller + normalize immutable intent
  find existing -> verify -> same receipt
  local transaction:
    dedup -> Session revision/claim -> user entry + Accepted Run + command receipt
  register cancellation / enqueue by durable RunId
  return committed receipt

Owned runner
  claim by executor generation
  load accepted intent + policy/profile references
  prepare approved model asynchronously
  build authorized context lazily
  Engine -> finalization as permitted -> atomic terminal commit + Session release
```

1. 필요한 state 구분은 **하나의 canonical Run 모델**에 추가한다. `Accepted/Executing/Finalizing/Cancelling/Finished` 같은 lifecycle과 execution/reply report를 분리하되, 같은 의미의 legacy 상태 enum을 authoritative로 병행하지 않는다. target 값은 application/storage/wire가 함께 사용한다.
2. admission transaction에 command receipt·user message·Run·Session claim을 같이 넣는다. 인메모리 queue 성공을 durable 수락으로 표현하지 않는다. 다른 신규 command의 같은 Session은 SessionBusy/Conflict로 거절하고 기존 Run을 취소하지 않는다.
3. 수락 직후 queue가 가득 차거나 process가 죽는 경로를 구현한다. 이미 committed Run을 잊지 말고 제한된 scheduler의 재조회 대상 또는 RecoveryRequired로 남긴다. 큐는 RunId만 보관하고 두 번째 업무 원본이 아니다.
4. receipt가 공개되기 전 live cancel handle을 등록하거나, handle이 없어도 **durable cancel intent를 runner가 시작 전 읽도록** 한다. crash/restart 후에도 취소 요청이 무시되지 않아야 한다. 둘의 순서 문제를 on_admitted 콜백만으로 해결했다고 가정하지 않는다.
5. 동기 FFI는 bounded 로컬 admission/조회만 기다린다. 저장 자체의 응답이 늦으면 caller는 결과 미확정과 같은 command 재조회 안내를 받는다. 모델 네트워크 timeout을 FFI 대기 시간에 맞추는 방식으로 해결하지 않는다.
6. runner task handle과 cleanup은 Conversation owner가 관리한다. 원본 Worker의 전체 execute `block_on`/job map은 관리 I/O proxy와 Run runner로 분해하고 실제 자원 조립은 05/07에서 끝낸다.

**삭제:** 한 job 슬롯이 query와 전체 LLM 작업을 함께 소유하는 실행 의미, frontend Release를 기다려 Session claim을 잡아 두는 경로.

## 03.3 모델 준비는 Inference, catalog는 Connections

**읽기:** [S11 HostInferenceRoutes:L1–126](../SOURCE_ANCHORS.md#s11), [S12 remote_model:L185–345](../SOURCE_ANCHORS.md#s12), `modules/inference/src/{api,application}`.

1. `HostInferenceRoutes::resolve(runtime, caller)`의 `runtime.block_on(resolve_remote_model_route)`를 없앤다. FFI에서 이 module을 참조하지 않게 한다. 필요 로직은 provider credential adapter와 Inference service로 분할한다.
2. `SavedServerConnection`의 Person/device binding·token 형식·loopback·exact recipient·동의 검증은 보존한다. Keychain 조회는 native credential port를 통해 하고 token은 메모리 secret handle로 다룬다. wire/Debug/로그/command digest에 노출하지 않는다.
3. `resolve_remote_model_route`에서 `/v1/connectors` 호출과 `calendar_connections` 조립을 제거한다. 모델 준비의 typed 결과는 `PlannedRoute + credential handle + authority refs`이며 source 목록을 담지 않는다.
4. `/v1/inference-purposes`의 model metadata 조회는 Inference의 비동기 준비다. 각 요청에 scope/deadline/cancellation을 적용하고 redirect/no_proxy/응답 크기 제한 등 현재 boundary를 유지한다. Connection catalog 장애는 별도 RefreshIssue에만 기록한다.
5. Inference와 Connections 사이 compile/runtime 서비스 의존을 만들지 않는다. 같은 credential resource를 사용할 수는 있지만 좁은 credential port로 각자 주입한다. 모델 실패와 source 실패의 관측 key를 분리한다.
6. Auto profile은 첫 수락 때 저장한 로컬 preference/policy ref를 사용해 실행 중 계획한다. 명시 profile 선택은 intent에 있다. profile이 삭제되거나 recipient가 바뀌면 새 승인 없이 다른 외부 모델로 넘어가지 않는다.
7. model/consent/credential 불가라면 **수락된 Run**에 scoped terminal outcome과 안전한 notice를 남긴다. 미승인 전송 없이 실패를 설명해야 하며 실제 모델 호출이 불가능하면 deterministic 안내를 사용한다.

**A 완료:** 동기 FFI 호출 graph에 model/catalog HTTP가 없고, accepted runner→Inference→provider 흐름이 실제 함수로 구현되어 있다. source 사용은 lazy port이다. source/credential 전체를 dummy 값으로 채워 컴파일하지 않는다.

## 03.4 재시작·취소·terminal 책임

1. `ConversationRepository`의 activate/recover/generation semantics를 실제 저장 port로 유지한다. 새 실행자는 과거 nonterminal Run을 자동 외부 재실행하지 않고 Interrupted/RecoveryRequired로 분류한다. 사용자 Continue와 read-only 재조회는 구분한다.
2. CancelRun은 같은 Conversation command namespace에 durable tombstone을 저장한 뒤 해당 Run handle만 signal한다. handle이 이미 사라졌거나 Run이 finished이면 같은 intent receipt를 반환하고 terminal을 덮지 않는다.
3. cancellation이나 deadline 때문에 Engine future가 drop되더라도 owner의 terminalization은 별도 제한된 cleanup 경로에서 실행한다. 종료된 scope의 `.run()`에 terminal DB commit을 넣어 항상 취소되게 만들지 않는다.
4. `finish_run`은 report·허용된 transcript/coverage·claim 해제를 원자 commit한다. 저장 ack가 불명확하면 같은 RunId로 재조회하고, 확정 전 Finished success를 반환하지 않는다.
5. events는 commit된 snapshot의 관측 힌트다. event publish 실패를 Run failure로 바꾸지 않는다. 마지막 event를 못 받더라도 GetRun/Session 조회로 결과를 찾을 수 있어야 한다.

**B 확인 시나리오:** 모델 준비와 catalog barrier 중 receipt/query/cancel 처리; token/catalog/서버 상태 변화 후 같은 command의 같은 receipt·추가 dispatch 0; SessionBusy가 다른 Session을 막지 않음; crash 후 같은 ID 재조회.

**단계 종료:** 실제 수락·scheduler·runner·취소 계약이 구현되어 있고, raw request/route를 통한 옛 identity와 동기 네트워크 경유를 제거했다. 구체 저장/FFI 조립이 남으면 그 정확한 위치만 원장에 표시하고 다음은 04다.
