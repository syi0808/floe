# R003 — 구조 우선·단일 에이전트 순차 리팩터링 계획

| 항목 | 결정 |
|---|---|
| 문서판 | **R003**, 2026-09-15 (Asia/Seoul) |
| 직전 문서판 | [R002](../r002-sequential/README.md) — 직전에 합의·커밋한 순차 구조 우선 계획 |
| 기준 main | `cfde8e24387454d519c9e3308606a7cc6bb7f6c9` |
| 제품 코드 기준 | `89452eb5523ef6b1c76b7fe857095748d76de22d`와 같은 product tree |
| 실행자 | 한 코딩 에이전트. 조사·구현·검토·통합을 직접 순차 수행 |
| 1차 완료 | 실제 구현·caller·adapter·단일 수정자·구형 제거·구조 검사 |
| 2차 완료 | 그 한 구현에서 실제 동작·장애·Apple 앱·모델 검증 |
| 상세 처방 | [EXECUTION_PLAN.md](EXECUTION_PLAN.md)와 순서대로 연결된 9개 실행 절 |
| 실행 지침 | [AGENT_PROMPT.md](AGENT_PROMPT.md) |
| 진행 기록 | [../../migration-ledger.md](../../migration-ledger.md) 한 곳 |

R003은 리팩터링 **문서의 판번호**다. 앱 구현의 v3, 프로토콜 협상, DB schema 증분이 아니다. 코드에 새로운 버전이나 평행 런타임을 만들지 않는다. R002의 구조 우선·단일 실행자 결정을 변경하지 않고, 모든 단계의 코드 변경 처방을 구체화한 판이다.

## 1. 목표와 범위

Floe의 이후 개발이 명확한 모듈 경계와 단일 상태 소유권을 따라 진행되도록 실제 코드를 정리한다. 폴더와 trait만 만드는 작업이 아니다. 제품의 현재 지원 기능은 실제 서비스·저장·I/O·Flutter caller에 연결되어야 한다. UI 시연을 위해 구형 실행기를 계속 보존하지 않는다.

이번 커밋은 문서만 발행한다. 아래 코드 변경은 아직 실행하지 않은 작업 명세다. 과거 테스트 결과와 새 문서의 발행을 구현 완료로 표시하지 않는다.

### 바꾸지 않는 결정

- 일반 대화 root는 하나다. host는 적격성·권한·예산, Manager LLM은 위임 여부·대상·자연어 목표, Directory는 AgentId 전달을 담당한다.
- Session/Run은 Conversation, A2A Task는 Experts, Connection Operation은 Connections가 유일하게 변경한다. 한 코딩 에이전트로 작업한다고 제품의 A2A와 Run/Task 동시성을 없애지 않는다.
- 같은 Session의 root 쓰기는 하나다. 다른 Session/관리 query가 그 실행을 암묵적으로 취소하지 않는다. 화면 종료와 관측 timeout은 Run 취소가 아니다.
- 기존 암호화·키 identity·current authority·exact recipient·provenance·CAS·durable intent·불확실한 외부 write 보호를 유지한다.
- 하위호환, old decoder, migration chain, Legacy 실행 wrapper, 별도 v2/v3/next 경로를 유지하지 않는다. 정상적인 repository/provider adapter와 persistence record 변환은 유지한다.
- 스키마 숫자를 추가로 올리지 않는다. 기준 app wire는 2, Conversation 저장 marker는 7이다. 이미 다른 HEAD에서 바뀐 값은 과거 값으로 되돌리지 않고 기록한다. authority revision, executor generation, runtime/vault epoch는 정상 갱신한다.
- 고정 번호는 구형 데이터 호환을 의미하지 않는다. 변경된 저장 정의는 명시적인 새 Floe 개발 profile로 확인하고, open 오류를 자동 DB 삭제·키 교체로 처리하지 않는다.
- 외부 OAuth/A2A/서명 형식과 Keychain namespace의 버전 문자열은 앱 API 접미사 제거와 별개다.

## 2. 구조 단계와 동작 단계를 분리

**A에서 한다:** 의미가 구현된 상태 전이와 오류 경계, 실제 repository와 provider 구현, 최종 AppHost/FFI/Flutter 연결, 구형 코드 삭제. 의미 있는 변경 묶음 끝에 관련 타입·compile·DAG·private import를 확인한다. 전체 앱 성공을 중간 조건으로 요구하지 않는다.

**A에서 예외적으로 앞당긴다:** 잘못되면 설계 자체가 무너지는 native thread/lifetime 또는 저장 원자성 가정, 이번에 변경하는 권한·키·중복 외부 실행·취소 방향. 기존 좁은 회귀를 재사용한다. live OAuth·실제 사용자 데이터·전체 앱 시연으로 확대하지 않는다.

**B에서 한다:** 통제된 통합·fault injection → 일반 Apple 앱의 Vault/Keychain → local/remote 대화와 Connect → 실제 LLM 위임 평가. 발생한 버그를 담당 owner에서 수정한다. 구조 결함이면 근거와 영향 범위를 기록해 계약을 고치며 구형 런타임으로 우회하지 않는다.

A 완료는 **구조 완료 / 동작 not_run**이다. B 이전에는 제품 안정성·배포 가능을 선언하지 않는다. 알려진 일반 앱 VaultUnavailable의 진단 코드는 A에서 준비하고, 현장 재현·서명/Keychain 수정은 B에서 수행한다.

## 3. 목표 코드 구조

```text
Flutter features -> FloeClient -> 한 C ABI -> floe-app 조립
                                         Conversation / Experts / Connections
                                         Inference / Access / Context
                                         Day / Knowledge / Actions
                                                -> owner-defined ports
                                                -> Vault / Provider / Native
snapshot + events -> AppReadModel slices -> selectors -> 화면
```

| 경로 | 역할 |
|---|---|
| `crates/contracts/{kernel,context,agent}` | 작은 공통 값·메시지·도구/위임 계약. 업무 상태 owner가 아님 |
| `crates/runtime/{execution,agent}` | 취소·예산 기계장치와 role-neutral loop |
| `crates/modules/{access,connections,inference,knowledge,day,context,actions,experts,conversation}` | 기능별 정책·상태 수정자·공개 API·repository port |
| `crates/experts/builtin/src/<expert>/` | 8개 Expert의 descriptor·role·endpoint·tools·artifacts |
| `crates/adapters/{vault,providers}` | 실제 저장 및 모델/source/control I/O 구현 |
| `crates/platform/{native,diagnostics}` | OS 자원·키 접근·진단 |
| `crates/app` | 실제 조립·host 수명주기·검증된 caller. 업무 판단은 각 모듈 |
| `crates/bindings/{protocol,ffi}` | 하나의 wire와 얇은 변환·메모리 수명주기 |

승인된 22개 package와 허용 직접 의존성의 기계 판독 원본은 [module-dependencies.json](../../../../tools/architecture/module-dependencies.json)의 `target`이다. 그 파일의 `current`는 과거 전사 기록이며 최신 그래프로 오인하지 않는다. 실제 manifest가 더 넓다고 허용 목록을 늘리지 않는다. FFI는 app/protocol에만 내부 의존한다. Vault는 Context에 직접 의존하지 않고, provider adapter는 protocol DTO를 내부 모델로 사용하지 않는다.

### 상태와 경계

| 사실 | 수정자 | 다른 계층이 받는 것 |
|---|---|---|
| Session·transcript·Run·command receipt | Conversation | revision snapshot·RunRef |
| Expert 등록·활성 상태·A2A Task | Experts | 카드·TaskReceipt |
| Connection 설정·관측·Operation | Connections | snapshot·RefreshIssue·OperationRef |
| Model profile·Attempt·실제 usage | Inference | profile/attempt receipt |
| 권한·전송/공개 허가·철회 fence | Access 및 실제 OS/Go authority | 불투명 permit·검증된 evidence |
| 허용 context·파생 lineage | Context | immutable projection·coverage |
| Memory·Playbook·Learner | Knowledge | 승인된 읽기·review 결과 |
| 제안·승인·외부 실행/불확실성 | Actions | proposal/action receipt |
| Calendar mirror·Event·할 일·Note | Day | revision snapshot |
| draft·선택·focus·scroll | Flutter | 화면 자체 상태 |

## 4. 이미 확보한 구현은 재사용

기준 코드에는 ConversationService와 새 Engine의 일반 대화 연결, Calendar-first root 제거, TaskCoordinator/Directory, durable command/cancel/retry, FloeClient 및 conversation read model이 있다. 이를 다시 구현하는 별도 프레임워크를 만들지 않는다.

남은 일은 연결되지 않은 owner/adapter를 완성하고, 실제 FFI의 LegacyComposition·변환 port·Session bridge·중앙 builtin 분기를 제거하는 것이다. 동기 FFI route 네트워크, command digest의 환경 상태 결합, root/child 실패와 정산, UI/Console 관리 상태가 핵심 교정 대상이다. 기준 근거는 [SOURCE_ANCHORS.md](SOURCE_ANCHORS.md)에 있다.

## 5. 순차 실행

| 순서 | 단계 | 상세 절 | 완료 범위 |
|---|---|---|---|
| 1 | A | [01 계약](steps/01-contracts.md) | canonical 타입·명령·archive·단일 API 정의 |
| 2 | A | [02 업무 owner](steps/02-business-owners.md) | Connections/Actions/Day/Knowledge/Go 상태·port |
| 3 | A | [03 수락·Inference](steps/03-admission-inference.md) | route-independent replay·durable admission·비동기 준비 |
| 4 | A | [04 실행·복구](steps/04-runtime-recovery.md) | batch journal·scope 실패·finalization·단일 usage |
| 5 | A | [05 실제 adapter](steps/05-adapters.md) | 실제 Vault/provider/native 및 진단 이관 |
| 6 | A | [06 Expert](steps/06-experts.md) | 독립 endpoint·공통 Directory·per-invocation route |
| 7 | A | [07 전체 연결](steps/07-composition-client.md) | 실제 AppHost·Session·한 ABI·Flutter slices |
| 8 | A | [08 구조 심사](steps/08-structure-review.md) | 구형 제거·기능 대응·DAG·컴파일 확인 |
| 9 | B | [09 동작 검증](steps/09-behavior-validation.md) | 현재 단일 구현의 실제 행위·결함 수정 |

기존 계획 §3.1–3.9 및 P00–P25 의미를 유지한다. `01.1` 같은 번호는 이 문서판의 하위 실행 절이지 새 진행 원장이 아니다. 한 변경 묶음에는 계약과 직접 소비자·DDL·validator·decoder를 함께 포함한다. 뒤 단계에서 필요한 최소 타입은 앞에서 정의할 수 있지만 여러 작업을 동시에 열지 않는다.

한 묶음에서 잠깐 컴파일이 깨질 수 있다. 대응 호출자를 같은 묶음에서 복구한다. 전체 조립이 아직 남은 사실은 정확히 표시하되 mock-only 배선이나 compatibility shim으로 숨기지 않는다. 코드 작성 뒤 반드시 자체 diff 검토를 한다.

## 6. 문서와 진행 기록의 수명

`docs/refactoring/versions/rNNN-*/`는 발행된 계획·프롬프트·실행서의 이력이다. 과거 판을 현행 상태로 덮어쓰지 않는다. 의미가 바뀌는 계획은 다음 문서판을 추가하고 최상위 README/CHANGELOG의 활성 판만 갱신한다. 작은 링크·오탈자는 해당 판에 Git 변경으로 남기되 정책을 몰래 바꾸지 않는다.

진행률·이동 후 위치·검사 결과는 버전 문서에 누적하지 않고 `docs/refactoring/migration-ledger.md` 한 곳에 기록한다. 역사적 계획의 상대 경로는 발행 당시 맥락이며 원본 commit permalink로 열 수 있게 버전 README에 명시한다. 일반 아키텍처 설명은 `docs/architecture/`에 유지하고 실행 이력과 섞지 않는다.

## 7. 인수 기준

A: 실제 코드가 승인 DAG와 단일 수정자를 사용한다. 모든 기존 지원 기능의 UI→owner→adapter→projection 대응이 존재하고, 정상 기능을 Unsupported/TODO/가짜 성공으로 대체하지 않는다. 구형 API/owner/실행기 선택기와 `_v2` alias가 없다. 가능한 target의 정적 검사와 변경한 고위험 계약의 제한된 확인을 끝낸다.

B: 동일 source snapshot으로 만든 앱·dylib·현재 개발 데이터에서 필수 사용자·장애 경로를 확인하고 버그를 고친다. 환경상 불가능한 항목은 명확하게 남기며 전체 통과로 표시하지 않는다. 문서 발행이나 A 완료는 B 통과가 아니다.

이번에 하지 않는다: 신규 음성/wake/cross-device 기능, Android parity, Learner 품질 연구, SDK 대규모 업그레이드, 새 상태관리 프레임워크, 전면 event sourcing, schema fingerprint registry, 재귀/병렬 A2A 확장, 별도 코딩 에이전트.
