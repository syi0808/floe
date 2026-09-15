# 05 — 실제 Vault·provider·native adapter의 최종 배치

**단계:** A. **기존 범위:** P13/P14/P21 및 앞 단계 owner 저장 구현. **선행:** 01–04의 공개 계약. **다음:** [06 Expert](06-experts.md).

원본을 새 경로에 복사하고 기존 구현으로 다시 forwarding하지 않는다. 아래 이동은 각 책임의 실제 함수를 가져오는 작업이다. 다른 소비자가 남으면 같은 변경 묶음에서 소비자를 교체한 뒤 원본을 지운다.

## 05.1 Vault engine과 업무 정책을 구분하여 이동

**읽기:** [S18 Core export:L1–60](../SOURCE_ANCHORS.md#s18), [S27 Worker/OpenVault:L1–325](../SOURCE_ANCHORS.md#s27), [S28 repository:L1–260](../SOURCE_ANCHORS.md#s28), `crates/floe-core/src/agent_vault.rs`의 create/open/connection/check_access, 해당 `mod`가 지정한 저장 구현.

**목표 파일:** `crates/adapters/vault/src/{lib,engine,error}.rs`, `src/repositories/{conversation,experts,connections,inference,access,knowledge,actions,day}.rs`. 생성 파일명은 목표이며 실제 기존 동등 파일은 재사용한다.

1. `EncryptedAgentVault`에서 키 취득·암호화 DB 열기·private file·connection 관리·transaction commit/rollback·키 건강 검사를 `engine`으로 옮긴다. Agent 실행, Expert 선택, 모델 route, UI event 판단은 가져오지 않는다.
2. `FloeCore`가 보유하던 Day store와 암호화 Vault를 하나의 평문 DB로 합치지 않는다. 기존 물리적 저장 구분은 유지한다. 논리 owner별 repository는 같은 engine의 좁은 transaction API를 사용할 수 있지만 raw DB handle을 업무 모듈에 공개하지 않는다.
3. `VaultConversationRepository`와 `VaultTaskRepository`의 실제 SQL/검증을 각 repository에 옮긴다. FFI의 repo struct와 `floe-core` 저장 facade로 돌아가는 호출을 제거한다. persistence record가 필요한 경우 해당 repository private 타입으로 둔다.
4. owner의 Run/Task 상태와 저장 representation의 의미를 맞춘다. 구형 `AgentSession`을 authoritative 원본으로 두고 새 Run을 거기에 끼워 넣는 변환은 제거한다. 저장 인코딩과 domain 값의 명시적 변환은 허용하되 두 state machine은 유지하지 않는다.
5. public `lib.rs`에는 app 조립에 필요한 생성자·repository 타입만 export한다. 과거 Core의 수백 개 export를 그대로 Vault public API로 복사하지 않는다.
6. 최종 `floe-vault` manifest는 승인된 owner package만 참조한다. `floe-context`, `floe-core`, `floe-agent`, `floe-infra`, FFI에 의존하지 않는다. 01.3에서 옮긴 archive 타입은 Conversation 공개 port를 통해 사용한다.

**삭제:** 이동한 FFI repo 원본, Core의 동일 저장 facade, 새 Vault가 구형 Vault를 호출하는 wrapper. 같은 파일을 두 package에서 `include!`하거나 `#[path]`로 이중 컴파일하지 않는다.

## 05.2 owner별 원자 작업을 실제 저장에 연결

**읽기:** [S05 repository port](../SOURCE_ANCHORS.md#s05), [S08 Conversation record/exact_admission](../SOURCE_ANCHORS.md#s08), [S26 TaskRepository](../SOURCE_ANCHORS.md#s26), [S21 Knowledge port](../SOURCE_ANCHORS.md#s21).

| 최종 원자 port | 한 transaction에서 보장할 것 | 실패 시 허용하지 않는 것 |
|---|---|---|
| `admit_turn` | principal+command dedup, Session revision/claim, user message, Run, receipt, executor fence | 일부 행만 저장하고 Accepted |
| `finish_run` | report, 허용된 final message/coverage, Run terminal, Session claim 해제 | UI Release에 claim 해제 위임 |
| `admit_cancel` | command 종류·target identity·receipt 저장 후 live cancel 통지 | 서로 다른 target의 같은 ID 수용 |
| `admit_task/settle_task` | parent intent/Task identity, revision/generation, terminal artifact+coverage | 결과 저장 전 parent에 Completed |
| `settle_attempt` | stable Attempt ID, handoff/usage 상태, 단일 정산 | 동일 provider 호출의 비용 이중 계상 |
| `commit_operation` | Connection desired/observed state와 operation generation/receipt | stale refresh가 Disconnect 반전 |
| `commit_review` | 현재 evidence·candidate revision 확인, decision 및 새 Knowledge revision | 검토 대상 변경을 같은 승인으로 처리 |
| `commit_action_intent/result` | 동결 proposal·승인·execution ID·확정/불명 결과 | ack 유실 뒤 새 ID로 blind write |

1. 각 port의 현재 구현에서 `save` 호출을 나열한 구간을 찾고 위 원자 경계로 묶는다. 같은 물리 DB에 있는 행만 ACID로 묶는다. Day DB/암호화 Vault/원격 provider 전체를 하나의 transaction이라고 부르지 않는다.
2. release/current authority 검사는 commit/handoff 직전에 수행한다. 권한 검사를 transaction 밖에서 한 번 했다는 이유로 이후 변경을 무시하지 않는다. 반대로 네트워크 응답 전체를 DB writer 안에서 기다리지 않는다.
3. terminal 저장 ack가 불명확하면 해당 receipt를 재조회하고 generation/CAS로 reconcile한다. `Ok(())`를 반환하는 fake repository나 자동 빈 데이터 fallback을 두지 않는다.
4. 저장 reopen에서 current-schema orphan Run/Task/Operation을 처리한다. 이전 executor writer가 새 세대에 결과를 저장하지 못해야 한다. 자동 외부 side effect 재실행은 금지한다.
5. DDL·record encoder·decoder·validator는 같은 변경 묶음에서 수정한다. app2/Conversation7 등 채택된 schema 숫자를 올리지 않는다. 변경된 의미에 필요한 새 개발 profile을 원장에 기록한다. A 중 일반 앱 데이터를 reset하지 않는다.

**A 검사:** transaction 호출 경계·실제 SQL·generation 조건을 읽고 타입 검사한다. admission/intent/권한 의미를 바꿨다면 기존 좁은 회귀를 실행한다. 광범위한 crash matrix는 B다.

## 05.3 Context·Knowledge·Actions의 저장 경계 누락 방지

1. `GovernedAgentSessionStore`, `context_history`, `context_evidence`가 수행하는 projection/lineage 계산은 Context/Conversation/Knowledge의 owner로 옮긴다. Vault는 저장된 coverage를 검증·반환하고 정책 결정을 중복하지 않는다.
2. archive 원문과 summary를 구별한다. 01.3의 canonical archive 값으로 저장·조회하고 실제 원문 release는 Context/Access에서 검사한다. compaction placeholder를 원래 user evidence로 사용하지 않는 기존 보호를 유지한다.
3. Knowledge evidence 검증은 review commit과 같은 일관된 snapshot에 연결한다. `read evidence → 모델 대기 → 무검증 commit`으로 바꾸지 않는다. 근거가 바뀌면 typed conflict다.
4. Actions의 local proposal/approval/uncertainty 기록을 정상 read/query로 노출하되 승인 없는 실행을 새 API의 편의 기능으로 추가하지 않는다. 동일 작업의 제안과 실제 write는 다른 상태다.
5. 각 기존 기능마다 `원본 함수 → 최종 owner 함수 → repository 함수`를 기존 원장에 짧게 기록한다. 기능 목록을 새 STATUS 파일로 복제하지 않는다.

## 05.4 provider transport와 Inference 서비스를 분리

**읽기:** [S11 HostInferenceRoutes:L1–126](../SOURCE_ANCHORS.md#s11), [S12 resolve_remote_model_route](../SOURCE_ANCHORS.md#s12), [S25 AttemptLifecycle](../SOURCE_ANCHORS.md#s25), `crates/floe-infra`의 실제 model/source/control adapters와 직접 호출자.

**목표:** `crates/adapters/providers/src/{models,sources,control}/`.

1. `models`는 외부 model HTTP 또는 native model transport 변환만 맡는다. public ModelPort를 구현하는 Inference service가 route·권한·attempt accounting을 소유한다. provider가 별도 policy router·usage ledger를 만들지 않는다.
2. `sources`는 실제 source 획득과 응답 shape/byte/identity 검증을 맡는다. 특정 모델을 선택하거나 connector catalog를 model route에 붙이지 않는다. source client는 Context 요청 때 필요한 connection으로 생성한다.
3. `control`은 기존 Go pairing/OAuth/authority API를 구현한다. provider protocol·서명 challenge·정확한 recipient 검증·redirect/no-proxy/크기 제한 등 기존 보안 계약을 유지한다. 앱 `_v2` 제거를 이유로 외부 `/v1`을 변경하지 않는다.
4. model credential은 좁은 opaque handle로 주입한다. Flutter command·Agent Card·model output에서 bearer나 임의 URL을 받지 않는다. managed source secret 입력이 필요한 현재 UI는 one-shot control command로만 보내고 prompt/trace에 포함하지 않는다.
5. 저장된 model profile 없음, credential unavailable, recipient consent 부족을 구별한다. 실패하면 임의로 local 모델로 넘어가지 않는다. 사전 승인된 fallback 정책만 Inference에서 적용한다.
6. actual provider 구현이 있는 상태에서 `LegacyModelPort`, `LegacyToolPort`를 제거한다. 변환은 외부 protocol↔canonical port에 한정하고 구형 런타임 값 전체를 serialize/deserialize해 우회하지 않는다.

## 05.5 native ownership과 안전한 진단

**읽기:** [S29 keyring:L1–87](../SOURCE_ANCHORS.md#s29), [S27 Worker](../SOURCE_ANCHORS.md#s27), [S35 Apple build](../SOURCE_ANCHORS.md#s35), native callback 선언·소유 스레드.

1. `platform/native`는 Keychain·OS source·native model의 실제 handle/lifetime을 소유한다. 업무 모듈은 thread-affine handle 대신 좁은 proxy/port를 받는다. `unsafe impl Send`로 제약을 덮지 않는다.
2. `key_entry/key_lookup/key_insert/key_read_back/key_length/vault_marker/db_open/identity/schema/host_lock`의 최초 실패를 incident+safe status로 보존한다. key-not-found와 permission-denied를 구별한다. secret·사용자 경로 원문·source content를 로그에 쓰지 않는다.
3. `insert_key`의 기존 키 미교체 조건과 기존 Vault의 missing key 오류를 유지한다. 읽기 실패를 새 키 생성·plaintext fallback·자동 DB 삭제로 해결하지 않는다.
4. Keychain service/account 문자열은 버전 숫자가 들어가도 identity다. 단일 ABI 정리 때문에 바꾸지 않는다. native source callback은 operation ID/epoch/expiry와 결합하고 늦은 callback이 해제된 host에 접근하지 않게 한다.
5. shutdown은 신규 admission 차단 → 소유 작업 취소 → bounded join/late-result fencing → 안전한 resource 해제로 구현한다. non-cooperative native 호출이 끝나지 않았으면 메모리를 강제로 free하지 않는다. 미지원 강제 취소를 성공으로 표시하지 않는다.
6. 정상 앱의 signing/entitlement/Keychain 환경 확인은 B에서 수행한다. native thread 가정이 설계 가능성을 좌우할 때만 A에서 작은 실험을 한다.

**단계 A 완료:** 실제 I/O 구현과 owner port가 연결되고 legacy 실행 변환이 없다. 가능한 대상의 compile과 안전 의미를 확인했다. 최종 app 생성자 wiring은 07에서 닫으며, 그 전에는 일반 앱 성공을 주장하지 않는다.
