# S4 assistant access experience feedback

Date: 2026-09-08. App experience integration; S4 remains **0/14**.

## Changes

- Conversation storage creation and unlock now happen automatically when the assistant
  loads. The assistant panel no longer presents setup, unlock or manual lock controls.
- The encrypted vault still locks when the assistant closes or the app becomes inactive.
  An open assistant restores its conversation automatically when the app resumes.
- Expert installation, assignment and Calendar scope controls moved out of the
  conversation panel and into Settings under Floe access.
- Assistant access is now presented directly in Settings rather than in nested dialogs.
  Internal package IDs, versions, source handles, registry counters and Calendar IDs are
  replaced with plain-language ability names and descriptions. One ability switch keeps
  its internal availability and conversation access settings in sync.
- `FloeSwitch` replaces platform-adaptive switches for Expert and Calendar enablement.
  It uses Floe colors and motion, a responsive pressed thumb, hover/focus states,
  reduced-motion behavior, keyboard activation and switch semantics.
- The switch is included in the design-system catalog and covered by focused pointer,
  keyboard and semantics tests.
- The crowded outlined assistant launcher is replaced by a roomier `FloeActionCard`
  with a clear title, supporting text and directional affordance.

## Evidence and limits

Focused controller, Settings, registry, Calendar consent, lifecycle and switch tests
pass. Flutter analysis passes. Intentional Agent and design-system goldens were updated.
The complete Flutter suite otherwise passes; one unrelated pre-existing action-review
golden comparison still differs on this host. This experience change does not open
Personal model input or promote an S4 acceptance criterion.
