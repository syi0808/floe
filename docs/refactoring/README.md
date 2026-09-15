# Floe refactoring documents

리팩터링 계획·실행서·프롬프트의 **문서 버전 이력**을 관리한다. 일반 아키텍처 설명은 [../architecture/](../architecture/README.md), 제품 요구사항은 [../planning/](../planning/README.md)에 둔다.

<a id="active-version"></a>
## 활성 문서판: R003

| 목적 | 현재 문서 |
|---|---|
| 목표·소유권·실행 순서 | [R003 PLAN](versions/r003-structure-first/PLAN.md) |
| 모든 단계의 코드별 실행 처방 | [R003 EXECUTION PLAN](versions/r003-structure-first/EXECUTION_PLAN.md) |
| 한 코딩 에이전트에 전달할 지시 | [R003 AGENT PROMPT](versions/r003-structure-first/AGENT_PROMPT.md) |
| 원본 파일·줄·심볼·blob SHA | [R003 SOURCE ANCHORS](versions/r003-structure-first/SOURCE_ANCHORS.md) |
| 현재 구현 상태·검사·다음 작업 | [migration-ledger.md](migration-ledger.md) — 유일한 가변 상태 원장 |

R003은 **구조 먼저, 동작 검증은 이후, 단일 에이전트의 순차 실행**이라는 기존 결정을 유지한다. 9단계·40개 하위 절의 실행서와 38개 소스 앵커를 추가했다. 기준 main은 `cfde8e24387454d519c9e3308606a7cc6bb7f6c9`이며 문서 발행 자체는 제품 코드 변경이 아니다.

## 버전 이력

| 문서판 | 원본/발행 기준 | 내용 | 상태 |
|---|---|---|---|
| [R001](versions/r001-initial/README.md) | 2026-09-14 원본 bundle; 이후 `89452eb`에서 확인 | 최초 모듈형 모놀리스 구현 패킷 | 보존용; 현재 지시 아님 |
| [R002](versions/r002-sequential/README.md) | `cfde8e2`, 2026-09-15 | 이미 합의한 단일 에이전트 구조 우선 계획·프롬프트 | 보존용; R003이 상세화 |
| [R003](versions/r003-structure-first/PLAN.md) | 2026-09-15, 기준 `cfde8e2` | 버전 디렉터리 분리와 전체 단계의 코드별 실행서 | **현재 활성** |

R001/R002라는 식별자는 이번 이력 정리에서 소급 부여했다. 원본이 그 이름으로 발행됐다고 주장하지 않으며, 원문의 자체 버전/날짜/내용은 변경하지 않았다. 자세한 차이는 [CHANGELOG.md](CHANGELOG.md), 실행 근거는 [history](history/README.md)를 본다.

## 변경 규칙

1. 의미 있는 목표·순서·계약 변경은 다음 `versions/rNNN-*/`에 발행한다. 과거 판을 현재 내용으로 덮어쓰지 않는다. 링크/오탈자 정정은 Git diff로 남길 수 있지만 정책을 조용히 바꾸지 않는다.
2. 이 README의 활성판과 CHANGELOG를 함께 갱신한다. `current/`에 본문 사본을 복제하거나 버전별 별도 진행 원장을 만들지 않는다.
3. 계획과 실행 결과를 구별한다. 실제 진행은 migration-ledger 한 곳에 기록하고, 과거 snapshot은 history 또는 해당 판의 명시적인 기록으로 보존한다.
4. 문서판 R003은 앱의 v3나 schema version 3을 의미하지 않는다. 코드의 하위호환 없음·단일 구현·스키마 추가 증가 없음 결정은 유지한다.
5. 이력 문서는 원래 위치의 상대 링크를 포함할 수 있다. 각 판 README의 고정 commit 링크로 원본 맥락을 확인한다. 옛 다음 작업이나 검증 수치를 현재 지시/성공으로 사용하지 않는다.
