# S4 native Calendar turn dispatch

Date: 2026-09-08. Native/Core integration with synthetic fixtures; S4 remains **0/14**.

## Implemented boundary

- The versioned vault protocol now accepts a distinct `calendar_turn` job with an
  explicit session revision, bounded Calendar day/window, prompt kind, model choice
  and optional S3 destination.
- The owned vault worker resolves the encrypted Calendar session, durable Expert
  setup and View binding before constructing a short-lived Core grant from the
  current Calendar connection revision.
- Destination provider, calendar, revision and timezone are checked before model
  execution. Disconnected or mismatched sources fail closed.
- `deterministic_fixture` is restricted to Fixture Calendar data. `foundation_models`
  uses the existing native adapter and never silently falls back to the fixture.
- Streaming events remain pollable while the job runs. The job is only marked done
  after Core finishes Manager proposal preparation, and its response identifies the
  Person, session, setup, selected model and each proposal outcome.
- Dart now exposes a separate Calendar-turn streaming gateway with typed prompt,
  model, day/window and destination inputs. It validates Person/session/setup/model
  identity on every completed native response and rejects changed retry payloads.

## Evidence

```text
CARGO_INCREMENTAL=0 cargo test -p floe-ffi --lib
20 passed; 0 failed

cd apps/client
flutter test test/agent_calendar_turn_test.dart
2 passed; 0 failed
flutter analyze
No issues found
```

Focused coverage verifies a streamed synthetic focus turn, a single prepared action
across duplicate submit, completion after proposal preparation, invalid grant bounds
before Foundation Models dispatch, stale destination revision and disconnected source.

## Limits and next work

This does not yet connect the turn to the app controller or UI. Stop/deadline/key-loss
behavior still needs dedicated Calendar-turn tests beyond the shared worker/Core
coverage. The native
Foundation Models adapter continues to reject Personal input, so this change does not
open live EventKit data to model generation. Protected-key, live model/source and S1/S3
gates remain open, and no S4 acceptance criterion is promoted.

The full Flutter suite reached 186 passing tests but seven pre-existing Agent golden
comparisons differed by 0.01–0.02%. The new non-visual Calendar transport tests and
analysis pass; no golden was updated by this increment.
