# Floe selection controls

## Select and Dropdown — September 2026 prototype

`Select` is a controlled single value; `Dropdown` is an action menu, not another
value field. The requested “Dropbox” is interpreted as Dropdown, not a connection
to the Dropbox service. Both live in `prototypes/floe-ui/src/components/ui/Select.jsx`.
The Settings interaction preview is explicitly non-persistent and has no external
effects. Select also supports calendar destination review without exposing IDs.

- Keep the visible label, current choice, and only useful distinguishing context
  (such as account or read-only status). Internal values never become labels.
- Use shared continuous-corner geometry, neutral surfaces, a faint floating shadow,
  violet focus and an explicit selected check. Rows have 44px minimum targets.
- Open toward available space and animate from the trigger-facing edge: 160ms,
  0.97 scale and opacity, Floe ease-out. Press feedback is 120ms/0.985 scale.
  Dismiss immediately rather than delaying a user's next action. There is no spring
  overshoot, blur or row-motion on frequent keyboard navigation.
- Reduced motion removes entrance/press transforms. Forced colors retains system
  borders, focus and disabled distinctions.

This interpretation draws specifically on Emil Kowalski's
[Good vs Great Animations](https://emilkowal.ski/ui/good-vs-great-animations):
origin-aware dropdown motion and strong ease-out, adapted to Floe's existing timing
contract rather than importing another visual system.

### Semantics and keyboard contract

The labeled button owns a listbox (`Select`) or menu (`Dropdown`). DOM focus moves
to the popup; `aria-activedescendant` tracks the active row, distinct from committed
selection. Enter/Space opens and explicitly commits; Up/Down opens or moves;
Home/End jumps; prefix typing searches enabled labels, repeated letters cycle.
Disabled rows are skipped and cannot invoke callbacks. Escape cancels and restores
trigger focus; Tab closes and continues from the trigger in normal tab order.
Pointer/focus outside dismiss without stealing focus. Portals stay inside a parent
native dialog to preserve its modal accessibility boundary. Resize and scroll
reposition the popup; the active row scrolls into view.

Callers supply unique stable `value` keys and readable `label` strings in options
or items, with optional `description` and `disabled`. `onChange(value)` and
`onAction(value)` fire only on explicit activation, never on focus or dismissal.
List options are expected to remain stable while open; loading/searchable or
multi-select controls are not part of this prototype contract.

Validation commands: `node scripts/check-selection-controls.mjs`,
`pnpm check:components`, and `pnpm build`. The standalone assertions exercise the
actual navigation helpers and guard the semantic/motion contract in source;
browser interaction validation is recorded separately from these static checks.

Prototype reference: 2026-09-05. Flutter implementation added 2026-09-06.

`Checkbox` and `Radio` share a controlled native input and Floe visual layer. Checkbox
uses a softened squircle (rounded fallback); Radio retains circular group semantics.
White surfaces, violet-600 selection, restrained inset light and secondary descriptions
reuse existing tokens. State is indicated by a check/dot, not color alone.

Both accept checked, label, optional description, disabled, name, value and onChange;
onChange receives the next boolean. Checkbox additionally supports compact task rows.
Radio groups must share a name and a semantic fieldset/legend. No custom keyboard logic
replaces native arrow/Space behavior. Inputs stay focusable with a visible outer ring.
Rows have at least 48px targets; compact controls retain 44px targets. Description IDs
are unique and associated with aria-describedby. No nested interactive label content.

Motion follows [Emil Kowalski's practical animation principles](https://emilkowal.ski/ui/7-practical-animation-tips):
immediate feedback, subtle 0.97 press, no zero-scale entrance, and strong ease-out.
Floe uses existing ease-out: 100ms press, 140ms surface, 120ms opacity, 160ms mark.
CSS transitions reverse from their current values on rapid input; no queued keyframes,
delays, spring overshoot, layout animation or added motion dependency. Reduced motion
removes transitions/transforms. Hover is gated to fine pointers; forced colors use
system Canvas/Highlight/GrayText colors and retain focus outlines.

Applied to calendar scope, Today context task, reference task completion and subtasks
through the existing CheckControl adapter. Scope persistence and save/cancel behavior
are unchanged. Build/catalog checks are supplemented by browser interaction checks.

Validation: production build, 45 component contracts and 26 existing action assertions
pass. Browser scope dialog visually inspected; ArrowDown changes the radio selection,
Space toggles a checkbox, All disables included-source checkboxes, and clearing the
selected subset disables Save. Reduced-motion/forced-color styles are implemented;
OS-mode and narrow-viewport visual verification remain unperformed in this checkpoint.

## Flutter

`FloeCheckbox`, `FloeCheckboxTile` and `FloeRadioTile` share native Material input
semantics, Floe theme colors, 0.97/120ms press feedback, and an explicit focus border.
Native mark transitions remain Flutter's standard control animation rather than a
pixel-identical CSS animation. Reduced motion suppresses the added press scale.
Disabled surfaces use neutral50, borders neutral200 and marks neutral300; labels
use neutral500. Radio uses a violet surface with a white 4px-radius center.
Calendar scope is now an explicit native RadioGroup (All/Selected); included calendars
and task/subtask completion use the shared checkboxes. Existing selection/save/cancel
behavior is unchanged. Tests cover disabled colors, keyboard checkbox/radio operation,
disabled activation prevention, reduced-motion press, and calendar selection regression.

Flutter validation: 57 tests pass; analysis, macOS release build and strict deep
codesign verification pass. Live native visual review is not recorded in this run.
