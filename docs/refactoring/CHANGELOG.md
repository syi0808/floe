# Refactoring document changelog

이 파일은 **문서판의 변경 이력**이다. 코드 구현 진척·검사 결과는 [migration-ledger.md](migration-ledger.md) 한 곳에서 관리한다.

## R003 — 2026-09-15

- `docs/refactoring/`을 일반 `docs/architecture/`에서 분리했다. 활성판은 refactoring README 한 곳에서 지정한다.
- R002의 구조 우선·한 에이전트·순차 실행 방침을 그대로 유지했다. A는 실제 구조/소유권/연결/삭제, B는 통합·앱·LLM 동작 검증이다.
- PLAN, AGENT_PROMPT, EXECUTION_PLAN, 9개 단계 파일, 38개 고정 source window와 blob SHA를 발행했다. 40개 하위 작업에 읽기 위치·목표 API·순서별 수정·삭제·A 판정·B 행위를 연결했다.
- archive 공통 값의 Context 역의존, command identity와 route 분리, admission 이후 비동기 준비, durable batch, 단일 usage, registered Expert, 실제 AppHost/Session/Flutter 연결의 구체 처방을 보강했다.
- 코드 `_v2` 또는 schema 숫자를 올리지 않는다. R003은 문서 버전일 뿐이다.
- 제품 소스·dependency·schema·빌드 설정·활성 테스트를 변경하지 않았다. 제품 테스트나 live Apple/model 검증 결과를 새로 주장하지 않는다.

## R002 — 원본 커밋 cfde8e2, 2026-09-15

- 승인된 단일 코딩 에이전트 계획과 실행 프롬프트를 저장소에 설치했다.
- 구조 우선/동작 후순위, 하위호환 제거, schema 추가 증가 없음으로 작업 지침을 정리했다.
- 진행 원장을 축약하고 원래 전체 ledger와 초기 bundle을 보존했다.
- 이번 정리에서 R002라는 이력 ID를 부여했다. PLAN/AGENT_PROMPT/당시 원장 snapshot은 원본 blob 그대로다.

## R001 — 원본 bundle 2026-09-14

- 최초 상세 구현 계획·작업 패키지·원본 앵커·그래프·검사 도구를 담은 bundle이다.
- 이번 정리에서 R001 ID를 부여하고 전체 Git tree를 내용 변경 없이 옮겼다.
- 과도기 Legacy/버전별 경로 및 구현 중 반복 실사용 검증 등 후속 결정과 다른 지시는 현재 실행에 적용하지 않는다.
