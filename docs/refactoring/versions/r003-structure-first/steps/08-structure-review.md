# 08 — 구형 경로 제거와 Stage A 구조 완료 심사

**단계:** A의 마지막. **기존 범위:** P22/P23/P24/P25 구조 범위. **선행:** 01–07 실제 구현. **다음:** [09 동작 검증](09-behavior-validation.md).

같은 에이전트가 수정 모드에서 검토 모드로 전환한다. 다른 리뷰 에이전트를 만들지 않는다. 이 단계는 앞 단계의 삭제를 유예할 허가가 아니라 누락을 잡는 최종 검사다.

## 08.1 실제 의존성과 최종 파일 위치 검사

**읽기:** [S01 workspace](../SOURCE_ANCHORS.md#s01), [S36 boundary checker:L1–140](../SOURCE_ANCHORS.md#s36), `tools/architecture/module-dependencies.json`의 `target` 및 실제 모든 Cargo manifest.

1. workspace에 22개 target package가 최종 경로로 존재하는지 검사한다. `floe-agent/core/domain/infra`는 유효 구현과 소비자를 이전한 뒤 삭제한다. Protocol/FFI는 package 이름은 같고 경로만 bindings 아래로 이동한다.
2. normal/build/target-specific dependency와 workspace alias를 실제로 해석한 DAG로 검사한다. 정책 JSON의 오래된 `current` 표를 실제 상태로 오해하지 않는다. 현재 manifest를 기준으로 한 결과를 원장에 기록한다.
3. 업무 모듈→concrete adapter/app/FFI, generic runtime→builtin, Inference↔Connections 경로가 없어야 한다. `dev-dependencies`, `include!`, `#[path]`, re-export를 사용해 금지 production coupling을 숨기지 않는다.
4. 공유 값의 실제 정의까지 따라간다. 01.3 archive 값은 contracts/agent 한 곳에 있고 Context runtime 타입을 Vault port에 재노출하지 않는다. consumer가 이름만 보지 말고 실제 API의 소유 모듈을 알 수 있어야 한다.
5. Dart의 feature간 private controller import, Go application→Console 역의존, native callback/키 handle lifetime도 확인한다. Rust checker 통과가 이 영역까지 증명하지 않는다.
6. checker의 실제 final mode가 최종 경로·missing target·forbidden path를 검사하는지 읽는다. 필요한 검사 결함만 보강하고 현재 코드가 불편하다고 허용 DAG를 넓히지 않는다.

**명령:** 아래는 구현 후 실행 예시다. 문서 작성 때 실행한 제품 검사 결과가 아니다.

```bash
cargo check --workspace --all-targets
python3 tools/architecture/check_boundaries.py --mode final
(cd apps/client && flutter analyze)
(cd server && go build ./... && go vet ./...)
git diff --check
```

환경상 못 실행한 target은 `environment_blocked`, 코드 오류는 `failed`다. 둘을 바꾸어 기록하지 않는다.

## 08.2 기능·단일 수정자·외부 효과의 실제 경로 심사

다음 표의 각 행에 **실제 파일:심볼**을 원장에 적는다. runtime 실행은 필요하지 않지만 선언만 보고 채우지 않고 call graph를 따라간다.

| 기능 | 추적할 연결 | 확인할 단일 원본 |
|---|---|---|
| 일반 대화 | UI→command→Conversation→Inference/Engine→repository→projection | Session/Run/Command |
| 위임 | Manager Delegate→Directory→Task owner→endpoint→report | Experts Task |
| 연결 관리 | UI→Connection command/query→control/source adapter→receipt | Operation/connection intent·observation |
| 승인 작업 | proposal→review→Actions intent→provider→receipt/reconcile | Action ledger |
| 개인 지식 | review→EvidenceReader→Knowledge transaction→read model | Knowledge revision |
| Day | capture/edit/refresh→Day policy→repository→snapshot | Day revision |
| 권한·공개 | current authority→dispatch/release fence→허용 payload | Access 및 실제 provider authority |
| 종료·재진입 | unsubscribe/cancel/close→각 owner→durable query | backend lifetime, UI는 관측 |

1. 관리 query/preview와 screen dispose에서 `stop/cancel_run`으로 가는 암묵적 edge가 없는지 확인한다. 명시적인 사용자 Cancel만 root 취소를 요청한다.
2. source catalog나 model HTTP future가 동기 FFI·전역 Vault transaction·원장 lock 안에서 대기하지 않는지 전체 경로를 확인한다.
3. accepted command의 동일성은 불변 intent만 사용하고, 신규 profile/credential 상태로 기존 receipt를 다시 판단하지 않는지 확인한다. 결과 payload의 현재 공개 권한 검사와 혼동하지 않는다.
4. Task/Run/Operation의 late writer는 generation으로 제한되고 terminal cleanup은 UI Release에 의존하지 않는지 확인한다. private active map이 durable 원본 역할을 겸하지 않는지 본다.
5. 모든 현재 지원 기능을 표에 포함한다. 기존 기능을 숨기거나 Unsupported로 바꾸어 구조 완료로 보고하지 않는다. 새 voice/wake/cross-device/Android parity는 이번 범위가 아니다.

## 08.3 테스트·구형 파일·문서 정리

1. 제거된 API 모양만 확인하던 fixture는 삭제한다. authorization/CAS/replay/키/uncertain-write를 보호하던 테스트는 최종 owner/실제 composition 경로로 옮긴다. 무조건 ignore·assert 완화로 green을 만들지 않는다.
2. 기존 테스트 이름과 경로는 `rg` 및 test target 목록에서 확인한다. 문서의 검증 시나리오 이름을 이미 존재하는 executable test 이름처럼 쓰지 않는다. Stage A에서 넓은 suite를 모두 실행할 필요는 없지만 테스트 코드도 current API로 컴파일 가능해야 한다.
3. 더 이상 product에 필요한 fixture-only entry, old DTO/serializer, legacy model/task 변환, stale include/build path를 제거한다. grep 결과가 0인 것만으로 완료하지 않고 08.2 실제 경로를 함께 확인한다.
4. 공개 ABI는 07.3의 앱 symbol만 남는다. OS callback은 별도 allowlist로 유지하고 실수로 제거하지 않는다. 가능한 Apple host에서 `nm` 결과로 선언과 export를 대조하되 앱 실행은 B다.
5. 리팩터링 문서 버전은 `docs/refactoring/versions` 아래 보존하고, 실제 진행은 `docs/refactoring/migration-ledger.md` 한 곳에만 갱신한다. R003 본문을 완료 로그로 덮어쓰지 않는다. 의미 있는 계획 변경은 후속 문서판을 만들며 앱 schema 숫자와 연결하지 않는다.
6. `docs/architecture/`는 현재 아키텍처 설명, refactoring은 변경 계획/실행, history/validation은 고정 증거다. README/AGENTS/PROGRESS는 현행 refactoring index로 연결하고 중복 활성 계획을 두지 않는다.

```bash
rg -n 'LegacyComposition|AppHost::legacy|legacy_services|Legacy(Model|Tool|Delegation)Port|LegacyExpertEndpoint' crates apps/client/lib
rg -n 'floe_core_(command|query|events)_v2|commandV2|queryV2|eventsV2' crates apps/client/lib
rg -n 'floe-(core|agent|domain|infra)' --glob Cargo.toml crates Cargo.toml
```

첫 두 검색은 제품 코드의 제거 후보다. 문서 이력의 과거 문자열은 삭제하지 않는다. `floe-agent-contract/runtime`처럼 정상 target 이름까지 마지막 검색으로 무조건 제거하지 않는다.

## 08.4 Stage A 종료 판정과 다음 시작점

| 조건 | 증거 |
|---|---|
| 실제 모듈/DAG 일치 | 현재 manifests와 final 검사 |
| public 계약 구현 | 실제 state transition/repository/adapter 코드 |
| 단일 수정자 | 08.2 기능별 call graph |
| 단일 앱 실행 경로 | export/binding/caller 및 구형 참조 제거 |
| supported 기능 전체 연결 | 실제 UI→owner→I/O 대응 |
| 현재 schema 한 정의 | 숫자 증분 없음, old decode 없음, 새 profile 필요 여부 |
| 안전 불변식 | 의미를 바꾼 좁은 검증·검토 근거 |
| 정직한 상태 | product behavior는 not_run 또는 실제 결과 |

1. 모든 구조 행이 닫히면 `Stage A complete / Stage B not_run`으로 기록한다. 컴파일만 성공했지만 구체 구현이 빠졌으면 complete가 아니다.
2. SDK가 없는 platform은 범위를 따로 남긴다. Stage B는 실행 가능한 host에서만 수행하고 미검증 플랫폼까지 acceptance를 확대하지 않는다.
3. 다음 첫 작업은 09.1의 controlled baseline 재조회다. 과거 성공 기록을 새 HEAD의 결과로 복사하지 않는다.
4. 요청 범위가 Stage A뿐이면 여기서 종료한다. 전체 수행을 승인받은 세션은 같은 에이전트가 순차적으로 B로 이어간다. 이 문서 발행이나 A 완료만으로 push/배포/외부 계정 변경을 자동 허가하지 않는다.
