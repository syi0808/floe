# Floe 단일 에이전트 순차 리팩터링 계획
## 구조 완성 → 동작 검증 · 하위호환 없음 · 스키마 번호 추가 증가 없음

| 항목 | 기준 |
|---|---|
| 작성일 | 2026-09-15 · Asia/Seoul |
| 저장소 / 직전 확인 소스 기준 | `syi0808/floe` / `89452eb5523ef6b1c76b7fe857095748d76de22d` |
| 기준 커밋 | `docs(inference): record host route cutover` · 2026-09-15 10:45:10 KST |
| 현행화 범위 | 승인된 단일 에이전트 계획을 저장소의 활성 명세로 설치. 기술 변경 범위와 순서를 유지 |
| 소스 확인 범위 | 문서 현행화 시 main의 `89452eb…`를 확인. 전체 코드 재감사·제품 검증은 수행하지 않음 |
| 실행 주체 | **하나의 구현 에이전트**가 설계 조정·수정·자체 검토·통합·검증을 전담 |
| 1차 목표 | 실제 구현·호출자 연결·구형 경로 제거·구조 검사 완료 |
| 2차 목표 | 그 하나의 구현에서 회귀·일반 앱·실제 모델 검증 및 결함 수정 |
| 실행 프롬프트 | [agent-prompt.md](agent-prompt.md) |
| 활성 기술 명세 | 저장소에서는 `docs/architecture/implementation-plan.md` 한 사본만 사용 |
| 상태 원본 | 기존 `docs/architecture/migration-ledger.md` 한 곳 |
| 추적 방식 | 기존 P번호에 연결. 본문 3.x는 실행 순서이며 새 작업 ID 체계가 아님 |

> **실행 결정:** 조사·구현·리뷰·검증을 다른 에이전트에 맡기지 않는다. 한 에이전트가 한 변경 묶음씩 `대상 확인 → 계약 결정 → 구현 → 직접 호출자 연결 → 대응 구형 코드 제거 → 자체 검토 → 필요한 검사 → 체크포인트`를 수행한다.
>
> **구분:** 여기서 단일 에이전트는 **리팩터링을 수행하는 코딩 에이전트**를 뜻한다. Floe 제품의 Manager가 A2A로 Expert에 위임하는 아키텍처는 그대로 구현한다.
>
> **1차 완료는 구조 완료다.** 실제 앱·Keychain·OAuth·LLM 성공을 매 중간 단계의 조건으로 두지 않는다. 타입·의존성 검사와 필요한 최소 안전 검증은 유지한다.

**읽는 순서:** 처음에는 §1–2와 §3의 순서표, §5의 실행 규칙을 읽는다. 이후 현재 단계의 상세 변경과 연결된 소스만 읽는다. 재개 시 원장의 현재 체크포인트부터 시작하고 전체 과거 계획을 다시 읽지 않는다.

## 1. 이번에 고정하는 정책

### 1.1 스키마 번호는 올리지 않는다

| 영역 | 현재 확인한 값 / 상태 | 이번 처리 |
|---|---|---|
| 앱 command/query/events | `APP_WIRE_VERSION = 2` | **2로 동결**. 현재 DTO 한 벌을 직접 수정하며 3이나 다른 평행 계약을 만들지 않음 |
| 구형 앱 전송 계열 | `PROTOCOL_VERSION = 1`과 별도 decoder/entry | 앱용 중복 구현을 제거. 외부·native 계약에서 필요한 1은 별도 책임으로 유지 |
| Conversation 저장 | `SCHEMA_VERSION = 7` | **해당 marker를 유지하는 동안 7로 동결**. 현재 DDL·레코드·validator를 함께 수정하고 이전 schema용 ALTER/mapper를 만들지 않음 |
| 다른 로컬 저장 marker | 모듈별로 존재 | 살아남는 marker의 숫자는 현재 값 유지. 일괄 1/2/7로 통일하지 않음 |
| 외부 provider·서명·A2A | 독립된 실제 계약 | 앱 정리를 이유로 변경하지 않음 |
| `revision`, `executor_generation`, `runtime_epoch`, grant authority | 상태·권한·실행 일관성 값 | 정상적으로 갱신. **스키마 버전과 다르므로 동결하지 않음** |

근거는 직전 확인 snapshot의 [앱 상수][schema], [Conversation 저장 상수·검증][store-schema]다. 실제 checkout에서 이미 값이 달라졌다면 과거 숫자로 되돌리지 않는다. 차이를 원장에 적고 현재 채택된 값을 확인한 뒤 이번 작업에서 추가 증가나 평행 버전 구현을 하지 않는다.

이미 존재하는 숫자 2를 1로 되돌리거나, 버전 필드를 전부 삭제하는 작업은 이번 범위에 넣지 않는다. 그것은 필요 없는 serializer·fixture 변경을 추가한다. 제거해야 할 것은 **두 구현과 버전별 분기**다. `floe_protocol_version()`을 유지한다면 살아남는 앱 상수 하나를 보고하도록 Rust/Dart를 함께 정리한다. 이것은 과거 1을 지원·변환하는 로직이 아니다.

과거 계획의 **“계약 변경 시 schema version을 올린다”**, **“단일 번호 검사만으로 모든 구형 입력을 판별한다”**는 지시는 적용하지 않는다. 숫자를 동결하면 같은 번호의 구형 의미 변경을 번호만으로 구별할 수 없다. strict decoding과 현재 불변식 검사는 계속 필요하지만, 같은 모양·다른 의미의 과거 데이터까지 모두 판별해 준다고 주장하지 않는다.

### 1.2 번호를 올리지 않는 대신 지킬 운영 조건

1. **같이 배포한다.** Flutter·dylib·생성 binding을 동일 소스 snapshot에서 빌드한다. bundle에 새 dylib가 실제 복사됐는지 확인한다. 정상 제품은 bundle 내부 library만 사용하고 구형 symbol fallback을 두지 않는다. 개발용 library override는 동일 snapshot으로 재생성한 검증 경로에만 사용한다.
2. **현재 정의 한 벌만 둔다.** app 요청·응답 decoder, 저장 레코드, DDL, 검증 함수를 함께 수정한다. 구형 enum variant·nullable default·alias를 오직 과거 입력 수용을 위해 남기지 않는다. 정상 현재 의미에 필요한 optional 필드는 유지한다.
3. **저장 형식·의미가 바뀌면 테스트 profile을 분리한다.** 새 정의로 생성한 Floe 개발 데이터 디렉터리에서 검증한다. 기존 데이터를 재사용한 채 같은 숫자라는 이유만으로 새 의미를 부여하지 않는다. 현재 구조와 의미가 바뀌지 않은 코드 이동은 데이터 재생성이 필요 없다.
4. **자동 삭제하지 않는다.** 일반 앱 open/Keychain 오류를 DB reset으로 연결하지 않는다. 명시적인 로컬 개발 조작 전에 대상 경로·holder·Vault identity를 확인하고 외부 실행 결과가 불명확한 기록은 보존한다.
5. **버전 대체 프레임워크를 만들지 않는다.** 새 schema hash registry·호환 행렬·버전 협상·자동 migration engine은 추가하지 않는다. 빌드/검증 기록에 소스 snapshot과 산출물 identity를 남기는 정도로 제한한다.

이 방침의 전제는 현재의 로컬 테스트·동시 배포·데이터 폐기 가능 조건이다. 독립적으로 배포되는 클라이언트나 보존해야 하는 사용자 데이터가 생기면 별도로 재판단해야 한다. 이 문서는 그런 미래 지원을 미리 구현하라는 지시가 아니다. 현재 저장소의 [하위호환 불필요 정책][repo-policy]와 범위가 일치한다.

**중요:** `7`인 과거 DB를 새 코드가 항상 자동 거절한다고 보장하지 않는다. 형식·의미가 바뀐 checkpoint에서 **새 개발 데이터로 전환하는 절차가 필수**다. 이를 지키지 않고 번호 고정·데이터 계속 사용·완전한 자동 판별을 동시에 요구할 수는 없다.

### 1.3 구조 검증과 제품 동작 검증을 분리한다

**단계 A — 구조 리팩터링:** 공개 계약, 상태 수정자, 오류 범위, 모듈 의존성, 실제 adapter와 caller 연결을 완성한다. 구형 실행 경로·중복 상태·호환 wrapper를 제거한다. 이 단계의 산출물은 동작을 구현한 하나의 코드베이스이지 빈 디렉터리·trait·mock 모음이 아니다.

**단계 B — 동작 검증·안정화:** A의 구조 완료 조건을 통과한 단일 구현에서 일반 앱, Keychain, 모델, Connect, 장애 주입 및 기존 기능을 검증한다. 발견한 버그를 해당 owner 안에서 고치고 필요한 회귀만 추가한다. 구조가 준비되기 전에 live OAuth·실제 LLM·앱 수동 조작을 작업마다 반복하지 않는다.

다음 이전 지침은 이 문서에서 대체한다.

| 이전 실행 지침 | 이번 실행 지침 |
|---|---|
| 일반 앱 Vault 성공을 먼저 확보한 뒤 다음 통합 | 키/Vault의 진단 경계는 A에서 구현하되 실제 앱 접근 문제의 재현·수리는 B로 이동 |
| 각 기능 checkpoint마다 실제 앱·모델 시연 | A의 checkpoint는 실제 코드 연결·타입·의존성·구형 제거로 판정 |
| 분업·병렬 구현 후 별도 통합 | 하나의 에이전트가 수정·연결·삭제·자체 검토를 순서대로 수행하고 B에서 제품 동작 확인 |
| 마지막까지 구형 경로로 기능을 살려두기 | 잠깐의 미통합 상태는 허용하고 해당 구조 단위가 닫힐 때 구형 경로 제거 |
| 구조 완료와 제품 완료를 한 상태로 표시 | 기존 원장에 구조 완료와 동작 미검증을 분리해 기록 |

**A에서 유지하는 검사:** 변경 모듈의 타입/컴파일, 실제 의존성 DAG, public API와 단일 상태 수정자, 구형 참조 제거, concrete adapter 연결 여부. 동작 검증은 기본적으로 B에 모으되 아래 두 경우만 좁게 앞당긴다.

- **설계 성립을 좌우하는 미확인 가정:** native thread affinity, FFI 호출 방식, 실제 저장소의 transaction 지원처럼 잘못되면 여러 모듈의 설계를 바꿔야 하는 사항. 최소 컴파일 예제나 통제된 작은 실험으로 해당 가정만 확인한다. 전체 앱 시연을 요구하지 않는다.
- **이번에 의미를 변경하는 고위험 불변식:** 권한 없는 전송·키 교체·저장 원자성·중복 외부 write·취소의 역전파. 관련 기존 테스트가 있으면 재사용하고, 없으면 보호할 의미 하나에 집중한 검증만 추가한다. 외부 계정·실제 비밀 데이터로 검사하지 않는다. 함수 이동만 했다면 같은 테스트를 매 파일마다 재실행하지 않는다.

보안·회복력의 의미 구현을 B로 미루지 않는다. 예를 들어 A에서 취소와 receipt의 상태 전이를 실제로 구현하고, 전체 경로에서 그 전이가 맞는지는 B에서 검사한다. 실패를 숨기는 stub, 무조건 성공을 반환하는 저장 port, 정상 기능을 영구 Unsupported로 바꾸는 방법은 허용하지 않는다.

A 완료는 실제 제품의 무결함을 증명하지 않는다. 타입과 DAG가 맞아도 동시성·권한 변화·OS 동작에 문제가 있을 수 있다. B에서 설계 자체의 결함이 확인되면 작은 근거와 영향 범위를 기록하고 계약을 수정한다. 설계 우선은 설계 불변을 뜻하지 않는다.

## 2. 다시 하지 않을 것과 완료 상태

직전 확인 [원장][status]과 같은 기준 소스에서 이미 확인한 다음 부분은 재구현하지 않는다.

- 일반 root의 `ConversationService`·generic Engine 연결, Calendar-first root 제거.
- `TaskCoordinator`·Directory·Task generation, Run/Cancel command의 durable 기록과 retry lineage.
- `FloeClient`·대화 read model·event/ack 정합성·일반 turn의 새 command/query 경로.
- Execution 취소·예산, Access 검증, Context provenance/SourceView/history, Day·Knowledge의 유효한 정책과 회귀 보호.

여기에 남은 계약과 호출자를 **직접 고치고 최종 경계로 이동**한다. 별도 새로운 Engine·Task service·read model은 만들지 않는다. 아래의 “남음”은 기준 커밋의 평가다. 실제 checkout이 진행됐으면 관련 코드와 검증만 대조해서 이미 끝난 항목은 건너뛴다.

모듈 정책은 [기존 22개 target 경계][policy]를 유지한다. 새 모듈을 더 발명하거나 모든 helper를 다시 추상화하지 않는다.

```text
Flutter feature -> FloeClient -> 단일 FFI -> floe-app의 조립된 서비스
  Conversation: Session/Run   Experts: Directory/Task   Connections: Operation
  Inference: Profile/Attempt  Access/Context: 권한/허용 자료
  Day/Knowledge/Actions: 각 업무 상태
    -> owner-defined repository/I/O ports -> Vault/provider/native

snapshots/events -> 앱 수명 AppReadModel -> feature selectors
```

최종 업무 모듈은 구체 provider·Vault·builtin·FFI를 import하지 않는다. FFI의 내부 의존은 app/protocol로 닫는다. `floe-app`은 조립·수명주기만 맡고 user text 분기나 별도 domain 상태를 소유하지 않는다. 필요한 실제 I/O adapter는 유지하되 **구형 런타임을 호출하기 위한 compatibility adapter는 제거**한다.

## 3. 필요한 작업을 한 에이전트가 순차 수행한다

아래 순서대로 진행한다. 이미 완료된 항목은 실제 코드·원장 근거를 확인해 건너뛴다. 나중 단계에서 필요한 최소 계약은 3.1에서 먼저 정하고, 구현 완료를 서로 기다리는 순환을 만들지 않는다. 독립 작업이 보여도 다른 에이전트를 띄우거나 동시에 여러 변경 묶음을 열지 않는다.

| 순서 | 단계 | 작업 | 기존 P 연결 | 다음으로 넘어갈 기준 |
|---|---|---|---|---|
| 1 | A · 3.1 | 실제 시작점·공유 계약·소유권·스키마 동결 | P00/P01/P16/P23 | 필요한 공개 계약과 직접 소비자 범위 확정 |
| 2 | A · 3.2 | Connections·Day·Knowledge·Actions의 남은 owner/port | P05/P06/P08/P10/P20 | 실제 정책·상태 전이와 공개 port 구현 |
| 3 | A · 3.3 | command identity·admission·비동기 Inference | P07/P12/P13/P14 | 불변 의도와 실행 환경 분리, 실제 수락/runner 구현 |
| 4 | A · 3.4 | Engine·Task·Run의 실패·journal·usage 계약 | P02/P03/P04/P09/P11/P12 | owner 간 같은 의미, 제한된 안전 검사 |
| 5 | A · 3.5 | 실제 Vault/provider/native adapter 이관 | P13/P14/P21 | 새 port의 구체 구현과 저장·키 진단 경계 완성 |
| 6 | A · 3.6 | builtin Expert 독립 endpoint | P11/P15 | 공통 Directory 등록과 좁은 port 사용 |
| 7 | A · 3.7 | AppHost·단일 ABI·Session·Flutter 조립 | P13/P14/P16/P17/P18/P19/P21 | 최종 caller와 실제 구현의 전체 연결 |
| 8 | A · 3.8 | 구형 제거·전체 구조 심사 | P22/P23/P24/P25의 구조 범위 | 구조 완료 조건 확인, 제품 검증 상태는 별도 |
| 9 | B · 3.9 | 회귀·일반 앱·모델·Connect 검증과 수정 | P13/P14/P21/P22/P23/P25의 동작 범위 | 실제 관측과 미검증 범위 기록 |

**단계 순서와 파일 수정 순서는 다르다.** 현재 계약을 바꾸면서 직접 소비자를 함께 수정해야 한다면 해당 파일도 같은 변경 묶음에 포함한다. 그것을 다른 단계 전체를 병행하는 근거로 삼지 않는다. 기능·owner를 새로 설계하는 범위가 늘어나면 작은 결정과 이유를 원장에 남기고 필요한 선행 부분만 먼저 끝낸다.

각 단계는 한 번의 거대한 작업이 아니다. 독립적으로 설명 가능한 책임 단위로 작게 닫는다. 생성한 owner가 컴파일된 것과 최종 제품에 연결된 것은 별도 상태다. 3.5/3.7에 남은 구체 구현·조립은 그 단계에서 명시적으로 끝내며, **A 전체 종료 시에는 빈 port·mock-only 연결·구형 bridge가 없어야 한다.**

짧은 compile break는 같은 계약 변경 묶음 안에서 복구한다. 단계 중 target 모듈 검사와 최종 workspace 검사를 구별하고, 아직 수정하지 않은 소비자의 오류는 정확한 파일·다음 변경과 함께 기록한다. 이를 고치려고 compatibility shim을 만들거나, 실패 target을 제외해 전체 green으로 보고하지 않는다. 원장이 `수정 중`인 변경을 방치한 채 다른 주제를 시작하지 않는다.

### 3.1 시작점과 canonical 계약 고정

**수정 시작점:** [DTO 상수][schema], [ABI][abi], [workspace][workspace], `tools/architecture/check_boundaries.py`, 현행 계획·원장.

**할 일**

1. 기준 commit과 working tree를 기록한다. 이 문서를 활성 계획에 반영하고 이전 실행 프롬프트의 분업·병렬 실행/version-bump/legacy 병행/실사용 우선 지침과 충돌하지 않게 한다. 저장소 `AGENTS.md`의 개발·검증 순서도 이번 구조 우선 결정에 맞춰 해당 workflow 단락만 정리한다. 보안·데이터 보호 지침은 유지한다. 사용자 변경을 reset하지 않는다.
2. `APP_WIRE_VERSION=2`, Conversation marker `7` 등 **현재 사용되는 상수 값과 쓰임을 먼저 확인**한다. 이번 교체 diff에 새 숫자 증가가 없도록 한다. 외부 계약과 상태 revision은 별도다.
3. 이 에이전트가 command의 canonical 입력, owner별 repository transaction, 모델 dispatch/journal, 단일 앱 envelope와 필요한 Session/Operation API를 정한다. 같은 의미의 타입을 추가로 정의하지 말고 기존 공개 타입을 수정한다. 필요한 계약만 고정하고 전체 저장소를 다시 설계하지 않는다.
4. 앱의 최종 이름을 `floe_core_command/query/events`, Dart `command/query/events`로 고정한다. 실제 rename은 호출자·binding·ABI를 같은 checkpoint에서 변경한다. `_v2` alias는 남기지 않는다.
5. 현재 바이너리의 export 및 호출자 목록에서 제거할 구형 entry를 확인한다. Day·CalendarAction·LocalContext 등 **기능은 canonical command/query로 옮기고**, 해당 독립 app entry와 dispatcher를 제거한다. OS 전용 callback ABI까지 무작정 삭제하지 않는다.

6. 기존 원장의 현재 표에 실제 코드 위치를 이용한 `기능 → 공개 owner API → adapter → 호출자 → 제거 대상` 대응을 간단히 적는다. 신규 task board나 파일별 배정표는 만들지 않는다. 이후 단계는 이 위치를 갱신하며 진행한다.

**A 구조 완료:** schema-bump가 없고 현재 Rust/Dart 계약 정의·필수 필드·호출자 변경 범위가 명확하다. 기존 golden의 입력 정의도 새 계약에 맞춰 정리하되 전체 roundtrip 동작 검증은 B에서 실행한다. 새 버전 협상·새 STATUS 파일을 만들지 않는다.

### 3.2 Connections·기존 업무 owner와 port 정리

**시작점:** [Connections 공개 API][connections], [Widget catalog 동기화][connector-screen], [OAuth polling][connector-panel], legacy Core의 Action/Vault 업무와 Go `internal/console` 잔여 상태.

**할 일**

1. Connections에 남은 `Query/Refresh/BeginPairing/BeginOAuth/ConfirmGrant/CancelOperation/Disconnect`를 기존 port 위에서 완성한다. receipt와 operation generation을 저장하고 화면이 아니라 앱 수명의 서비스가 소유한다.
2. query/preview는 연결 변경·승인·chat cancel을 하지 않는다. catalog refresh 실패는 stale/unknown+issue이고 disconnect가 아니다. mutation 성공 뒤 projection refresh 실패는 별도 issue다. 늦은 응답은 expected generation/authority로 거절한다.
3. OAuth의 타이머·대기·예외 종료를 Connections operation으로 옮긴다. 화면 dispose는 구독 해제이고 명시 Cancel은 operation 취소다. 원격 ack 유실은 기존 remote operation 조회로 처리하며, 해당 API가 없으면 `Indeterminate`로 남긴다.
4. Day·Memory/Playbook/Learner·Action은 이미 분리된 정책을 다시 쓰지 않고 **남은 호출자와 repository만** owner 경계로 완성한다. Actions는 승인된 payload와 dispatch intent/불확실성 ledger를 유지한다. 최소한 기존에 지원하던 기능을 조용히 끄지 않는다.
5. Go는 실제로 Console에 남은 업무 상태·판단만 해당 서비스로 옮긴다. 이미 고친 optional bootstrap·exact-recipient fence는 재작성하지 않는다. 외부 HTTP 계약을 바꿀 필요가 없으면 변경하지 않는다.

**삭제:** Widget 안의 reconciliation·OAuth owner, 다른 feature 상태 setter 호출, 업무 상태를 갖는 Console/전역 gateway의 해당 책임. 제품 경로에서 쓰지 않는 단순 이름·미관 정리는 뒤로 미룬다.

**B에서 확인할 행위:** 대화 중 Connect query/preview가 끝나고 chat cancel은 0회다. OAuth 화면 재진입은 같은 operation을 관측한다. 연결 성공 뒤 화면 갱신 실패를 연결 실패로 표시하거나 중복 연결하지 않는다.

**이 단계 경계:** owner의 실제 정책·상태 전이·저장 port를 구현한다. 구체 저장/외부 I/O adapter의 최종 배치는 3.5, 전체 제품 조립은 3.7이다. 아직 조립하지 않은 경로를 `wired`로 표시하지 않는다.

**A 구조 완료:** Operation/Day/Knowledge/Action의 실제 변경 경로가 각 owner에 있고 Widget·전역 gateway에서 옮길 상태와 호출자 전환 지점이 확정되어 있다. 지금 수정 가능한 직접 호출자는 함께 전환하며, 전체 Flutter 전환은 3.7에서 끝낸다. 현재 지원 기능과 새 command·service·repository의 대응을 모두 기록한다. 실제 OAuth 로그인이나 외부 계정 mutation은 A 완료 조건이 아니다.

### 3.3 Conversation 수락·멱등성과 비동기 Inference 분리

**근거:** 현재 [LegacyComposition::start_turn][app-compose]은 기존 receipt 조회 뒤에도 [HostInferenceRoutes::resolve][host-route]를 호출한다. resolver는 `block_on`으로 model inventory와 catalog를 기다린다. [일반 root][root]의 request 전체 직렬화와 [turn_digest][coordinator], [저장 exact_admission][store-schema]에는 가변 실행 설정의 영향이 남아 있다.

**목표 위치:** `modules/conversation/{api,application,ports}`, `modules/inference`, `adapters/vault/conversation`, `adapters/providers/{models,control}`, `modules/connections`의 관측.

**할 일**

1. canonical command digest는 `kind + session + expected_revision + text + mode/continuation + retry_of + 사용자가 명시한 profile 선택` 등 **사용자의 불변 의도**에서 계산한다. principal namespace를 bind한다. `request_id`, bearer/token, catalog, 자동 선택 route, 가용성, wall deadline, 임시 context는 포함하지 않는다. `Debug` 문자열 대신 명시적인 canonical 인코딩을 사용한다.
2. `request_context_digest`에 전체 `AgentConversationTurnRequestDto`를 넣는 경로와 `exact_admission`의 자동 선택 `model_placement` 비교를 제거한다. 명시 profile 선택의 동일성은 보존한다. 실행 route/placement는 admission 입력이 아니라 별도 Run/Attempt 관측으로 기록한다. Continue에는 저장된 continuation 참조를 사용한다.
3. 수락 순서를 **현재 caller/Vault 접근 검사 → 같은 command 조회·입력 동일성 검증 → 신규 요청만 Session revision/claim 검사 → user entry+Run+receipt+claim 원자 저장**으로 만든다. 같은 command는 모델·catalog·credential 조회 없이 원래 receipt를 반환한다. 원래 답변 내용의 공개 권한 검사는 receipt dedup과 별개로 유지한다.
4. 수락된 Run의 비동기 실행에서 모델을 계획한다. 이 시점의 consent·실제 recipient·credential·권한 재검증에 실패하면 그 Run을 적절한 `blocked/failed` 결과로 마무리한다. 수락이 곧 외부 전송 허가는 아니다.
5. `resolve_remote_model_route`의 catalog 읽기는 Connections의 별도 refresh로 이동한다. 기본 채팅은 해당 endpoint를 아예 호출하지 않는다. Source는 실제 사용 시 lazy 획득하며, malformed/denied binding도 해당 사용 범위에서 거절한다.
6. FFI는 네트워크 응답 전체를 기다리지 않는다. **bounded 로컬 admission/조회**만 처리하고, 짧은 저장 작업 외에 Vault writer를 점유하지 않는다. admission wait timeout은 같은 command 재조회로 복구한다. Run은 caller timeout 때문에 취소하지 않는다.

**삭제:** FFI의 동기 route resolver, route를 가진 app command, catalog를 model route에 붙이는 조립, 단순히 3초 timeout을 늘려 문제를 가리는 수정.

**B에서 확인할 행위:** catalog를 계속 대기시켜도 StartTurn 수락·GetRun·CancelRun이 처리된다. 같은 command를 catalog 변화/토큰 교체/서버 장애 이후 재전송해도 같은 receipt이며 **추가 모델 호출 0회**다. 다른 payload는 명확한 conflict다.

**A 구조 완료:** canonical digest와 execution snapshot을 분리한 실제 admission/runner 코드를 구현하고 repository 동일성 비교 계약도 맞춘다. 이미 전환 가능한 호출자는 직접 수정한다. 구체 DB 경계는 3.5, FFI의 최종 호출자 교체는 3.7에서 닫는다. 해당 경로가 연결되기 전에는 전역적으로 동기 네트워크 대기가 제거됐다고 보고하지 않는다. 변경한 dedup/외부 dispatch 보호는 최소 범위만 검사한다.

### 3.4 Engine·Task·Run의 실패·복구·정산 의미 통일

**수정 시작점:** [Engine::drive 1–245][engine], [ExecutionJournal/ModelPort][ports], [TaskCoordinator][task], [Conversation coordinator][coordinator], [finalization][finalization], [저장 Run/terminal validation][store-schema].

**할 일**

1. 현재 durable journal·Task/Run generation을 재사용한다. 남은 iteration-only checkpoint에는 실행 전 검증된 **model batch, batch/step identity, next step, 결과 참조**를 기록한다. 저장된 intent와 호출 ID를 복구 기준으로 삼고 모델을 다시 호출해 나온 다른 batch를 이전 실행으로 간주하지 않는다.
2. 자식 denied/timeout/cancel과 root 취소·Vault 무결성·journal 실패를 구별한다. 실패 Task의 상태는 그대로 보존하고 Manager에는 민감 payload 없는 typed observation을 전달한다. `Unknown` source 결과를 편의상 `Independent`로 승격하지 않는다.
3. root가 승인된 모델·안전한 자료·남은 예산을 가진 경우 부분 실패 뒤 답변을 마무리한다. work child deadline은 root hard deadline보다 앞서 끝나도록 분리한다. 최대 1회·출력 1,024 token·root 한도 내 10초의 기존 finalization 정책을 재사용한다. 사용자 취소·hard deadline·Vault 불가·수신자 동의 부재에는 추가 모델 호출하지 않는다.
4. 저장 validator와 wire/read model도 **업무 거절 + 답변 전달**을 표현하게 수정한다. 현재 budget/stalled에만 답변 있는 Failed를 허용하는 제한을 필요한 scoped 실패에 맞춰 교정한다. 실패 작업을 `Completed`로 위장하지 않고 execution/reply 결과를 나눈다.
5. model usage의 reserve→dispatch intent→handoff→settle을 Inference 한 owner로 모은다. Engine과 Legacy adapter가 같은 요청을 두 번 계상하지 않는다. queue에서 취소된 요청과 보냈으나 결과를 모르는 요청을 구별한다.
6. Task 취소 요청, 실제 자식 종료, durable terminal을 구별한다. 부모의 명시 취소는 자식으로 전파하고 자식 취소는 부모·형제를 취소하지 않는다. 늦은 결과는 generation/authority fence에서 거절한다. journal 저장 실패는 `recovery required`로 보존한다.

**삭제:** 같은 호출의 중복 budget bookkeeping, 모든 child 오류에 대한 blanket `?` 전파, 저장하지 않은 terminal 성공 보고, 별도 legacy journal 변환.

**B에서 확인할 행위:** child timeout 후 root가 안전한 설명을 내고 실패 상태는 유지한다. 사용자 cancel 후 추가 dispatch는 0회다. Task 완료→부모 저장 ack 유실 후 같은 Task를 조회하며 재실행하지 않는다. model batch의 저장 ack 전 도구 side effect는 0회다.

**A 구조 완료:** 실제 Engine·Task·Conversation·Inference와 저장 port의 입력·결과 모델에 동일한 실패/종료 의미가 구현되고, finalization과 usage의 결정권자가 하나다. error code만 추가하거나 handler를 TODO로 남기지 않는다. 변경한 journal ack·권한·취소 방향의 고위험 조건만 좁게 확인한다. 구체 저장 transaction은 다음 3.5에서 이 계약을 구현한다.

### 3.5 실제 Vault·provider·native adapter를 최종 경계로 이동

**수정 시작점:** [기존 Vault][vault], [Conversation repository][repository], [Task repository][task-repository], [keyring.rs 1–100][keys], [HostInferenceRoutes][host-route], [Worker][worker].

**목표 위치:** `crates/adapters/vault`, `crates/adapters/providers`, `crates/platform/native`. 기존 구현을 이동·수정하며 새 평행 구현을 만들지 않는다.

**할 일**

1. 실제 암호화 엔진·private file·key-health와 owner별 저장 transaction을 `adapters/vault`로 옮긴다. `VaultConversationRunRecord`와 domain Run의 중복 상태 의미는 canonical owner 모델·명시 persistence record로 정리한다. 저장 표현의 변환은 허용하지만 구형 런타임 모델을 계속 원본으로 쓰지 않는다.
2. `admit_turn`, `finish_run`, Task admission/settlement, Cancel 명령 receipt, Connection operation, Action intent/불확실성, Knowledge review 등의 실제 저장 작업을 이미 고정한 owner port에 연결한다. 다중 저장을 단순 `save` 호출 나열로 구현해 원자적이라고 부르지 않는다. DB/provider를 넘는 원자성은 없는 것으로 취급하고 receipt·재조회·reconciliation을 보존한다.
3. Context의 계산과 정책을 Vault에 통째로 복사하지 않는다. 승인 DAG에 따라 owner port로 coverage를 저장하고 projector/reader는 해당 모듈에 둔다. `floe-vault`가 `floe-context`에 직접 의존하도록 정책을 넓히지 않는다. 네트워크 대기 중 DB transaction·전역 writer를 점유하지 않는다.
4. 실제 model/source/control 변환을 `adapters/providers`, 키·OS 접근을 `platform/native`에 배치한다. canonical ModelPort 등 정상 port를 직접 구현한다. `LegacyModelPort/LegacyToolPort`를 새 wrapper로 감싸 보존하지 않는다. 하나의 원격 credential을 사용하더라도 모델 route와 source catalog의 의존은 다시 합치지 않는다.
5. thread-affine native 자원은 실제 소유 스레드와 종료 규칙을 가진 경계로 제공한다. `unsafe impl Send`로 제약을 숨기지 않는다. 키/Vault의 `key_entry/key_lookup/key_insert/key_read_back/key_length/vault_marker/db_open/identity/schema/host_lock` 최초 단계·안전한 OS status·incident ID가 상위 오류 매핑에서 유실되지 않게 한다. 실제 키나 사용자 경로 원문은 로그에 넣지 않는다.
6. 현재 DDL·record·validator를 함께 수정하되 스키마 번호를 증가시키지 않는다. 저장 형식·의미가 바뀐 사실과 B에서 필요한 새 개발 profile 조건을 기존 원장에 남긴다. A에서 schema 검증을 빌미로 일반 앱 데이터를 초기화하지 않는다.
7. adapter 정의와 현재 target 모듈의 port가 맞는지 타입·의존성을 검사하고 원본 구현의 잔여 소비자를 확인한다. 최종 app 조립에서 필요한 생성 인자는 3.7에서 직접 연결한다. 실제 조립 전에는 제품 연결 완료로 표시하지 않는다.

**삭제:** FFI 내부 repository의 원본, 구형 Core/Infra의 이동 완료 구현, legacy 런타임을 호출하는 변환 계층. 다른 직접 소비자가 남았으면 그 소비자를 같은 교체 범위에서 수정하고, 호환 wrapper를 만들지 않는다.

**A 구조 완료:** 실제 I/O 구현이 owner-defined port를 구현하고 올바른 디렉터리·crate에 존재한다. 키/암호화/권한/저장 보호를 생략한 stub이 없고, native lifetime 가정이 명시되어 있다. 일반 앱 Keychain 성공은 A 조건이 아니다.

**B에서 확인할 행위:** 실제 앱 create/unlock/reopen, 변경한 DB의 재시작·receipt 복원, 외부 결과 불명 처리, 실제 provider 전송과 취소. ad-hoc 서명·Keychain 문제의 현장 재현은 3.9에서 한다.

### 3.6 builtin Expert를 독립 등록 endpoint로 완성

**현재 유지:** Calendar-first root는 이미 제거됐다. 이를 다시 제거하는 작업은 없다. 남은 것은 [LegacyExpertEndpoint / ConversationExperts][dispatch]의 중앙 builtin 해석·부모 모델 기반 카드 필터·Schedule 특수 전달이다.

**목표 위치:** `crates/experts/builtin/src/{schedule,commitments,communication,relationships,focus_attention,wellbeing,work_context,life_logistics}/`.

**선행 확인:** 3.2–3.5에서 확정·구현한 Actions/Context/Inference port와 Task 의미를 사용한다. 구현 에이전트가 하나라는 이유로 제품의 Manager→Expert 위임을 제거하지 않는다.

**할 일**

1. 각 폴더로 descriptor·role·endpoint·필요 tools·artifact 변환을 이동한다. 각 endpoint는 기존 generic Engine을 자신의 role로 사용한다. Task 상태는 기존 Experts owner만 변경한다.
2. composition은 검토된 endpoint를 정적으로 등록하고, dispatch는 AgentId lookup만 한다. 의미 기반 위임 대상·자연어 목표는 Manager가 정한다. 공용 runtime/FFI에서 enum·텍스트·provider로 다시 선택하지 않는다.
3. `supports_device_model`에 의해 **부모 모델 종류만으로** 카드가 제거되는 경로를 없애고 invocation별 Inference 계획을 사용한다. 필요한 capability/권한을 host가 검증하는 것은 유지한다.
4. Schedule에 raw Vault/전체 FloeCore/전체 app을 넘기는 staging bridge를 제거한다. 허용된 Source/Actions/Model port와 child scope만 주입한다. 원래 Calendar source grant·provenance·action 승인 검증은 각 경계에 직접 이식한다.
5. local-only Expert 결과를 remote Manager에게 전달할 때도 별도 release를 확인한다. 요약했다는 이유로 전송 제한을 해제하지 않는다. history 보호는 Calendar ID 접두어가 아니라 실제 dependency로 적용한다.

**삭제:** `LegacyExpertEndpoint`, `LegacyDelegationPort`, generic builtin 중앙 match, Schedule 전용 root/특수 전달 경유, endpoint의 별도 authoritative Task map.

**B에서 확인할 행위:** Calendar 활성 상태에서 Manager가 Communication을 고르면 그대로 전달된다. Schedule도 같은 경로로 동작한다. 새 테스트 Expert 등록에 공용 Engine/Conversation/FFI 수정이 필요하지 않다. 실제 LLM 선택 품질과 scripted dispatch 검증은 별도로 기록한다.

**A 구조 완료:** 각 endpoint에 descriptor·role·실행·필요 port가 실제로 배치되고 공통 Directory로 연결된다. 공용 코드에서 구체 Expert 이름으로 dispatch하는 분기가 없다. 확장성은 테스트 endpoint를 동일 계약으로 조립·컴파일하는 정도로 우선 확인하고, live LLM 선택 평가는 B로 남긴다.

### 3.7 AppHost·단일 ABI·Session·Flutter caller를 연결

**수정 시작점:** [FFI LegacyComposition][app-compose], [VaultBridge/Worker][worker], [ABI][abi], [legacy Session gateway][gateway], [NativeTransport][transport], [client][client], [controller][controller], [FfiDayGateway 조립][day-gateway].

**선행:** 3.2–3.6의 실제 서비스·repository·adapter·endpoint를 사용한다. 구현이 빠졌다면 그 필요한 부분만 같은 에이전트가 완료한 후 이어간다. placeholder로 조립을 통과시키지 않는다.

**순서대로 할 일**

1. `floe-app`이 실제 owner 서비스와 adapter를 직접 조립하게 한다. `AppHost<LegacyComposition>`, `AppHost::legacy`, FFI 내부 서비스 조회 우회를 삭제한다. host open은 trust·identity·자원을 준비하고 model/source health 조회는 수락 이후 해당 owner가 비동기로 수행한다.
2. **Session 생성·재개·조회·복구·보관/compaction과 Turn/Cancel/Retry/Continue**를 하나의 command/query 집합으로 연결한다. SessionId로 active Run을 찾을 수 있어야 한다. 재진입이 과거 인메모리 CommandId 목록에만 의존하지 않게 한다. 구형 Session snapshot bridge를 유지하지 않는다.
3. Connections/Access/Day/Knowledge/Actions/OS interaction까지 필요한 DTO를 같은 현재 app envelope로 전달한다. host interaction은 operation ID·epoch·expiry와 묶는다. 스키마 번호는 추가로 올리지 않는다. DTO·serializer·handler·Dart decoder가 같은 의미를 사용하게 직접 수정한다.
4. ABI symbol과 Dart binding을 함께 `floe_core_command/query/events`, `command/query/events`로 교체한다. 구형 `agent_vault`, 업무별 별도 앱 exported entry, `_v2` alias/fallback, 이중 `invoke_json` 구현은 삭제한다. 실제 OS driver callback은 별도 경계이며 필요한 것까지 지우지 않는다. String/handle 단일 해제·panic 경계·크기 제한을 보존한다.
5. `FloeClient`와 읽기 모델 생성 위치를 Day feature 밖 앱 수명 bootstrap으로 이동한다. feature ViewModel은 snapshot/selector·draft 같은 UI 상태만 소유한다. `_conversationBusy`의 registry/memory/calendar OR을 없애고 해당 Session claim·Vault gate·pending admission에 필요한 조건만 사용한다. 권한 철회 영향과 조회 busy는 구별한다.
6. Widget·전역 gateway에 남은 OAuth/reconciliation/업무 mutation을 3.2의 공개 API로 연결한다. 다른 feature의 private 상태 setter를 호출하지 않는다. 각 기능의 실제 UI command→owner→adapter→projection 경로를 확인한다.
7. NativeTransport의 open/request/close에서 success/error/worker-exit/timeout마다 대기자를 정확히 한 번 완료한다. 동기 FFI 실행 중 timeout을 이유로 isolate를 무조건 kill/free하지 않는다. 종료는 신규 admission 중지→소유 작업 cancel·bounded join→안전한 handle free의 순서다. OS가 제공하지 않는 강제 취소를 구현했다고 주장하지 않는다.
8. snapshot/event gap은 durable Session/Run 조회로 복원한다. runtime/vault epoch와 현재 공개 권한을 검사해 lock/revoke 후 민감한 이전 결과가 UI에 재적용되지 않게 한다. 마지막 event가 유실돼도 terminal을 조회할 수 있고, 화면 dispose는 Run cancel이 아니다.
9. Cargo 경로, Apple build script, ABI 헤더와 generated binding, dylib bundle 경로를 같은 변경 묶음에서 맞춘다. 선언만 맞고 실제 구현이 없는 export는 허용하지 않는다. 지원 SDK가 없는 플랫폼은 미확인 상태로 구분한다.

**삭제:** `LegacyComposition`·legacy accessor, 구형 Session/AgentVault 제품 경로, 이중 decoder와 버전별 symbol 선택, fixture-only 제품 API, 전역 mutable gateway의 이관 완료 책임.

**A 구조 완료:** 모든 현재 기능이 같은 AppHost·typed API·실제 adapter를 사용한다. mock-only 조립, 영구 Unsupported, TODO branch, 구형 런타임 경유가 없다. 가능한 target의 Rust·Dart 타입/컴파일과 native binding·빌드 선언이 일치한다. 실제 앱을 실행해 성공했는지는 별도로 남긴다.

**B에서 확인할 행위:** Vault→Session→모델→저장→다음 turn→재진입, 대화 중 Connect, Release 없는 다음 Run, worker exit/close의 대기자 정리, epoch 변경 후 resync와 민감 출력 차단.

### 3.8 구형 경로 제거와 단일 에이전트 구조 심사

이 단계는 삭제를 마지막까지 유예하는 단계가 아니다. 3.1–3.7의 각 책임 이관 때 관련 구형 구현을 함께 제거하고 여기서는 잔존·누락을 심사한다. 별도 리뷰 에이전트를 실행하지 않고, 같은 에이전트가 구현 모드에서 검토 모드로 전환해 실제 diff·호출 경로를 다시 확인한다.

1. old caller·old state owner·compatibility decoder·fixture-only product entry를 제거한다. 사라진 API 모양만 검증하는 테스트는 삭제할 수 있지만 CAS·privacy·replay·authority를 보호한 테스트는 새 owner/계약으로 옮긴다. 기존 보장을 깨뜨리는 테스트 무력화나 무조건 ignore는 하지 않는다.
2. `floe-agent/core/domain/infra`의 유효 구현을 target로 옮긴 뒤 package와 참조를 제거한다. `floe-protocol/ffi`는 최종 `bindings/` 경로로 이동한다. 같은 package를 old/new 두 곳에 등록하지 않는다. 허용 DAG를 넓혀 legacy 의존을 합법화하지 않는다.
3. 실제 Cargo 의존성, Dart feature private import, Go application→Console 역의존, Apple binding/library build 경로를 확인한다. 이름만 바꾼 거대 facade, 범용 service locator, 중복 authoritative map을 남기면 미완료다.
4. 모든 기존 제품 기능에 `UI command → 공개 owner API → 실제 adapter → 저장/응답 projection`의 대응 위치를 남긴다. 실제 호출 지점을 검사하며 선언·grep 0건만으로 완료하지 않는다. 테스트 대역만 연결하거나 기존 기능을 숨겨 누락을 가리지 않는다.
5. 각 mutable 사실의 수정자, 각 외부 effect의 승인·intent 기록·결과 불명 처리, 각 Run/Task의 종료 책임을 확인한다. UI 조회 timeout과 root cancel의 호출 경로가 명시적으로 구별되어야 한다.
6. 현재 구조 설명·공개 API·실행 방식·원장 current 표를 일치시킨다. 활성 계획과 원장은 각각 하나만 유지한다. 과거 증거는 history로 보존한다.

**구조 완료 통과 조건**

| 조건 | 필요한 근거 |
|---|---|
| 승인된 최종 모듈/DAG | 실제 manifest와 경계 검사 결과; 목표 policy만 확인하는 것으로 대체하지 않음 |
| 기능별 단일 수정자 | Run/Task/Operation/권한/업무 상태의 실제 mutation 경로 |
| 단일 앱 API·실행 경로 | 현재 ABI·binding·caller, legacy alias/선택기·bridge 없음 |
| 구체 구현 연결 | 현재 기능의 production composition과 adapter 구현; 빈 shell 없음 |
| 코드 정합성 | 가능한 현재 target의 타입/컴파일·정적 분석 통과 |
| 고위험 의미 보존 | 수정한 안전 불변식의 제한된 테스트/검토 근거 |
| 검증 상태 정직성 | 동작 검증 항목은 `not_run` 또는 실제 상태로 남김 |

A 완료 결과는 **“구조 리팩터링 완료 / 제품 동작 검증 대기”**다. 이 상태는 B를 시작할 수 있는 기준이지 배포·제품 안정성 선언이 아니다. 사용할 수 없는 SDK 때문에 확인 못한 platform 구조는 `환경 미검증`으로 남기고 전체 target compile 성공이라고 표시하지 않는다.

### 3.9 단계 B — 구조 완료 후 동작 검증·안정화

3.8을 통과한 단일 구현에서 수행한다. 과거 helper나 구형 앱을 성공시키는 작업이 아니다. 검증 중에는 실패를 네 가지로 분류한다: 구현 버그 / 계약·설계 결함 / 빌드·OS·환경 문제 / 이미 삭제한 구형 API를 전제로 한 테스트. 설계 결함은 원인을 기록하고 해당 계약을 수정하며, 당장 green을 만들기 위해 구형 runtime·권한 우회·전역 catch를 복원하지 않는다.

#### 3.9.1 통제된 회귀·통합

현재 owner·repository·FFI·Dart를 실제로 조립한 경로에서 아래 4절의 검증을 수행한다. 같은 보장을 주는 기존 검증은 재사용한다. 특히 replay·권한 철회·child timeout·terminal 저장 실패·observer timeout을 확인한다. mock이 호출자를 우회하는 테스트만으로 통과 처리하지 않는다. 외부 provider 호출은 제어된 대역으로 시작하고 실제 계정 변경은 별도 허가 없이는 하지 않는다.

#### 3.9.2 일반 앱의 Vault 진입 문제

**근거:** 기준 snapshot 원장의 `P16/P17 host-owned inference route checkpoint`는 실제 앱이 route 선택 전에 `VaultUnavailable`로 끝났다고 기록한다. 원인까지 확정된 것은 아니다. [원장][status]

**수정 시작점:** 기준의 [keyring.rs 1–100][keys] `entry/read_key/insert_key`, [agent_vault.rs][vault] create/open·identity·key-health, [AgentController.load][controller]가 **A에서 이동한 최종 위치**를 원장에서 찾는다. 과거 경로를 다시 만들지 않는다.

**할 일**

1. `key_entry`, `key_lookup`, `key_insert`, `key_read_back`, `key_length`, `vault_marker`, `db_open`, `identity`, `schema`, `host_lock` 중 **최초 실패 단계**와 안전한 OS status/incident ID를 보존한다. 상위 경계에서 `VaultUnavailable`로만 덮어 원인을 잃지 않는다.
2. 정상 앱 bundle의 signing/entitlement·사용 중 library·데이터 경로와 성공했던 helper의 차이를 확인한다. 키 없음, 접근 거절, schema 불일치를 구별한다. sandbox 때문이라고 미리 단정하지 않는다.
3. 해당 원인만 수정한다. 기존 DB에 새 키를 덮거나 plaintext fallback, 권한 검사 해제로 해결하지 않는다.
4. schema 변경 검증은 명시적으로 새 개발 데이터에서 수행한다. 새 schema가 아닌 원래 Keychain 접근 문제까지 데이터 reset으로 해결됐다고 보고하지 않는다.
5. 실제 앱에서 create/unlock→조회→종료→재실행을 확인한다. 단계 진단은 native/Vault adapter로 이동하고 임시 UI 디버깅 코드는 남기지 않는다.

**삭제:** blanket error 재분류 중 원인을 유실하는 부분, 이전 앱/helper에만 맞춘 진입 우회.

**완료:** 동일한 일반 앱 경로가 정상 키·암호화 DB로 진입한다. 환경 접근이 없으면 `blocked`로 남기고 독립 구현만 계속한다. 테스트용 key provider 성공을 실제 앱 성공으로 대체하지 않는다.

#### 3.9.3 실제 제품 흐름

같은 snapshot의 앱과 dylib를 빌드해 실제 bundle을 실행한다. Vault 진입 → Session 생성 → 모델 답변 → 두 번째 입력 → 응답 중 Connect 조회/preview → 명시적 취소 → 화면 재진입/재시작 후 같은 Run 관측을 확인한다. Day/Memory 검토·기존 승인 Action·OAuth operation도 현재 canonical API에서 확인한다. 외부 write는 통제된 환경·명시적인 승인 범위에서만 수행한다.

#### 3.9.4 실제 LLM 평가와 종료

scripted Manager가 선택한 AgentId를 보존하는 구조 검증과, 실제 LLM이 적절한 Expert를 선택하는 품질 평가는 별개다. Calendar 활성 상태에서 다른 Expert 선택, 필요 없는 경우 직접 답변, Expert 실패 후 제한 설명을 실제 모델로 평가한다. 최초 성공 한 번을 전체 안정성으로 확대하지 않는다. 각 결과의 source snapshot·환경·대역·미검증 범위를 기존 원장에 남긴다. 이 단계도 같은 에이전트가 순차적으로 수행하며 별도 검증 에이전트를 만들지 않는다.

B 완료는 현재 단일 구현에서 필수 사용자 경로와 변경한 안전 불변식이 검증되고, 남은 환경 제한이 명확한 상태다. 품질 탐색을 무한히 이어가기보다 이번 리팩터링의 수용 범위까지만 판정한다.

## 4. 검증 정책과 실행 비용

### 4.1 A에서는 구조를 검사한다

| 검사 | 수행 시점 | 하지 않는 것 |
|---|---|---|
| 변경 모듈 타입·compile·format | 의미 있는 구조 묶음을 닫을 때 | 저장할 때마다 workspace 전체 재빌드 |
| DAG·private import·공개 경계 | 모듈 이관/의존성 변경 시 | 빈 폴더 개수로 구조 완료 판단 |
| 실제 caller·adapter 연결·구형 제거 | 해당 책임 이관 checkpoint | 앱을 살리기 위한 새 compatibility wrapper |
| 좁은 안전 검증 | 변경한 고위험 의미에 한정 | 모든 테스트 새로 작성·모든 API마다 exhaustive fixture |
| 설계 가정 확인 | 오류면 구조를 바꿔야 할 불확실성이 있을 때만 | live UI/LLM 반복을 feasibility 검사라고 부르기 |
| 전체 정적 검사·구조 심사 | A 종료 checkpoint | 제품 동작 검증 완료로 표시 |

실제 앱 실행·Keychain 조작·live OAuth·실제 LLM·전체 기능 회귀를 A의 기본 작업마다 요구하지 않는다. 예상 동작과 금지 동작은 A에서 명세/코드로 분명히 하고, 그 제품 수준 판정은 B에 둔다.

```bash
# 작업 시작 — 사용자 변경을 보존하고 직전 기준과의 차이를 확인
BASE=89452eb5523ef6b1c76b7fe857095748d76de22d
git status --short
git rev-parse HEAD
git cat-file -e "$BASE^{commit}"
# 기준 object가 있을 때만 실행. 없으면 현재 경로·원장을 기준으로 차이를 기록한다.
git diff --name-status "$BASE" HEAD

# A 중 — 변경한 실제 package에 대해서만 수행하는 예시
cargo check -p floe-conversation
python3 tools/architecture/check_boundaries.py --mode migration
git diff --check

# A 종료 — 최종 모듈 배치 후 같은 integration snapshot에서 수행
cargo check --workspace --all-targets
(cd apps/client && flutter analyze)
# Go를 변경했으면 동작 실행 없는 compile/정적 검사도 수행한다.
(cd server && go build ./... && go vet ./...)
python3 tools/architecture/check_boundaries.py --mode final
```

예시는 존재하는 package/target와 SDK에 맞춰 적용한다. checker가 검사하지 못하는 Dart/Go 의미 경계는 코드 검토와 해당 도구로 추가 확인한다. 기존 경계 검사 범위를 넓히되 금지 의존을 허용 목록에 넣어 결과만 맞추지 않는다. A에서 고위험 변경에 대한 테스트가 필요하면 관련 테스트 이름을 확인하고 **그 범위만** 실행한다. 변경 묶음마다 full suite를 반복하지 않는다. 같은 에이전트가 좁은 검사부터 수행하고 A 종료 시 전체 구조를 확인한다.

### 4.2 B에서 제품 행위를 검증한다

아래 식별자는 검증 이름의 제안이지 현재 이미 존재하는 테스트 이름이나 실행 결과가 아니다. 같은 보장을 주는 기존 테스트가 있으면 재사용한다. 기존 T01–T48 전체를 복사·재생성하지 않고 이 변경과 연결되는 원장 항목에 증거를 붙인다.

| 시나리오 | 필수 관측 / 금지 동작 |
|---|---|
| `current_contract_without_bump` | 살아남는 app 상수 2·Conversation marker 7 유지. 같은 소스의 Rust/Dart golden 일치; old alias/fallback 없음 |
| `fresh_profile_and_current_reopen` | 변경된 정의는 새 개발 데이터로 생성. 같은 현재 코드로 재실행·복구 성공. DB open 실패의 자동 삭제/키 교체 0 |
| `normal_app_vault_access` | helper가 아닌 실제 앱에서 key 생성·재조회·unlock·reopen. 실패 시 최초 안전한 OS/저장 단계 확인 |
| `admission_without_catalog_or_model_network` | catalog/model inventory barrier 중에도 신규 Run receipt 수락. query/cancel 처리. source 미사용 대화의 catalog 호출 0 |
| `same_command_after_environment_change` | token/catalog/가용성이 바뀌어도 같은 command→같은 receipt, 추가 모델·도구 호출 0. 다른 의도는 conflict |
| `same_session_claim_and_terminal` | 다른 command의 동일 Session 동시 쓰기 차단. terminal commit 후 Release 없이 다음 turn. 저장 미확정이면 해당 Session만 recovery 요구 |
| `child_failure_and_root_cancel` | child 거절/timeout은 실패 상태+안전한 설명. root cancel/hard deadline/무승인 dispatch는 추가 호출 0 |
| `journal_ack_and_task_result_loss` | batch/intent ack 전 side effect 0. Task 결과 ack 유실→같은 Task 조회, 작업 중복 0. usage 한 번 정산 |
| `expert_registry_without_builtin_switch` | Calendar 활성 여부와 무관한 Manager 선택. Schedule와 추가 Expert가 같은 등록 경로. 공용 코드 수정 없이 추가 |
| `dependent_output_release` | local-only 원문·파생 결과의 remote 전송 0. revoke 이후 신규 공개 0. Unknown을 임의 independent로 승격하지 않음 |
| `connect_during_chat` | query/preview가 chat stop을 보내지 않음. refresh 실패는 disconnect 아님. 늦은 refresh가 새 상태를 덮지 않음 |
| `oauth_reentry_and_uncertain_write` | 재진입은 동일 operation 관측. ack 불명 mutation을 새 ID로 blind retry하지 않음. Actions 승인 범위 보존 |
| `transport_and_event_recovery` | open/close/exit/timeout 대기자 settle. 화면 dispose는 cancel 아님. event gap·재시작 후 Session/Run query 복구 |
| `single_product_path` | 일반 macOS 앱에서 인사→두 번째 입력→대화 중 Connect→명시 취소→재진입. 기존 Day/Memory 검토·승인 액션도 canonical API 사용 |

단위·controlled composition·실제 앱·실제 LLM 선택 품질은 서로 다른 증거다. 실제 앱 접근이 안 되면 정확히 `blocked`로 남긴다. 테스트 개수로 보완했다고 하지 않는다.

```bash
# B — 현재 정의에 맞게 정리한 대상 검증. 없는 target을 실행했다고 쓰지 않는다.
cargo test -p floe-conversation
cargo test -p floe-agent-runtime
cargo test -p floe-experts
cargo build -p floe-ffi
(cd apps/client && flutter test)
(cd apps/client && flutter build macos --debug)
# 이후 해당 bundle에서 사용자 경로를 직접 수행한다.

# Go 변경 검증
(cd server && go test ./... && go vet ./...)
```

넓은 검증은 같은 snapshot에서 묶어 실행하고 실패를 분류한다. 코드 수정으로 영향받은 범위를 재검사하되 새 근거 없이 동일 실패를 반복하지 않는다. source snapshot·산출물·환경이 다르면 과거 pass를 현재 pass로 복사하지 않는다. schema의 숫자가 같아도 정의가 바뀐 개발 데이터는 분리한다. 빌드 도구 내부의 병렬 컴파일은 허용하지만 여러 기능 리팩터링이나 같은 앱·DB를 대상으로 한 검증을 동시에 진행하지 않는다.

### 4.3 구조 완료의 증거는 grep보다 넓다

```bash
rg -n 'LegacyComposition|AppHost::legacy|Legacy(Model|Tool|Delegation)Port|LegacyExpertEndpoint' crates apps/client/lib
rg -n 'floe_core_(command|query|events)_v2|commandV2|queryV2|eventsV2' crates apps/client/lib
rg -n 'APP_WIRE_VERSION|SCHEMA_VERSION|schema_version|user_version' crates apps/client/lib
```

첫 두 검색은 제품 경로의 구형 참조 탐색이다. 0건이라고 구조 이관이 증명되지는 않는다. 상태 수정자와 concrete caller도 확인한다. 마지막 검색은 숫자 증가나 옛 decoder가 없는지 확인하는 용도다. 외부 버전·credential namespace·상태 revision을 전역 삭제하라는 지시가 아니다.

## 5. 단일 에이전트 실행·자체 검토·중단 후 재개

### 5.1 수행 주체와 작업 수

이 에이전트가 공유 계약·manifest·DDL·생성 코드·구현·삭제·diff 검토·통합·검증을 모두 직접 수행한다. 조사 전용·구현 전용·리뷰 전용 서브 에이전트도 생성하지 않는다. 위임 ticket, 에이전트별 branch/worktree, patch 취합, 별도 오케스트레이터를 만들지 않는다.

동시에 수정 중인 변경 묶음은 하나다. 작업 공간도 하나를 사용한다. 필요하면 현재 작업을 보존하는 리팩터링 브랜치 하나에서 작업하되, 사용자 미커밋 파일을 강제로 이동·정리하지 않는다. Cargo 등 빌드 도구의 내부 병렬 처리는 이 제한과 무관하다.

이 실행 방식은 **개발 작업의 순차 수행**이다. 제품의 Run/Task concurrency, cancellation tree, Manager→Expert A2A 위임은 설계대로 유지한다.

### 5.2 한 변경 묶음의 고정 절차

1. **현재 위치 확인:** 원장의 현재 단계, 실제 HEAD, working tree와 직전 체크포인트 이후 변경만 읽는다. 완료된 계약을 다시 설계하지 않는다.
2. **범위 선택:** 현재 단계에서 다음 책임 한 가지를 고른다. 바꿀 public API/owner, 읽을 함수·직접 호출자, 수정·삭제할 파일, 끝낼 조건을 짧게 정한다. 별도 ticket 파일은 만들지 않는다.
3. **필요한 계약 결정:** 입력·출력·실패·원자성·수정자를 확인한다. 같은 에이전트가 세부 결정을 내리고 구현한다. 다른 에이전트 승인이나 사용자 확인을 사소한 helper 결정마다 기다리지 않는다.
4. **구현과 호출자 수정:** 기존 유효 구현을 최종 모듈에서 직접 고친다. 바뀐 계약의 직접 소비자와 필요한 DDL/validator/decoder도 같이 맞춘다. 아직 작성하지 않은 기능을 성공 stub으로 대체하지 않는다.
5. **대응 구형 코드 제거:** 책임을 옮긴 원본·옛 caller·중복 state·호환 경로를 지운다. 남은 소비자 때문에 삭제하지 못했다면 그 소비자를 현재 교체 범위에 포함해 해결한다. 제거하지 못한 경로는 완료로 표시하지 않는다.
6. **자체 검토:** 실행 중인 작업이나 외부 효과를 추가하지 않고 diff와 실제 호출 방향을 다시 읽는다. 상태 수정자 중복, 구형 구조의 단순 이름 변경, 전역 catch, 무승인 fallback, intent 누락, 다중 usage 정산, 잘못된 부작용을 확인한다.
7. **필요한 검사:** A에서는 변경 모듈의 타입·의존성·구조 검사와 해당 변경에 필수인 좁은 안전 검증만 한다. B에서는 배정된 현재 시나리오를 실행한다. 같은 결과를 이유 없이 반복하지 않는다.
8. **체크포인트:** 원장에 실제 수정/연결/삭제 범위·검사·미검증·다음 한 작업을 기록한다. 그다음 순서로 넘어간다. 변경 묶음 하나의 완료가 전체 리팩터링 완료는 아니다.

권한·원자성·native lifetime 같은 설계 전제가 틀린 증거가 나오면 그 범위의 계약을 먼저 교정한다. 승인된 아키텍처를 임의로 바꾸거나 동작을 살리기 위한 구형 경로를 복원하지 않는다. 해석에 필요한 실제 코드·직접 호출자는 읽되 전체 저장소 재감사로 확장하지 않는다.

### 5.3 진행 기록은 기존 원장 하나

`docs/architecture/migration-ledger.md`의 현재 표와 필요한 체크포인트만 갱신한다. 기존 P ID를 사용하며 새 STATUS JSON, 파일 소유권 표, 세션별 별도 원장을 만들지 않는다. 아래는 원장 안에 넣을 최소 형식이다.

```text
단계/항목: A 또는 B / §3.x / 기존 P / 세부 책임
실제 기준: HEAD SHA + 미커밋 변경 범위
현재 상태: 진행 중 | 구조 묶음 완료 | 환경 차단
구현·연결·삭제: 실제 파일/심볼과 남은 호출자
검사: 실제 명령 / 결과 / 환경 / 검사한 snapshot
동작: not_run | passed | failed | environment_blocked / 해당 범위
미완료: 남은 계약·컴파일 오류·제거 경로·필수 검사
다음: 이어서 수행할 정확한 한 작업 / 첫 파일·심볼
```

`wired`는 코드상 실제 구현 연결이고, `passed`는 실행 결과다. A에서 제품 검증을 하지 않은 것은 계획된 `not_run`이며 리팩터링 blocker가 아니다. SDK가 없어 구조 compile을 확인하지 못했다면 해당 검사만 `environment_blocked`로 둔다. 코드 자체의 컴파일 오류는 환경 문제로 바꾸어 적지 않는다.

명령 결과는 실행 당시의 snapshot에 귀속된다. 이후 변경의 영향이 없는 검사만 재사용하고, 영향을 받은 검증은 다시 수행한다. 과거 pass를 새 HEAD의 pass로 복사하지 않는다.

### 5.4 세션 중단과 재개

한 에이전트가 모든 작업을 한 번의 세션에서 끝내야 한다는 뜻은 아니다. 여러 세션을 사용해도 매 세션에 실행자는 하나이고, 이전 체크포인트를 이어받아 순차 진행한다.

**중단 전:** 가능하면 현재 변경 묶음을 닫는다. 도구·권한·세션 한도로 중간 종료해야 하면 부분 수정 파일, 현재 깨진 호출자/컴파일 오류, 실행하지 않은 검사, 다음 첫 수정까지 원장에 남긴다. 깔끔한 종료 보고를 만들기 위해 stub·ignore·호환 wrapper를 추가하지 않는다. 자신이 시작한 실행 중 도구가 있다면 완료 결과를 수집하거나 안전하게 종료하고, 확인 못한 결과를 성공으로 적지 않는다. 사용자가 실행한 프로세스를 임의로 종료하지 않는다.

**재개 순서:**

```text
원장 현재 체크포인트 읽기
  → git status / 현재 HEAD / 실제 diff 대조
  → 중단된 변경 묶음이 있으면 먼저 복구·완료
  → 이미 끝난 항목은 코드 근거로 건너뛰기
  → 기록된 다음 파일·심볼부터 계속
```

 dirty 상태를 실패로 보거나 `reset --hard`로 정리하지 않는다. 예상과 다른 변경이 있으면 관련 diff와 owner 계약만 대조한다. baseline으로 되돌리지 않고 현재 소스를 기준으로 계속한다. 변경 묶음을 commit할 때는 현재 권한과 저장소 관례를 따르고, 관련 파일만 명시적으로 stage한다. 이 문서는 push·배포·원격 변경을 자동 허가하지 않는다.

### 5.5 A 종료와 B 시작

A의 단계가 모두 닫히면 같은 에이전트가 3.8 구조 심사를 수행한다. 결과를 **`구조 리팩터링 완료 / 제품 동작 검증 대기`**로 분리해 기록한 후에만 B로 이동한다. A까지만 요청받은 실행이면 그 지점에서 끝내고 다음 작업은 B로 남긴다. 전체 수행 범위라면 허용된 도구·환경에서 B도 같은 에이전트가 순차 수행한다.

B의 실패는 구현 버그/설계 결함/환경/구형 테스트 가정으로 분류해 해당 owner에서 수정한다. 구조를 바꾸는 수정은 영향받은 A 검사도 다시 한다. B가 끝나기 전 중간 브랜치를 제품 안정성 검증 완료나 배포 가능 상태로 보고하지 않는다.

**추가하지 않을 것:** 서브 에이전트·병렬 코딩·리뷰 위임, 구형 데이터 migration, v3/next 평행 구현, schema fingerprint 관리 시스템, 전면 event sourcing, 재귀·병렬 A2A 기능 확장, 새 상태관리 프레임워크, 필요 없는 SDK 업그레이드, Android parity, Learner 품질 연구, 새로운 진행 관리 체계.

## 6. 근거와 작성 범위

승인된 단일 에이전트 계획을 저장소에 설치했다. 기술 변경 범위, 구조 우선 순서, 하위호환 제거, 스키마 추가 증가 금지와 안전 불변식은 유지한다. 문서 현행화 시 main이 `89452eb5523ef6b1c76b7fe857095748d76de22d`임을 재확인했지만, 이번 정리는 새로운 전체 코드 감사나 제품 검증이 아니다. 현재 실행 상태는 [migration-ledger.md](migration-ledger.md), 이전 계획과 검증 기록의 위치는 [문서 안내](README.md)에서 확인한다.

기준 SHA와 아래 소스 링크는 직전 조사 snapshot이다. 실제 작업자는 현재 checkout의 차이를 확인하고 이미 구현·연결·삭제된 작업을 반복하지 않는다. 앞으로 생길 파일에 가상의 줄 번호를 부여하지 않는다. 기존 줄 범위는 탐색 window이며 자동 적용 patch hunk가 아니다.

제품 소스 수정·Cargo/Flutter/Go 실행·Apple UI·실제 모델 평가를 이 문서 작성 과정에서 수행하지 않았다. source에서 확인한 구조, 원장 작성자의 과거 실행, 앞으로 검증할 동작을 구분한다.

**1차 종료:** 실제 코드와 호출자가 승인된 경계에 있고, 구형 경로를 제거했으며, 가능한 target의 정적 검사와 필요한 최소 안전 검증이 완료된 상태.

**전체 종료:** 그 하나의 구현에서 필요한 사용자·장애 경로를 검증하고 발견한 결함을 수정한 상태. 단일 에이전트로 진행했다는 사실이나 구조 완료만으로 제품 안정성을 보장하지 않는다.

---

<!-- 기준 소스의 고정 링크. 구현 후 현재 상태는 migration-ledger.md에 기록한다. -->

[abi]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/abi.rs
[app-compose]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/lib.rs#L35-L180
[client]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/runtime_client/floe_client.dart
[connections]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/modules/connections/src/lib.rs
[connector-panel]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/features/day_canvas/presentation/server_connector_panel.dart
[connector-screen]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/features/day_canvas/presentation/connector_screen.dart
[controller]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/features/agent/agent_controller.dart
[coordinator]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/modules/conversation/src/application/coordinator.rs
[day-gateway]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/features/day_canvas/application/ffi_day_gateway.dart
[dispatch]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch.rs
[engine]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/runtime/agent/src/engine.rs#L1-L245
[finalization]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/modules/conversation/src/application/finalization.rs
[gateway]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/features/agent/agent_vault_gateway.dart
[host-route]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/inference_routes.rs#L1-L126
[keys]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-core/src/agent_vault/keyring.rs#L1-L100
[policy]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/tools/architecture/module-dependencies.json
[ports]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/contracts/agent/src/ports.rs
[repo-policy]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/AGENTS.md
[repository]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/vault_host/conversation_repository.rs
[root]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/vault_host/conversation_turn.rs#L360-L490
[schema]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-protocol/src/dto/mod.rs#L1-L12
[status]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/docs/architecture/migration-ledger.md
[store-schema]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-core/src/agent_vault/conversations.rs#L1-L165
[task]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/modules/experts/src/task.rs
[task-repository]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/vault_host/task_repository.rs
[transport]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/apps/client/lib/infrastructure/native/native_transport.dart
[vault]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-core/src/agent_vault.rs
[worker]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/crates/floe-ffi/src/vault_host.rs
[workspace]: https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/Cargo.toml
