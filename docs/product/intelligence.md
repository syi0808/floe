# Manager, Experts and extensions

## One Manager

The Manager owns the continuous user relationship, final synthesis and intervention timing. It chooses domain expertise relevant to the request, but deterministic host policy owns permissions, budgets and external effects.

The Manager may answer directly from sufficient already-admitted Conversation, Persona, Memory or product context, delegate, ask for clarification, defer or remain silent. Fresh source-backed domain acquisition and bounded domain judgment belong inside an admitted Expert Task reached through A2A delegation. An unavailable, disabled or unbound Expert is a limitation, not permission for a direct-source fallback.

## Shared reasoning, separate responsibilities

Manager, shipped Experts and Learner use one canonical Inference policy: Gateway/server Primary, device-local LLM Fallback. Manager and delegated Experts normally use everyday_assistance; Learner uses deep_work. Knowledge still owns Learner proposals, review semantics, prompt, budget and job lifecycle.

Fallback is only for valid planning-time Primary absence, including an inventory that truthfully declares the purpose unavailable. Credential/identity errors, invalid inventory, transport failure, timeout, cancellation and source-processing denial do not trigger silent local retry. Availability and execution must agree. Source-processing mismatch produces a source/connection review, not a model card or a local authorization bypass.

The user relates to Floe and product purpose, not a provider/model/profile. Ordinary reasoning is not a per-model approval event, and Learner has no separate background model-recipient grant.

Health privacy transformation is not Agent reasoning: the source must first produce a typed, still-HighlySensitive WellbeingView with valid local-transform evidence. Only that View can be used by either Gateway Primary or local reasoning Fallback, subject to its source processing policy. The two local-model roles have separate contracts and invocations.

These are accepted product requirements under [ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md); [current architecture](../architecture/README.md) distinguishes implemented behavior from pending cutovers. Primary and Fallback qualification are evaluated separately after the cutover, without weakening the factual-support rule or treating unavailable evidence as success.

## Experts are domain judgment agents

Experts are not connector wrappers or provider-specific Tools. They receive bounded context and apply an independent domain perspective through the A2A Task lifecycle.

Experts declare semantic source needs and users configure compatible source choices in Assistant feature settings. A required source may remain unconfigured without hiding the feature; a durable source review links to those settings instead of pretending a permission grant is missing. Context acquires only the Task's selected targets, and Access authorizes the actual consumer against current source authority. A blocked selected source produces a typed no-conclusion result and durable review interaction, never a fallback to another connected account. Source choice and source processing approval remain separate from the feature setting and operation authority; a concrete model recipient is not another user approval identity.

Representative judgment domains are:

| Domain | Perspective |
|---|---|
| Schedule | conflicts, available time, calendar constraints and realistic plan changes |
| Commitments | promises, deadlines, expected replies and missing follow-up |
| Communication | response need, summary, draft/tone and supported channel |
| Relationships | identity, interaction context and important follow-up |
| Focus & Attention | interruption cost, focus protection and context-switch pressure |
| Wellbeing | non-diagnostic recovery/capacity implications |
| Work Context | projects, decisions, documents, blockers and next actions |
| Life Logistics | reservations, travel, delivery, errands and supported home context |

This table is a semantic domain map, not a requirement to run eight models or eight processes.

Tools/Skills are executable capabilities. Experts decide within a domain and may use granted Tools/Playbooks. Manager-to-Expert collaboration is delegation, not Tool execution.

## Bounded execution

The host supplies and enforces:

- Expert identity/revision and assignment eligibility;
- granted Views and capabilities;
- source-owned processing authority and required Health transform provenance;
- cancellation/deadline and token/cost limits;
- output bounds;
- Task lifecycle and stable identity.

An Expert cannot expand its own permissions or mutate authoritative Memory directly. External changes still require explicit operation authority and durable recovery.

## Extensibility

Built-in, user-created and future marketplace Experts should share the same semantic concepts where practical:

- versioned Agent Card/discovery description;
- Message / Task / Artifact lifecycle;
- required Views/capabilities;
- private state namespace;
- typed evidence/proposal references;
- permission declaration.

Third-party Experts are separate authorization principals. They receive no implicit database, credentials, unrestricted network, filesystem or other Experts' state.

Code-based third-party execution may use a sandbox on supported desktop/server hosts; mobile does not assume arbitrary third-party native code. Host-rendered settings/review surfaces preserve Floe's design system rather than allowing arbitrary plugin UI.

The current in-process A2A binding is an implementation placement; future remote bindings should not require Manager semantics to change.
