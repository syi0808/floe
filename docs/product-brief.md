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

## Initial user and job

The first user is the product team through daily dogfooding. The product must nonetheless be understandable and installable by non-technical people.

When a user begins or moves through their day, Floe should help them answer:

- What matters now?
- What is next, and what needs preparation?
- What did I just capture, and where does it belong?
- Is there a commitment or conflict I am likely to miss?

## Experience promise

- A calm Day Canvas prioritizes **Now** and **Next** over dense dashboards.
- Universal capture accepts a thought without demanding immediate classification.
- Conversation and voice are quick entry points, not the primary information architecture.
- Floe speaks first only when an intervention is important, timely, credible, actionable, personally relevant, and worth the interruption.
- A user can inspect, correct, or remove what Floe remembers and see where it came from.

## Boundaries

Floe is not an agent-orchestration framework, no-code automation product, ChatGPT wrapper, health dashboard, productivity dashboard, or Apple-only product.

Internally, experts may analyze distinct domains. Externally, the user has one assistant. AI reasoning and execution authority are separate: every external mutation crosses a deterministic policy and confirmation boundary.

## Product principles

1. Calm by default; progressive disclosure and low cognitive load.
2. Context before conversation.
3. Proactive but quiet.
4. Privacy is a visible product feature; sensitive raw data is local where practical.
5. Core interfaces and domain models are Floe-owned and provider-neutral.
6. Hosted and self-hosted distributions should share the same core system.

## Core product loops

1. **Plan the day:** events, tasks, notes, commitments, and interventions retain their own semantics but project into one Now/Next-oriented Day Canvas.
2. **Capture → structure → act:** typed or voiced input is preserved, interpreted into candidates, then updates the timeline/memory or produces an action proposal.
3. **Observe → understand → intervene:** state and connected data can inform internal experts; one user-facing manager decides whether an interruption is worthwhile.
4. **Remember safely:** evidence, observations, claims, and inferences stay distinct; durable context has provenance and user controls.
5. **Propose → authorize → execute:** intelligence only proposes; deterministic policy, validation, permissions, and confirmation gate every external mutation.

## Risks to contain early

- The Day Canvas may not be materially better than separate tools; dogfooding must test it first.
- Incorrect proactive suggestions cause intervention fatigue and loss of trust.
- Voice, health data, memory extraction, identity resolution, sync, and connector maintenance are distinct high-risk systems—not features to bundle into the first proof.
- Durable personal context can be corrupted by transcription error, false inference, or hostile external content; provenance and review cannot be deferred.

## Evidence

This brief synthesizes the supplied planning bundle: `README.md`; `00-overview/product-vision.md`, `product-principles.md`, `product-boundaries.md`, and `roadmap.md`; `01-experience/day-canvas.md`, `capture-and-transcription.md`, and `interventions.md`; `02-domain/personal-memory.md`; `03-intelligence/manager-and-experts.md` and `skills-and-actions.md`; and `08-engineering/technical-risks.md`.
