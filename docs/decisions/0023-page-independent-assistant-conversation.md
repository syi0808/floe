# ADR 0023: Page-independent assistant conversation and request-scoped Views

- **Status:** accepted
- **Date:** 2026-09-10
- **Amends:** ADR 0014, ADR 0018 and ADR 0020 implementation boundaries

## Context

The first Schedule Expert path is mounted in the Day Canvas and currently creates a
Calendar-scoped session from the selected `DayQuery`. The client chooses an evidence
range before the Manager or Expert interprets the message, eagerly refreshes that
range and sends it through a Calendar-specific turn protocol.

This reverses the intended ownership. “오늘 일정” followed by “이번 주 일정” can keep
using the original day's evidence because the page, rather than the delegated domain
work, selected the query. Moving the same chat UI to another page would also remove
its Calendar capability. A wider prefetch window would hide some examples but would
retain the wrong boundary and still fail for other user-selected ranges.

## Decision

Floe has one Person-scoped assistant conversation. A page may mount its presentation,
but does not select the session type, enabled Experts, source provider or evidence
range.

```text
any presentation surface
→ AssistantConversation(person, message)
→ Manager
→ active domain Expert
→ request-scoped Observe capability(start, end, cursor)
→ connector
→ evidence-backed Expert artifact
→ Manager response
```

### Conversation and presentation

- `AgentSession` is Person-scoped and remains valid when its presentation surface
  changes. Calendar is not a conversation scope.
- The client sends conversation identity, message and inference consent only. It does
  not send a `DayQuery`, calendar selection or precomputed evidence interval.
- Page state may later be supplied as a separately typed, optional UI hint. A hint is
  non-authoritative context, cannot narrow source access and cannot replace a source
  read required by the request.
- Calendar setup and proposal review remain separate settings/action flows; they do
  not create a Calendar-flavored chat.

### Source authority and request scope

- An Observe grant identifies the Person, connector, selected source set, allowed
  data class and policy bounds. It does not identify the interval for every future
  turn.
- The Schedule Expert resolves the relevant interval from the current assignment and
  conversation context, then calls a range-aware Calendar Observe capability.
- The host validates each requested interval, item/byte/time budget, source selection
  and freshness. The connector reads that interval on demand and returns coverage
  metadata with the evidence.
- A later request whose evidence scope differs materially causes another Observe
  call. Prior evidence may be reused only when its verified coverage and freshness
  fully satisfy the new request.
- Empty, partial, stale, denied and unavailable results are distinct. An empty result
  is authoritative only for the returned coverage; an unavailable read must not be
  summarized as an empty calendar.
- Pagination or bounded chunking extends large valid ranges. A UI-selected day is not
  used as an implicit safety bound.

### Manager and Expert behavior

- The Manager delegates when domain evidence or judgment is needed and owns the final
  response. It does not describe internal routing or tool availability unless that
  information materially helps the user recover.
- Experts choose reads based on the assignment rather than the presentation surface.
  They preserve evidence scope and uncertainty in their artifacts.
- Host enforcement owns absolute security and resource limits. Prompt guidance stays
  general and outcome-oriented; it does not encode examples as exhaustive workflows.

## Migration

Migration is vertical to avoid silently removing Calendar behavior:

1. Add request-scoped Calendar Observe and coverage contracts.
2. Let the generic Person conversation discover and delegate to installed Experts.
3. Move connector refresh behind the Observe call.
4. Switch the client to the generic conversation on every surface.
5. Remove Calendar session/turn and Day Canvas context plumbing after compatibility
   tests and proposal review use the generic path.

The temporary multi-day Calendar turn support remains useful contract coverage, but
is not the target client architecture and must not be expanded with UI-side natural
language range parsing.

## Acceptance criteria

- The same conversation and installed Experts work when mounted outside Day Canvas.
- “오늘 일정” followed by “이번 주 일정” performs a new Calendar read whose verified
  coverage includes the requested week.
- Explicit past, future and multi-day ranges work within policy bounds without a page
  supplying dates.
- A missing permission, stale source, partial page and genuinely empty range produce
  distinguishable Expert evidence and user-facing outcomes.
- Tests fail if a new request is answered from evidence that does not cover its
  resolved interval.

## Consequences

- The current Calendar-specific session and turn DTOs become migration-only APIs.
- Connector reads move closer to capability execution; eager Day Canvas sync remains
  a canvas projection concern only.
- Source-scope grants can survive multiple turns, while evidence Views stay bounded,
  expiring and request-specific.
- S5.5 broadens this same contract to other providers and domains; it is not required
  to correct the conversation ownership boundary.

## References

- [Ambient assistant model](0020-ambient-assistant-expert-connector-model.md)
- [Generalizable agent guidance](0022-generalizable-agent-guidance.md)
- [Manager and Experts](../planning/03-intelligence/manager-and-experts.md)
- [Vertical Slice Delivery](../planning/08-engineering/vertical-slice-delivery.md)
