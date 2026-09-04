# ADR 0008: Unified read of connected calendars

- Date: 2026-09-04
- Status: selection scope revised; native multi-selection implemented, broader recovery work pending
- Supersedes: ADR 0007's one-selected-calendar scope, not its EventKit/read-only boundary

## Decision

One Person connects macOS Calendar once and selects one or more calendars using
checkboxes in Connect. Floe combines the selected calendars, across accounts, in one
chronological day. There is no source selector above the timeline. This remains one connector,
not multiple provider adapters. S3 must still explicitly select a write destination.

Permission disclosure explains that macOS grants full access even though Floe only
reads selected calendars and never writes external events. Inclusion toggles limit
Floe's reads, not the OS permission. Newly discovered calendars are not automatically
selected. The user can cancel or revise the selection; at least one calendar is required.

## Native multi-selection delivery

The native client persists the selected calendar IDs and names, restores legacy
single-calendar connections, and reads all selected sources on explicit refresh.
Occurrences are reconciled by calendar ID plus external ID. Removing a selection
deletes only that calendar's cached events; retained sources keep their identities.
Selection changes invalidate in-flight reads. A read failure preserves the entire
previous cache rather than committing partial results. Per-source partial recovery,
automatic date reads, and disconnect below remain future work, not shipped behavior.

## Data and recovery contract

- Connection identity is Person + device + connector. Preserve account/calendar IDs
  and names on every event; occurrence identity includes the source calendar to prevent
  collisions across calendars. Never deduplicate unrelated events by title/time alone.
- Date navigation automatically reads the destination date with a loading indicator;
  explicit refresh remains available. Neither navigation nor launch requests permission.
  Known access/provider errors retain their recovery path rather than silently retrying.
  Every date read reads each selected source. Newly discovered calendars require
  explicit selection in Connect. Launch does not request permission.
- Reconcile complete, validated results per calendar and date range. A successful
  source read may update/remove only that source's overlapping records. Partial source
  failure retains its cache and last successful timestamp, without blocking other sources.
- A missing calendar/account is unavailable, not a successful empty result. Preserve
  its last saved events with a warning until recovered or the integration is disconnected.
- Retain per-source collection status/time. “No events” requires successful coverage
  of every included source for that date; incomplete coverage is unknown, not free time.
- Protect source-scoped commits against stale concurrent reads and disconnect races.
  Disconnect invalidates pending reads and removes only imported copies, never local items
  or external records. OS permission remains separately managed.
- Cached data and failures have contextual banners. Healthy Today has no status badge,
  source toolbar, permission footer, marketing heading, or prototype controls.
  Provenance/timezones remain accessible in event details; inventory lives in Connect.

## Delivery boundary

The HTML prototype demonstrates the unified composition with Work, Personal and
Product team fixtures. It does not implement native storage migration, per-source
partial-fetch reconciliation, discovery, or real permissions. Existing single-calendar
native tests remain historical evidence, not acceptance of this expanded contract.
Multi-selection tests now cover selection persistence, source-ID collisions,
retained identities, deselection, and all-or-nothing failure recovery.

Next native work must add per-source partial recovery, automatic date reads and
disconnection, then verify real-account discovery, revocation and disconnection
against S1-A1–A4. Fixture tests do not replace live acceptance evidence.
