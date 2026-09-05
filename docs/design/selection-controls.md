# Floe selection controls

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
