# Assistant and Intervention Presentation

**Status:** normative presentation layer for the intervention model

## Principle

Floe is always available but not always speaking. Presentation follows the intervention budget in [`docs/planning/01-experience/interventions.md`](../planning/01-experience/interventions.md); the UI does not promote every generated insight.

## Presentation levels

### Passive

Show one persistent entry such as `Floe is here to help` in the contextual rail, command palette, or platform-appropriate assistant surface. It may include the 36px mascot and one short supporting line.

Passive presence never overlays content and does not animate to solicit attention.

### Suggestion

Show one structured `Floe suggests` card when an intervention passes policy. Required anatomy:

1. 32px mascot or Floe attribution;
2. concise proposition with relevant evidence or time context;
3. one clear primary response;
4. dismiss, snooze, or secondary response where applicable;
5. accessible reason or provenance when the suggestion depends on inferred context.

Suggestions do not mutate data. Accepting one may open confirmation or prepare an action proposal.

### Active

Open an assistant panel when the user invokes Floe or accepts a suggestion. Use a sheet on narrow layouts and a contextual panel on wide layouts. Conversation history is subordinate to the task at hand; the product does not become a full-screen chat by default.

### Confirmation

Before an external or consequential mutation, state the proposed action, target, time, and relevant side effects. The primary button names the action. `Cancel` or `Keep current` must remain available.

## Placement

- Wide Day Canvas: bottom region of the contextual rail, after tasks and notes.
- Wide Task Detail: first or second rail card when directly relevant to the task.
- Notes collection: no suggestion inside individual cards; invoke Floe after selection or in detail.
- Narrow layout: a reserved inline suggestion after Now/Next or object summary, never between title and required controls.

## Attention rules

- At most one proactive suggestion is expanded in a viewport.
- Passive entry and suggestion collapse into one component; do not show both.
- Do not use speech bubbles, bouncing mascots, unread badges, or pulsing sparkle.
- Do not open a modal merely because a suggestion exists.
- Dismissal removes the suggestion without leaving an empty card shell.
- Repeated dismissals feed intervention policy rather than stronger visual pressure.

## States

Every suggestion supports loading, ready, stale, acting, success, recoverable failure, and dismissed states. Stale suggestions explain why they can no longer apply. Failures remain local and preserve the user's intended action for retry.

## Copy pattern

Use observable context before advice: `Team retro starts at 3:15 PM. Review the launch brief first?` Avoid personification that implies hidden authority, certainty, or emotion. Distinguish facts, inference, and recommendation in wording.
