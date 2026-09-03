# Floe product brief

**Status:** working baseline  
**Last updated:** 2026-09-03

## Product thesis

Floe is a long-lived personal assistant that helps a person’s day flow well. It earns usefulness from context that persists across time—not from another chat window, a productivity score, or a collection of autonomous agents.

Its durable product assets are:

1. **Personal timeline** — what happened, is happening, and is planned.
2. **Personal state** — relevant current conditions such as availability, energy, and attention.
3. **Personal memory** — people, commitments, episodes, preferences, and their provenance.
4. **Integration fabric** — consented, reliable connections to real services and devices.

## Primary experience

**Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

The default Day Canvas starts from a familiar day-calendar mental model rather than a summary dashboard.

- Timed Events define the primary spatial structure.
- The current time is visible directly in the calendar grid.
- Scheduled Tasks can live lightly inside the time grid.
- Unscheduled Today Tasks remain available in a compact secondary surface.
- Notes are quiet context: Today notes or annotations attached to relevant objects.
- Floe proposals attach to the time or object they affect instead of occupying a permanent AI dashboard.

`Now / Next` remains important, but it is navigation intelligence rather than a large hero component. Current-time position, current-event emphasis, and compact next-event navigation should make the day immediately understandable without repeating calendar information above the calendar.

## Experience promise

- A calm calendar-first Day Canvas lets a user understand **what is happening now, what is coming, and what still needs doing** at a glance.
- Calendar, Todo, and Notes coexist without becoming an equal-weight generic feed.
- Universal capture accepts a thought without demanding a permanent context switch; the original input is preserved even when structure is deferred.
- Conversation and voice are quick entry points, not the primary information architecture.
- Floe speaks first only when an intervention is important, timely, credible, actionable, personally relevant, and worth the interruption.
- A user can inspect, correct, or remove what Floe remembers and see where it came from.

## Extension promise

Users ultimately interact with one Manager Secretary, even when many Experts contribute advice. Expert is a public extension boundary: first-party, user-created, and Marketplace Experts use the same semantic trigger/view/output model where practical.

Third-party Experts are not trusted applications. They receive only approved semantic views and capabilities, keep isolated private state, and emit InsightCandidate, InterventionCandidate, ActionProposal, MemoryCandidate, or StateSuggestion records. They do not receive raw database access, connector credentials, unrestricted network/filesystem access, direct authoritative memory writes, or arbitrary Flutter UI execution.

Expert extensibility must not turn Day Canvas into a plugin dashboard. Expert outputs are mediated by the Manager and rendered through Floe-owned surfaces.

## Initial integration boundary

Calendar, Mail, Contacts, and Health are the initial logical connector domains. Tasks and Notes are Floe-native; Voice, Location, Notifications, Secure Storage, and Invocation are Device Providers.

Retention is domain-specific: Calendar is mirrored, Mail is indexed with body access on demand, Contacts are identity references, and Health is derived-only. External source text is untrusted content and must never become product instructions or policy.

Calendar is also the first external domain that materially completes the product hypothesis. Floe should eventually use direct provider connectors where appropriate while avoiding duplicate canonical ingestion through both provider APIs and OS calendar mirrors.

## Boundaries

Floe is not an agent-orchestration framework, no-code automation product, ChatGPT wrapper, health dashboard, productivity dashboard, generic agenda feed, or Apple-only product.

It is also not a permanent `Now / Next` summary dashboard. The calendar itself should answer those questions.

Intelligence only proposes; deterministic policy, validation, permission checks, and confirmation govern external actions.

## Core product loops

1. **Plan the day:** Events form the temporal canvas; scheduled Tasks, unscheduled Tasks, Notes, Commitments, and Floe proposals retain their own semantics while contributing to one day view.
2. **Capture → structure → act:** typed or voiced input is preserved, interpreted into candidates, then updates the timeline/memory or produces an action proposal. Structure may be immediate or deferred.
3. **Observe → understand → intervene:** state and connected data can inform internal or installed Experts; one Manager decides whether an interruption is worthwhile and attaches it to an appropriate surface.
4. **Remember safely:** evidence, observations, claims, and inferences stay distinct; durable context has provenance and user controls.
5. **Propose → authorize → execute:** intelligence only proposes; deterministic policy, validation, permissions, and confirmation gate every external mutation.

## Evidence

The detailed specification lives in `docs/planning/`. For the primary experience see `01-experience/day-canvas.md` and `DESIGN.md`. Extension and integration boundaries are defined under `03-intelligence/`, `05-integrations/`, `06-security/`, and `10-ecosystem/`.
