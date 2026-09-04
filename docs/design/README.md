# Floe Design Specifications

**Status:** target specification  
**Last updated:** 2026-09-03

## Purpose

These documents turn Floe's product principles into implementable interface rules. They describe the intended product, not the current completion state of the Flutter client.

## Authority order

When documents disagree, use this order:

1. Accepted product, privacy, and action-authority principles in `docs/planning/`.
2. Normative tokens and system rules in [`DESIGN.md`](../../DESIGN.md).
3. Shared component and accessibility specifications in this directory.
4. Screen specifications in `screens/`.
5. Reference mockups and implementation snapshots.

Mockups are evidence of a useful direction, not a pixel contract. A screen may depart from them to preserve Floe semantics, accessibility, platform conventions, or responsive behavior.

## Documents

| Document | Scope |
| --- | --- |
| [Foundations](foundations.md) | Color, type, spacing, continuous-corner geometry, depth |
| [Components](components.md) | Shared controls, cards, timeline items, rails, and overlays |
| [Prototype component boundaries](prototype-components.md) | As-built React units, state/props/style ownership and reverse-spec catalog (not normative product requirements) |
| [Assistant and interventions](assistant-and-interventions.md) | Passive, suggestion, active, and confirmation states |
| [Accessibility and motion](accessibility-and-motion.md) | Input, focus, scaling, semantics, motion, validation |
| [Application shell](screens/application-shell.md) | Global navigation, local tools, desktop and narrow layout |
| [Day Canvas](screens/day-canvas.md) | Today timeline, Now/Next, tasks, notes, and contextual rail |
| [Notes](screens/notes.md) | Note collection and detail transitions |
| [Task Detail](screens/task-detail.md) | Task fields, subtasks, related context, and suggestions |
| [Reference analysis](reference-analysis.md) | What the September 2026 mockups contributed or did not decide |
| [Flutter visual parity](flutter-visual-parity.md) | Paired HTML/Flutter captures, measured geometry, reproduction, and remaining differences |

## Production UI renders

The following images translate the specifications into coherent desktop product screens. They are implementation references rather than pixel contracts; the written tokens, semantics, responsive rules, and accessibility requirements remain authoritative.

The interactive [`prototypes/floe-ui/`](../../prototypes/floe-ui/README.md) HTML implementation is now the primary visual validation surface. Use the PNG files below only as composition history and comparison material.

| Screen | Render | Governing specification |
| --- | --- | --- |
| Day Canvas | [day-canvas.png](renders/day-canvas.png) | [Day Canvas](screens/day-canvas.md) |
| Task Detail | [task-detail.png](renders/task-detail.png) | [Task Detail](screens/task-detail.md) |
| Notes | [notes.png](renders/notes.png) | [Notes](screens/notes.md) |
| Assistant confirmation | [assistant-confirmation.png](renders/assistant-confirmation.png) | [Assistant and interventions](assistant-and-interventions.md) |

## Product-wide invariants

- Day Canvas is the primary experience; collection views support it.
- Event, Task, Note, Commitment, and Intervention retain separate semantics.
- Floe proposes; deterministic policy and user confirmation govern mutations.
- Only one assistant entry and at most one surfaced suggestion compete for attention.
- Shared squircles are the default shape; per-screen radii are prohibited.
- Narrow layouts reorder and stack content rather than miniaturizing desktop UI.

## Implementation status

The Flutter client implements the shared squircle primitive, semantic tokens, responsive application shell, Day Canvas, Notes collection, and Task Detail baseline. Current domain data limits editing, note excerpts, subtasks, and live suggestions; those controls must stay explicit about unavailable behavior until their application services are connected.

The September 4 parity pass compares the actual HTML and Flutter widget renders at desktop and phone sizes. See the [comparison gallery](renders/flutter-parity/index.html); the preview fixture is isolated from the production database.
