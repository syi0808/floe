---
version: alpha
name: Floe — Calm Flow
description: A quiet, contextual personal workspace shaped by continuous squircles, soft light, and restrained violet.
colors:
  primary-50: "#F7F5FF"
  primary-100: "#EFEBFF"
  primary-200: "#DED6FF"
  primary-300: "#C4B5FD"
  primary-400: "#9B86F7"
  primary-500: "#7C63EE"
  primary-600: "#654BE0"
  primary-700: "#5138BF"
  primary-800: "#432F98"
  primary-900: "#372878"
  neutral-0: "#FFFFFF"
  neutral-25: "#FBFAFF"
  neutral-50: "#F8F7FB"
  neutral-100: "#F1EFF6"
  neutral-200: "#E8E6F0"
  neutral-300: "#D2CEDC"
  neutral-400: "#A8A2B4"
  neutral-500: "#777184"
  neutral-600: "#5B5668"
  neutral-700: "#403C4A"
  neutral-800: "#292632"
  neutral-900: "#1B1922"
  neutral-950: "#15182B"
  blue-50: "#F1F6FF"
  blue-100: "#E4EEFF"
  blue-300: "#A9C8FA"
  blue-500: "#3D86EC"
  blue-700: "#225EBC"
  blue-900: "#173B71"
  mint-50: "#F0FAF7"
  mint-100: "#DDF5ED"
  mint-300: "#8DDCC2"
  mint-500: "#43B590"
  mint-700: "#267B62"
  mint-900: "#194D3F"
  amber-50: "#FFF8ED"
  amber-100: "#FFEBCB"
  amber-300: "#F8C66B"
  amber-500: "#E99B20"
  amber-700: "#A55E0D"
  amber-900: "#663909"
  coral-50: "#FFF4F2"
  coral-100: "#FFE2DD"
  coral-300: "#F6A69C"
  coral-500: "#E7675D"
  coral-700: "#A63D37"
  coral-900: "#652A27"
  canvas: "{colors.neutral-25}"
  surface: "{colors.neutral-0}"
  surface-subtle: "{colors.neutral-50}"
  text-primary: "{colors.neutral-950}"
  text-secondary: "{colors.neutral-600}"
  text-tertiary: "{colors.neutral-500}"
  border: "{colors.neutral-200}"
  border-strong: "{colors.neutral-300}"
  focus: "{colors.primary-600}"
  success: "{colors.mint-700}"
  warning: "{colors.amber-700}"
  error: "{colors.coral-700}"
typography:
  display-lg:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 40px
    fontWeight: 650
    lineHeight: 1.12
    letterSpacing: -0.03em
  display-md:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 32px
    fontWeight: 650
    lineHeight: 1.16
    letterSpacing: -0.025em
  headline-lg:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 24px
    fontWeight: 620
    lineHeight: 1.25
    letterSpacing: -0.018em
  headline-md:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 20px
    fontWeight: 620
    lineHeight: 1.3
    letterSpacing: -0.012em
  body-lg:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: -0.006em
  body-md:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: -0.003em
  label-lg:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 14px
    fontWeight: 560
    lineHeight: 1.35
    letterSpacing: -0.002em
  label-md:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 12px
    fontWeight: 560
    lineHeight: 1.35
    letterSpacing: 0.01em
  numeric:
    fontFamily: Inter, Pretendard, system-ui, sans-serif
    fontSize: 13px
    fontWeight: 520
    lineHeight: 1.35
    letterSpacing: 0em
    fontFeature: "tnum"
rounded:
  none: 0px
  sq-xs: 8px
  sq-sm: 12px
  sq-md: 16px
  sq-lg: 20px
  sq-xl: 28px
  sq-frame: 32px
spacing:
  none: 0px
  xxs: 2px
  xs: 4px
  sm: 8px
  md: 12px
  base: 16px
  lg: 24px
  xl: 32px
  2xl: 48px
  3xl: 64px
  4xl: 96px
components:
  button-primary:
    backgroundColor: "{colors.primary-600}"
    textColor: "{colors.neutral-0}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.sq-sm}"
    padding: 12px 16px
    height: 44px
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    borderColor: "{colors.border}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.sq-sm}"
    padding: 12px 16px
    height: 44px
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    borderColor: "{colors.border}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sq-md}"
    padding: 12px 16px
    height: 48px
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    borderColor: "{colors.border}"
    rounded: "{rounded.sq-lg}"
    padding: 20px
  dialog:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    borderColor: "{colors.border}"
    rounded: "{rounded.sq-xl}"
    padding: 28px
  focus-ring:
    color: "{colors.focus}"
    width: 2px
    offset: 2px
---

# Floe Design System: Calm Flow

## Status and authority

This file is Floe's normative design-system entry point. The token block is the target contract; prose defines usage. Detailed specifications live in [`docs/design/`](docs/design/README.md).

The September 2026 mockups and their accompanying design discussion are **reference material, not a source of truth**. Floe's accepted product principles, domain semantics, accessibility requirements, and tested implementation behavior take precedence. The current Flutter client predates parts of this specification; differences are migration work, not permission to create more one-off styles.

## Experience character

Floe is **calm, contextual, and unobtrusive**. The user's calendar, task, or note is always the protagonist. Floe remains available without constantly speaking.

Five rules control the visual system:

1. **Content first.** Now and Next lead; assistant presence supports them.
2. **Squircle first.** Shared continuous-corner geometry unifies frames, cards, controls, events, fields, and overlays.
3. **Soft flat surfaces.** Hierarchy comes from spacing, type, borders, and faint tint before shadow.
4. **Purple has a job.** Violet identifies Floe, primary actions, focus, and selection—not arbitrary content categories.
5. **Quiet intervention.** Overview canvases do not accumulate mascots, speech bubbles, or unsolicited modals.

## Squircle language

A Floe squircle is a continuous-corner shape, not a conventional rounded rectangle with a large `border-radius`. Every standard container and control must use the shared `FloeSquircle` primitive or a platform-native continuous-corner equivalent.

- `sq-xs`: compact checkboxes, icon targets, and dense inline controls.
- `sq-sm`: buttons, segmented items, compact event blocks, and menu rows.
- `sq-md`: inputs, capture controls, and small cards.
- `sq-lg`: standard cards, right-rail surfaces, and note cards.
- `sq-xl`: dialogs, sheets, assistant panels, and large feature surfaces.
- `sq-frame`: the outer desktop workspace when the operating-system window treatment does not already provide the frame.

Nested shapes step down at least one level. Do not specify per-screen corner values. Pills are exceptional: short status labels, progress tracks, and system-provided text selections may use them. Ordinary buttons, tabs, fields, filters, events, and cards do not.

The mascot is an organic brand mark and is not forced into a squircle container.

## Color roles

Neutral surfaces carry most of the product. `canvas` is the page ground, white is the focused surface, and `neutral-950` is the strongest ink. Broad tinted surfaces use only the `50` or `100` steps.

Violet is reserved for:

- Floe identity and assistant-originated content;
- the primary action on a surface;
- keyboard focus and selected navigation;
- explicit AI affordances such as the sparkle mark.

Blue, mint, amber, and coral may distinguish user content categories, but color is always paired with a label, icon, or position. Semantic state wins over category: error is coral, warning is amber, and success is mint.

Gradients are limited to the Floe mascot and one optional, low-contrast ambient field per viewport. Do not use gradients on body text, borders, ordinary cards, event blocks, note cards, or primary buttons.

## Typography and content

- Use Inter for Latin, Pretendard for Korean, then the platform system stack.
- Keep one compact region to at most three type sizes and two weights.
- Use sentence case. Avoid broadly tracked uppercase labels.
- Use tabular figures for times and changing numeric values.
- Prefer direct, calm copy. Never manufacture urgency or praise routine completion.
- Review Korean line breaks manually and preserve user-authored text exactly where meaning matters.

## Layout hierarchy

Desktop uses a stable application shell: brand and global destinations in the header, a screen-specific toolbar below it, primary content on the left, and an optional contextual rail on the right. The rail supports the current object; it never becomes an equal-weight dashboard column.

- Standard desktop split: primary content `minmax(0, 7fr)`, rail `minmax(280px, 3fr)`.
- Collapse the rail below the primary content before reducing readable spacing.
- Use 32px desktop side padding, 20px below 800px, and 16–20px on narrow touch layouts.
- `Day / Week / Month` is a Calendar or Day Canvas view control only. It never appears as global navigation on Notes or Task Detail.
- The home destination is `Today`/Day Canvas. Tasks and Notes are supporting collections, not separate competing products.

## Floe presence

Floe has three presentation levels:

1. **Passive:** one persistent assistant entry in the contextual rail or platform-appropriate command surface.
2. **Suggestion:** one structured `Floe suggests` card in the contextual rail, or one Floe-icon squircle button anchored to a relevant time block. Activating a timeline button opens one anchored suggestion popover.
3. **Active:** a panel, sheet, or confirmation dialog opened by the user or by accepting a suggestion.

Do not place conversational bubbles over calendar events or note cards. A timeline may use exactly one icon-only Floe squircle button, slightly overlapping the relevant time block; it replaces the passive rail entry while visible. Do not automatically cover primary content with a modal. A suggestion can lead to an action confirmation, but intelligence only proposes and the user remains in control.

## Surface and depth

- Default surfaces use a 1px border or tonal separation, not both plus a shadow.
- Hover strengthens the border or adjusts the surface tone.
- Floating popovers may use `0 8px 24px #15182B14`.
- Dialogs and sheets may use `0 20px 60px #15182B1F`.
- Large shadows, glass blur, neon glow, and stacked translucent panels are not Floe patterns.

## Interaction and motion

Motion provides feedback or spatial continuity. It is never decorative.

- Press feedback: 100–140ms.
- Color and border transitions: 120–160ms.
- Popovers: 140–180ms.
- Segmented selection: 160–220ms.
- Dialogs and sheets: 200–280ms.
- Enter/exit easing: `cubic-bezier(0.23, 1, 0.32, 1)`.

Frequent keyboard navigation does not animate. Reduced-motion mode removes translation, scale, parallax, and springs while retaining brief opacity or color changes that preserve comprehension.

## Accessibility baseline

- Meet WCAG 2.2 AA contrast for text, controls, focus, and meaningful graphics.
- Provide a visible 2px focus ring with 2px separation.
- Use at least 44×44px touch targets and 36×36px pointer targets.
- Preserve semantics and reading order when the rail stacks.
- Support keyboard operation, screen readers, reduced motion, high contrast, and 200% text scaling.
- Never communicate type, selection, completion, warning, or Floe authorship by color alone.

## Prohibited patterns

- Dense productivity dashboards, scores, streaks, or badge walls.
- Chat-first home screens or multiple competing assistant entry points.
- Floating mascot speech bubbles on overview content.
- Unsolicited centered assistant modals.
- Per-screen radii or direct `border-radius: 9999px` for ordinary controls.
- Full-card categorical gradients or saturated tinted card grids.
- Sparkle icons on non-Floe content.
- More than one saturated primary action in a surface.

## Detailed specifications

- [Design documentation map](docs/design/README.md)
- [Foundations](docs/design/foundations.md)
- [Components](docs/design/components.md)
- [Application shell](docs/design/screens/application-shell.md)
- [Day Canvas](docs/design/screens/day-canvas.md)
- [Notes](docs/design/screens/notes.md)
- [Task Detail](docs/design/screens/task-detail.md)
- [Assistant and interventions](docs/design/assistant-and-interventions.md)
- [Accessibility and motion](docs/design/accessibility-and-motion.md)
- [Reference analysis](docs/design/reference-analysis.md)
