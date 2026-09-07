# S3 approved Calendar action

2026-09-06: [Native execution](../validation/s3-native-executor.md) binds the
Flutter review to the durable ledger. A later product decision replaces this
Calendar-specific surface with shared Review and Activity and makes runtime
Action Authority the authorization gate. All app builds include the trusted
executor.

This document retains the validated decision surface but no longer defines the
Calendar entry point. The labeled planning button is superseded by the Calendar
toolbar `+`, empty-slot double-click and event direct-manipulation contract in
[Calendar Direct Manipulation](../planning/01-experience/calendar-direct-manipulation.md).

## Decision-first review

Review is a decision surface, not a ledger inspector. Its first screen answers:
**what will change, where, when, and what will not change**. Show the event title,
human-readable destination, date/time, and the one-event scope. Keep guests,
alerts, recurrence and effects on existing events explicit. Reducing noise must
not reduce the user's understanding of what they authorize.

Flutter displays both endpoints in the device's local time. Full
dates avoid ambiguity for overnight events. The user neither enters nor reviews a
timezone, offset or UTC string. This is presentation only: approval retains the
original immutable UTC instants and internal scheduling metadata, never a reparsed
display string.

The planning dialog accepts `yyyy-MM-dd HH:mm` in device-local time. Destination,
title, start and end fields use a consistent 12px vertical gap; explanatory copy
and the submit action use the larger section spacing. Local input is strictly
validated before conversion to UTC at the FFI boundary.

Provider codes, Person/calendar/proposal/execution IDs, local approval timestamps
and external IDs belong in collapsed **Technical details**. Raw UTC timestamps and
timezone metadata stay internal rather than appearing even in this disclosure.
They remain selectable for support, but are not prerequisites for a decision.
Block reasons are translated into ordinary language outside that disclosure.
Unknown outcomes, permission/conflict failures and created-but-not-collected
states remain visible, alongside the correct lookup-only or read-only recovery.
Closing review remains neither consent nor rejection. Simplification does not
change expiry, fresh validation, write gating or duplicate suppression.

Today includes a quiet focus suggestion beside the timeline. Review opens the
existing accessible dialog shell with explicit destination (writable fixture
calendars only), title, local date/start/end and no guests/alerts.
Destination changes produce a new proposal revision before approval. Decline does
not create an event; closing the dialog is not approval or rejection.

Approval shows revalidation, create and re-import as separate steps. The created
event appears in Today only after successful re-import. Closing/reopening or
navigating during execution does not restart its request. All terminal and
ambiguous states remain accessible through the card; never rely only on a toast.

## Validation boundary

The simplified review is implemented in Flutter. Production decisions and state
remain bound to the Rust ledger. All app builds include Calendar create. Review the
surface with the Flutter preview and design-feedback mode. This presentation does not
advance the remaining live acceptance or dogfood gates recorded in
[S3 native validation](../validation/s3-native-executor.md).
