# S1 scope and recovery — 2026-09-05

## Contract

Selected persists explicit IDs even when all current sources are individually selected.
All discovers new sources on refresh/date reads. Legacy connections default to Selected.
Cancel does not save. Missing sources retain cache and display an unavailable status.
Healthy source batches commit within one atomic CAS while failed sources retain cache.
Disconnect removes imported copies only; revision tombstones prevent late-read resurrection
across reconnect. External events and OS permission are not changed by disconnect.

Scope UI was implemented in the HTML prototype before Flutter. Prototype fixtures
`?state=partial`, `?dst=spring`, `?dst=fall` simulate partial recovery and 23/25-hour
elapsed timelines. These are not EventKit acceptance evidence.

## Evidence

- Rust workspace tests and Clippy with warnings denied pass. Source tests cover scope
  persistence/discovery, malformed/stale/incomplete batches, partial recovery/reopen,
  disconnect races, and ordinary EventKit legacy-key migration across date moves.
- Rust DST tests check 23/25-hour import/projection and exclusive next midnight.
- Flutter suite and analysis pass. FFI tests cover discovery and missing-source cache;
  method-channel tests assert civil-day endpoints and no automatic permission request.
- `TZ=America/Los_Angeles flutter test test/calendar_day_boundary_test.dart` passes:
  spring skip, repeated fall hour labels and final-day elapsed position are distinct.
- macOS release build passes. Prototype production build, 43 component contracts
  and 26 action reducer assertions pass. Browser fall fixture shows two 01:30 events
  with distinct GMT-7/GMT-8 labels in a 25-hour timeline.

## Remaining live gates

The [earlier isolated PoC](eventkit-live-poc.md) established ordinary timed-event
create/recovery/read/edit/delete, not full S1 acceptance. Its auto-inclusion observation
is a defect only for All mode under the clarified contract. New All discovery has
automated, not live new-calendar evidence. Controlled denial/revocation/reconnect,
recurrence exceptions, live DST normalization and provider-failure restart still need
a recorded run. Three-day dogfood cannot be established in one session. S1 is 0/4 verified.
