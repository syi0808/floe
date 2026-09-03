# Accessibility and Motion

**Status:** release baseline

## Accessibility

Every supported platform must provide equivalent access, even when controls use native conventions.

- WCAG 2.2 AA contrast for text, focus, controls, and meaningful graphics.
- 44×44px touch targets and 36×36px minimum pointer targets.
- Complete keyboard operation with visible, logical focus order.
- Screen-reader names, roles, values, states, and change announcements.
- 200% text scaling without clipped required content or horizontal page scrolling.
- Meaning expressed through text, structure, or icon as well as color.
- High-contrast mode that strengthens borders and focus without adding visual noise.

The primary content precedes the contextual rail in semantic order. When the rail stacks visually, DOM/widget reading order must remain task-oriented and predictable.

## Keyboard model

- `Tab` moves between regions and controls; arrow keys move within segmented controls and menus.
- `Enter` or `Space` activates according to platform convention.
- `Escape` closes the topmost temporary surface and restores trigger focus.
- Calendar navigation exposes date and event context without requiring pointer drag.
- Global shortcuts never override text editing or assistive-technology commands.

## Motion

Motion must provide feedback, continuity, or state explanation. Routine keyboard movement and high-frequency list actions use immediate state changes.

| Interaction | Duration |
| --- | ---: |
| Press | 100–140ms |
| Color/border | 120–160ms |
| Tooltip/popover | 140–180ms |
| Segmented selection | 160–220ms |
| Dialog/sheet | 200–280ms |

Use strong ease-out for entry and exit. Animate transform and opacity rather than frequently changing layout dimensions. Transitions must reverse from their current state when interrupted.

Reduced-motion mode removes translation, parallax, scale, bounce, and spring. Short fades and color changes may remain when they clarify state. The Floe mascot never idles with perpetual motion.

## Validation checklist

- Test keyboard-only use and focus restoration.
- Test VoiceOver on Apple platforms and the corresponding screen reader elsewhere.
- Test 200% text and the platform's largest practical accessibility size.
- Test reduced motion, increased contrast, and color-vision simulations.
- Test 320px logical width, 760×560 desktop minimum, and a wide desktop split.
- Verify empty, loading, stale, offline, conflict, overdue, error, and destructive states.
