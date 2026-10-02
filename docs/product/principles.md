# Product principles

## Calm by default

More available data should produce a simpler experience, not a denser dashboard.

- Prioritize Now and Next.
- Use progressive disclosure.
- Keep important information foregrounded and competing information quiet.
- Avoid scores, streaks, badge walls and gamified productivity.
- Treat empty space and silence as valid outcomes.

## Show the information needed for the decision

A review surface explains **what will change, where, when, and with what material effects**.

Keep user-facing identity, destination, date/time, side effects, limitations, expiry, conflict and uncertain outcomes when they affect the decision. Keep UUIDs, internal provider codes, execution IDs and raw protocol detail in inspectable technical details rather than the primary surface.

Creation, collection and reconciliation are distinct outcomes. Recovery tells the user what can safely happen next rather than exposing an internal state machine.

## One assistant, many Experts

The user relates to one Floe Manager. Experts contribute independent domain judgment; they do not independently compete for attention.

```text
Experts -> Manager -> User
```

Expert boundaries follow judgment domains, not providers.

## Context before repetition

Conversation is not a form for re-entering Timeline, State and Memory on every request. Floe should use appropriately authorized context that it already has while preserving scope, provenance and freshness.

## Voice first, UI when useful

Voice is the long-term primary conversational channel. Text uses the same assistant semantics as an accessible fallback. UI escalates when consent, consequential approval, complex comparison, provenance, recovery or audit benefits from a screen.

## Proactive, but quiet

Attention has a cost. A potential intervention is judged by importance, urgency, confidence, actionability, personal relevance, attention state and recent interruption burden. Silence and defer are first-class outcomes.

## Memory is inspectable

Durable memory must be source-backed where applicable and support inspection, correction, deletion and provenance. Inference does not silently become fact.

## Privacy is a product feature

A person should be able to understand:

- what Floe can access;
- where data is stored;
- whether a source's bounded View may be processed only on Device or through Floe Gateway;
- which Expert may consume a view;
- what external actions Floe may perform.

Gateway processing includes its configured downstream reasoning providers; the person reviews the source processing boundary, not a concrete model/profile. Derived data can remain highly sensitive even when raw data stays local.

## Sensitive processing is source-owned

Raw Health data stays on its owning device. Before Health enters reasoning, deterministic minimization is followed by a mandatory device-local semantic privacy transform producing a typed Wellbeing View. No available local transformer means Health-derived reasoning is unavailable, not permission to send raw data elsewhere or substitute the old deterministic semantic classifier. The result remains HighlySensitive and still needs the source's processing permission.

This mandatory local-LLM rule applies to Health/Wellbeing only. Other sensitive signals retain their own domain-specific minimization policies; do not prebuild a universal transform framework. Privacy transformation is a source operation, not an Agent Tool or reasoning fallback.

## Floe owns its core interfaces

Connector, Context/View, Memory, Action/Authority, Expert and product-wire contracts belong to Floe. Third-party ecosystems are adapted behind those boundaries rather than defining them.

## Experience parity, not API parity

Platforms may use different OS capabilities while preserving the same trust and assistant experience. Do not weaken an Apple/Android/Windows boundary merely to make APIs look identical.

## Open and self-hostable

The core stack should remain operable by users. Hosted Floe is a managed distribution of the same product direction, not a separate closed architecture.

## Intelligence is not authority

Models and Experts may understand, recommend and propose. Access and Actions own authorization and consequential execution. No prompt or model output grants itself more authority.

## Reasoning is purpose-based, not a model approval ceremony

Business domains express purpose, data requirements and constraints. The client does not choose or expose provider/model/profile/recipient; Gateway routing belongs to the server. Ordinary conversation does not require a separate model-use approval. Missing source processing permission is reviewed on the owning source/connection, never on a model card.

Manager, shipped Experts and Learner share Gateway-primary/local-fallback reasoning. Local fallback is eligible only for valid planning-time Primary absence, not credential, inventory, transport, timeout or authorization failures. In particular, it must not bypass source processing review.

These product requirements are defined by [ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md). They describe the accepted product contract, not implementation completion; current behavior and cutover evidence remain in architecture and the authoritative execution plan.
