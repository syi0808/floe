---
version: alpha
name: Floe — Quiet Current
description: Calm architectural clarity, softened by glacial light and precise motion.
colors:
  primary: "#4B64D8"
  on-primary: "#FFFFFF"
  primary-50: "#F3F6FF"
  primary-100: "#E7ECFF"
  primary-200: "#CED8FF"
  primary-300: "#AAB9FF"
  primary-400: "#7E94F4"
  primary-500: "#6078E8"
  primary-600: "#4B64D8"
  primary-700: "#3F50B5"
  primary-800: "#354493"
  primary-900: "#303C75"
  neutral-0: "#FFFFFF"
  neutral-50: "#F8F9FB"
  neutral-100: "#F1F3F6"
  neutral-200: "#E3E6EB"
  neutral-300: "#CCD1D9"
  neutral-400: "#9DA4AF"
  neutral-500: "#707884"
  neutral-600: "#505761"
  neutral-800: "#272B31"
  neutral-950: "#111317"
  aqua-50: "#EFFBFA"
  aqua-100: "#D5F5F2"
  aqua-200: "#AEEBE6"
  aqua-300: "#78D8D2"
  aqua-400: "#49BDB8"
  aqua-500: "#2F9D99"
  aqua-600: "#267D7A"
  aqua-700: "#246461"
  violet-50: "#F8F4FF"
  violet-100: "#EEE5FF"
  violet-200: "#DDCEFF"
  violet-300: "#C4A8FF"
  violet-400: "#A67BF6"
  violet-500: "#8B5DE0"
  violet-600: "#7045BB"
  violet-700: "#5A3895"
  success-50: "#F0FAF4"
  success-100: "#DDF4E5"
  success-200: "#BDE8CB"
  success-400: "#59B879"
  success-600: "#2D7D49"
  success-800: "#245D39"
  warning-50: "#FFF9EB"
  warning-100: "#FFF0C7"
  warning-200: "#FBD981"
  warning-400: "#DDAA36"
  warning-600: "#946215"
  warning-800: "#654313"
  error-50: "#FFF4F2"
  error-100: "#FFE2DD"
  error-200: "#FFC5BB"
  error-400: "#E97969"
  error-600: "#B8463A"
  error-700: "#9D3E34"
  error-800: "#7E342D"
  canvas: "{colors.neutral-50}"
  surface: "{colors.neutral-0}"
  surface-subtle: "{colors.neutral-100}"
  text-primary: "{colors.neutral-950}"
  text-secondary: "{colors.neutral-600}"
  text-tertiary: "{colors.neutral-500}"
  border: "{colors.neutral-200}"
  border-strong: "{colors.neutral-300}"
  focus: "{colors.primary-600}"
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
  xs: 4px
  sm: 6px
  md: 10px
  lg: 14px
  xl: 20px
  full: 9999px
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
    textColor: "{colors.on-primary}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.sm}"
    padding: 12px
    height: 40px
  button-primary-hover:
    backgroundColor: "{colors.primary-700}"
    textColor: "{colors.on-primary}"
  button-primary-pressed:
    backgroundColor: "{colors.primary-800}"
    textColor: "{colors.on-primary}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.sm}"
    padding: 12px
    height: 40px
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
    padding: 12px
    height: 44px
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: 20px
  hero-panel:
    backgroundColor: "{colors.primary-50}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
    padding: 24px
  status-chip:
    backgroundColor: "{colors.surface-subtle}"
    textColor: "{colors.text-secondary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.full}"
    padding: 8px
---

# Floe Design System: Quiet Current

## Overview

Floe should feel like a precise workspace lit by soft glacial daylight: calm, spacious, and quietly capable. It combines the architectural black-and-white structure and selective atmospheric gradients seen in Krepling with the airy blue confidence, generous whitespace, and clean product framing seen in Letters. The result must be recognizably Floe rather than a reproduction of either reference.

The visual character is **simple, modern, square-clean, and spacious**. Surfaces are mostly flat. Corners are controlled rather than bubbly. Strong hierarchy comes from scale, alignment, whitespace, borders, and tonal contrast—not from a pile of cards, badges, shadows, or colors.

Floe is a personal assistant, not enterprise analytics software. The interface should make the user's day feel lighter. `Now` and `Next` are obvious at a glance, while distant or secondary information recedes. Empty space is functional: it separates moments, reduces cognitive competition, and makes a calm default possible.

This file follows the [Google Labs DESIGN.md alpha format](https://github.com/google-labs-code/design.md/blob/main/docs/spec.md). YAML tokens are normative; the prose explains how and why to apply them.

### Signature Motifs

- **Quiet Current:** One clear vertical or horizontal flow guides the eye. Avoid equal-weight dashboard grids.
- **Glacial Light:** Large, low-contrast fields may use a restrained blue, aqua, or violet atmospheric gradient.
- **Dark Instrument:** A focused dark surface may appear for a high-attention mode such as voice capture or review, but never as decorative visual noise.
- **Structured Float:** Small product previews or context panels may overlap a gradient field with crisp borders and very soft shadow. They must still align to the underlying grid.
- **One Vivid Moment:** Most screens are neutral. One primary action or one focused state receives saturated color.

### Reference Interpretation

- [Krepling's public reference](https://getdesign.md/design-md/krepling?category=productivity-saas) contributes contrast, modular product framing, selective spectral gradients, and confident square geometry.
- [Letters' public reference](https://getdesign.md/design-md/letters?category=productivity-saas) contributes pale sky color, white space, approachable precision, and large soft fields around crisp product UI.
- [Emil Kowalski's design-engineering skill](https://emilkowal.ski/skill) contributes purposeful, responsive, interruptible micro-interactions. Motion is judged by frequency and function, never added merely because it looks impressive.
- Floe's existing product principles remain controlling: calm by default, Now/Next first, progressive disclosure, and no scoreboards, streaks, or noisy dashboards.

## Colors

Neutral colors carry most of the interface. `neutral-50` is the default canvas and pure white is reserved for focused surfaces, inputs, and raised panels. `neutral-950`, not true black, is the strongest text color.

**Glacier Blue** is Floe's primary family. Use `primary-600` for the single highest-priority action, focus rings, selected navigation, and small moments of assistant presence. Pale primary shades can fill broad surfaces. Do not tint every control blue.

**Aqua** expresses local processing, capture, continuity, and positive system presence. **Violet** is reserved for rare intelligence or memory moments. They may meet primary blue in atmospheric gradients, but should not become competing category colors.

Semantic families retain stable meanings:

- Success is green and confirms completion or a safe finished state.
- Warning is amber and signals overdue or attention-required states without alarm.
- Error is red and is limited to destructive actions, failures, and unsafe states.
- Event, Task, and Note are never distinguished by color alone; use label, icon, shape, and language.

### Gradients

Gradients are environmental light, not decoration applied to every component.

- **Glacial Field:** `linear-gradient(135deg, #F3F6FF 0%, #D5F5F2 52%, #F8F4FF 100%)`.
- **Blue Hour:** `linear-gradient(145deg, #AAB9FF 0%, #78D8D2 48%, #C4A8FF 100%)`.
- **Deep Current:** `linear-gradient(145deg, #111317 0%, #20263B 55%, #303C75 100%)`.
- Keep saturation low behind text. Text over a gradient must have a stable local contrast of at least WCAG AA.
- Never use gradient text, rainbow borders, gradient-filled body copy, or gradient primary buttons.
- At most one meaningful gradient field is visible in a standard application viewport.

## Typography

Typography is modern and neutral, with enough geometric discipline to support the square-clean visual language. Use Inter for Latin text and Pretendard for Korean, followed by the platform system font. Ship the selected fonts or use the native system stack consistently; do not silently mix unrelated sans-serifs.

- Display styles are compact and confident, with tight tracking. Reserve them for dates, major empty states, and marketing-level statements.
- Headlines use medium-to-semi-bold weight. Avoid extra-bold typography, which makes the interface feel promotional.
- Body text favors legibility and generous line height.
- Labels should be short, sentence case, and never broadly letter-spaced uppercase.
- Times and changing numeric values use tabular figures through the `numeric` token.
- Use no more than three type sizes and two weights in one compact product region.
- Korean line breaks must be reviewed manually; do not create orphaned particles or single-character final lines.

## Layout

The base rhythm is 4px, with 8px and 16px as the most common intervals. Large spatial breaks use 24, 32, 48, 64, or 96px. Avoid arbitrary one-off spacing unless optical alignment requires a documented exception.

### Application Frame

- Desktop minimum target: 760×560. Default content canvas: 960–1120px wide.
- Reading and Day Canvas columns should generally stay between 680px and 780px.
- Desktop side padding is 32px; reduce to 20px below 800px window width.
- Mobile side padding is 20px, with 16px allowed for dense timeline rows.
- Align titles, timeline content, and capture controls to the same dominant edge.
- Prefer one continuous canvas with deliberate sections over a dashboard of equally weighted cards.

### Whitespace

- Whitespace must express hierarchy: 8–12px within a control, 16–24px within a component, 32–48px between functional sections, and 64–96px between major narrative regions.
- Empty states may occupy substantial space. Do not fill them with suggestions, statistics, or decorative widgets merely to avoid emptiness.
- Dense days should fold or progressively disclose secondary details before reducing the base rhythm.

### Responsive Behavior

- Preserve experience parity, not identical composition.
- On narrow screens, stack Now and Next rather than shrinking their text or padding.
- Sheets replace centered dialogs on mobile. Their information and action order remain identical.
- Never introduce horizontal scrolling for primary Day Canvas content.

## Elevation & Depth

Floe is primarily flat. Depth comes from tonal layers, precise borders, overlap, and local contrast.

- Default surfaces use a 1px `border` on white or no border on a contrasting canvas.
- Hover may strengthen a border to `border-strong` or change the surface tone. It should not suddenly add a large shadow.
- Floating popovers use `0 8px 24px #11131714` with a 1px border.
- Modal sheets use `0 20px 60px #1113171F`. No other standard surface may use this depth.
- Inner highlights may use a single 1px near-white edge on dark or gradient fields.
- Avoid glassmorphism as a default. Blur is allowed only when it clarifies overlay hierarchy and does not harm legibility or performance.

## Shapes

The shape language is **architectural softness**: mostly square geometry with just enough rounding to avoid feeling severe.

- Tiny controls, icon containers, and compact buttons use 4–6px radius.
- Inputs and ordinary cards use 10px radius.
- Hero fields and large sheets use 14px radius.
- 20px is exceptional and limited to large atmospheric or onboarding surfaces.
- Pills are allowed only for status, tags, avatars, and segmented selection—not ordinary buttons, inputs, or cards.
- Nested radii decrease toward the inside: a 14px outer panel contains 10px cards and 6px controls.
- Icons use a consistent 1.5–2px stroke and a simple geometric family. Filled icons indicate selection, not decoration.

## Components

### Buttons

- Primary buttons are filled Glacier Blue. Limit each surface or dialog to one.
- Secondary buttons are white or transparent with a crisp border.
- Tertiary actions are text or icon controls with an explicit hover target of at least 36×36px desktop and 44×44px touch.
- Ripple effects are never used. Hover changes only surface, border, icon, or text color; it never creates a spreading ink animation.
- Pressing an occasional pointer-driven action scales it to `0.97` for 120ms, then returns with the strong ease-out curve. High-frequency navigation uses color feedback only, and keyboard activation remains instant.
- Disabled controls retain readable labels and lose saturation; do not communicate disabled state through opacity alone.

### Inputs and Capture

- Inputs are clean white fields with a 1px border. Focus uses a 2px `focus` ring without changing layout size.
- Universal Capture is visually anchored and always easy to find, but it must not resemble a chat composer with a stream of assistant messages.
- Preserve draft text on recoverable errors. Submission feedback begins immediately.
- Classification starts unselected. Event, Task, and Note require explicit user choice until a separately approved trustworthy suggestion policy exists.

### Day Canvas

- `Now` is the strongest local element. `Next` is clear but quieter. Distant items recede through typography and spacing, not reduced accessibility contrast.
- Event, Task, and Note share alignment and rhythm while preserving semantic differences.
- Timeline rows are open list structures by default, separated with whitespace or hairlines rather than individual raised cards.
- Overdue state uses amber text plus explicit language or icon; never red alone.
- Conflicts are calm but unmistakable. Explain the conflict before offering an action.

### Dialogs, Sheets, and Popovers

- Dialogs enter from `scale(0.96)` and opacity 0, never `scale(0)`.
- Centered modals transform from their center. Popovers and menus transform from the edge closest to their trigger.
- Preserve the user's input and focus context after closing or reversing an interaction.
- Destructive confirmations name the affected object and action. Never rely on color alone.

### Feedback and Progress

- Prefer local progress on the affected control or row over a global blocking spinner.
- A successful capture should settle into the timeline with a short spatial transition; do not celebrate routine actions with confetti.
- Toasts are for transient confirmation, not required reading. Errors that need action remain near the source.
- Loading placeholders preserve the final geometry to prevent layout jumps.

### Motion System

Motion is a hierarchy of feedback, not a layer of spectacle.

**Decision rule:** first ask how frequently the interaction occurs. Keyboard-driven or 100-times-per-day actions do not animate. Frequent hover and navigation receive only subtle color or border feedback. Occasional dialogs, sheets, and toasts may use standard motion. Rare onboarding moments may carry restrained delight.

**Purpose rule:** every animation must provide feedback, preserve spatial continuity, explain a state change, or prevent a jarring replacement. If its only purpose is to look impressive, remove it.

**Timing:**

- Press feedback: 100–140ms.
- Hover, color, and border transitions: 120–160ms.
- Tooltip and compact popover: 140–180ms.
- Dropdown and segmented selection: 160–220ms.
- Dialog and sheet: 200–280ms.
- Routine product animations should remain below 300ms.

**Easing:**

- Enter and exit: strong ease-out, `cubic-bezier(0.23, 1, 0.32, 1)`; Flutter `Cubic(0.23, 1, 0.32, 1)`.
- Movement or morph already on screen: ease-in-out, `cubic-bezier(0.77, 0, 0.175, 1)`.
- Gesture-driven sheet: `cubic-bezier(0.32, 0.72, 0, 1)` or an interruptible low-bounce spring.
- Constant determinate motion only: linear.
- Never use ease-in for direct UI feedback.

Animate transform and opacity where practical. Avoid animating layout dimensions in frequently updated lists. Transitions should be interruptible and reverse naturally from their current state. Use springs only for gesture momentum or directly manipulated objects; keep bounce between 0.1 and 0.2 and avoid bounce in ordinary controls.

Respect reduced-motion settings. Remove translation, scaling, parallax, and spring movement while keeping short opacity or color transitions that preserve comprehension. Touch devices must not inherit hover-only motion.

## Do's and Don'ts

### Do

- Do make one thing obviously important and let everything else support it.
- Do use neutral space, alignment, and typography before adding containers.
- Do use one restrained atmospheric gradient to establish a signature moment.
- Do keep Event, Task, and Note semantically distinct without turning them into competing color categories.
- Do provide immediate press, focus, loading, success, and error feedback.
- Do preserve input during errors and make transitions interruptible.
- Do verify WCAG AA contrast, keyboard navigation, focus visibility, screen reader labels, 200% text scaling, and reduced motion.
- Do use the exact tokens in this file; create a reviewed token rather than an untracked color or spacing value.

### Don't

- Don't build a dense analytics dashboard, scorecard, streak system, badge wall, or chat-first home screen.
- Don't make every section a floating rounded card.
- Don't use excessive pills, bubbly 20px radii, heavy shadows, glass blur, or neon glow.
- Don't use gradients on buttons, borders, text, small controls, or more than one major field per viewport.
- Don't animate keyboard actions, routine list navigation, or anything without a functional reason.
- Don't animate from `scale(0)`, use slow ease-in feedback, or exceed 300ms for routine interaction.
- Don't distinguish meaning by color alone or use low-contrast gray for required information.
- Don't copy Krepling, Letters, or any individual designer literally; translate their strengths through Floe's calm personal-assistant purpose.
