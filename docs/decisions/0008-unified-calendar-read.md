# ADR 0008: Unified read of connected calendars

- Date: 2026-09-04
- Status: accepted product/design scope; native migration pending
- Supersedes: ADR 0007's one-selected-calendar scope, not its EventKit/read-only boundary

## Decision

One Person connects macOS Calendar once. Floe includes every calendar available to
that connector, across accounts, in one chronological day. There is no single-calendar
picker, switch action, or source selector above the timeline. This remains one connector,
not multiple provider adapters. S3 must still explicitly select a write destination.

Permission disclosure explains the full scope before the OS handoff: all available
calendars are read, including calendars discovered on later date changes or refreshes.
The user can decline or disconnect the entire integration. S1 exposes no external writes.
Per-calendar inclusion toggles are out of scope; do not suggest selective consent.

## Data and recovery contract

- Connection identity is Person + device + connector. Preserve account/calendar IDs
  and names on every event; occurrence identity includes the source calendar to prevent
  collisions across calendars. Never deduplicate unrelated events by title/time alone.
- Date navigation automatically reads the destination date with a loading indicator;
  explicit refresh remains available. Neither navigation nor launch requests permission.
  Known access/provider errors retain their recovery path rather than silently retrying.
  Every date read enumerates calendars and reads each available source.
  Newly discovered calendars join that read. Launch does not request permission.
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

Next native work must migrate connection storage/commands, enumerate and reconcile
multiple calendars, then verify restart, partial failure, source-ID collisions,
discovery, revocation and disconnection against S1-A1–A4. Acceptance counts remain unchanged.
