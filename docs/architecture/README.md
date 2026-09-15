# Architecture documentation

이 디렉터리는 현재 아키텍처의 역할·책임·의존성 설명을 위한 공간이다. **리팩터링 계획과 실행 프롬프트의 버전 이력은 [docs/refactoring](../refactoring/README.md)으로 분리했다.** 같은 계획의 활성 사본을 이곳에 두지 않는다.

| 필요한 정보 | 기준 문서 |
|---|---|
| 현재 리팩터링 계획·실행서·프롬프트와 버전 이력 | [Refactoring index](../refactoring/README.md) |
| 현재 구현 상태·검사·다음 순차 작업 | [Refactoring ledger](../refactoring/migration-ledger.md) |
| 승인된 목표 모듈 경계 | [Dependency policy](../../tools/architecture/module-dependencies.json)의 target |
| 제품 의미·장기 범위 | [Product planning](../planning/README.md) |
| 개별 설계 결정 | [ADRs](../decisions/) |
| 검증의 실제 근거 | [Validation](../validation/)와 [refactoring history](../refactoring/history/README.md) |

목표 policy와 현재 manifest는 다를 수 있다. 아직 이관 중인 구현을 최종 구조처럼 기술하지 않는다. source-level wiring, compile 결과, 실사용 결과를 구별한다. 코드 구조가 바뀌면 그 구조 설명을 갱신하되, 발행된 refactoring 판을 진행 로그로 덮어쓰지 않는다.
