# Open Questions

> Status: Living backlog

## Product / UX

- Day Canvas의 정확한 visual grammar는 무엇인가?
- Now/Next horizon은 사용자마다 어떻게 달라져야 하는가?
- Note와 Task의 자동 분류를 어느 정도까지 자동화할 것인가?
- Floe가 먼저 말하는 빈도의 기본값은?
- Memory inspection UI는 어디에 위치해야 하는가?

## Personal Memory

- Memory storage는 graph/document/event-log 중 무엇을 중심으로 할 것인가?
- Evidence와 claim의 exact schema는?
- 사람 identity merge threshold는?
- Highly Sensitive memory의 자동 저장 허용 범위는?
- 삭제 시 derived artifact를 어디까지 추적할 것인가?
- long-term consolidation/sleep cycle을 어느 시점에 도입할 것인가?

## Intelligence

- Manager runtime은 server 중심인가 device 중심인가?
- Expert를 process/service/pure function 중 어떤 실행 모델로 표현할 것인가?
- Domain-specific model package의 distribution/versioning은?
- subscription provider가 offline일 때 fallback 정책은?
- local model bundle 크기/업데이트 전략은?

## Health

- recovery/state estimator의 deterministic baseline은?
- 개인 baseline 계산 기간은?
- 사용자가 직접 입력하는 subjective condition을 어떻게 섞는가?
- 의료적 조언과 생활 관리 advice의 제품 경계는?

## Integration

- optional JavaScript compatibility host가 실제로 필요한가?

- Connector Contract의 실제 언어/runtime은?
- Activepieces Piece 중 어느 subset을 ConnectorSpec으로 자동/반자동 변환할 것인가?
- connector process isolation이 필요한가?
- webhook ingress를 central server가 담당하는가?
- connector update compatibility를 어떻게 보장하는가?

## Platform

- macOS client stack
- Windows client stack
- iOS/Android 공유 코드 범위
- desktop wake model
- speaker recognition model
- meeting audio capture의 플랫폼별 제약

## Server

- DB 선택
- event/mutation log
- queue/worker model
- encryption key hierarchy
- E2EE 범위
- self-host backup/recovery
- multi-user isolation

## OAuth

- 초기 BYO credential UX
- optional managed OAuth broker의 token flow
- self-host server가 token을 소유하지 않는 구조 가능성
- provider별 공식 subscription integration policy

## OSS

- monorepo vs multi-repo
- connector repository 분리 여부
- plugin/extension SDK
- 라이선스
- hosted service와 OSS 기능 경계


## Expert Ecosystem

- Expert API v1의 최소 View/Output contract는 무엇인가?
- declarative pipeline primitive를 어디까지 제공할 것인가?
- Wasm Expert를 Go server에 직접 embed할지 Rust worker로 분리할지?
- Marketplace Expert가 remote model을 요청하는 permission/비용 정책은?
- network egress permission을 아예 금지할지 allowlist로 지원할지?
- built-in Expert를 어느 수준까지 동일 runtime에서 dogfood할 것인가?
- Expert state migration 실패 시 rollback 정책은?
- Marketplace의 검증/서명 체계는?
- 유료 Expert와 오픈소스 Expert의 distribution model은?
