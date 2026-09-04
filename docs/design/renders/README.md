# Production UI Renders

**Created:** 2026-09-04
**Revised:** 2026-09-04
**Status:** implementation reference

These renders use the four September 2026 shared mockups as visual input and apply Floe's current design specifications. They show intended product UI rather than reproducing the source mockups.

All four screens share one canonical application shell, navigation geometry, content inset, typography, and continuous-corner squircle hierarchy. Detail and timeline views also share the same 70/30 grid. Timeline suggestions use one anchored Floe button and a non-modal popover rather than a detached modal or speech bubble.

- `day-canvas.png` — default Today state with timeline, contextual rail, and universal capture.
- `task-detail.png` — task content, subtasks, related note, and integrated suggestion.
- `notes.png` — supporting Notes collection with restrained categorical tint.
- `assistant-confirmation.png` — non-modal anchored proposal opened from the timeline Floe button.

The images communicate composition, hierarchy, density, and tone. They do not override tokens, copy rules, accessibility, localization, responsive behavior, or platform conventions in [`DESIGN.md`](../../../DESIGN.md) and [`docs/design/`](../README.md).
