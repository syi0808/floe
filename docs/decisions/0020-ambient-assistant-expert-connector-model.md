# ADR 0020: Ambient assistant, domain Experts and capability connectors

- **Status:** accepted
- **Date:** 2026-09-10
- **Amends:** ADR 0013, ADR 0014 and ADR 0018 product framing

## Context

The initial S4 design is presented through a user-invoked assistant panel and names
Experts and connectors close to the first Calendar/Gmail implementation. That is a
useful validation surface, but it can accidentally make Floe look like a chat UI
with service-specific agents.

The intended product is an ambient personal chief of staff. Most explicit
interaction should happen through voice. Floe may also understand supported device
and service changes in the background and decide whether useful assistance is
warranted without first being invoked. Visual UI is exceptional: it supplies
decision context, obtains consent or approval, supports recovery and exposes an
auditable record when speech alone is insufficient.

This requires stable boundaries that do not equate an Expert with a provider, a
connector with an application, or a generated insight with permission to interrupt.

## Decision

### One ambient Manager

The user encounters one Manager, not a collection of visible assistants. The
Manager owns conversation continuity, situation triage, Expert delegation,
cross-domain synthesis, interruption timing and the final user-facing response.

```text
supported device/service changes or user speech
→ bounded Context/Event fabric
→ situation and relevance evaluation
→ Manager delegation to domain Experts
→ evidence-backed insight or action proposal
→ silence, voice, notification, or approval/report UI
```

Voice is the primary long-term interaction channel, but it is not a separate
Manager or Expert. Text and the S4 assistant panel remain development, accessibility,
inspection and fallback surfaces over the same `AgentCommand`/event/session contract.

The Manager must be able to choose silence. An Expert result never grants permission
to contact the user. Intervention policy evaluates urgency, importance, confidence,
actionability, attention, interruption cost and user preferences before selecting a
delivery channel.

### Experts follow life domains, not providers

An Expert is a bounded domain-judgment agent. It consumes granted provider-neutral
Views, may use granted Tools and Playbooks, and returns an Artifact with typed
evidence, uncertainty and optional proposals. It does not own user-facing presence,
expand its grants, access credentials/raw stores or mutate external state directly.

The long-term built-in domain map is:

| Expert | Responsibility |
| --- | --- |
| Schedule & Feasibility | time, conflicts, free windows, travel constraints and realistic plan changes |
| Commitments | promises, deadlines, expected replies and follow-up gaps across sources |
| Communication | response need, summary, draft, tone and appropriate communication channel |
| Relationships | identity and evidence-backed interpersonal context, cadence and important follow-up |
| Focus & Attention | interruption cost, focus protection and context-switching pressure |
| Wellbeing | coarse non-diagnostic capacity, recovery and sustainable schedule implications |
| Work Context | projects, documents, decisions, meetings, blockers and next actions |
| Life Logistics | reservations, travel, deliveries, errands and supported home context/actions |

These are product-domain boundaries, not a requirement to run eight models or ship
eight packages at once. Closely related judgments may initially share an
implementation. Expert identities remain stable while provider adapters change.
Calendar, Gmail, Slack, Screen Time and HealthKit are therefore not Expert names.

S4 continues to validate the existing `floe.schedule` identity as the first built-in
Expert, with its product meaning widened toward Schedule & Feasibility. It adds a
minimal Commitments perspective over Gmail and a deterministic declarative fixture.
Communication and bounded Wellbeing/Attention projections may initially be composed
inside those two paths rather than being falsely presented as complete Experts.

### Connectors expose Observe and Act capabilities

Provider integrations expose versioned, provider-neutral capabilities and Views:

- **Observe:** bounded state or change evidence such as Calendar, Mail, Contacts,
  location/ETA/weather, health, attention, files, projects, travel or home state.
- **Act:** typed external mutations such as calendar change, message send, task
  completion, reservation change or device/home control.
- **Interact:** voice, notifications, lock screen, watch, car and visual surfaces are
  delivery/invocation providers, not ordinary data connectors.

Observe and Act grants are separate. A read grant never implies write authority.
Act capabilities always pass through Policy/Review/Validation/Executor. Every View
declares source, scope, freshness, retention, sensitivity and provenance; Experts
receive neither provider-native objects nor credentials.

Floe-native Task, Note, Memory, Review, Activity and AgentSession stores may
participate in the Context/Event fabric but are not labeled external connectors.

### UI is an escalation surface

The system opens or requests visual UI only when it materially improves safety or
comprehension, including:

- consequential external mutation requiring explicit approval;
- sensitive access or external-transfer consent;
- comparison of choices too complex or ambiguous for speech;
- evidence, provenance or permission inspection;
- connection, authentication or failure recovery;
- Activity/audit review and durable settings.

An ordinary insight does not open a panel or modal. Voice confirmation is acceptable
only when policy classifies the action as suitable for voice and the user gives an
unambiguous, transaction-bound response. Otherwise the Manager presents a concise
report and hands off to the existing approval UI.

## Consequences

- S4 still uses text and foreground Calendar flows to validate the semantic runtime;
  it does not claim ambient voice, background monitoring or proactive intervention,
  which remain S6, S7 and S9 delivery work.
- UI specifications treat the assistant panel as fallback/inspection and visual
  escalation rather than the product's primary home.
- New integrations extend common Views/capabilities instead of adding provider names
  to Expert prompts.
- Background observation requires explicit source permission, OS-compliant lifecycle,
  freshness and retention bounds. “Ambient” never means unrestricted surveillance.
- Silence and deferred delivery become first-class traceable Manager outcomes.

## References

- [Manager and Experts](../planning/03-intelligence/manager-and-experts.md)
- [Voice and Presence](../planning/01-experience/voice-and-presence.md)
- [Interventions](../planning/01-experience/interventions.md)
- [Initial Connector Set](../planning/05-integrations/initial-connector-set.md)
- [Vertical Slice Delivery](../planning/08-engineering/vertical-slice-delivery.md)
