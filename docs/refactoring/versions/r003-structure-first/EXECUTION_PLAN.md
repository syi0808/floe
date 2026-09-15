# R003 상세 실행 계획서 — 읽기·수정·삭제·구조 판정

이 문서는 [PLAN.md](PLAN.md)의 모든 단계에 대한 실행 진입점이다. **고정 코드 줄을 읽고 아래 처방을 순차 실행한다.** 새 source를 다시 설계하라는 추상적인 조사 과제가 아니다. 다만 소스가 변경되었거나 앞 단계에서 이동한 경우 줄 번호를 맹목적으로 적용하지 말고 해당 심볼·직접 호출자를 대응시킨다.

기준: `cfde8e24387454d519c9e3308606a7cc6bb7f6c9`. 연결된 단계 문서도 모두 이 판의 일부다. [고정 소스 앵커](SOURCE_ANCHORS.md)와 [기계 판독 목록](source-anchors.json)은 같은 기준을 사용한다.

## 실행 규칙

1. `AGENTS.md`, 이 판의 PLAN/AGENT_PROMPT, 원장 current만 읽고 시작한다. 과거 R001/R002를 매번 읽지 않는다.
2. 작업 시작에 HEAD/dirty diff를 확인한다. 현재 완료된 부분은 심볼·실제 caller 근거로 건너뛴다. baseline으로 reset하지 않는다.
3. 각 하위 절의 **읽기 → 목표 위치/계약 → 순서별 수정 → 삭제 → A 검사 → B 검증**을 따른다. A 단계에서는 B 검증을 수행 조건으로 붙이지 않는다.
4. 하나의 하위 절이 여러 파일을 바꾸는 것은 정상이다. 관련 public 타입과 소비자를 함께 바꾼다. 다른 주제의 리팩터링이나 별도 에이전트를 병행하지 않는다.
5. 함수 이동과 기능 제거를 구별한다. 구조 이관에 필요한 유효 코드는 재사용하고, 호환을 위해 old 실행기를 부르는 코드만 없앤다.
6. target로 표시한 파일/함수는 **새 위치 또는 변경할 API의 명세**다. 이미 존재한다고 가정하지 않는다. 같은 책임의 기존 구현이 있으면 그것을 수정한다.
7. source window는 1-based 탐색 구간이다. EOF를 넘으면 EOF에서 멈춘다. 심볼이 창 밖이면 `rg -n`으로 같은 파일에서 찾고 함수 전체와 직접 caller를 읽는다. 아래 코드는 계약 스케치이며 제품에 그대로 붙여 컴파일됐다고 주장하지 않는다.

```bash
BASE=cfde8e24387454d519c9e3308606a7cc6bb7f6c9
git status --short
git rev-parse HEAD
git cat-file -e "$BASE^{commit}"
# object가 있으면 이동/변경된 파일만 비교한다.
git diff --name-status "$BASE" HEAD
# 예: S03의 원본과 현재 호출자
git show "$BASE:crates/contracts/agent/src/ports.rs" | nl -ba | sed -n '1,80p'
rg -n 'JournalEvent|ExecutionJournal' crates --glob '*.rs'
```

## 단계·하위 절 색인

| 절 | 상세 실행서 | 하위 작업 |
|---|---|---|
| 3.1 | [01-contracts.md](steps/01-contracts.md) | 01.1 기준/기능 대응, 01.2 canonical command, 01.3 archive 경계, 01.4 공개 facade·스키마 |
| 3.2 | [02-business-owners.md](steps/02-business-owners.md) | 02.1 Connections, 02.2 OAuth/관측, 02.3 Actions, 02.4 Day/Knowledge, 02.5 Go |
| 3.3 | [03-admission-inference.md](steps/03-admission-inference.md) | 03.1 digest/replay, 03.2 수락/스케줄링, 03.3 비동기 route, 03.4 재시작/취소 |
| 3.4 | [04-runtime-recovery.md](steps/04-runtime-recovery.md) | 04.1 checkpoint, 04.2 scoped failure, 04.3 finalization, 04.4 attempt 정산, 04.5 취소/공개 |
| 3.5 | [05-adapters.md](steps/05-adapters.md) | 05.1 물리 저장, 05.2 owner repository, 05.3 Context/Knowledge/Actions 저장 경계, 05.4 provider, 05.5 native/진단 |
| 3.6 | [06-experts.md](steps/06-experts.md) | 06.1 pack 이관, 06.2 Task/Directory, 06.3 invocation context·Schedule·모델, 06.4 결과/lineage |
| 3.7 | [07-composition-client.md](steps/07-composition-client.md) | 07.1 AppHost, 07.2 Session/관리 API, 07.3 단일 ABI, 07.4 Flutter, 07.5 전송/빌드 |
| 3.8 | [08-structure-review.md](steps/08-structure-review.md) | 08.1 의존성/배치, 08.2 기능/상태 경로, 08.3 테스트/구형/문서, 08.4 A 인수 |
| 3.9 | [09-behavior-validation.md](steps/09-behavior-validation.md) | 09.1 통제 검증, 09.2 실제 Vault, 09.3 앱/모델, 09.4 결함 수정/인수 |

총 40개 하위 작업이다. 작업 수는 완료율이 아니며 아래 코드를 실행했다는 뜻도 아니다. 각 단계는 앞 단계의 실제 구현을 사용한다. 미확정 공유 계약은 현 단계에서 필요한 범위만 먼저 결정하고 돌아온다.

## 이행 중 반드시 일치시킬 계약

| 경계 | 단일 의미 |
|---|---|
| `request_id` | 전송 시도. 재전송마다 달라도 됨 |
| `command_id` | principal namespace 안의 사용자 의도. 같은 입력이면 같은 receipt |
| Run 수락 | user entry + Run + command record + Session claim의 원자 commit |
| 모델 준비 | 수락 이후 비동기. catalog 조회는 채팅 필수 전제가 아님 |
| root/child | parent 취소는 소유 child로 전달, child 실패는 parent 취소가 아님 |
| Task 결과 | Experts가 저장한 사실. Manager가 실패를 성공으로 재분류하지 않음 |
| 최종 답변 | execution 결과와 reply 결과 별도. 권한·예산 안에서만 생성 |
| 종료 | durable terminal + transcript/coverage + claim 해제. UI Release에 의존하지 않음 |
| 관측 | 유실 가능한 bounded events + 다시 읽을 수 있는 durable snapshot |
| 스키마 | 현재 정의만 수정, 숫자 추가 증가·old decoder 없음 |

## 검사 명령의 사용 시점

A에서 의미 있는 묶음마다 변경 package의 `cargo check -p <actual-package>`, 필요한 `flutter analyze <actual-path>`, `git diff --check`, 의존 변경 시 migration checker만 실행한다. stage 08에서 workspace/실제 산출물의 구조 검사를 모아 한다. 테스트는 변경한 고위험 의미/설계 성립 가정의 최소 범위만 A에서 앞당긴다.

B에서는 stage 09의 통제된 시나리오와 실제 앱을 수행한다. 기존 테스트를 재사용한다. 제시한 신규 테스트 이름은 목표 이름이지 이미 실행된 테스트가 아니다. target/SDK가 없으면 환경 제한을 기록한다. 로컬 Docker나 mock만으로 Apple Keychain 성공을 대신하지 않는다.

## 중단·재개

원장에 `R003/절/기존 P`, actual HEAD와 dirty 파일, 수정·연결·삭제한 심볼, 검사 결과, 남은 컴파일/계약, 다음 첫 파일·심볼을 적는다. 별도 STATUS/ticket/원장을 만들지 않는다. 원장 → 실제 diff → 중단된 절부터 재개한다. 발행 문서에 진행률을 덧붙이지 않는다.
