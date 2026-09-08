# S4 Calendar conversation app integration

Date: 2026-09-08. App integration with injected Calendar/model evidence; S4 remains
**0/14**.

## Implemented boundary

- `PersonalDayScreen` supplies the current loaded `DayQuery` and Calendar connection
  to the assistant without copying event bodies into Flutter conversation setup.
- On load, the controller reads the encrypted Expert overview and selects only a setup
  whose View, Tool/Expert installations and Person assignments are enabled and whose
  exact provider/calendar scope remains present in the current connection. No eligible
  setup keeps the existing, visibly labeled sample route.
- An eligible setup resumes or starts its isolated Calendar session. Briefing and focus
  requests use that session's immutable setup/provider, current day bounds and current
  connection revision. Focus proposals use one calendar already contained in the
  granted View and remain governed by the existing S3 review/action path.
- Calendar begin/poll/stop/release, retry, recovery, committed-message streaming and
  saved proposal presentation are owned by the controller. Changed or missing context
  fails before dispatch rather than switching to a sample or different provider.
- The panel labels connected Calendar state and evidence separately from synthetic
  samples. It intentionally exposes the two bounded prompt kinds supported by the
  native contract, not arbitrary free text.
- Foundation Models accepts Personal-class requests only when the trusted vault host
  constructs it with `SessionProtection::Encrypted`. Synthetic construction still
  rejects Personal, credential and device-only raw classes. Placement remains
  device-local and there is no remote fallback.

## Evidence

```text
cd apps/client
flutter test test/agent_calendar_controller_test.dart \
  test/agent_calendar_turn_test.dart \
  test/agent_calendar_session_test.dart \
  test/agent_controller_test.dart \
  test/agent_calendar_expert_controller_test.dart
21 passed; 0 failed

flutter analyze
No issues found

CARGO_INCREMENTAL=0 cargo test -p floe-ffi --lib local_model
6 passed; 0 failed

CARGO_INCREMENTAL=0 cargo test -p floe-ffi --lib \
  vault_host::tests::calendar_turns
2 passed; 0 failed

bash tools/validation/run-local-model-smoke.sh --availability
AppleIntelligenceNotEnabled; personal_data=false
```

The Flutter connected test injects an EventKit-shaped enabled setup, encrypted session
and model result. It verifies that the panel is no longer the sample composer, that the
current day is dispatched with explicit Foundation Models selection, and that committed
messages are presented. Rust verifies encrypted-only Personal admission and the bounded
Calendar host path.

## Limits and next work

No test in this checkpoint claims a real EventKit read or Apple Foundation Models
generation. Availability, signed-app key lifecycle, actual Personal prompt behavior,
injection/cancellation/deadline evaluation and outbound privacy capture still require
live validation. The bundled availability smoke was rechecked for this increment and
reported Apple Intelligence not enabled without personal data. The other S4 connectors
and S1/S3 acceptance gates also remain open, so no S4 acceptance criterion is promoted.
