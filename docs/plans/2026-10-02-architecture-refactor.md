# Floe architecture refactor: target and execution contract

## Active execution update — 2026-10-06

The user approved continuing the improvement plan on 2026-10-06. This section is the single current sequence for remaining work. It supersedes overlapping old stage-order and Actions-retention language below; historical evidence remains in Git, and preserved safety invariants remain mandatory. Historical records are not proof for the new snapshot.

**Source baseline:** `a31bb5ac3a173590ed563da20a9d99fbe39d9712`. The earlier 1,172-file ledger belongs to the October 2 baseline and does not establish exhaustive reading of this snapshot. Current planning has verified the main owner/transport/storage anchors. Before each implementation scope, the owner must read all affected bodies and reconcile its caller/import/deletion map.

**Current checkpoint — verified 2026-10-06:** P1a external-model injection and five Conversation integration tests were implemented at `81e48835406bfe2eb563d731d913138590103102` and merged to main in `53c0c9fc7bef261961b74ea6829bcb7d528b9ee2`. The root independently reviewed the changed source and ran the focused target on Linux: 5 passed, 0 failed; first build 2m49s, tests 0.29s. Cases cover a persisted text turn, primary error without fallback, cancellation, orderly close/reopen without redispatch, and unsupported Manager-tool rejection. P1b real Expert/source calls, process-crash recovery, T2 transport, actual Floe GUI and native-platform qualification remain open. A working Flutter desktop sample is environment evidence only.

### Final-state decisions

- Keep `floe-app` responsible for composition, verified request admission, runtime lifetime and one typed stateless product router. Do not remove lifecycle correctness merely because it remains in App.
- Product command/query groups converge to Conversation, Day, Connections and Memory. Native-host callbacks and runtime readiness are explicit non-business control/observation contracts. No public Calendar Operations, Experts registry or Vault lifecycle namespace.
- Re-scope and rename Rust Actions to Calendar Operations. Existing Access owns a bounded operation-policy/approval subdomain. Calendar Operations owns exact immutable effects, dispatch intent, receipt validation and lookup-only uncertainty recovery. Day keeps its projection/local-command role.
- Distinguish source/processing grants, one-operation consent and direct product-user intent. Ordinary Gateway LLM invocation does not gain an approval prompt. Direct Day edits must not depend on the unrelated Expert calendar-create policy revision.
- Keep operation consent and effect aggregates in the same encrypted Vault custody. The storage adapter applies owner-defined transitions in one local transaction for live local policy/consent checks and dispatch intent. External source/OS continuity retains separate preflight/native fences; it is not globally transactional.
- Conversation owns interaction display/correlation and linked Run continuation, not a second approval truth. Resume/approval never resends an uncertain external effect.
- Preserve command/request identity, admission disposition, generation fences, CAS, signed Gateway identity, source authority and lookup-only recovery. No automatic data/key reset, compatibility wrapper, restored product CLI or production test backdoor.

### QA layers and the first executable slice

1. **T1, primary regression:** real Conversation/RunCoordinator/Engine/Experts/Inference/Access and temporary encrypted Turso/Vault; scripted external model provider and source/tool I/O only. No GUI, real provider account or Go process required.
2. **T2, transport:** real Rust and Go pairing/authority/HTTP/provider decoding, with external LLM/source endpoints mocked. T1 does not certify these wire paths.
3. **T3, limited UI:** production Flutter widgets/controllers and FFI for a few startup/conversation/approval/Day scenarios, headless or actual desktop.
4. **T4, macOS:** EventKit, TCC, signing and production Keychain; no iOS/Android expansion in this checkpoint.

**P1a contract:** make the existing `ModelProvider` boundary object-safe with boxed prepared transports; retain actual `InferenceService` and Access admission. Freeze a required model-provider factory in App open options through VaultBridge/ReadyGeneration. Production constructs CompositeModelProvider and test composition supplies a scripted provider through the same installation/activation path. No global mutable hooks, environment model overrides, fake ModelPort/ConversationOwner/repository, or FFI injection knob.

**P1a initial cases:** persisted text turn; primary error without fallback; cancellation at a model barrier; close/reopen without redispatch; unsupported Manager tool rejection where the current output contract supports it. Run the completed slice with `cargo test -p floe-app --no-default-features --features development-storage --test conversation_integration`.

**P1b:** real Expert delegation/tool-source path with a scripted external acquisition host. Current Manager uses `NoManagerTools`; do not grant it tool powers just to make a test pass. `ExpertTools` and Context policy/journal logic remain real. Operation approval cases follow the P3 contract rather than blocking P1 on a not-yet-implemented target.

### Linux QA fixture and desktop contract — 2026-10-07

The user requested an explicit Linux-only fake Calendar connector and visible QA in dot's desktop environment. This is a bounded P1b prerequisite, not a claim that Linux production support or Floe GUI qualification is complete.

- **Connector identity:** reuse `calendar.fixture` / `CalendarProvider::Fixture`, visibly labeled synthetic QA data, with a distinct device-bound `fixture:<device-id>` execution owner. Never impersonate EventKit, an Apple owner or a paired Gateway.
- **Authority path:** real Connections review/configuration, Access source/processing grants, Context selection/provenance/dependency reauthorization and Expert bindings remain mandatory. Native/local Calendar connector/provider/owner classification must be coherent across these owners; a non-EventKit source must not accidentally enter the remote Gateway branch. No raw-SQL authority seeds or allow-all fixtures.
- **External seam:** deterministic synthetic catalog/events and permission outcomes belong to the provider adapter. Platform availability facts belong behind an external adapter contract, not compile-target checks inside a business owner. Preserve current Apple support limits and source/person/device/revision/subject fences. This slice does not simulate external write effects.
- **Opt-in:** use an explicit Linux `qa-fixtures` build feature tied to development storage. Ordinary production builds must not expose or activate the fixture backend. Existing shared enum values do not by themselves authorize or provision a source. Do not add a mutable FFI switch, environment authority override or product test CLI.
- **Qualification:** require actual connection/resource selection/read, denied/no-payload and unselected-resource exclusion before claiming the fixture foundation complete. Then test real Schedule Expert delegation/tool reads through Conversation, with scripted model requests bound to consumer/run/task/attempt. Keep T1 distinct from HTTP, actual Flutter GUI and native-platform evidence.
- **Desktop host:** add a minimal Flutter Linux runner and same-snapshot `libfloe_ffi.so` bundling. Preserve product UI and existing Apple behavior. Debug QA uses encrypted development storage; release/Profile must not silently select weaker storage. The runner and fixture changes have disjoint implementation scopes and require root review before integration.
- **Execution environment:** root performs actual GUI/server QA in the visible dot Linux desktop under its existing user environment. Preserve its HOME and XDG configuration; do not copy credentials or create a second login context. Toolchain/cache files and explicit Floe test profiles may remain project-scoped. Cloud implementation-task builds are not desktop GUI evidence.

**Status — 2026-10-07:** the Linux runner, generated plugin registration and exact Cargo artifact staging were independently built by root in the visible desktop (`4bea2433395d1320ab09d13868ab40e0f4ba3aed` code snapshot). The bundled Rust library hash matches the Cargo/staged artifact. Real Floe Calendar, Connections and Settings screens rendered; navigation and orderly close/reopen were exercised, and the development Go server started alongside the client on port 18431. This is a limited startup/UI smoke, not pairing or inference qualification. Browser dashboard inspection hit `ERR_BLOCKED_BY_CLIENT` for localhost and was not bypassed. The fixture backend and Expert-source tests remain incomplete: recovery snapshot `4bdff790c5ba50f04e910a2f6699c1de875ecbff` contains partial unvalidated classification/feature work only. Documentation cleanup is published at `d0220d1e42858ee3e799e920d893d12bec404004`. P1a remains the five independently verified tests above.

### Remaining ordered slices

#### P0 — 기준과 계약 고정

코드 이동보다 먼저 최종 owner·제품 계약·원자성의 경계를 고정한다.

Prerequisite: current source and accepted target.

- **단일 계획:** 기존 docs/plans/2026-10-02-architecture-refactor.md의 남은 실행 순서를 현재 a31 기준으로 갱신한다. 새 계획을 또 활성화하지 않는다. 과거 1,172개 baseline 읽기 기록은 이번 snapshot 전수 읽기 증거로 재사용하지 않는다.
- **제품 계약:** conversation / day / connections / memory 네 제품 경계와 별도의 runtime readiness/native-host lane을 확정한다. request_id·command_id·disposition·CAS·cursor는 제품 의미이므로 유지한다.
- **권한 계약:** source/processing grant, operation consent, 직접 사용자 명령을 별도 타입으로 둔다. Access operation authorization과 Calendar Operations는 동일 암호화 Vault에 저장한다. local policy·consent 검증/사용과 dispatch intent를 단일 transaction에서 확정한다.
- **수명 결정:** Vault를 UI에서 숨기되 암호화 owner generation은 유지한다. 수동 외부 Calendar 변경도 준비된 runtime을 사용한다. locked 상태에서도 쓰기 위해 별도 평문 저장소나 두 번째 실행 경로를 만들지 않는다.
- **실행 전 파일 대조:** 아래 파일 앵커와 실제 caller/import를 기준으로 이관 표를 확정한다. 아직 개별 body가 미검토인 영향 파일은 읽기 전 구현 대상으로 넘기지 않는다. 범위가 달라지면 root가 판단하고 계획에 반영한다.

**Source anchors and disposition:**

- `docs/architecture/modules.md` — 각 owner의 최종 책임과 수명 수정
- `docs/architecture/authority-recovery.md` — operation 승인·dispatch·recovery 불변식 명시
- `tools/architecture/module-dependencies.json` — 새 Calendar Operations와 허용 의존 방향 설계
- `docs/plans/2026-10-02-architecture-refactor.md` — 기존 활성 계획의 남은 단계만 현재 기준으로 개정

**Delete after caller cutover:** 이전 계획을 근거로 새 설계를 덮는 설명; 별도 active migration plan / 진행 원장 중복

**UI impact:** UI 변경 전후를 구현 전에 해당 slice에 기록한다. 기존 카드·기능을 단순화 명목으로 삭제하지 않는다.

**Completion evidence:**

- 각 상태·판단의 owner가 하나로 지정됨
- 모든 slice에 caller 이관·폐기·완료 조건이 있음
- 그대로 둘 코드와 변경할 코드가 구분됨

**Constraint:** 계획 완성도를 구현 완료율로 표현하지 않는다. 이 HTML은 설계/실행 초안이며 새 commit이나 실제 QA 통과가 아니다.

#### P1 — 화면 없는 Conversation 통합 테스트

LLM/provider와 tool의 외부 경계만 mock하고 실제 Conversation 실행·권한·저장·복구를 반복 검증한다.

Prerequisite: P0.

- **실제 owner graph:** 실제 ConversationService·RunCoordinator·Engine·Experts·Inference·Access와 임시 encrypted Turso/Vault repository를 함께 사용한다. App/ReadyGeneration의 공통 조립 helper에 외부 adapter 입력만 주입하며 테스트용 별도 domain graph를 만들지 않는다.
- **LLM mock 위치:** Conversation의 ModelPort 전체를 성공 stub으로 바꾸지 않는다. 실제 InferenceService 아래 ModelProvider / PreparedModelTransport에 scripted adapter를 넣어 primary/fallback·capability·Access dispatch를 그대로 통과시킨다. provider JSON/HTTP codec은 다음 T2 계층에서 검증한다.
- **Tool mock 위치:** 현재 Manager는 NoManagerTools로 direct tool을 거부하고 ExpertTools가 source read를 담당한다. 생산 경로에 없는 manager tool 실행을 테스트 때문에 열지 않는다. 실제 Experts/Context/권한/CalendarOps를 통과시킨 뒤 OS·remote source·외부 effect I/O만 fake한다.
- **시나리오와 기록:** 텍스트 응답, 실제 Expert 위임/tool 결과, 허용되지 않은 tool, invalid output, budget/cancel, provider failure와 valid absence의 차이, 중복 command·중단·reopen을 검사한다. run/task/attempt별 모델 요청과 tool attempt/effect ledger를 둔다. 전역 응답 큐로 Learner/Expert 응답이 섞이지 않게 한다.
- **시간과 crash:** sleep 추측 대신 barrier·bounded deadline을 사용한다. orderly close와 process kill을 별개로 검증하고 provider 상태는 client 재시작과 독립시킨다. 승인/CalendarOps의 새 target 시나리오는 P3 계약 구현과 함께 추가하며 미구현 case를 통과로 세지 않는다.
- **반복 실행 단위:** 새 테스트는 crates/app/tests/conversation_integration.rs 및 support에 배치하는 안이다. cargo test -p floe-app --no-default-features --features development-storage --test conversation_integration 으로 GUI/Go/OAuth 없이 실행하는 것을 첫 완료 조건으로 둔다. mock은 외부 adapter만; synthetic authority fixture는 owner contract로 구성한다.

**Source anchors and disposition:**

- `crates/modules/conversation/src/application/service.rs` — 실제 owner/Run/interaction 실행 경로 유지
- `crates/modules/conversation/src/application/manager_policy.rs` — NoManagerTools 제한과 금지 tool 회귀 확인
- `crates/modules/inference/src/ports/model_provider.rs` — scripted ModelProvider/PreparedModelTransport 구현
- `crates/modules/inference/src/application/service.rs` — 실제 selector·projection·Access dispatch 보존
- `crates/modules/experts/src/application/engine_endpoint.rs` — 실제 ExpertTools와 journal 경로
- `crates/app/src/ready_generation.rs` — 외부 adapter만 주입하는 공통 조립 helper
- `crates/app/src/composition.rs` — 실제 임시 development installation 경로
- `crates/adapters/vault/src/vault/agent_actions.rs` — 실제 transaction/replay/recovery adapter
- `crates/app/Cargo.toml` — 격리 integration-test target/dev dependencies

**Delete after caller cutover:** Conversation 결과를 통째로 돌려주는 fake owner; 권한을 무조건 허용하는 fake; model 출력으로 direct-user intent 위조; 테스트마다 실제 클라이언트/Go/OAuth 기동을 요구하는 조건; 테스트용 제품 CLI / 두 번째 domain 실행 경로

**UI impact:** 제품 UI 변경 없음. 자주 반복할 검증을 Rust 통합 테스트로 옮겨 사용자와 GUI를 병목에서 뺀다. Flutter headless/desktop은 이후 소수의 화면-계약 연결 시나리오에 사용한다.

**Completion evidence:**

- 실제 Conversation+Inference+Access+Vault가 실행된 텍스트/위임 경로 증거
- 예상하지 않은 model/tool 요청은 fail, 금지 경로는 zero I/O
- command replay·cancel·reopen에서 상태/ledger 일치
- GUI·Go server·실사용 OAuth 없이 반복 가능
- provider protocol·UI·native OS 미검증 범위를 별도로 표기

**Constraint:** P1a harness 및 위 5개 사례는 구현·검증되었다. 실제 Expert/source 호출은 P1b에서 추가한다. orderly close/reopen 통과를 강제 종료 복구나 외부 효과 exactly-once 증거로 사용하지 않는다. 도구 설치와 Flutter desktop sample 성공은 실제 Floe GUI 통과 증거가 아니다. 전체 legacy suite 재작성과도 구분한다.

#### P2 — 앱 준비와 제품 Router 정리

Vault 내부 lifecycle을 숨기고 transport 독립 제품 dispatch를 한 경로로 만든다.

Prerequisite: P1.

- **공유 client bootstrap:** main._start에서 support directory·library·OS reader 해석을 분리해 production/headless/desktop QA가 같은 openDefault·broker 등록·runtime 준비·close 경로를 사용하게 한다. Flutter 샘플은 실제 desktop에서 실행/버튼 조작까지 확인했다. Floe Linux runner는 제한된 QA host로 추가한다.
- **Rust 준비 수명:** VaultBridge/ReadyGeneration의 create/open/activate/retire/drain을 App runtime 내부로 둔다. callback host 준비 뒤 기동하며 준비 상태·실패·진단 ID·허용 복구 행동만 Flutter에 제공한다. 자동 reset은 없다.
- **Typed Router:** crates/app/src/api.rs에 transport-neutral ProductCommand/Query/Outcome를 정리하고 새 router.rs가 host admission 뒤 owner API로 dispatch한다. FFI는 DTO 구조 검증·변환·메모리/ABI만 담당한다. App이 serde wire DTO나 FFI에 역의존하지 않는다.
- **한 dispatch 경로:** Conversation/Connections의 FFI 직접 owner 호출과 나머지 App *_services forwarding을 차례로 동일 router로 이관한다. routing 검증 이후 기존 중복 enum/trait/scope 생성 경로를 제거한다. 새 forwarding facade를 겹치지 않는다.
- **Flutter 분리:** VaultController/AgentVaultGateway 대신 RuntimeReadiness 관찰 모델로 교체한다. NativeCommandDisposition 의미는 product-level CommandOutcome로 이동해 feature가 NativeTransport 구현 타입을 import하지 않게 한다.
- **Memory 제품화:** knowledge.memory.*를 memory.* 제품 의도로 교체하되 overview/review/decide 의미·불명 command identity를 보존한다. 기존 소비자를 함께 이관한다.

**Source anchors and disposition:**

- `apps/client/lib/main.dart` — production과 QA가 공유할 bootstrap 추출
- `crates/app/src/api.rs` — 제품 intent/outcome canonical 계약
- `crates/app/src/host.rs` — verified request admission 유지
- `crates/app/src/owner_handles.rs` — scope/actor/generation 전달의 단일화
- `crates/app/src/vault_lifecycle.rs` — 내부 queue·retirement 보존, 제품 노출 제거
- `crates/app/src/vault_services.rs` — 제품 command/query 폐기 후 내부 lifecycle로 축소
- `crates/app/src/knowledge_services.rs` — 중복 enum/forwarding 제거
- `crates/bindings/ffi/src/app_wire.rs` — 직접 orchestration을 router로 이관
- `crates/bindings/protocol/src/dto/vault.rs` — 제품 DTO 폐기
- `apps/client/lib/app/runtime/app_runtime.dart` — readiness 관찰로 교체
- `apps/client/lib/features/vault/application/vault_controller.dart` — 기능 caller 이관 후 삭제
- `apps/client/lib/app/runtime/owner_operation.dart` — transport-neutral command outcome 적용
- `apps/client/lib/app/startup_view.dart` — 준비·실패·다시 확인 제품 projection

**Delete after caller cutover:** product vault.create/unlock/lock/status/read_result; Flutter features/vault 및 AgentVaultGateway 제품 의존; FFI와 App의 중복 routing / forwarding; knowledge.memory.* 이전 제품 namespace

**UI impact:** 앱은 자동 준비. profile 설정이나 수동 Vault unlock을 정상 onboarding으로 요구하지 않는다. 실패 화면은 데이터 보존, 이유·진단 ID·가능한 재시도를 안내한다. Memory 기능과 기존 navigation은 유지한다.

**Completion evidence:**

- fresh install·reopen·startup failure·retirement·재시도 QA 통과
- UI disposal이 owner cancellation을 만들지 않음
- 삭제 namespace를 protocol/Dart/FFI caller에서 찾을 수 없음
- 진단·indeterminate command 재관찰이 보존됨

**Constraint:** App의 lifetime 코드 자체는 삭제 대상이 아니다. owner publish 전에 callback과 custody를 준비하고, 종료 때 admission fence를 먼저 닫는다.

#### P3 — 권한·일정·대화 승인 완결 slice

Actions를 역할에 맞게 재편하고 수동/에이전트 일정 경로를 하나의 안전한 실행 owner로 모은다.

Prerequisite: P2.

- **Owner 재편:** crates/modules/actions를 calendar_operations로 바꾸고 effect normalization·operation identity·dispatch·receipt·Unknown/reconcile은 남긴다. authority 정책·검토 결정은 기존 Access의 operation_authorization 하위 책임으로 이관한다.
- **권한 aggregate:** Access의 OperationSubject/ApprovalRef/DecisionReceipt는 exact operation ID·effect digest·actor·policy revision·expiry에 묶인다. Calendar Operations가 만든 불변 subject와 owner-defined 검증을 사용한다. UI/model이 approved=true나 임의 JSON으로 권위를 만들 수 없다.
- **원자적 저장:** Vault adapter가 owner별 순수 transition을 같은 Immediate tx에서 적용한다. initial operation+review 연결, approval 결정 replay, live local policy 재검증+approval 사용+dispatch intent CAS를 각각 명시한다. 외부 OS/source의 연속성 검사는 preflight/실행 adapter fence로 별도 보존한다.
- **수동 Day 명령:** Day에 external calendar command용 inward port를 두고 Calendar Operations가 구현한다. Day가 concrete CalendarOperations crate를 import해 역방향 cycle을 만들지 않는다. Day mutex/transaction을 놓은 뒤 외부 owner를 호출한다. tool/model registry는 direct-user 진입점을 노출하지 않는다.
- **Conversation interaction:** OperationApproval target과 receipt correlation을 추가한다. Conversation은 보여주기·decision command ID·재개 연결을 소유하고 승인 상태는 Access를 읽는다. 승인 후 같은 operation을 이어가며 linked fresh Run이 write를 재제안하지 않게 한다.
- **직접 명령 정책 분리:** 직접 Day 편집은 agent calendar_create Allow/Ask/Deny revision에 매이지 않게 한다. actor·대상·source/OS 권한 검사는 유지한다. 단순 정책 toggle이 이미 승인된 수동 작업을 무관하게 중단시키는 결합을 제거한다.
- **미완결 상태 처리:** spawn 실패·pre-dispatch 오류가 Approved에 고착되지 않도록 durable outcome 또는 동일 command 재관찰 의미를 명시한다. dispatch 이후에는 취소·timeout을 미실행으로 단정하지 않는다.

**Source anchors and disposition:**

- `crates/modules/actions/src/application/submit.rs` — 불변 intent는 CalendarOps, authority 분기는 Access로
- `crates/modules/actions/src/domain/transitions.rs` — 순수 approval transition과 effect transition 분리
- `crates/modules/actions/src/application/execution.rs` — dispatch/Unknown/lookup-only recovery 유지
- `crates/modules/actions/src/ports/repository.rs` — operation repository와 policy/approval repo 분리
- `crates/modules/access/src/lib.rs` — operation_authorization 모듈 공개 계약 추가
- `crates/adapters/vault/src/vault/agent_actions.rs` — 동일 DB의 cross-owner atomic commit 구현
- `crates/modules/conversation/src/domain/interaction.rs` — OperationApproval typed target
- `crates/modules/conversation/src/application/interaction_resolution.rs` — 결정 receipt 검증과 재개 correlation
- `crates/modules/conversation/src/application/resume.rs` — 기존 effect와 fresh Run 연결
- `crates/modules/day/src/application/mutations.rs` — 수동 external operation 제품 진입점
- `apps/client/lib/features/actions/presentation/agent_proposal_card.dart` — Conversation interaction card로 이관
- `apps/client/lib/features/day/presentation/personal_day_screen.dart` — day 제품 gateway로 전환
- `crates/bindings/ffi/src/actions_wire.rs` — caller 이관 후 product wire 제거

**Delete after caller cutover:** floe-actions crate 및 ActionsService 일반 명칭; product actions.*와 새 calendar_operation.* 공개 API; 독립 Actions UX/직접 approval API; 기존 CalendarActionFacade와 LocalOwnerGateways.actions 직접 사용

**UI impact:** 대화에서 제안·승인·거절·진행·결과 불명 상태가 한 interaction 흐름으로 보인다. 수동 Day 편집은 추가 agent 승인 카드 없이 실행 상태를 보여준다. 일정 변경 확인 정책은 기능 설정으로 옮기며 일반 Gateway LLM 호출에는 prompt를 추가하지 않는다. 취소 가능한 시점과 이미 실행 여부가 불명인 상태를 UI에서 구분한다.

**Completion evidence:**

- Allow/Ask/Deny, stale review/policy/source/event, 중복 클릭·변경된 digest 거부
- 승인 commit 전후·dispatch 전후·effect 후 ACK 유실 crash matrix 통과
- 금지/거절은 zero write, 허용된 단일 작업은 attempt/effect ledger로 중복 없음 확인
- update/delete receipt 부재는 Unknown 유지
- manual command가 agent policy와 독립이며 Day↔CalendarOps compile cycle 없음

**Constraint:** 새 approval DB나 분산 reservation protocol을 추가하지 않는다. 같은 custody를 이용한 transaction 계약이 성립하지 않으면 이 slice를 완료 처리하지 않는다. provider create marker·crypto domain separator는 의미 검토 없이 문자열 rename하지 않는다.

#### P4 — Expert·Context 제품 경계 정리

제품에 필요한 기능은 남기고 내부 registry·source 해석·native 의미를 숨긴다.

Prerequisite: P3.

- **Expert 제품 projection:** Expert package/installation/binding을 내부에 유지하고 Conversation의 assistant feature 설정으로 기능 on/off·사용할 자료를 노출한다. 설정 한 번이 여러 불완전 mutation으로 갈라지지 않도록 owner command를 사용한다.
- **Common model 확인:** built-in도 공통 package/runtime 계약과 immutable Task receipt 경로를 사용하는지 대조한다. 이미 동작하는 registry/task 기반을 다시 만들지 않는다. 외부 marketplace·remote A2A 배포·plugin sandbox 확대는 범위 밖.
- **Context→Access 정리:** Context의 revoked/paused grant 해석은 Access classification/read contract로 이관한다. Context는 획득·projection·증거를 소유한다. Context가 DayRepository 전체를 직접 받는 대신 필요한 bounded evidence/read port로 좁힌다.
- **Native completion 분리:** context_services의 People/Wellbeing/Attention payload·Health transform 의미 검증은 Context/source adapter로 옮긴다. App은 verified caller/runtime epoch 전달·host 등록/retire만 맡는다. Flutter native callback lane은 유지한다.
- **Learner와 event:** Knowledge가 scheduling을 계속 소유한다. Conversation commit 후 signal은 필요가 확인되면 작은 typed hint로 넣고 evidence 재탐색을 진실의 근거로 유지한다. 범용 event bus/Event Sourcing 전환은 이번 필수 조건으로 만들지 않는다.

**Source anchors and disposition:**

- `crates/app/src/expert_services.rs` — 제품 route cutover 뒤 forwarding 삭제
- `crates/bindings/ffi/src/experts_wire.rs` — 직접 제품 namespace 폐기
- `crates/bindings/protocol/src/dto/experts.rs` — 내부 registry DTO를 제품 feature projection으로 대체
- `apps/client/lib/features/experts/application/agent_registry_controller.dart` — assistant feature controller로 소비 경계 전환
- `crates/modules/context/src/application/expert_sources.rs` — 권한 상태 해석을 Access로 이동
- `crates/app/src/context_services.rs` — completion 의미 검증을 source owner/adapter로 이동
- `crates/app/src/local_context.rs` — 등록·수명과 source semantics 분리
- `crates/modules/knowledge/src/lib.rs` — Learner owner 수명·발견 경로 보존
- `crates/modules/experts/src/lib.rs` — 공통 package/task 계약과 feature projection 정리

**Delete after caller cutover:** experts.* public command/query와 registry를 복제한 UI; Context의 독자 grant 상태 해석; App의 source-specific payload/transform 판단; 단지 위임만 하는 새 service locator

**UI impact:** Expert라는 backend 용어 대신 사용 가능한 기능과 자료 선택을 보여준다. 기능 on/off·source 선택·권한 검토를 삭제하거나 하나로 뭉개지 않는다. macOS Calendar connection 카드는 유지한다.

**Completion evidence:**

- feature enable/binding command가 원자적으로 반영되고 기존 Task selection 불변
- Context source drift·revocation·transform failure closed 유지
- App에 source별 의미 분기가 남지 않음
- product experts.* 제거, feature UX 동작 확인

**Constraint:** Knowledge polling 자체를 결함으로 취급하지 않는다. Health transform 구현 경로는 검토하지만 이번에 iOS build/실행 확대는 하지 않는다.

#### P5 — Go owner·adapter 완결 slice

실제 View 한 개에서 의미·권한·저장·HTTP 타입 경계를 완성한 뒤 같은 구조로 확장한다.

Prerequisite: P4.

- **의존 DAG 먼저:** Trust는 identity/principal/custody port, Authority는 signed enforcement, Views는 parse/read/validate workflow. 공유되는 SourceReference/Snapshot/Bounds 등 순수 값만 contracts/source로 옮긴다. authority는 views application을 import하지 않고 Views가 자신의 Authority port로 협력한다.
- **첫 View:** authority.SourceService의 Preview/Admit/Read/Release orchestration을 views/application으로 이관한다. Engine의 issue/claim/stage/release 검증은 authority에 남긴다. canonical query bytes·proof binding·one-use release·fence를 그대로 보존한다.
- **Repository 역전:** Trust.Repository, Integrations.Repository, Inference.ConfigRepository와 실제 필요한 credential capability port를 owner가 정의한다. owner transition/원자적 commit 의미는 남기고 파일명·JSON 저장·암호화 mechanics만 adapters/storage로 이동한다.
- **Typed result:** Catalog/authorization attempts/pairing/View 결과를 (TypedResult,error)로 반환한다. transport/http만 JSON DTO와 HTTP status를 만든다. 공통 error category는 유지할 수 있다. core response map과 Value any는 폐기한다.
- **외부 adapter 이관:** connectors/* → adapters/integrations/*, provider/Codex → adapters/models/*, OAuth runtimes → adapters/oauth/*, credentials/storage → adapters/credentials·storage. Node에서 구현체를 조립한다. import 역전과 함께 cutover하며 이전 경로 wrapper를 남기지 않는다.
- **동일 구조 확장:** Calendar View 이후 Communication/Work/Logistics와 connector caller를 이관한다. 모델 capability는 provider/model metadata에서 계산해 사용자 체크박스 의존을 제거하고 explicit unknown/unsupported를 표시한다.
- **Go gate:** go list import graph에 owner→concrete adapter / authority→views application / core→HTTP 금지 규칙을 적용한다. 테스트 fixture로 금지 edge를 넣으면 실패해야 한다. 기존 Rust Cargo gate를 Go 검증으로 세지 않는다.

**Source anchors and disposition:**

- `server/internal/authority/source_service.go` — View workflow를 views service로 이동
- `server/internal/authority/authorization.go` — authority enforcement와 neutral source contract 사용
- `server/internal/authority/ports.go` — View reader 계약과 authority fence 구분
- `server/internal/views/contracts.go` — 순수 shared source 값 추출, View query/result는 유지
- `server/internal/trust/service.go` — storage.Files 의존을 Repository로 역전
- `server/internal/integrations/service.go` — typed 결과와 Repository/credential capability
- `server/internal/integrations/state.go` — persistence mechanics를 adapter로 이동
- `server/internal/inference/config_store.go` — ConfigRepository adapter로 이관
- `server/internal/operation/result.go` — Value any 및 Accept(any) 사용 제거
- `server/internal/node/integration_factories.go` — 이관된 adapter 주입
- `server/internal/transport/http/source.go` — typed ViewsService 소비와 DTO mapping
- `server/internal/transport/http/connectors.go` — typed integration 결과 mapping
- `server/internal/transport/http/web/app.js` — 모델 capability 수동 선택 UI를 metadata projection으로 교체

**Delete after caller cutover:** authority의 View workflow/reader 선택; owner의 concrete storage·credential import; operation.Result.Value any와 core 응답용 map[string]any; 이전 connector/provider 경로의 compatibility wrapper

**UI impact:** 대시보드 기능을 유지하며 사용자가 모델별 tool/JSON 지원 여부를 선언하지 않게 한다. 서버 연결·pairing 단계가 다시 늘어나지 않게 한다.

**Completion evidence:**

- Preview→admit→proof→read→stage→release happy/negative/replay 검증
- Trust activation/credential readback/cleanup 중단·재시작 의미 보존
- HTTP contract·typed result caller 전부 이관
- Go dependency gate positive/negative fixture 통과
- 구체 provider는 같은 protocol fixture에서 동일 결과

**Constraint:** storage별 commit을 무작정 잘게 쪼개지 않는다. pairing/trust/credential의 현재 복구 계약을 repository port가 표현해야 한다. 외부 protocol JSON과 동적 schema의 adapter-local map은 제한적으로 허용한다.

#### P6 — 폐기 확인과 통합 검증

구조·행동·UI·복구 증거를 한 snapshot으로 묶고 리팩토링을 종료한다.

Prerequisite: P5.

- **폐기 검색:** 사용하지 않는 Actions/Experts/Vault product API, 예전 adapter import, duplicated Router와 owner forwarding을 검색한다. crypto domain separator·provider marker 등 의도적으로 유지하는 문자열은 이유를 별도로 기록한다.
- **계층별 통합:** T1 Rust Conversation 통합을 주 회귀로 삼고, T2 실제 Rust/Go+mock HTTP, T3 소수 Flutter UI 연결, T4 macOS 실제 OS 경계를 순서대로 확인한다. GUI 앱을 매 테스트마다 띄우지 않는다.
- **회복/오류:** 동일 command replay, 중복 결정, stale revision, source 변경, model 오류, 화면 닫기, process kill, server restart, native receipt cache loss를 실제 통합 경로로 점검한다.
- **빌드·구조 gate:** 완료 slice의 format/compile을 모아 실행하고 최종 구조에서 Rust/FFI·Flutter·Go 전체 compile/build를 한 번 확정한다. 최소 QA smoke와 행동 검증 이후 필요한 테스트를 새 owner 구조에 맞게 순차 재작성한다.
- **문서 정합성:** current architecture·dependency policy·제품 wire 목록·native 경계·run-local 개발/production custody를 실제 코드와 맞춘다. 남은 platform gap은 명시한다.

**Source anchors and disposition:**

- `tools/architecture/check_boundaries.py` — Rust 새 topology 검사
- `tools/architecture/module-dependencies.json` — 이관된 실제 허용 edge 반영
- `scripts/run-local.sh` — 같은 snapshot client/server 기동 경로 점검
- `docs/architecture/README.md` — 최종 책임과 current source 링크
- `docs/architecture/modules.md` — owner별 실제 배치 확정
- `docs/architecture/authority-recovery.md` — 검증된 원자성·Unknown 계약 반영

**Delete after caller cutover:** 임시 dual path; 전환용 nullable state; 동작 검증 없는 완료 주장; 미사용 development harness/설정이 production에 포함되는 경로

**UI impact:** 현재/목표 UI 차이를 side-by-side로 최종 점검한다. 원하는 기능을 잃은 채 단순화됐다고 완료하지 않는다.

**Completion evidence:**

- 13개 과제별 구조 증거·동작 증거·잔여 제한을 기록
- 전체 compile/build 성공
- macOS client+server 대표 사용자 시나리오 성공
- 되돌릴 수 없는 삭제·보안 설정 변경은 별도 승인 범위 준수

**Constraint:** iOS/Android 수평 확대는 보류한다. Linux mock QA는 실제 EventKit/TCC/Keychain/서명을 인증하지 않는다. 단위 테스트 숫자나 파일 이동량으로 진행률을 부풀리지 않는다.

### Validation cadence and publication

No per-file compile/format/test loop and no repeated full workspace builds. Validate at a dependency-closed slice. The newly requested T1 integration harness is explicit early behavior validation, not wholesale legacy-suite reconstruction. Use focused tests for the completed scope; complete final structure compilation/build and subsequent test reconstruction under the repository verification skill. Do not mark skipped/unimplemented target cases passed.

Root owns design and review. Implementation workers receive bounded contracts and return changes for integration. Preserve unrelated user work. Commits use the authorized user identity. Do not infer credentials, reset or irreversible-delete approval from this sequence.

### UI and behavior carryovers

- UI baseline evidence is the pre-refactor `3f4b407f8079d611224cd7adbef121f9e7e75e8e` snapshot. The prior restoration log is [historical evidence](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/plans/2026-10-04-flutter-ux-restoration.md), not a second active execution plan.
- Preserve the Calendar connection card, normal Remote server flow, owner-driven machine pairing proof and administrator approval; do not restore the extra client code-confirm button or per-model invocation consent.
- Future-calendar inclusion is a distinct persisted policy, not selecting today's inventory. Global assistant Calendar preference and per-Expert binding must not silently widen grants. These choices need explicit semantics before implementation.
- Remaining UI qualification includes Memory/Settings/detail/error states, bounded-history cursor recovery, retained-command recovery, source switching while a review is pending, active Observe, Calendar write/recovery and the full-screen comparison. Source changes alone do not close these gates.
- Production Keychain/signing identity and actual native permission behavior remain separate platform checks, even though encrypted development profiles and server encrypted private files are implemented.

### Historical evidence

The previous cutover task sheets, command gates, review reports and progress logs were retired from active documentation. They remain in [the pre-cleanup Git snapshot](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/plans/). The [legacy behavior evidence](../testing/legacy-behavior/README.md) is retained for reconstructing meaningful tests; old assertions are evidence, not automatic target requirements.

Model quality is a separate qualification dimension: deterministic provider mocks do not prove grounded real-model answers. Preserve ADR 0033/0034's strict Primary corpus/rubric and separately reported fallback safety floor for the later real-provider qualification; do not revive old checkpoint order or infer a pass from T1 mocks.
