# Experience model

## Day Canvas

Day Canvas places Calendar, Tasks, Notes, commitments and Floe interventions on one calm view without collapsing their underlying semantics.

- Now and Next have the strongest visual priority.
- Distant or secondary information is quieter.
- Event, Task and Note remain distinct domain types.
- Floe suggestions use a contextual rail, reserved inline space or one relevant anchored control; they do not cover the timeline with speech bubbles.
- Empty days retain an interactive time canvas rather than becoming a blocking empty-state hero.
- The design source of truth is [DESIGN.md](../../DESIGN.md) and [Day Canvas screen specification](../design/screens/day-canvas.md).

### Calendar direct manipulation

User-authored Calendar intent begins at the calendar: toolbar create, empty-slot interaction, event drag/edit or explicit delete. Direct user edits still pass through the same deterministic authority, validation, idempotency and Activity audit boundaries as delegated actions; they do not require a second generic "AI proposal" ceremony when the user already made the decision.

## Conversation and presence

The long-term interaction hierarchy is:

```text
user invocation or justified report
  -> one Manager conversation
  -> optional Expert delegation
  -> spoken/text result
  -> visual handoff only when useful
```

Voice is not a separate assistant core. Wake detection, text chat, voice sessions and future device surfaces reuse the same Session/Run, Memory, Expert and Action boundaries.

Ambient does not mean continuous surveillance. Background understanding comes from explicitly permitted OS/provider changes and bounded context, not an assumption of always-on microphone, screen capture, precise-location history or raw activity collection.

Ordinary conversation does not ask the person to choose or approve a concrete model/provider/profile. Product policy selects a purpose; the client may explain Device or Floe Gateway processing when useful. The Gateway owns downstream routing. Internal terms such as server-model, selected model adapter and recipient IDs are not conversation UI.

## Interventions

An insight does not automatically become an interruption.

```text
Observe -> Understand -> Predict
                    |
                    v
          intervention decision
          /    |      |       \
      silence defer notify/report act
```

Manager owns final expression and interruption timing. Individual Experts do not send independent notifications.

Use the lowest sufficient presentation level:

1. silence/defer;
2. voice or quiet notification;
3. passive visual entry;
4. structured report/comparison;
5. explicit transaction-bound confirmation for consequential mutation.

## Review, authority and Activity

User-facing concepts are:

- **Review requests** — work that currently needs a person's decision.
- **Action permissions** — durable scoped rules such as allow / ask / deny.
- **Activity** — completed, blocked and unresolved work with an audit trail.

Internal Action intent and execution records remain durable safety mechanisms, not primary navigation concepts.

Intelligence may request work but does not provide its own authority, approval, provider permission, policy snapshot or execution receipt. Automatic authority can skip the human decision only when an explicit rule allows it; it never skips fresh validation, provider preconditions, idempotency or reconciliation.

Closing a review is neither approval nor rejection. An uncertain provider result leads to lookup/reconciliation rather than blind retry.

A source/configuration block is represented by an inline **Review request** bound to the original work. The original Run does not wait indefinitely for the person. A review card is separate from generated assistant output; no synthetic model-consent reply is stored as though a model answered. The person can choose **Not now** or a safe backend-projected action; neither dismissing nor leaving the card grants approval. Missing Gateway processing permission is reviewed for the source/connection, not for a concrete downstream model.

Once the origin's review group is terminal with at least one verified resolution, Floe durably requests automatic continuation. Crash/reopen, duplicate decisions and lost acknowledgements must converge on one linked follow-up without a routine **Continue** button. That follow-up is a fresh Run, revalidates source/grant authority and does not duplicate the original user text. Source or processing review is not an Action proposal and never authorizes a consequential effect.

Health's unavailable local privacy transformer is a source limitation, not an approval request. A Gateway transport failure is a reasoning-service failure, not permission to retry on a local model. Explain the affected capability and supported remainder without implying an observation or successful action that did not occur.

These are accepted experience requirements under [ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md), not a claim that the current client/runtime already implements them. Implementation order and evidence belong to the [active execution plan](../plans/2026-10-02-architecture-refactor.md).
