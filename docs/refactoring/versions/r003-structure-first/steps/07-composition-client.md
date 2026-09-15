# 07 — AppHost·단일 ABI·Session·Flutter 전체 연결

**단계:** A. **기존 범위:** P16–P19/P21. **선행:** 02–06의 실제 구현. **다음:** [08 구조 완료 심사](08-structure-review.md).

여기서 기능을 선언만 하고 성공 stub으로 연결하지 않는다. 빠진 구체 구현이 있으면 같은 에이전트가 그 필요한 부분을 먼저 완료한다. 제품 동작 테스트는 09지만 실제 caller wiring은 이 단계의 필수 결과다.

## 07.1 AppHost가 실제 서비스 수명을 소유

**읽기:** [S10 LegacyComposition:L35–180](../SOURCE_ANCHORS.md#s10), [S30 AppHost:L1–158](../SOURCE_ANCHORS.md#s30), [S27 Worker](../SOURCE_ANCHORS.md#s27), `crates/app/src/{api,bootstrap,services}.rs`.

**목표:** `crates/app/src/{bootstrap,host,services}.rs`에서 실제 owner와 05 adapter를 조립. FFI에는 opaque handle만 보관.

1. `AppHost<LegacyComposition>` 대신 app이 생성한 실제 service bundle을 사용한다. 순서는 native verified caller → 현재 storage engines/repositories → Access/Context/Inference/Day/Knowledge/Actions/Connections → Directory endpoints/Task owner → Conversation → observer다.
2. generic/runtime↔concrete adapter의 순환은 좁은 port와 생성자 주입으로 해결한다. `AppHost.get<T>()` 같은 범용 service locator를 domain에 전달하지 않는다. 생성 순서상 서로의 전체 service가 필요하다면 port가 너무 넓은지 먼저 교정한다.
3. `caller: Option`과 `AppHost::legacy`, `legacy_services()`를 제거한다. test host도 explicit controlled identity provider와 현재 service contract로 생성한다. caller identity는 Flutter DTO에서 수용하지 않는다.
4. Worker의 짧은 저장 요청·Run scheduler·Connection operation·learner 수명을 각 owner로 분리한다. 공통 executor는 자원 제한만 맡고 모든 domain을 전역 active job 하나로 직렬화하지 않는다.
5. event publication은 owner의 durable mutation 뒤 observer에 전달한다. buffer가 가득 차도 실행 결과의 원본을 잃지 않으며 snapshot query로 복구한다. event buffer를 FFI/VaultBridge가 domain 상태처럼 소유하지 않는다.
6. `shutdown`은 요청 admission 차단, owner cancel/drain, 안전한 자원 회수를 한 번 수행한다. 같은 종료를 여러 caller가 기다려도 동일 결과를 관측한다. 진행 중인 native callback의 메모리를 먼저 free하지 않는다.
7. `LegacyComposition`과 FFI 내 model route/repository/Expert 조립을 삭제한다. final FFI 내부 dependency는 `floe-app`, `floe-protocol`뿐이다.

## 07.2 Session과 모든 업무 API를 같은 typed 면으로 연결

**읽기:** [S32 app_wire:L1–190](../SOURCE_ANCHORS.md#s32), [S05 SessionRepository](../SOURCE_ANCHORS.md#s05), `agent_vault_gateway.dart::_conversationSession`, `conversation_runtime_gateway.dart`, [S33 FfiDayGateway](../SOURCE_ANCHORS.md#s33).

| 호출 | 최종 공개 owner | 중요 조건 |
|---|---|---|
| create/resume/get/recover/compact Session | Conversation | 현재 principal, revision, active Run 조회 포함 |
| StartTurn/CancelRun/Retry/Continue | Conversation | stable command, 원래 receipt, 명시적 lineage |
| GetTask/CancelTask/Expert settings | Experts | parent/principal/definition 범위, child-only cancel |
| Query/Refresh/Begin/Confirm/Cancel/Disconnect | Connections | Operation receipt, desired/observed/issue 분리 |
| Grant review/revoke·Vault lock | Access / native vault lifecycle | 검토한 authority, 동기 release fence |
| Day capture/edit·Memory/Playbook review | Day / Knowledge | owner CAS와 evidence 검증 |
| Proposal approve/execute/reconcile | Actions | 승인과 외부 write 분리, uncertain 결과 유지 |
| OS permission/authorization interaction | 해당 owner의 operation | epoch·request·expiry, UI는 중계만 |

1. 위 행마다 현재 AppCommand/AppQuery의 실제 variant, app facade method, owner port, Dart model을 한 번씩 연결한다. 임의 JSON의 `kind`를 받아 legacy dispatcher로 다시 보내지 않는다.
2. SessionId에서 durable Session snapshot과 active Run을 찾을 수 있게 한다. 재시작 후 client가 과거 인메모리 CommandId 목록을 기억해야만 복구되는 구조를 제거한다.
3. mutation은 command receipt, 조회는 snapshot으로 구분한다. Session resume가 새 Session을 생성할 수 있다면 조회가 아니라 명시 mutation으로 분류하고 멱등성을 정의한다.
4. receipt에는 routing credential이나 개인 source payload를 넣지 않는다. GetMessage/Task artifact는 현재 권한에 맞게 release하며 같은 command의 재조회가 현재 공개 제한을 우회하지 않는다.
5. transport request_id와 업무 command_id를 구별한다. ack 유실 retry는 같은 업무 ID, 새 사용자 Retry만 새 ID+retry_of다. conflict와 observer timeout을 동일 UI fatal error로 처리하지 않는다.
6. 외부/native wire shape는 도메인 저장 entity 전체와 동일하지 않다. 작은 명시 변환을 FFI에 두며 `serde_json` 왕복으로 private owner 상태를 전부 노출하지 않는다.

**삭제:** Session 전용 legacy bridge, typed app query가 `services.agent_vault`에 직접 접근하는 경로, UI가 Person/device/bearer를 새 앱 command로 주장하는 입력.

## 07.3 하나의 ABI·serializer·binding으로 직접 교체

**읽기:** [S31 abi:L1–180](../SOURCE_ANCHORS.md#s31), [S02 상수](../SOURCE_ANCHORS.md#s02), `FloeNativeBindings` 정의와 `native_transport.dart::_nativeWorkerMain`.

```text
floe_protocol_version
floe_core_open
floe_core_command
floe_core_query
floe_core_events
floe_core_free
floe_string_free
```

1. 현행 `_v2` command/query/events 구현을 위 접미사 없는 이름으로 직접 바꾼다. Dart `commandV2/queryV2/eventsV2`도 `command/query/events`로 바꾼다. deprecated alias나 symbol fallback은 남기지 않는다.
2. 기존 `floe_core_agent_vault`, `load_day`, `execute`, `calendar_actions`, `agent_fixture*`, 앱용 `local_context` entry의 제품 기능을 07.2에 옮긴 뒤 export와 dispatcher를 제거한다. OS 드라이버 callback ABI는 별도 목록으로 관리하며 필요한 callback까지 삭제하지 않는다.
3. `invoke_json`과 `invoke_json_v2`를 하나의 guarded decode→validate→typed call→encode로 합친다. request correlation, invalid input 거절, panic quarantine, input/output bounds를 유지한다.
4. `APP_WIRE_VERSION`은 현재 한 값만 사용한다. `floe_protocol_version`과 생성된 Dart의 기대값을 맞춘다. 같은 숫자를 쓰는 오래된 앱을 자동 구별한다고 주장하지 않으며 동일 source의 bundle로 배포한다.
5. UTF-8 입력 길이·null pointer·오류 응답·String free·Handle free의 소유권을 실제 C 선언과 Dart binding에서 맞춘다. static 문자열과 heap 반환값을 같은 free 함수에 잘못 넣지 않는다.
6. `floe-protocol`과 `floe-ffi`는 동일 package 이름을 유지하며 `crates/bindings/{protocol,ffi}`로 이동한다. old/new 두 package나 `#[path]` 재사용으로 병행하지 않는다. include_str/include_bytes·build.rs·상대 fixture 경로를 같은 변경에 갱신한다.

## 07.4 앱 수명 client·read model과 feature 경계

**읽기:** [S33 FfiDayGateway:L1–95](../SOURCE_ANCHORS.md#s33), [S34 AppReadModel:L1–180](../SOURCE_ANCHORS.md#s34), `agent_controller.dart::_conversationBusy/canSend/load`, `FloeClient`의 waiter map과 prepared command.

1. `_runtimeClient`, `_readModel`, native transport의 생성/종료를 Day feature 밖 `apps/client/lib/app/` bootstrap으로 옮긴다. 같은 앱에 client 또는 authoritative domain snapshot owner를 두 개 만들지 않는다.
2. `runtime_client`를 commands/queries/models/transport/read_model의 실제 책임으로 나눈다. 파일이 짧아도 계층마다 wrapper만 추가하지 않는다. Flutter framework/OS API는 transport·presentation 경계에 한정한다.
3. read model은 Conversation/Experts/Connections/Access/Day/Knowledge/Actions slice를 갖되 변경은 backend snapshot/event reducer로 들어온다. UI draft/focus/scroll와 transport pending은 UI 소유이며 backend Run 상태와 다르다.
4. `canSend(sessionId)`는 해당 Session의 active claim, 필요한 Vault gate, 같은 Session의 아직 수락 결과를 모르는 command를 반영한다. `registryBusy || memoryBusy || calendarBusy || connectionBusy` 전역 OR을 제거한다.
5. 현재 pendingCommandIds만으로 session을 모르면 prepared command의 session mapping을 transient map으로 보유한다. observer timeout은 pending의 불확실성으로 남기고 durable query로 해소한다. local bool로 Run을 Finished 처리하지 않는다.
6. bootstrap은 snapshot+cursor 일관성을 정의한다. gap/epoch mismatch는 Session/Run snapshot을 재조회하고 stale revision을 무시한다. lock/revoke 뒤 이전 민감 payload가 늦은 event로 다시 보이지 않게 vault/authority epoch를 검증한다.
7. Widget의 직접 LocalServerClient/AgentController private mutation 호출을 새 API로 바꾼다. UI에는 operation 상태 표시와 OS 승인 화면 중계만 남긴다. query 실패를 연결 삭제나 대화 취소로 변환하지 않는다.
8. `AgentController`의 domain state를 새 이름의 거대 controller로 옮기지 않는다. 제품 화면을 `features/{conversation,connections,experts,day,knowledge,actions,settings}`로 이동할 때 import·localization·router·actual widget caller도 함께 수정한다.

## 07.5 transport lifecycle·Apple 빌드 정합성

**읽기:** `native_transport.dart::open/_appWireRequest/close/_nativeWorkerMain`, [S35 macOS build:L1–40](../SOURCE_ANCHORS.md#s35), `apps/client/ios/build_rust.sh`, Xcode build phases·Swift callback declarations.

1. open/command/query/events/close 요청의 waiter가 success/error/worker-exit/timeout마다 정확히 한 번 완료되게 한다. reply port는 finally로 닫고 late response는 무시한다. close 뒤 새 요청은 명시 거절한다.
2. worker exit/error 채널을 연결하여 죽은 isolate에 대한 무한 `reply.first`를 없앤다. client timeout은 Run cancel이 아니다. 동기 native 실행이 계속 중일 때 isolate kill/free를 timeout의 일반 정리 수단으로 사용하지 않는다.
3. FFI에서는 local admission/query만 제한된 시간 기다리고 원격 I/O는 owner runtime에 남긴다. control queue가 model generation이나 OAuth 완료를 기다리지 않는다. cancellation과 observation은 해당 owner의 별도 명령이다.
4. `libfloe_ffi.dylib` artifact 이름은 유지한다. manifest 이동, native symbol, header/generated Dart, `@rpath`, bundle copy/signing, iOS target-specific path를 한 묶음으로 갱신한다. Mac script의 root `--package floe-ffi`처럼 그대로 유효한 줄은 불필요하게 바꾸지 않는다.
5. 가능한 host에서 link/export 정합성을 검사하되 A에서 매번 앱 실행을 요구하지 않는다. SDK 부재는 해당 platform 검사 미실행으로 기록한다. 실제 Keychain·Foundation·OAuth 확인은 09다.

**단계 A 완료:** 모든 현재 기능의 UI→단일 ABI→owner→실제 adapter 경로가 연결됐다. legacy 서비스 접근·이중 decoder·구형 Session bridge가 없고 생성/종료/복구의 소유권이 하나다. 제품에서 성공했는지는 아직 별도다.
