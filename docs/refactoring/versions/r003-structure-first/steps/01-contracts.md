# 01 — 시작점·canonical 계약·스키마 동결

대응: 계획 §3.1 / P00·P01·P16·P23. 선행 없음. 다음은 [02](02-business-owners.md).

## 01.1 실제 시작점과 기능 대응을 고정

**읽기:** [S01 workspace](../SOURCE_ANCHORS.md#s01), [S02 wire 상수](../SOURCE_ANCHORS.md#s02), [S18 Core exports](../SOURCE_ANCHORS.md#s18), [S31 ABI](../SOURCE_ANCHORS.md#s31). `AGENTS.md`와 원장 current를 읽는다.

1. 실제 HEAD/working tree를 보존한다. 이 판 기준에서 제품 소스가 바뀌었다면 변경된 심볼만 대응시킨다. 문서판의 SHA로 checkout/reset하지 않는다.
2. 원장 안에 기존 제품 기능을 아래 owner에 연결한다. 구현 위치·현 caller·새 공개 API·남은 제거 대상만 적는다. 별도 파일별 작업 보드나 조사 보고서를 만들지 않는다.
3. `rg -n 'floe_core_|AppHost::legacy|LegacyComposition' crates apps/client/lib`로 제품 ABI와 조립 호출을 수집한다. fixture-only export도 표시하되 실제 기능과 동일시하지 않는다.
4. 이번에 그대로 사용할 Execution, Access, Context, Day, Knowledge, Engine, Directory, TaskCoordinator, ConversationService, FloeClient의 정의를 확인한다. 이름만 다른 대체 서비스를 새로 만들지 않는다.

| 기존 기능 | 최종 owner/API 계열 |
|---|---|
| create/unlock/lock·권한 검토·철회 | Access + native/Vault port |
| Session·대화·Continue·Retry·Cancel·archive | Conversation |
| Registry·Expert 설정·A2A Task | Experts |
| Pairing/OAuth·연결 조회·refresh·disconnect | Connections |
| 모델 profile·attempt·receipt·usage | Inference |
| Memory/Playbook 검토·learner | Knowledge |
| Calendar/Task/Note 읽기·capture·mirror | Day |
| 제안·승인·실행·불확실 결과 재조회 | Actions |
| OS authorization UI·획득 callback | Native driver; 요청 수명은 해당 owner |

**산출:** 원장 한 곳의 현재 대응. 코드 이동 없이 표만 작성했다면 구현 완료가 아니다.

## 01.2 명령 identity와 Run 입력을 먼저 분리

**읽기:** [S06 TurnRequest](../SOURCE_ANCHORS.md#s06), [S07 수락](../SOURCE_ANCHORS.md#s07), [S08 exact_admission](../SOURCE_ANCHORS.md#s08), [S09 request_context](../SOURCE_ANCHORS.md#s09), [S10 host 시작](../SOURCE_ANCHORS.md#s10).

**목표 파일:** `crates/modules/conversation/src/api.rs`, 새 private `domain/intent.rs` 또는 현재 동등 정의. 저장 port는 기존 `ports/mod.rs`를 수정한다.

아래 값의 의미를 고정하고 같은 역할의 기존 타입에 직접 반영한다. 이름은 목표 API이며 현행 API라고 주장하지 않는다.

```rust
// 공개 입력의 의미. principal은 AppHost가 공급한다.
struct StartTurn {
    command_id: CommandId,
    session_id: Uuid,
    expected_revision: u64,
    text: String,
    mode: TurnMode,                 // New 또는 검증 가능한 ContinueRef
    retry_of: Option<RunId>,
    profile: ProfileSelection,       // Auto 또는 사용자가 고른 안정적인 profile ID
}
// 선택된 실제 recipient/token/catalog를 위 구조에 넣지 않는다.
struct AdmittedExecution {
    receipt: RunReceipt,
    intent: CanonicalTurnIntent,
    // 승인된 시점의 로컬 policy/profile preference 참조. credential 원문 없음.
}
```

1. text는 **한 번만** trim하고 UTF-8 byte 상한 8192를 검증한다. Dart 미리보기와 Rust authoritative 검증의 의미를 맞춘다. 그 normalized text를 저장과 digest에 동일하게 사용한다.
2. canonical struct를 고정 필드 순서로 직렬화한다. kind·session·expected_revision·text·mode 전체·retry_of·명시 profile 선택을 포함한다. optional 값은 하나의 canonical 표현으로 통일한다. `Debug` 출력·해시맵 순회·임의 formatter는 금지한다.
3. digest namespace에 검증된 principal을 bind한다. 같은 Conversation namespace에서 StartTurn/CancelRun의 command ID 충돌을 막는다. 다른 업무 owner의 command 조회에는 owner를 명시해 모호한 전역 조회를 만들지 않는다.
4. transport request ID, deadline, clock, bearer, catalog, 자동 선택 placement와 현재 availability는 제외한다. 기존 `request_context_digest`에 request 전체를 넣는 caller를 지운다. 새 execution snapshot을 같은 digest 이름으로 우회 삽입하지 않는다.
5. ContinueRef는 기존 RunId·executor_generation·level을 사용한다. Retry는 새 command ID + terminal source RunId이고 Continue와 상호 배타다. 권한 재검사는 digest와 별개다.
6. `run_turn_observed`의 입력과 `exact_admission`이 **같은 canonical identity**를 검증하도록 교정한다. 환경에 따라 재계산한 `execution_profile/model_placement` 비교로 기존 command를 거절하지 않는다.

**A 검사:** 영향받은 Conversation contract/port와 직접 소비자의 타입. dedup 의미가 바뀌므로 같은 intent의 canonical bytes 안정성·changed payload conflict만 좁게 검사한다. token/catalog/network를 호출하지 않는 조건은 stage 03에서 wiring하고 B에서 barrier로 판정한다.

## 01.3 Archive 타입의 역의존을 제거

**읽기:** [S04 archive_reader.rs:L1–74](../SOURCE_ANCHORS.md#s04), [S05 Conversation ports:L1–85](../SOURCE_ANCHORS.md#s05), [S28 실제 archive repository](../SOURCE_ANCHORS.md#s28).

현재 `SessionArchiveRepository`가 `floe_context::ArchiveReadRequest/ArchiveSnapshot`을 직접 노출한다. 이를 그대로 `floe-vault`로 옮기면 Vault→Context 금지 의존을 요구한다. public re-export 뒤에 Context 구현 타입을 숨기는 것도 해결이 아니다.

1. `ArchivePointer`, `ArchiveReadRequest`, `ArchivedMessage`, `ArchiveSnapshot`의 canonical 값 정의를 `crates/contracts/agent/src/archive.rs`로 이동한다. `AgentMessage`를 포함하므로 **context-contract로 옮기지 않는다**. agent-contract→context-contract 역전 사이클을 만들 수 있기 때문이다.
2. 해당 값의 bound/identity validation을 함께 이동한다. projection의 필터링·권한 재검사·가공은 `floe-context`에 남긴다. `ArchiveProjection`은 Context 결과로 남길 수 있다.
3. Context의 `ArchiveReader`는 canonical 타입을 import해 읽기 port만 제공한다. Conversation의 `SessionArchiveRepository`도 동일 타입을 사용하고 공개 port에 필요한 값만 re-export한다.
4. Vault 구현은 `floe_conversation`의 공개 port와 그 공개 계약 타입을 통해 구현한다. Context crate의 타입/상수/함수를 직접 import하지 않는다. `_context`라는 module alias로 우회하지 않는다.
5. `conversation/src/application/{archive,recovery}.rs`, FFI의 `conversation_repository.rs`, 기존 Context archive projection 및 테스트의 imports를 같이 바꾼다. old 동일 타입을 복제하거나 legacy mapper를 남기지 않는다.
6. 실제 crypto payload encoding의 변경 필요 여부를 따로 기록한다. 단순 Rust 타입 경로 이동만이면 데이터 재생성 사유로 삼지 않는다.

**삭제:** Context 안의 옮긴 값 정의. 남기는 것은 port와 projector뿐이다.
**A 검사:** agent-contract/context/conversation을 차례로 type-check. Vault는 stage 05에서 이 port를 구현할 때 금지 의존 없이 컴파일되어야 한다.

## 01.4 공개 facade·owner port·한 wire를 확정

**읽기:** [S03 journal/model ports](../SOURCE_ANCHORS.md#s03), [S05](../SOURCE_ANCHORS.md#s05), [S16 action contract](../SOURCE_ANCHORS.md#s16), [S30 AppHost](../SOURCE_ANCHORS.md#s30), [S32 app_wire](../SOURCE_ANCHORS.md#s32).

1. FFI가 의존할 facade 타입은 `floe-app::api/services`에 선언하고 실제 owner 값을 좁게 re-export하거나 명시적으로 변환한다. FFI가 Conversation/Kernel/Vault 내부를 직접 import하게 두지 않는다. facade는 다른 authoritative 상태를 보관하지 않는다.
2. Session/Run/Task/Operation/Action ID를 의미별로 유지한다. Day의 할 일 TaskId와 A2A TaskId를 합치지 않는다.
3. repository는 `admit_turn`, `finish_run`, `admit_task/settle_task`, `commit_review`, `admit_operation/settle_operation`, `record_attempt`처럼 원자 업무 단위로 제공한다. 순차 `save_*`만 공개하고 caller가 원자성이라고 가정하게 하지 않는다.
4. `schema_version`은 현재 숫자를 유지한다. ABI 최종 명칭은 `floe_core_command/query/events`; 메모리 관련 open/free/string_free는 하나만 둔다. old export alias와 decoder는 stage 07의 소비자 교체와 함께 삭제한다.
5. app payload는 verified caller 대신 person/device/credential을 신뢰하는 필드를 받지 않는다. 사용자가 입력하는 일회성 연결 secret은 별도 민감 입력 port로 처리하고 읽기 모델·일반 command 로그에 넣지 않는다. 외부 provider 원문 계약은 adapter 책임이다.
6. 오류에는 stable reason·origin·affected scope·incident ref를 유지한다. 업무 `Denied/Unavailable/ApprovalRequired`와 저장/무결성 `BoundaryFault`를 구별한다. blanket enum 추가만 하고 caller 의미를 방치하지 않는다.

**단계 종료:** 필요한 계약이 실제 타입과 port에 반영되어 있다. 뒤 단계가 구현할 handler는 위치와 signature가 정해져 있으나 성공 stub을 만들지 않는다. 원장에 stage 02의 첫 파일을 남긴다.
