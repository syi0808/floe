---
version: alpha
name: Floe — Quiet Current
description: Calm temporal clarity, softened by glacial light and precise motion.
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

Floe should feel like a precise personal calendar lit by soft glacial daylight: calm, spacious, familiar, and quietly capable.

The primary application metaphor is **time**, not cards and not an AI dashboard. The main Day Canvas begins with a calendar time grid that a user can understand before learning anything about Floe. Tasks, notes, assistant proposals, and connected context are progressively layered onto that familiar temporal structure.

The visual character is **simple, modern, square-clean, and spacious**. Surfaces are mostly flat. Corners are controlled rather than bubbly. Strong hierarchy comes first from time, position, scale, alignment, whitespace, borders, and tonal contrast—not from a pile of cards, badges, shadows, hero panels, or category colors.

Floe is a personal assistant, not enterprise analytics software. The interface should make the user's day feel lighter. The current time and nearby schedule are obvious at a glance. Unscheduled work and notes remain available without competing with the calendar. Assistant intelligence should usually appear as a subtle proposal attached to the relevant moment rather than as a permanent AI section.

This file follows the [Google Labs DESIGN.md alpha format](https://github.com/google-labs-code/design.md/blob/main/docs/spec.md). YAML tokens are normative; the prose explains how and why to apply them.

## Controlling Product Rule

> **Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

This rule supersedes the earlier interpretation where a large `Now / Next` hero panel and a generic Event/Task/Note feed defined the main Day Canvas.

`Now / Next` remains important, but it is expressed through the time grid:

- current-time line;
- subtle current-event emphasis;
- optional compact navigation to the next off-screen event.

Do not duplicate the same information in a large summary hero above the calendar.

### Signature Motifs

- **Temporal Spine:** A clear calendar grid or chronological axis gives the product its primary structure. Time should organize the screen before containers do.
- **Quiet Current:** One continuous reading flow guides the eye. Avoid equal-weight dashboard grids and disconnected widget collections.
- **Glacial Light:** Restrained blue, aqua, or violet atmosphere is reserved for rare framing moments, onboarding, empty states, voice focus, or other non-routine surfaces. It is not the default Day Canvas background treatment.
- **Ghost Proposal:** Floe suggestions appear as lightweight proposed states attached to the time or object they affect. They should feel reversible before acceptance.
- **One Vivid Moment:** Most screens are neutral. One primary action, current state, or explicit focus may receive saturated color.

### Reference Interpretation

- Native calendar applications contribute immediate temporal legibility, clear event geometry, current-time affordances, and familiar navigation.
- Krepling's public reference contributes contrast, selective atmospheric gradients, and confident square geometry, but not its dashboard/product-panel composition.
- Letters' public reference contributes pale sky color, whitespace, approachable precision, and calm framing.
- Emil Kowalski's design-engineering principles contribute purposeful, responsive, interruptible micro-interactions. Motion is judged by frequency and function.
- Floe's product principles remain controlling: calm by default, calendar-first, progressive disclosure, and no scoreboards, streaks, or noisy dashboards.

## Colors

Neutral colors carry most of the interface. `neutral-50` is the default canvas and pure white is reserved for focused surfaces, calendar event fills, inputs, sheets, and raised overlays. `neutral-950`, not true black, is the strongest text color.

**Glacier Blue** is Floe's primary family. Use `primary-600` for the single highest-priority action, focus rings, selected navigation, the current-time marker when appropriate, and small moments of assistant presence. Do not tint every event or control blue.

**Aqua** expresses local processing, capture, continuity, and positive system presence. **Violet** is reserved for rare intelligence or memory moments. They must not become category colors for every Expert or data type.

Semantic families retain stable meanings:

- Success is green and confirms completion or a safe finished state.
- Warning is amber and signals overdue or attention-required states without alarm.
- Error is red and is limited to destructive actions, failures, and unsafe states.
- Event, Task, Note, and Floe proposal are never distinguished by color alone.

### Calendar Source Colors

Calendar source color is useful information and may be preserved in a restrained form.

Prefer:

- 2–3px leading accent;
- a very pale tint derived from the source color;
- explicit text/icon/source labels where necessary.

Avoid full-saturation event blocks across the entire calendar. Floe should remain calm even when several calendar sources are visible.

### Gradients

Gradients are environmental light, not structural application chrome.

- **Glacial Field:** `linear-gradient(135deg, #F3F6FF 0%, #D5F5F2 52%, #F8F4FF 100%)`.
- **Blue Hour:** `linear-gradient(145deg, #AAB9FF 0%, #78D8D2 48%, #C4A8FF 100%)`.
- **Deep Current:** `linear-gradient(145deg, #111317 0%, #20263B 55%, #303C75 100%)`.
- Never use gradient text, rainbow borders, gradient-filled body copy, or gradient primary buttons.
- The routine Day Canvas calendar grid should normally remain neutral, not sit inside a large gradient hero.
- A gradient may appear in onboarding, empty states, voice focus, or a rare non-routine framing surface where it does not compete with temporal information.

## Typography

Typography is modern and neutral, with enough geometric discipline to support the square-clean visual language. Use Inter for Latin text and Pretendard for Korean, followed by the platform system font.

- Display styles are compact and confident. Reserve them for dates, onboarding, major empty states, and marketing-level statements—not routine `Now` cards.
- Headlines use medium-to-semi-bold weight. Avoid extra-bold typography.
- Body text favors legibility and generous line height.
- Labels should be short, sentence case, and never broadly letter-spaced uppercase.
- Times and changing numeric values use tabular figures through the `numeric` token.
- Calendar event typography must stay legible in compact blocks without becoming visually dominant.
- Use no more than three type sizes and two weights in one compact product region.
- Korean line breaks must be reviewed manually; do not create orphaned particles or single-character final lines.

## Layout

The base rhythm is 4px, with 8px and 16px as the most common intervals. Large spatial breaks use 24, 32, 48, 64, or 96px. Avoid arbitrary one-off spacing unless optical alignment requires a documented exception.

### Application Frame

- Desktop minimum target remains approximately 760×560, but the full Day Canvas should make productive use of wider windows.
- The calendar grid is the dominant flexible region.
- A desktop Today rail, when visible, should generally stay compact at roughly 240–280px and never receive equal visual weight with the calendar.
- Do not constrain the full calendar-first Day Canvas to the former 680–780px reading-column width. That width remains appropriate for prose, settings, memory detail, and other reading surfaces.
- Desktop outer padding should normally be 20–32px. Internal calendar grid geometry may use smaller, regular gutters.
- Align the date/navigation header, all-day region, time grid, and capture affordance to a stable application frame.
- Prefer one continuous canvas over a dashboard of equally weighted cards.

### Calendar Geometry

- Time labels occupy a narrow, stable gutter.
- The current-time line crosses the meaningful schedule area and remains visible without overpowering event text.
- Event vertical geometry reflects actual duration when the selected zoom makes that practical.
- All-day events live in a dedicated strip instead of pretending to occupy `00:00–24:00`.
- Overlapping events use lanes or controlled overlap rather than destroying time geometry.
- Short events may use a compact representation, but their time relationship remains understandable.

### Today Rail

The Today rail exists only for context that does not naturally occupy a time slot.

Typical contents:

- unscheduled Tasks due or intended today;
- short Today Notes.

Do not put the following in the rail by default:

- productivity statistics;
- health metrics dashboard;
- per-Expert widgets;
- AI insight feed;
- duplicate calendar summary.

If the rail is empty, allow whitespace. Do not invent content to fill it.

### Whitespace

- Whitespace expresses hierarchy: 8–12px within a control, 16–24px within a component, and larger spatial breaks between independent regions.
- Calendar hour spacing is a functional zoom variable, not ordinary component padding.
- Dense days should first use zoom, overlap lanes, compact short-event treatment, folding, and progressive disclosure before reducing typography or touch targets.
- Empty time in the calendar is valuable information. Do not decorate every gap.

### Responsive Behavior

- Preserve experience parity, not identical composition.
- Narrow desktop/tablet hides or overlays the Today rail before shrinking the calendar below comfortable legibility.
- Mobile keeps the time/day mental model but may move Tasks and Notes into a sheet, secondary surface, or mode switch.
- Sheets replace centered dialogs on mobile where appropriate.
- Never introduce horizontal scrolling for the primary single-day mobile timeline.

## Elevation & Depth

Floe is primarily flat. Depth comes from tonal layers, precise borders, overlap, and local contrast.

- Calendar events use flat or nearly-flat surfaces. Avoid treating every event as a floating card.
- Default surfaces use a 1px `border` on white or no border on a contrasting canvas.
- Hover may strengthen a border or change the surface tone. It should not suddenly add a large shadow.
- Floating popovers use `0 8px 24px #11131714` with a 1px border.
- Modal sheets use `0 20px 60px #1113171F`. No ordinary calendar item uses this depth.
- Ghost Floe proposals should look proposed through border style, tint, iconography, or reduced material certainty—not through blur-heavy glassmorphism.
- Avoid glassmorphism as a default.

## Shapes

The shape language is **architectural softness**: mostly square geometry with just enough rounding to avoid feeling severe.

- Tiny controls, event accents, icon containers, and compact buttons use 4–6px radius.
- Inputs and ordinary surfaces use 10px radius.
- Sheets and rare large atmospheric surfaces use 14px radius.
- 20px is exceptional and limited to large onboarding or atmospheric surfaces.
- Pills are allowed only for status, tags, avatars, and segmented selection—not ordinary buttons, inputs, or events.
- Calendar event blocks should generally use smaller radii than onboarding/hero surfaces.
- Icons use a consistent 1.5–2px stroke and a simple geometric family.

The `hero-panel` token remains for onboarding, empty, promotional, or rare focused surfaces. **Do not use it as the recurring top section of Day Canvas.**

## Components

### Buttons

- Primary buttons are filled Glacier Blue. Limit each surface or dialog to one.
- Secondary buttons are white or transparent with a crisp border.
- Tertiary actions are text or icon controls with an explicit hover target of at least 36×36px desktop and 44×44px touch.
- Ripple effects are never used. Hover changes only surface, border, icon, or text color.
- Pressing an occasional pointer-driven action scales it to `0.97` for 120ms, then returns with the strong ease-out curve. High-frequency calendar navigation uses color feedback only.
- Disabled controls retain readable labels and lose saturation; do not communicate disabled state through opacity alone.

### Inputs and Universal Capture

- Inputs are clean white fields with a 1px border. Focus uses a 2px `focus` ring without changing layout size.
- Universal Capture is easy to reach but must not resemble a chat composer with a stream of assistant messages.
- Preserve original input and draft text on recoverable errors.
- Capture should minimize context switching. The long-term product does not require every capture to immediately complete a blocking Event/Task/Note classification dialog.
- Explicit classification is acceptable as an early trust-building PoC. The mature UX must support immediate classification, Floe suggestion acceptance, and deferred organization.
- Pending captures need a discoverable recovery/review path; `나중에` must not mean disappearing from the UI forever.

### Day Canvas

The Day Canvas is a **calendar view first**.

#### Current Time

- Use a clear current-time line and marker.
- Subtly emphasize the Event containing the current instant.
- Do not repeat the same information in a large `Now` hero card.

#### Next Event

- The next Event normally remains visible in the calendar itself.
- If it is outside the viewport, a compact sticky navigation affordance may show its time/title and scroll toward it.
- Do not reserve a permanent large `Next` panel.

#### Events

- Timed Events occupy time-space according to their interval.
- All-day Events use a dedicated all-day strip.
- Calendar source may appear as a thin accent and pale tint.
- Event metadata is progressively disclosed; the block should stay scannable.

#### Tasks

- A Task with a planned execution time may appear in the time grid as a light checkbox-oriented item.
- An unscheduled Today Task belongs in the Today rail or equivalent secondary surface.
- Deadline and planned execution time are separate semantics and must not be visually conflated.
- Completed Tasks recede or fold before calendar readability is sacrificed.

#### Notes

- Notes are normally quiet context, not equal timeline rows.
- A Today Note may live in the rail.
- A Note associated with an Event/Task/Person may appear as a compact annotation or affordance attached to that object.

#### Floe Proposals

- Floe has no permanent `AI Insights` region on the home canvas.
- Interventions attach to the time/object they affect.
- Proposed schedule changes use ghost blocks or other clearly reversible states.
- Accept converts a proposal into canonical state; dismiss removes it; details reveal rationale progressively.
- Raw Health, Memory, and Expert state are not exposed merely to prove that Floe used them.

### Dialogs, Sheets, and Popovers

- Dialogs enter from `scale(0.96)` and opacity 0, never `scale(0)`.
- Centered modals transform from their center. Popovers and menus transform from the edge closest to their trigger.
- Preserve the user's input and focus context after closing or reversing an interaction.
- Destructive confirmations name the affected object and action. Never rely on color alone.
- Frequent calendar editing should prefer lightweight inline/popover/sheet editing over a procession of modal dialogs.

### Feedback and Progress

- Prefer local progress on the affected control, event, task, or proposal over a global blocking spinner.
- A successful capture or event creation should settle into the appropriate surface with a short spatial transition.
- Toasts are for transient confirmation, not required reading. Errors that need action remain near the source.
- Loading placeholders preserve final geometry to prevent layout jumps.

## Motion System

Motion is a hierarchy of feedback, not a layer of spectacle.

**Decision rule:** first ask how frequently the interaction occurs. Keyboard-driven or 100-times-per-day actions do not animate. Frequent calendar navigation receives subtle color or direct-position feedback. Occasional dialogs, sheets, proposal acceptance, and toasts may use standard motion.

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

Animate transform and opacity where practical. Avoid expensive layout animation in dense calendar regions. Direct manipulation of an Event/Task may animate position because the movement itself communicates schedule change.

Respect reduced-motion settings. Remove translation, scaling, parallax, and spring movement while keeping short opacity or color transitions that preserve comprehension. Touch devices must not inherit hover-only motion.

## Accessibility

- Current time, Event duration, task completion, and proposal state must be available to screen readers without relying on geometry alone.
- Calendar source color never carries unique meaning by itself.
- Keyboard users can navigate dates, focus events/tasks, invoke capture, and return to the current time.
- Touch targets remain at least 44×44px where users directly interact even if visual event geometry is smaller; use accessible hit regions when necessary.
- Verify WCAG AA contrast, focus visibility, 200% text scaling, reduced motion, and Korean text wrapping.

## Do's and Don'ts

### Do

- Do let **time** create the primary hierarchy of Day Canvas.
- Do make the current time and nearby schedule immediately legible.
- Do keep Event, scheduled Task, unscheduled Task, and Note visually distinct by role.
- Do let empty calendar time remain empty.
- Do attach Floe suggestions to the moment or object they affect.
- Do use neutral space, alignment, hairlines, and typography before adding containers.
- Do use calendar source colors as restrained accents when they improve scanning.
- Do keep Universal Capture low-friction and recoverable.
- Do preserve explicit provenance and user control behind AI-derived structure.
- Do provide immediate press, focus, loading, success, and error feedback.

### Don't

- Don't use a large permanent `Now / Next` hero panel above Day Canvas.
- Don't render Event, Task, and Note as one equal-weight generic feed.
- Don't build a dense analytics dashboard, scorecard, streak system, badge wall, or chat-first home screen.
- Don't add a permanent `Floe Insights` or per-Expert dashboard to the home canvas.
- Don't make every event, task, note, and section a floating rounded card.
- Don't use excessive pills, bubbly 20px radii, heavy shadows, glass blur, or neon glow.
- Don't place a large atmospheric gradient behind routine calendar content.
- Don't force every captured thought through a blocking classification workflow forever.
- Don't animate keyboard actions, routine calendar navigation, or anything without a functional reason.
- Don't distinguish meaning by color alone or use low-contrast gray for required information.
- Don't copy native Calendar, Krepling, Letters, or any individual designer literally; use familiar temporal grammar and translate it through Floe's calm personal-assistant purpose.
