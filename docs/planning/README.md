# Floe Planning Documents

> Status: Working specification  
> Purpose: Floe의 제품 기획과 기술 설계를 단일 거대 문서가 아니라 변경 가능한 작은 문서 단위로 유지한다.

## Floe in one sentence

**Floe는 사용자의 시간, 할 일, 기록, 건강, 인간관계와 개인사를 장기적으로 이해하고, 사용자의 주요 기기에서 조용히 곁에 존재하는 오픈소스 개인 비서다.**

Floe의 제품 중심은 Agent Framework나 채팅 UI가 아니다.

핵심 자산은 다음 네 가지다.

1. **Personal Timeline** — 언제 무엇을 하는가
2. **Personal State** — 지금 어떤 상태인가
3. **Personal Memory** — 누구이고 어떤 삶을 살아왔는가
4. **Integration Fabric** — 실제 서비스와 기기에서 데이터를 안전하게 연결하는 방법

LLM, Agent Runtime, MCP, 특정 inference provider는 이 위에서 교체 가능한 구현 요소로 취급한다.

---

## 문서 구조

```text
floe-planning/
├── 00-overview/
│   ├── product-vision.md
│   ├── product-principles.md
│   ├── product-boundaries.md
│   └── roadmap.md
│
├── 01-experience/
│   ├── day-canvas.md
│   ├── capture-and-transcription.md
│   ├── voice-and-presence.md
│   ├── interventions.md
│   └── platform-experience.md
│
├── 02-domain/
│   ├── personal-timeline.md
│   ├── personal-state.md
│   ├── personal-memory.md
│   └── people-and-relationships.md
│
├── 03-intelligence/
│   ├── manager-and-experts.md
│   ├── skills-and-actions.md
│   ├── model-layer.md
│   └── health-intelligence.md
│
├── 04-platform/
│   ├── device-agent.md
│   ├── apple-platforms.md
│   └── android-and-windows.md
│
├── 05-integrations/
│   ├── integration-fabric.md
│   ├── connector-contract.md
│   └── connector-sources.md
│
├── 06-security/
│   ├── privacy-and-data-classification.md
│   └── sensitive-local-compute.md
│
├── 07-server/
│   ├── server-and-self-hosting.md
│   ├── account-person-membership.md
│   └── sync-and-multi-device.md
│
└── 08-engineering/
    ├── architecture-map.md
    ├── technical-risks.md
    ├── poc-plan.md
    ├── decisions.md
    └── open-questions.md
│
└── 09-implementation/
    ├── technology-selection.md
    ├── client-architecture.md
    ├── rust-core.md
    ├── native-platform-bridges.md
    ├── server-stack.md
    ├── turso-storage.md
    ├── connector-runtime.md
    ├── local-ai-runtime.md
    ├── performance-design.md
    └── repository-layout.md

├── 10-ecosystem/
│   ├── expert-package.md
│   ├── expert-marketplace.md
│   └── expert-development.md
```

## 문서 관리 규칙

- 제품 의미와 기술 구현을 한 문서에 섞지 않는다.
- 공통 abstraction은 하위 도메인 세부 구현을 숨길 만큼만 둔다.
- 아직 결정되지 않은 것은 `Open Questions`에 남기고 확정된 것처럼 쓰지 않는다.
- 중요한 설계 변경은 `08-engineering/decisions.md`에도 기록한다.
- 플랫폼별 feature parity가 아니라 **experience parity**를 목표로 한다.
- 보안과 privacy는 사후 제약이 아니라 제품 요구사항이다.

## 추천 읽기 순서

처음 보는 경우:

1. `00-overview/product-vision.md`
2. `00-overview/product-principles.md`
3. `01-experience/day-canvas.md`
4. `02-domain/personal-memory.md`
5. `03-intelligence/manager-and-experts.md`
6. `05-integrations/integration-fabric.md`
7. `06-security/privacy-and-data-classification.md`
8. `08-engineering/architecture-map.md`

구현을 시작하는 경우:

1. `08-engineering/poc-plan.md`
2. `04-platform/device-agent.md`
3. `05-integrations/connector-contract.md`
4. `07-server/account-person-membership.md`
5. `03-intelligence/model-layer.md`
6. `08-engineering/technical-risks.md`

## Runtime Note

Default Floe runtime does not require Node.js. Third-party TypeScript connector ecosystems are treated as port/import sources; connector execution is native Rust/Go or declarative ConnectorSpec.


## Expert Ecosystem Reading

1. `03-intelligence/expert-extension-model.md`
2. `06-security/expert-permissions-and-sandbox.md`
3. `09-implementation/expert-runtime.md`
4. `10-ecosystem/expert-package.md`
5. `10-ecosystem/expert-marketplace.md`
6. `10-ecosystem/expert-development.md`
