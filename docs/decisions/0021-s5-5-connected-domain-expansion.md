# ADR 0021: Add S5.5 connected domain expansion before voice

- **Status:** accepted
- **Date:** 2026-09-10
- **Amends:** ADR 0013 delivery sequence, ADR 0014 source placement and ADR 0020 rollout

## Context

S4 proves the bounded Manager/Expert/model/source contracts and S5 gives them
governed Memory and Playbooks. Moving directly from that foundation to voice would
make the primary interaction channel arrive before the assistant has enough domain
breadth to behave like a personal chief of staff. It would risk optimizing speech
around a Calendar demo instead of around connected personal context.

ADR 0020 defines long-term domain Experts and Observe/Act/Interact boundaries, but
does not provide a delivery gate for implementing the planned connector portfolio,
evaluating each Expert or proving cross-domain synthesis.

## Decision

Insert **S5.5 — Connected Context and Domain Expert Expansion** after S5 and before
S6. It productizes the source and Expert portfolio behind the stable S4/S5 contracts
before voice becomes the primary interaction.

```text
S4  semantic Manager/Expert/source foundation
 → S5 governed Memory, Persona and Playbooks
 → S5.5 connected domain Experts and connector portfolio
 → S6 voice mode
 → S7 wake
 → S8 cross-device/server
 → S9 background Situation and proactive intervention
```

S5.5 ships in independently testable waves rather than one provider-at-a-time UI:

1. **Personal context:** productize Calendar, Gmail, Contacts, location/ETA/weather,
   Apple Health, Screen Time feasibility and Floe Task/Note inputs.
2. **Provider parity:** Google Calendar, Microsoft Calendar/Mail, Android
   Calendar/Contacts and Health Connect on their valid native execution hosts.
3. **Work context:** selected Slack/Teams reads, file search/read and project-system
   adapters behind common Communication/File/Project Views.
4. **Life logistics:** reservation/travel/delivery evidence and at least one supported
   home-state/action adapter behind common Logistics Views.

The long-term built-in Expert map remains Schedule & Feasibility, Commitments,
Communication, Relationships, Focus & Attention, Wellbeing, Work Context and Life
Logistics. Each requires an evaluation scenario, bounded grants, typed evidence,
failure behavior and a demonstrated reason to exist as independent judgment rather
than merely a provider wrapper. A perspective that fails that test remains a Tool,
View or projection instead of becoming an Expert package.

S5.5 is accepted by logical-domain coverage and live evidence on each connector's
valid execution host. It does not require every provider variant to block the whole
slice when a common View already has a live implementation; incomplete variants stay
visible in the provider matrix. Conversely, a fixture cannot satisfy a live connector
criterion.

## Boundary with adjacent slices

- S4 retains its current acceptance criteria and required-source contract work; S5.5
  turns those foundations into a broader, evaluated product portfolio.
- S5 supplies confirmed Memory and Playbooks needed by Commitments, Relationships and
  Work Context. S5.5 cannot write around S5 governance.
- S6 depends on S5.5 Accepted so voice is evaluated against representative domains,
  degraded sources and approval handoff rather than Calendar alone.
- Native iOS/Android connectors may be validated locally in S5.5. Their cross-device
  delivery, convergence and arbitration remain S8.
- Connector changes may produce bounded events in S5.5, but autonomous background
  Situation detection, interruption timing and proactive delivery remain S9.

## Consequences

- S5.5 is a large slice, so its board reports Expert and connector criteria separately
  and preserves wave-level evidence.
- Connector breadth follows user scenarios and common Views, not a goal of maximizing
  integration count.
- Voice work starts later but against a materially useful assistant rather than a thin
  demonstration.
- Unsupported public API, entitlement, region or provider behavior is a valid recorded
  matrix result; private APIs and unbounded ingestion are not substitutes.

## References

- [Vertical Slice Delivery](../planning/08-engineering/vertical-slice-delivery.md)
- [Manager and Experts](../planning/03-intelligence/manager-and-experts.md)
- [Assistant Context Portfolio](../planning/05-integrations/assistant-context-portfolio.md)
- [Initial Connector Set](../planning/05-integrations/initial-connector-set.md)
- [Ambient assistant model](0020-ambient-assistant-expert-connector-model.md)
