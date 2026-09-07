# S4 sample assistant panel and cancellable transport

Date: 2026-09-07. Evidence: synthetic fixtures, macOS arm64.

## User-facing increment

Today exposes one quiet `Floe is here to help` entry when its gateway supports
the Agent transport. Opening it replaces the desktop contextual rail with a
conversation panel; a narrow layout opens a user-invoked bottom sheet instead.
Closing the surface or navigating away requests Stop. Completed messages remain
available when the conversation is reopened.

The panel explicitly says **Sample conversation** and explains that personal chat
is locked until secure storage is ready. Its composer selects from two fixed
sample questions; there is no free-text field, microphone or automatic collection.
Calendar, Gmail, Contacts, location, Health and Screen Time are not read by this
flow. No model provider or OAuth endpoint is contacted.

The panel shows committed user/assistant messages, collapsed sample-source evidence,
human-readable capability progress, Stop, retry and recoverable errors. It never
renders hidden reasoning or provider IDs. After a transport error, the primary
action reloads saved state; it does not silently replay the question. Interrupted
saved runs require explicit recovery before another question.

Retry uses the last recognized fixed prompt, including after controller restart.
New conversation preserves older records and selects the newly created session.
Resume selects the most recently **created** Person-scoped fixture session, not
an arbitrary session with the largest revision. Session inventory, search, delete
and compaction remain outside this increment.

## Native transport

`floe_core_agent_fixture_run` accepts a version-1 request containing Person,
session ID, the revision before the turn, and one operation:

| Operation | Behavior |
| --- | --- |
| `begin` with a fixed prompt | Reserve one cooperative run per native handle; an identical begin reattaches rather than dispatching again |
| `poll` with `after_sequence` | Return all committed/progress events after the cursor and a terminal session/failure when available |
| `stop` | Request cancellation; completion racing with Stop remains authoritative |
| `release` | Drop the completed run buffer; an unfinished run cannot be released |

The `(Person, session ID, starting revision)` tuple addresses the run. Wrong Person,
wrong revision, unknown fields and future cursors fail. Poll does not consume
events: a lost response can be requested again at the same cursor. The queue is
bounded to 64 events, and a second distinct run cannot replace an unreleased one.

The existing current-thread Tokio runtime drives cooperative work during native
calls. Poll awaits readiness for up to 2 ms, not an entire model call, and the Dart
controller polls at 80 ms while a run is active. A fixture-only 500 ms async delay
per model step makes progress and cancellation observable. These are incremental
**semantic events**, not provider token deltas or simulated character-by-character
typing. Final assistant text is still committed atomically with its outcome.

Closing the native handle cancels and drains its run for up to two seconds before
aborting an unresponsive task. An abrupt process loss can leave an active-turn
pointer; recovery preserves committed messages and does not replay a capability.
Recovery is blocked while the same native handle owns an unfinished run. As in
the initial foundation, multi-process ownership/lease enforcement is not delivered;
recovery assumes the old host has stopped. This remains a production-host gate.

The Dart controller deduplicates progress by cursor, checks session identity,
keeps Send disabled until native cleanup finishes, and stops/releases work even
when its presentation is disposed. Raw transport errors are not displayed.

## Design and accessibility evidence

- Shared squircle, button, selection, type and color primitives; all new UI copy
  participates in the existing localization pipeline.
- One saturated action per panel; source details stay collapsed by default.
- Fixed composer with a scrollable conversation at ordinary desktop heights;
  short or enlarged-text layouts scroll the whole panel instead of overflowing.
- Keyboard Send/Stop, Escape dismissal, composer focus restoration and live status
  semantics; motion follows existing reduced-motion controls.
- Widget validation at 320/390 pixels and 200% text scaling; Today entry checks at
  1280 and 390 pixels.
- Rendered/reviewed `apps/client/test/goldens/agent_panel.png`, using bundled
  Pretendard and icon fonts rather than the test framework's placeholder font.

The existing empty Calendar layout overflowed in the narrow integration test with
the placeholder font's artificial character widths. Running this suite with the
bundled production font passed; no unrelated Calendar layout changes were made.

## Reproduce

Recorded results:

- Rust workspace: 77 passing tests, including three new async C ABI tests.
- Flutter: 117 passing tests, including 13 new native-gateway/controller/widget tests.
- `flutter analyze`, formatting and whitespace checks pass.
- macOS Debug application builds with both Agent C ABI symbols embedded; no app
  launch, real Calendar collection or live model/connector acceptance is claimed.

```sh
cargo test --workspace
cargo build -p floe-ffi
cargo fmt --all -- --check
cd apps/client
flutter analyze
flutter test test/agent_fixture_gateway_test.dart test/agent_controller_test.dart test/agent_panel_test.dart
flutter test
flutter build macos --debug
```

The strict whole-workspace Clippy gate still has the two previously recorded
Calendar warnings (`too_many_arguments`, `collapsible_if`). The workspace also
passes Clippy with only those two lint categories excluded; no source suppressions
or unrelated Calendar edits are added.

## Acceptance and next work

S4 stays **0/14**, not Accepted. This implements the sample panel/progress/recovery
increment, not personal chat, actual generative streaming, the encrypted vault,
Expert/Connector registries, real model adapters, connected-source briefing or S3
proposal execution. S1/S3 live acceptance evidence is unchanged.

Next: validate the encrypted session/key-unavailable and exclusive-host boundary
before opening a free-text composer or using real personal context. Then extend
the same event path to the supported local/remote model and typed Expert/View
contracts without relaxing placement or action authority.
