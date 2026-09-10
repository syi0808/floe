# S5.5 Android Context Foundation

> Date: 2026-09-11
> Acceptance status: Android Calendar/Contacts/Health Connect production code and build evidence;
> physical-device evidence pending

## Delivered boundary

- Added a native Android MethodChannel owned by the main Flutter activity. It exposes connection
  snapshots, explicit runtime permission requests, selected-calendar reads and contact identity
  reads. Unknown methods are not implemented and overlapping permission requests fail typed.
- Calendar reads use the public CalendarContract Instances provider, one adapter-owned allowlist of
  at most four calendar IDs, a maximum 32-day range, a maximum 128 items and bounded offset cursors.
  Queries execute on a single background worker and results return on the UI thread.
- Calendar projection emits only opaque evidence, bounded untrusted title, start/end and all-day
  state through the same strict `calendar.timeline` wire contract used by Google and Microsoft.
  Native calendar/event IDs, descriptions, locations, attendees and write authority are excluded.
- Contacts reads use the public ContactsContract provider with a maximum 64 identities. Projection
  emits display name, opaque identity/evidence handles and no email, phone, note, provider ID or
  mutation capability.
- Both adapters expose device-executed, Observe-only connector descriptors and typed pending,
  revoked/permission-denied, ready and stale lifecycle states. Last-success and View snapshots are
  published only after an actual successful read; cached Views remain in memory and disappear after
  their five-minute expiry.
- Health Connect uses the stable AndroidX 1.1 client and requests only Sleep, Steps and Exercise
  read permissions. It runs one bounded, deduplicated 36-hour read per record type on the existing
  worker and derives coarse capacity/recovery states locally. Raw records, counts, durations,
  measurements, notes, routes and provider identifiers never cross the native channel.
- The Health connector exposes derived-only retention and typed unsupported, permission-revoked,
  source-unavailable, ready and stale lifecycle states. Its View cache is memory-only and follows
  the same five-minute expiry boundary.
- Added a Dart gateway that binds calendar selection at adapter construction and strictly validates
  native View keys, versions, freshness, ranges, counts, duplicates and evidence handles before
  returning context to Flutter callers.
- The production Android entrypoint now injects the native gateway into Data & privacy. Android
  Calendar, Contacts and Health Connect lifecycle snapshots join the shared local/server connection
  inventory. Health consent and the first derived Wellbeing read happen only from an explicit
  settings button, after which the UI reloads the native lifecycle state.
- Android fixtures cross the shared Rust Calendar, People and connected-context validators. The
  Health fixture additionally crosses the shared Wellbeing validator. The Android debug APK
  compiles with the native provider code and permission declarations included. Health Connect
  raises the Android minimum SDK from 24 to 26, matching the library and bounded-provider floor.

## Automated evidence

```sh
flutter analyze lib/infrastructure/native/android_context_gateway.dart \
  test/infrastructure/native/android_context_gateway_test.dart
flutter test test/infrastructure/native/android_context_gateway_test.dart
flutter test test/features/server/settings_screen_test.dart \
  test/features/agent/agent_connections_test.dart
flutter build apk --debug
cargo test -p floe-agent --test calendar_context --test personal_context --test connected_context
```

## Remaining gate

No physical Android device permission/read evidence was captured, and the Android settings surface
does not yet list or persist selected calendar IDs. Health permission declarations and the in-app
permission-rationale destination still require release/store configuration and physical-device
validation. Therefore S5.5-C3 remains pending and the slice stays **0/14**.
