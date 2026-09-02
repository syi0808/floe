# Floe product brief

**Status:** working baseline  
**Last updated:** 2026-09-02

## Product thesis

Floe is a long-lived personal assistant that helps a person’s day flow well. It earns usefulness from context that persists across time—not from another chat window, a productivity score, or a collection of autonomous agents.

Its durable product assets are:

1. **Personal timeline** — what happened, is happening, and is planned.
2. **Personal state** — relevant current conditions such as availability, energy, and attention.
3. **Personal memory** — people, commitments, episodes, preferences, and their provenance.
4. **Integration fabric** — consented, reliable connections to real services and devices.

## Experience promise

- A calm Day Canvas prioritizes **Now** and **Next** over dense dashboards.
- Universal capture accepts a thought without demanding immediate classification.
- Conversation and voice are quick entry points, not the primary information architecture.
- Floe speaks first only when an intervention is important, timely, credible, actionable, personally relevant, and worth the interruption.
- A user can inspect, correct, or remove what Floe remembers and see where it came from.

## Extension promise

Users ultimately interact with one Manager Secretary, even when many Experts contribute advice. Expert is a public extension boundary: first-party, user-created, and Marketplace Experts use the same semantic trigger/view/output model where practical.

Third-party Experts are not trusted applications. They receive only approved semantic views and capabilities, keep isolated private state, and emit InsightCandidate, InterventionCandidate, ActionProposal, MemoryCandidate, or StateSuggestion records. They do not receive raw database access, connector credentials, unrestricted network/filesystem access, direct authoritative memory writes, or arbitrary Flutter UI execution.

## Initial integration boundary

Calendar, Mail, Contacts, and Health are the initial logical connector domains. Tasks and Notes are Floe-native; Voice, Location, Notifications, Secure Storage, and Invocation are Device Providers.

Retention is domain-specific: Calendar is mirrored, Mail is indexed with body access on demand, Contacts are identity references, and Health is derived-only. External source text is untrusted content and must never become product instructions or policy.

## Boundaries

Floe is not an agent-orchestration framework, no-code automation product, ChatGPT wrapper, health dashboard, productivity dashboard, or Apple-only product. Intelligence only proposes; deterministic policy, validation, permission checks, and confirmation govern external actions.

## Core product loops

1. **Plan the day:** events, tasks, notes, commitments, and interventions retain their own semantics but project into one Now/Next-oriented Day Canvas.
2. **Capture → structure → act:** typed or voiced input is preserved, interpreted into candidates, then updates the timeline/memory or produces an action proposal.
3. **Observe → understand → intervene:** state and connected data can inform internal or installed Experts; one Manager decides whether an interruption is worthwhile.
4. **Remember safely:** evidence, observations, claims, and inferences stay distinct; durable context has provenance and user controls.
5. **Propose → authorize → execute:** intelligence only proposes; deterministic policy, validation, permissions, and confirmation gate every external mutation.

## Evidence

This brief synthesizes the supplied planning bundle in `docs/planning/`, especially `00-overview/`, `03-intelligence/expert-extension-model.md`, `05-integrations/initial-connector-set.md`, `05-integrations/connector-data-policy.md`, `06-security/expert-permissions-and-sandbox.md`, and `10-ecosystem/`.