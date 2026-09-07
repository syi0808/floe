# S4 assistant access experience feedback

Date: 2026-09-08. App experience integration; S4 remains **0/14**.

## Changes

- Conversation storage creation and unlock now happen automatically when the assistant
  loads. The assistant panel no longer presents setup, unlock or manual lock controls.
- The encrypted vault still locks when the assistant closes or the app becomes inactive.
  An open assistant restores its conversation automatically when the app resumes.
- Expert installation, assignment and Calendar scope controls moved out of the
  conversation panel and into Settings under Assistant permissions.
- `FloeSwitch` replaces platform-adaptive switches for Expert and Calendar enablement.
  It uses Floe colors and motion, a responsive pressed thumb, hover/focus states,
  reduced-motion behavior, keyboard activation and switch semantics.
- The switch is included in the design-system catalog and covered by focused pointer,
  keyboard and semantics tests.

## Evidence and limits

Focused controller, Settings, registry, Calendar consent, lifecycle and switch tests
pass. Flutter analysis passes. Intentional Agent and design-system goldens were updated.
The complete Flutter suite otherwise passes; two unrelated pre-existing Agent
proposal/action-review golden comparisons still differ on this host. This experience
change does not open Personal model input or promote an S4 acceptance criterion.
