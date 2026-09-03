# September 2026 Mockup Reference Analysis

**Status:** non-normative design rationale

## Source role

Four shared mockups depicted a calendar overview, task detail, notes collection, and calendar suggestion modal. They helped expose a coherent visual direction, but they contain inconsistent navigation and assistant behavior. This document records the translation into Floe rather than treating images or generated specifications as canonical assets.

## Adopted

- Calm off-white canvas, dark navy-like ink, generous whitespace, and thin borders.
- Continuous, soft container silhouettes as the basis for a squircle-first system.
- Timeline as primary content with a narrower contextual rail.
- Task Detail hierarchy of summary, context, subtasks, related notes, and suggestion.
- Responsive note-card collection as a useful supporting view.
- Small, mouthless, two-eyed Floe mascot with limited contextual sizes.

## Adapted

- Violet is reserved for Floe, focus, selection, and primary interaction; user content categories use blue, mint, amber, coral, and neutral with redundant labels.
- Strong card gradients become flat white or step-50/100 tints.
- The desktop content/rail split becomes responsive stacking on narrow screens.
- `Calendar / Tasks / Notes` becomes `Today / Tasks / Notes` to preserve Day Canvas as the product center.
- Assistant suggestion visuals become one shared rail component rather than a floating special case.

## Rejected

- `Day / Week / Month` appearing in Notes or Task Detail.
- Multiple mascot speech bubbles floating over calendar or note content.
- An unsolicited centered modal covering the day.
- Sparkles on ordinary user notes.
- Full-card categorical tint as the only identifier.
- The sample dates, copy, task names, event names, counts, and pixel dimensions as product requirements.

## Open decisions

- Whether Week and Month ship in the first supported Day Canvas release.
- Whether the Tasks collection is a first-class destination during MVP or initially reached from Today.
- Exact breakpoints after real-content and localization testing.
- The final platform implementation of the continuous-corner path.

Until these decisions are tested, implementations must use shared primitives and preserve semantics rather than copy a mockup literally.
