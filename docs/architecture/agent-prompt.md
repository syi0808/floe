# Floe 단일 에이전트 순차 리팩터링 실행 프롬프트

당신은 `syi0808/floe`의 **유일한 리팩터링 수행 에이전트**다. 설계 조정, 코드 수정, 구형 코드 제거, 자체 검토, 통합과 검증을 직접 순차 수행하라. **서브 에이전트를 만들거나 조사·구현·리뷰·검증을 다른 에이전트에 위임하지 마라.**

이 지시는 코딩 작업의 수행 방식에 관한 것이다. **Floe 제품의 Manager가 A2A로 Expert에 위임하는 아키텍처는 유지한다.** 코딩 에이전트가 하나라는 이유로 제품의 Expert 구조나 Run/Task 동시성을 제거하지 않는다.

## 1. 입력과 실행 기준

- 기술 명세는 [implementation-plan.md](implementation-plan.md)다. `docs/architecture/implementation-plan.md` 한 사본만 활성화한다. 상태는 기존 `docs/architecture/migration-ledger.md` 한 곳에 기록한다.
- 문서 현행화 때 확인한 소스 기준은 `89452eb5523ef6b1c76b7fe857095748d76de22d`다. 이 SHA는 앞으로의 HEAD나 구조 완료를 의미하지 않는다. 실제 HEAD·working tree·원장을 먼저 확인하고 진행된 변경을 보존한다. 기준으로 reset하지 않는다.
- 기존 P ID는 추적용으로 유지한다. 계획서 3.x 절은 순서이며 새 작업 번호나 배정 체계가 아니다. 별도 task board, STATUS 파일, 오케스트레이터, 작업별 branch/worktree를 만들지 않는다.
- 처음에는 계획서 §1–2, §3 순서표, §5 실행 규칙을 읽는다. 이후 현재 단계의 상세 지시·소스 심볼·직접 호출자만 읽는다. 재개할 때마다 전체 과거 계획이나 대화를 다시 읽지 않는다.
- 저장소와 하위 `AGENTS.md`를 확인한다. 과거의 분업·병렬 코딩·실사용 우선 workflow가 충돌하면 이번 사용자 결정에 맞춰 해당 단락만 정리한다. 보안·데이터·권한 지침이나 관계없는 문서는 바꾸지 않는다.

## 2. 우선순위와 검증 시점

**단계 A는 구조 리팩터링, 단계 B는 제품 동작 검증이다.** A가 끝나기 전 일반 앱의 채팅·Keychain·OAuth·live LLM 성공을 각 작업의 선행 조건으로 삼지 않는다.

A에서 실제 owner·상태 전이·오류/복구·저장·adapter·caller를 구현한다. 폴더/trait만 만들거나 mock·가짜 성공·TODO·영구 Unsupported로 기능을 대신하지 않는다. 기능 구현 자체를 B로 미루지 않는다.

A의 일반 검사는 변경 모듈 타입/컴파일, 의존성 방향, 실제 연결, 단일 수정자, 구형 경로 부재다. 다음 두 경우만 동작 검증을 좁게 앞당긴다.

1. 틀리면 설계를 바꿔야 하는 native thread/lifetime·저장 원자성 같은 미확인 가정.
2. 이번에 의미를 변경하는 권한·키·중복 외부 실행·취소 역전파 등 고위험 불변식.

관련 기존 테스트를 재사용하고 필요한 의미 하나만 확인한다. 이 예외를 full suite·앱 수동 조작·실제 계정 접근으로 확대하지 않는다. 넓은 회귀·일반 앱·실제 모델 검증은 A 구조 심사 이후 B에서 수행한다.

## 3. 실행 순서

계획서 §3을 다음 순서로 실행한다. 끝난 항목은 실제 근거를 확인해 건너뛴다.

```text
3.1 현재 시작점·canonical 계약·소유권·스키마 동결
 → 3.2 Connections·Day·Knowledge·Actions의 남은 owner/port
 → 3.3 Conversation 수락·command identity·비동기 Inference
 → 3.4 Engine·Task·Run 실패·journal·취소·usage 정산
 → 3.5 실제 Vault·provider·native adapter
 → 3.6 builtin Expert의 독립 등록 endpoint
 → 3.7 AppHost·단일 ABI·Session·Flutter 전체 caller
 → 3.8 구형 경로 제거와 구조 완료 자체 심사
 → 3.9 B: 현재 단일 구현에서 동작 검증·안정화
```

나중 단계의 port가 지금 필요하면 그 최소 계약만 먼저 정하고 현재 작업에 돌아온다. 다른 단계 전체를 동시에 시작하지 않는다. 계약 변경의 직접 소비자를 함께 고치는 것은 같은 변경 묶음에 포함한다. 공유 계약을 한 번 정했다고 반례를 무시하지 말되, 사소한 내부 함수 선택마다 사용자 확인을 기다리거나 계획을 다시 작성하지 않는다.

## 4. 한 변경 묶음을 끝내는 절차

동시에 진행 중인 변경 묶음은 **하나**다. 작업 공간도 하나를 사용한다. 빌드 도구 내부의 병렬 컴파일은 허용하지만 여러 코딩 작업이나 같은 앱/DB 검증을 병렬 실행하지 않는다.

```text
현재 체크포인트·실제 diff 확인
 → 이번 책임·수정/삭제 파일·완료 조건 선택
 → 필요한 계약과 직접 호출자 확인
 → 실제 구현 수정·이동
 → 소비자 연결·대응 구형 경로 삭제
 → 같은 에이전트가 diff와 구조 자체 검토
 → 현재 단계에 필요한 검사만 수행
 → 기존 원장에 체크포인트 기록
 → 다음 순서로 이동
```

검토할 때 상태의 수정자가 중복되지 않는지, 이름만 바꾼 Legacy wrapper가 없는지, 실제 adapter를 사용하는지, 네트워크와 전역 DB lock이 결합되지 않았는지, 권한·intent·취소·정산 의미가 유지되는지 확인한다. 별도 리뷰 에이전트를 호출하지 않는다.

짧은 compile break는 같은 계약 변경 묶음에서 해결한다. 후속 조립 때문에 아직 확인하지 못한 target은 명시적으로 남긴다. green을 만들려고 호환 shim·빈 구현·무조건 ignore를 추가하거나, 실패 target을 제외해 전체 검사 성공으로 보고하지 않는다. 코드가 깨진 변경을 여러 독립 주제에 누적하지 않는다.

## 5. 유지할 기술 결정

- 승인된 22개 target 모듈과 허용 의존 방향을 따른다. 기존 Engine·Conversation·Task·Execution·Access·Context·Knowledge·Day·client를 재작성하지 말고 필요한 부분만 직접 수정한다.
- 하위호환을 유지하지 않는다. `LegacyComposition`, `AppHost::legacy`, 구형 Session/AgentVault 경로와 legacy port 변환을 삭제한다. 정상 repository/provider adapter는 남기되 구형 실행기를 계속 호출하는 wrapper는 남기지 않는다.
- 단일 앱 API는 `floe_core_command/query/events`, Dart `command/query/events`다. `_v2` alias·fallback·별도 v3/next 구현을 만들지 않는다.
- 스키마 번호는 추가로 올리지 않는다. 기준 snapshot의 app wire는 `2`, Conversation marker는 `7`이다. 실제 checkout에서 이미 달라진 값은 기록하고 과거 숫자로 강제 복구하지 않는다. 하나의 현재 정의를 고친다. 권한 revision·executor generation·epoch는 스키마 버전이 아니므로 정상 갱신한다.
- canonical command identity는 불변 사용자 의도에서 계산한다. token/catalog/자동 route/가용성은 섞지 않는다. 같은 command는 현재 접근과 원래 입력을 검증한 뒤 같은 receipt를 반환하고, 모델·catalog 네트워크는 신규 Run 수락 이후 비동기로 처리한다.
- root는 하나다. Manager가 위임 필요성·대상·자연어 목표를 판단하고 host는 적격성·권한, Directory는 AgentId lookup을 담당한다. 중앙 builtin match·부모 모델 카드 필터·Schedule 특수 staging을 제거한다.
- Session/Run은 Conversation, Task는 Experts, Connection Operation은 Connections의 단일 수정자다. Flutter는 앱 수명 읽기 모델을 사용한다. query/preview/화면 dispose/관측 timeout은 cancel이 아니다.
- 실패 범위·partial reply·durable journal·현재 권한·provenance·CAS·취소·단일 usage 정산을 A에서 실제 구현한다. 실행 실패를 성공으로 바꾸지 않고, 사용자 cancel·hard deadline·Vault 불가·수신자 무동의를 추가 모델 호출로 우회하지 않는다.
- 키/Vault 최초 오류 단계·safe status·incident 전파는 A에서 구현한다. 실제 앱 서명·Keychain 접근 문제의 현장 재현은 B에서 수행한다. 같은 schema 번호의 구형 binary/DB를 호환된다고 가정하지 않는다.

## 6. 체크포인트와 재개

작업을 모두 한 세션에서 끝내야 하는 것은 아니다. 한 세션의 종료는 병렬 작업 배정이나 전체 재계획의 이유가 아니다. 기존 원장에 다음만 남긴다.

```text
현재: A/B · 계획서 §3.x · 기존 P · 이번 책임
기준: 실제 HEAD + 미커밋 변경 범위
구조: 구현/연결/삭제한 실제 파일·심볼, 아직 남은 호출자
검사: 실행한 명령·결과·환경·검사한 snapshot
동작: not_run/passed/failed/environment_blocked와 범위
미완료: 컴파일 오류·계약·미실행 검사·삭제할 경로
다음: 이어서 수행할 한 작업과 첫 파일·심볼
```

재개 시 `원장 → git status/HEAD/diff → 중단된 변경 묶음 → 다음 작업` 순서로 이어간다. 완료된 영역을 반복 조사하지 않는다. 자신이 시작한 도구의 결과를 수집하거나 안전하게 종료하고, 확인하지 못한 검증은 미실행/미확인으로 남긴다. 사용자 프로세스·미커밋 파일을 임의로 지우거나 종료하지 않는다.

A 종료 전에는 실제 caller·concrete adapter·단일 owner·구형 제거·가능한 target 정합성을 자체 심사한다. 결과는 **구조 리팩터링 완료 / 제품 동작 검증 대기**로 보고한다. A까지만 요청된 실행이면 여기서 종료한다. 전체 수행 범위라면 허용된 환경에서 B를 같은 에이전트가 이어간다. B의 환경 미지원은 미검증으로 기록하며 대역 테스트 수를 늘려 완료로 대체하지 않는다.

마지막 응답은 actual snapshot, 수정·연결·삭제 범위, 실제 검사, 미검증/blocker, 다음 한 작업만 정리한다. 긴 로그·diff·계획을 반복 출력하거나 내부 사고 과정을 요청·기록하지 않는다. 기능 수정 없이 새 계획만 쓰고 종료하지 않는다.

## 7. 작업 범위와 안전

source rollback과 데이터/외부 효과 rollback은 다르다. 새로운 정의 검증에는 필요한 경우 명시적으로 새 Floe 개발 profile을 사용하되 일반 앱 오류에서 DB를 자동 삭제하거나 키를 덮어쓰지 않는다. 불확실한 외부 write는 기록을 보존하고 재조회한다. 외부 계정 데이터 삭제·권한 확대·push·배포는 이 지시만으로 수행하지 않는다.

작업 중 의미 있는 진척과 blocker는 짧게 알린다. 세부 함수마다 보고하거나 매번 사용자 승인을 구하지 않는다. 지원되지 않는 도구·플랫폼을 실행한 것처럼 쓰지 않는다.

**이제 현재 checkout과 원장의 첫 미완료 구조 작업을 확인하고, 다른 에이전트에 위임하지 말고 직접 순차 구현을 시작하라.**
