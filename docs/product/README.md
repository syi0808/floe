# Floe product

Floe is an open-source personal assistant that helps a person's day and life run well by understanding durable personal context and acting only within explicit authority.

The product is designed around **one assistant**. Internal Experts, models, connectors and device providers are implementation capabilities behind that experience, not separate products the user must orchestrate.

## Product thesis

A useful personal assistant should already understand enough context that the user does not repeatedly reconstruct their situation. Floe therefore builds around four durable assets:

1. **Personal Timeline** — what happened, is happening and is planned.
2. **Personal State** — relevant current conditions such as availability, attention and capacity.
3. **Personal Memory** — people, commitments, episodes, preferences and their provenance.
4. **Integration Fabric** — consented connections to real services and devices.

LLMs and inference providers are replaceable implementation elements over these product assets.

## Experience promise

- A calm **Day Canvas** emphasizes Now and Next rather than dashboard density.
- Conversation is continuous with one Manager; voice is the long-term primary interaction channel and text remains a first-class fallback.
- Floe may speak first only when the information is important, timely, credible, actionable and worth the interruption.
- Consequential changes use explicit authority, deterministic validation and durable recovery.
- Memory and connected evidence remain inspectable, correctable, deletable and source-backed.
- Visual UI appears when it materially improves understanding, consent, approval, comparison, provenance, recovery or audit.

## Product boundaries

Floe is not an agent-orchestration framework, workflow builder, ChatGPT wrapper, health dashboard, productivity dashboard or Apple-only product.

Internal distinctions remain explicit:

- Account ≠ Person
- Skill/Tool ≠ Expert
- Expert ≠ Manager
- Integration ≠ Automation
- Connector ≠ model route
- Memory ≠ instruction
- Intelligence ≠ authority
- speaker recognition ≠ authentication
- UI projection ≠ underlying domain model

Apple platforms are the current implementation priority, but long-term experience parity includes Windows and Android.

## Read next

- [Product principles](principles.md)
- [Experience model](experience.md)
- [Personal domains](domains.md)
- [Manager, Experts and extensions](intelligence.md)
- [Integrations, privacy and distribution](integrations-and-privacy.md)
- [Capability roadmap](roadmap.md)

Implementation ownership belongs to [current architecture](../architecture/README.md). Active refactoring belongs to [Stage 3](../refactoring/stage-3.md); [Stage 2](../refactoring/stage-2.md) is complete/frozen. Historical planning bundles and slice plans live in Git history.
