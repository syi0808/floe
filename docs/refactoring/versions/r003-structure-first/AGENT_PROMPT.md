# R003 실행 프롬프트 — 한 에이전트, 구조 먼저

당신은 `syi0808/floe` 리팩터링의 유일한 코딩 에이전트다. 조사·설계 세부 결정·코드 수정·삭제·자체 검토·통합을 직접 순차 수행하라. 서브 에이전트·리뷰 위임·병렬 coding worktree를 만들지 마라. 제품의 Manager→Expert A2A와 Run/Task 동시성은 그대로 구현한다.

## 읽을 문서와 상태

- 이 버전의 [PLAN.md](PLAN.md)가 목표/정책, [EXECUTION_PLAN.md](EXECUTION_PLAN.md)와 `steps/`가 실제 변경 절차다. [SOURCE_ANCHORS.md](SOURCE_ANCHORS.md)는 원본 읽기 위치다.
- 현재 활성판은 [../../README.md](../../README.md), 진행은 [../../migration-ledger.md](../../migration-ledger.md) 한 곳에서 확인한다. 다른 판의 처방을 섞지 않는다. 과거 원문은 역사이지 현재 지시가 아니다.
- R003 기준은 `cfde8e24387454d519c9e3308606a7cc6bb7f6c9`다. 실제 HEAD/dirty diff를 확인하고 이미 완료된 부분은 재사용한다. 이 SHA로 reset하지 않는다.
- 최초에는 PLAN과 실행서 공통 계약을 읽고, 이후 현재 step의 소스와 직접 소비자만 읽는다. 전체 과거 문서·로그를 반복해서 읽거나 새 전체 계획을 작성하지 않는다.

## 수행 순서

```text
01 계약·단일 소유권·schema 동결
→ 02 Connections/Day/Knowledge/Actions와 Go owner
→ 03 stable command admission·비동기 Inference
→ 04 Engine/Task/Run의 failure/journal/accounting
→ 05 실제 Vault/provider/native adapter
→ 06 builtin endpoint와 LLM 선택 경계
→ 07 실제 AppHost·단일 ABI·Session·Flutter 연결
→ 08 구조 심사와 구형 제거
→ 09 구조 완료 후 동작 검증
```

한 번에 한 하위 절만 수정한다. 필요한 직접 소비자를 함께 바꾸는 것은 같은 변경 묶음이다. 뒤 단계의 최소 port가 필요하면 그 계약만 먼저 정하고 현재 작업으로 돌아온다. 파일을 나누기 위해 새로운 추상화 층을 무조건 추가하지 않는다.

## 각 변경 묶음의 실행

1. 현재 원장·HEAD/diff에서 중단된 작업을 먼저 확인한다.
2. 해당 step의 원본 줄/심볼과 직접 caller를 읽고 입력·출력·오류·원자성·수정자를 확정한다. 줄은 baseline 탐색 창이지 자동 patch가 아니다.
3. 유효한 기존 구현을 최종 owner로 이동·수정한다. 실제 repository·adapter·caller를 연결하고 대응 old path를 삭제한다.
4. 같은 에이전트가 diff를 다시 읽어 중복 state owner, 이름만 바꾼 Legacy, 무승인 fallback, global catch, intent 누락, 이중 usage, lifetime 결함을 확인한다.
5. A에서는 관련 compile/type/DAG/caller 검사만 수행한다. 변경한 고위험 안전 의미 또는 설계 가정에 필요한 경우만 좁은 controlled test를 실행한다. live 앱/Keychain/OAuth/모델과 full suite는 09로 미룬다.
6. 원장에 실제 구현·연결·삭제·검사와 다음 첫 파일/심볼을 남기고 다음 하위 절로 이동한다. 빈 trait/mock-only/영구 Unsupported/TODO로 단계 완료를 표시하지 않는다.

짧은 compile break는 같은 계약 변경 묶음에서 복구한다. 호환 wrapper로 잠시 green을 만들지 말고, 깨진 소비자를 여러 독립 주제에 누적하지도 마라. 환경상 못 검사한 것과 코드 오류를 구별하라.

## 바꾸면 안 되는 기준

- 하위호환·old decoder·migration chain·parallel v2/v3/next를 만들지 않는다. `_v2` ABI 이름은 접미사 없는 단일 이름으로 caller와 함께 교체하고 alias를 남기지 않는다.
- 문서판 R003은 코드/DB 버전이 아니다. app2/Conversation7 등 실제 채택값을 추가 증가시키지 않는다. 이미 올라간 HEAD 값을 과거로 되돌리지 않는다. authority revision/generation/epoch는 계속 갱신한다.
- command identity는 불변 의도만 포함한다. token/catalog/auto route/현재 context는 제외한다. 모델 준비는 durable admission 뒤 비동기로 한다.
- host는 eligibility/authority, Manager는 의미 선택, Directory는 등록 ID 전달이다. Calendar-first, 중앙 builtin switch, parent-model 필터, parent-Run staging을 남기지 않는다.
- Run/Session은 Conversation, Task는 Experts, Operation은 Connections가 수정한다. UI는 읽기 모델이며 query/preview/dispose/observer timeout은 cancel이 아니다.
- 암호화·키 identity·recipient consent·provenance·CAS·intent ack·current-schema recovery·uncertain external write 보호를 유지한다. Unknown payload를 Independent로 바꾸지 않는다.
- 외부 OAuth/A2A/서명/credential namespace를 일괄 rename하지 않는다. 일반 open 오류에 자동 DB 삭제·키 교체를 연결하지 않는다.

## 중단·완료

단계/하위 절/기존 P, 실제 HEAD와 dirty 범위, 수정·연결·삭제 심볼, 실행 검사, 동작 not_run/결과, 미완료 및 다음 한 작업을 기존 원장에 적는다. 같은 변경 묶음을 다음 세션에 이어가고 새 task board·STATUS 파일을 만들지 않는다.

08을 통과하면 **구조 완료 / 제품 동작 검증 대기**다. 요청이 A만이면 멈춘다. 전체 수행 범위면 허용된 환경에서 09를 이어간다. 실제 제품 성공을 확인하지 않고 구조 완료를 배포 가능으로 바꾸지 않는다. push/배포/외부 계정 변경은 사용자의 별도 권한을 따른다.

이제 현재 checkout과 원장을 확인하고 첫 미완료 하위 절부터 직접 구현하라. 계획을 다시 요약하는 것으로 작업을 대신하지 마라.
