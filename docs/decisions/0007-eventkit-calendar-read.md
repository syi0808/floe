# ADR 0007: EventKit for the first connected Calendar slice

- Date: 2026-09-04
- Status: selected for S1 implementation; live read/create PoC pending

## Decision

Use macOS EventKit through a small device-native Swift adapter. Flutter transports
normalized records to typed Rust commands; Rust alone validates, reconciles,
persists, and projects the canonical mirror. No JavaScript runtime, credentials,
model, background agent, or external write operation is introduced.

EventKit is the preferred candidate because it exposes calendars already connected
to macOS without a separate OAuth service. This choice is not evidence that a real
account has passed the read/create PoC. The app builds against the actual SDK;
permission, provider behavior, and S3 creation still require explicit live evidence.

## Permission exception approved by the user

EventKit has no read-only authorization level. Reading requires full access on
macOS 14+, and legacy event access on older systems. The user explicitly approved
this exception on 2026-09-04: request the OS permission but keep Floe read-only.
The connection dialog and usage strings disclose the broader OS grant. The native
adapter exposes only calendar listing, range reads, and the system settings link.
There is no save, remove, or other external mutation entry point in S1.

This is an OS permission exception to the slice plan, not approval for S3 execution.
S3 must still implement its separate explicit approval and execution boundary.

## Mirror contract

- One selected calendar per Person. Identity is Person + provider + calendar ID.
- EventKit local item ID plus original recurrence occurrence date identifies an
  occurrence. SHA-256 of the normalized title/schedule and modification date is
  the external change token. These identifiers are device-local, not cross-device IDs.
- Swift expands occurrences in the requested interval. Timed records carry UTC
  instants and a timezone ID; all-day records carry an exclusive date interval.
- Rust accepts only a complete, validated overlapping-range result. Duplicate IDs,
  invalid intervals, blank identity, and out-of-range records reject the whole batch.
- A successful read removes missing records only inside that range. A later read
  of a moved occurrence replaces its previous cached position using its identity.
- Connection status and events are persisted in one compare-and-swap SQL write,
  so stale concurrent results cannot replace newer state. A failed fetch retains
  events and the last successful range/time. Selection changes invalidate old reads.
- Cached imported events live outside local CRUD tables; local edit/delete commands
  cannot mutate them. Selecting another calendar replaces only the old mirror.
- Reads are explicit: connect or refresh the selected date. Launching the app does
  not request permission; cached state is clearly labeled rather than called fresh.

## Known limits and live gate

See [S1 validation](../validation/s1-calendar.md). In particular, EventKit identifiers
may change when calendars/accounts are rebuilt or an event changes calendars.
Recurring exception identity needs live testing. Existing day queries use one fixed
offset per day; DST-transition-day correctness is not yet verified. S4 cannot reuse
these local identifiers as a cross-device identity scheme.

The embedded store remains unencrypted as in the Personal Day baseline. The adapter
does not import descriptions, attendees, locations, or credentials, and does not log
calendar contents. OS permission revocation stops new reads; it does not erase the
last local copy, which is required by S1-A4 and is disclosed as cached data.

## Sources

- [Apple: accessing the event store](https://developer.apple.com/documentation/eventkit/accessing-the-event-store)
- [Apple: requesting full access](https://developer.apple.com/documentation/eventkit/ekeventstore/requestfullaccesstoevents(completion:))
- [Apple: local calendar item identifier](https://developer.apple.com/documentation/eventkit/ekcalendaritem/calendaritemidentifier)
- [Apple: event identifier caveats](https://developer.apple.com/documentation/eventkit/ekevent/eventidentifier)
