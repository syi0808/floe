# 06 — builtin Expert를 독립된 등록 endpoint로 완성

**단계:** A. **기존 범위:** P11/P15. **선행:** 02–05의 Actions/Context/Inference/Task 계약. **다음:** [07 실제 앱 조립](07-composition-client.md).

Calendar-first root는 기준 코드에서 이미 제거됐다. 다시 새 root를 만들거나 기존 제거 작업을 반복하지 않는다. 남은 중앙 builtin dispatch, parent-model 카드 필터, parent Run별 context staging과 legacy 결과 변환을 교체한다.

## 06.1 여덟 Expert의 정의·역할·도구를 자체 폴더로 이동

**읽기:** [S37 LegacyExpertEndpoint:L1–180](../SOURCE_ANCHORS.md#s37), [S38 ConversationExperts:L280–530](../SOURCE_ANCHORS.md#s38), `crates/floe-agent/src`의 `BuiltinExpertKind` 정의와 각 `run_*_expert` 직접 호출, `crates/floe-agent/prompts/*expert_role.txt`.

**목표 공통 형태**

```text
crates/experts/builtin/src/
  lib.rs
  schedule/{mod.rs,manifest.rs,role.md,endpoint.rs,tools.rs,artifacts.rs}
  commitments/{...}
  communication/{...}
  relationships/{...}
  focus_attention/{...}
  wellbeing/{...}
  work_context/{...}
  life_logistics/{...}
```

1. 각 Expert의 기존 role 내용·source 필요조건·출력 검증·artifact 의미를 해당 폴더로 옮긴다. 같은 정책을 공용 enum과 endpoint 양쪽에 복제하지 않는다.
2. `manifest`는 ID, definition revision, 설명, 필요한 capabilities를 선언한다. manifest가 실제 grant를 발급하거나 개인 source 존재를 공개하지 않는다. 권한 증거는 Access/Directory의 현재 결정이다.
3. 각 `endpoint`는 기존 generic Engine을 다른 RoleSpec으로 실행한다. 제품 Expert를 `schedule(args)`라는 단일 결정론적 도구로 축소하지 않는다. 구체 도구 실행은 host가 승인한 좁은 ToolPort를 거친다.
4. 여덟 폴더를 만든다는 이유로 새 crate 여덟 개를 추가하지 않는다. 승인된 `floe-experts-builtin` 하나에서 공개 endpoint factory만 제공한다. 현재 일반 대화가 사용하는 role/prompt도 이동된 include 경로를 함께 갱신한다.
5. 기존 domain 결과를 변환할 필요가 있으면 해당 Expert의 artifacts 모듈에서 canonical Artifact로 변환한다. legacy A2ATask 생성→다시 ExpertReport로 역변환하는 공용 왕복은 제거한다.

## 06.2 Directory는 ID 조회, Manager는 의미 선택

**읽기:** [S26 TaskCoordinator/Repository](../SOURCE_ANCHORS.md#s26), [S38 agent_cards/handle_message](../SOURCE_ANCHORS.md#s38), `crates/modules/experts`의 Directory 등록·resolve 코드.

1. App 조립은 `register(schedule::endpoint(...))`, `register(communication::endpoint(...))`처럼 구현을 정적으로 등록한다. 이 목록은 설치 구성이지 요청 의미를 분류하는 router가 아니다.
2. Manager에게 제공하는 CatalogSnapshot은 reviewed definitions와 현재 eligibility에서 만든다. `matches!(parent_model, Server)` 또는 `BuiltinExpertKind::supports_device_model`을 사용해 후보를 결정하지 않는다.
3. Manager의 `Delegate(agent_id, definition_revision, goal)`는 선택한 ID를 보존한다. Directory는 등록·revision·trust·권한을 검사하고 endpoint를 반환한다. 임의 URL을 model output에서 resolve하지 않는다.
4. 실행 직전 최신 definition/authority를 재검사한다. stale/disabled/권한 부족이면 실제 endpoint 호출 없이 typed rejection을 Task에 남긴다. parent가 안전한 observation으로 처리하게 한다.
5. 중앙 `BuiltinExpertKind::from_package_id`와 `match expert`의 업무 dispatch를 지운다. 설정 화면에서 순서를 표현하기 위한 정적 metadata가 필요해도 runtime 실행 분기에 사용하지 않는다.
6. 기존 `source_granted`가 setup 없음에서 허용하던 의미를 그대로 복사하지 않는다. setup 부재/미설정과 trust 손상을 구별하고 필요한 권한 증거가 없으면 관련 source만 거절한다. 무관한 일반 답변은 계속 가능해야 한다.

**A 구조 검사:** 새로운 테스트 endpoint의 ID를 등록하고 generic Engine/Conversation/FFI 소스 변경 없이 같은 계약으로 compile 가능한지 확인한다. 실제 LLM의 선택 정확도 평가는 B다.

## 06.3 parent-Run staging을 invocation-scoped context로 교체

**현재 수정 지점:** [S37](../SOURCE_ANCHORS.md#s37)의 `LegacyExpertEndpointContext`, `contexts: Mutex<HashMap<Uuid,...>>`, `stage/clear`, `.remove(&run_id)`; [S38](../SOURCE_ANCHORS.md#s38)의 `ScheduleTaskRunner` 및 source/model 조합.

1. parent Run ID를 key로 context를 한 번 꺼내 쓰는 staging을 제거한다. 같은 Run이 여러 번 위임할 수 있으므로 context는 admitted TaskId/invocation key에 묶인 immutable 참조여야 한다.
2. TaskCoordinator가 endpoint에 전달할 값은 검증된 principal, parent Run, Task ID, definition revision, 자연어 assignment, 허용 context refs, child scope, 좁은 source/model/action handles다. 전체 FloeCore/EncryptedAgentVault/AppHost를 전달하지 않는다.
3. child deadline·budget은 root에서 할당한다. 현재 invocation factory의 전체 40,960 tokens/50,000 cost를 모든 Expert에 복제하는 방식을 제거한다. 자식 모델 route는 해당 consumer/purpose/recipient 제약으로 Inference가 별도 계획한다.
4. `ServerSourceClient`와 각 personal reader는 필요한 source를 사용할 때만 호출한다. source-local-only Expert가 local 모델을 쓸 수는 있지만, 그 결과를 remote Manager에게 보내는 허가까지 자동 발생하지 않는다.
5. endpoint 자체는 authoritative Task map이나 별도의 root Session을 만들지 않는다. Task 상태 변경·replay·취소·terminal은 Experts owner에 남는다. endpoint는 report/typed failure만 반환한다.
6. Schedule의 `run_registered`, `RegisteredScheduleTaskRunner` 및 별도 staging을 위 공통 흐름으로 교체한다. Calendar grant·freshness·proposal 검증은 Schedule tools/Context/Access/Actions에 각각 이식한다.

## 06.4 Task 결과·provenance·제안을 안전하게 반환

1. `LegacyExpertEndpoint`의 `TaskState == Completed`만 허용하는 공용 변환을 없애고 04의 TaskOutcome 처리로 통일한다. denied/failed/timed_out은 실패 Task로 보존하며 그 때문에 무관한 root 결과까지 폐기하지 않는다.
2. `record_result_independent(...)`를 모든 Expert 시작에 호출하거나, coverage가 없으면 `unwrap_or(Independent)`로 만드는 경로를 삭제한다. 실제 읽은 자료와 파생 결과에서 coverage를 계산한다. 출처를 모르는 값은 Unknown이다.
3. local-only 원문·summary·artifact를 remote parent에게 내보내기 전에 release한다. 불가하면 payload 없는 안전한 제한 설명만 반환한다. endpoint의 자체 선언만으로 Independent/허가를 인정하지 않는다.
4. Calendar Action은 proposal/reference를 만들 뿐이다. 실제 mutation은 02의 Actions owner에서 승인된 payload와 execution ID로 수행한다. Expert가 승인·write ledger를 직접 변경하지 않는다.
5. history cleanup에서 `calendar.`/`schedule.` 접두어로 개인정보를 식별하는 조건을 실제 provenance/ref/authority 기반 처리로 대체한다. 새 Expert도 같은 보호를 받아야 한다.
6. 옮긴 여덟 endpoint의 원본 역할·tool·proposal·실패 의미가 모두 대응되는지 확인한 뒤 legacy endpoint/central dispatcher/특수 runner를 삭제한다. `/focus` 같은 UX 단축 입력은 같은 root의 명시적 사용자 목표로 처리하고 별도 root를 열지 않는다.

**단계 A 완료:** 여덟 endpoint가 공통 등록 계약·child scope·독립 모델 계획을 사용한다. generic 코드의 구체 Expert 실행 분기와 parent staging이 없다. 제품의 Manager→Expert A2A는 유지된다.

**B에서 확인:** Calendar 활성 상태의 Communication 선택, 같은 root의 Schedule 실행, 필요 없을 때 직접 답변, child timeout 뒤 설명, 다른 recipient로 파생 자료 전송 차단, 추가 endpoint의 실제 전달.
