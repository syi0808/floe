# Floe Planning Documents

> Status: Product planning specification, not the active refactoring schedule.  
> 현재 코드 리팩터링의 방향은 [Stage 1](../refactoring/stage-1.md), [Stage 2](../refactoring/stage-2.md), [Stage 3](../refactoring/stage-3.md) overview가 정의한다. Stage 2·3의 구체 실행은 각 overview가 링크하는 단계별 실행 문서에 둔다. 현재 실행 순서와 checkpoint는 Stage 2 overview가 소유한다. 이 디렉터리의 slice/PoC 순서는 현재 리팩터링의 선행 gate가 아니다.

## Floe in one sentence

**Floe는 사용자의 시간, 할 일, 기록, 건강, 인간관계와 개인사를 장기적으로 이해하고, 사용자의 주요 기기에서 조용히 곁에 존재하는 오픈소스 개인 비서다.**

Floe의 제품 중심은 Agent Framework나 채팅 UI가 아니다.
장기적으로 사용자는 하나의 ambient Manager와 주로 음성으로 관계를 맺고, visual UI는
명시적 결재·consent·복잡한 비교·근거 확인·recovery가 필요할 때만 나타난다. 내부
Expert는 provider가 아니라 삶의 판단 domain을 담당한다.

핵심 자산은 다음 네 가지다.

1. **Personal Timeline** — 언제 무엇을 하는가
2. **Personal State** — 지금 어떤 상태인가
3. **Personal Memory** — 누구이고 어떤 삶을 살아왔는가
4. **Integration Fabric** — 실제 서비스와 기기에서 데이터를 안전하게 연결하는 방법

LLM, Agent Runtime, MCP, 특정 inference provider는 이 위에서 교체 가능한 구현 요소로 취급한다.

## 제품 문서 읽기

| 관심 영역 | 시작 문서 |
|---|---|
| 제품 비전·원칙·범위 | [Vision](00-overview/product-vision.md), [principles](00-overview/product-principles.md), [boundaries](00-overview/product-boundaries.md), [roadmap](00-overview/roadmap.md) |
| Day Canvas와 사용자 경험 | [Day Canvas](01-experience/day-canvas.md), [voice and presence](01-experience/voice-and-presence.md), [review and authority](01-experience/review-authority-and-activity.md) |
| 개인 데이터의 의미 | [Timeline](02-domain/personal-timeline.md), [state](02-domain/personal-state.md), [memory](02-domain/personal-memory.md), [relationships](02-domain/people-and-relationships.md) |
| Agent·Expert·학습 | [Manager and Experts](03-intelligence/manager-and-experts.md), [runtime and learning](03-intelligence/agent-runtime-and-learning.md), [extension model](03-intelligence/expert-extension-model.md), [model layer](03-intelligence/model-layer.md) |
| 기기·플랫폼 | [Device agent](04-platform/device-agent.md), [Apple](04-platform/apple-platforms.md), [other platforms](04-platform/android-and-windows.md) |
| 연결·source | [Integration fabric](05-integrations/integration-fabric.md), [connector contract](05-integrations/connector-contract.md), [context portfolio](05-integrations/assistant-context-portfolio.md), [initial set](05-integrations/initial-connector-set.md) |
| privacy·격리 | [Data classification](06-security/privacy-and-data-classification.md), [local compute](06-security/sensitive-local-compute.md), [Expert permissions](06-security/expert-permissions-and-sandbox.md) |
| 서버·다중 기기 | [Self-hosting](07-server/server-and-self-hosting.md), [Person/membership](07-server/account-person-membership.md), [sync](07-server/sync-and-multi-device.md) |
| 기술 선택·미확정 위험 | [Architecture map](08-engineering/architecture-map.md), [risks](08-engineering/technical-risks.md), [decisions](08-engineering/decisions.md), [open questions](08-engineering/open-questions.md) |
| 구현 배경 | [Technology selection](09-implementation/technology-selection.md), [client](09-implementation/client-architecture.md), [Rust](09-implementation/rust-core.md), [repository layout](09-implementation/repository-layout.md) |
| Expert 생태계 | [Package](10-ecosystem/expert-package.md), [marketplace](10-ecosystem/expert-marketplace.md), [development](10-ecosystem/expert-development.md) |

## 현재 리팩터링을 시작하는 경우

[Stage 2 — Canonical Internal Runtime](../refactoring/stage-2.md)의 현재 checkpoint → 실제 코드와 직접 호출자 순서로 읽는다. Stage 1은 완료된 ownership 기준이고 Stage 3는 제품 경계/실사용 검증의 후속 단계다. 과거 전체 planning bundle이나 모든 PoC를 다시 수행하지 않는다.

Stage 2에서는 canonical owner의 실제 production path와 old-path cutover를 우선한다. 일반 앱·Keychain·OAuth·실제 LLM을 포함한 제품 경계 end-to-end 검증은 Stage 3에서 닫는다. 설계 성립을 좌우하는 고위험 불변식은 해당 owner 단계에서 최소 범위로 먼저 검증한다.

[Vertical Slice Delivery](08-engineering/vertical-slice-delivery.md)와 기존 slice/ADR은 제품 시나리오·인수 조건·과거 결정의 근거로 보존한다. 현재 리팩터링의 작업 순서와 상태 원본은 아니며, 이 문서 변경이 이전 acceptance를 통과시키지도 않는다.

## Connection 권한·관측 배경

1. [공통 의미와 불변 조건](05-integrations/connection-access-and-observation.md)
2. [런타임 검증·인수 기준의 배경](09-implementation/connection-authorization-runtime.md)
3. [ADR 0027](../decisions/0027-connection-authority-and-observation.md)
4. [ADR 0028](../decisions/0028-pairing-integrated-authority-and-connection-permissions.md)
5. [페어링·connection permission 배경 계획](09-implementation/pairing-and-connection-permissions.md)

각 문서의 결정/제안 상태와 당시 근거를 구별한다. 설계에 적혀 있다는 이유로 현재 runtime 구현·검증을 완료로 표시하지 않는다. 현행 리팩터링 상태는 Stage 2의 Current checkpoint를 확인한다.

## 문서 관리 규칙

- 제품 의미, 현재 기술 실행 계획, 진행 상태, 과거 검증을 구별한다. 현재 구조 이관의 세부 지시를 이 디렉터리에 복제하지 않는다.
- 공통 abstraction은 하위 도메인 세부 구현을 숨길 만큼만 둔다. 미확정 사항을 구현된 것으로 쓰지 않는다.
- 중요한 제품 설계 변경은 관련 결정에 기록하되 리팩터링 실행 상태는 현재 활성 Stage 문서 한 곳에만 둔다.
- 플랫폼의 장기 experience parity와 현재 Apple 우선 범위를 혼동하지 않는다. Android parity는 현재 리팩터링 gate가 아니다.
- 보안·privacy·원자성·복구 불변식은 유지한다. 과거 migration/버전 계획보다 현재 하위호환 없음·스키마 추가 증가 없음 방침이 우선한다.

## Runtime note

Default Floe runtime does not require Node.js. Third-party TypeScript connector ecosystems are treated as port/import sources; connector execution is native Rust/Go or declarative ConnectorSpec. Product roadmap breadth is not permission to add new runtime platforms during the current refactor.
