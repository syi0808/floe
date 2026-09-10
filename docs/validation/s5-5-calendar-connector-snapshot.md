# S5.5 Durable Calendar Connector Snapshot

> Date: 2026-09-10  
> Acceptance status: integration foundation only; S5.5-C1 remains pending

## Delivered boundary

- The existing durable Calendar mirror now projects into the common versioned
  `ConnectorSnapshot` contract instead of requiring a provider-specific status reader.
- Fixture and EventKit connections publish the same `calendar.timeline` View descriptor,
  device execution location, mirror retention, five-minute freshness, bounded item/byte
  limits and per-item provenance requirement.
- Calendar read and create are separate Observe and Act capability descriptors. The
  snapshot grants read only; write authority is never inferred from a connected source.
- Ready, pending, degraded, unavailable and disconnected states derive from persisted
  source status. Stale data and permission/provider failures remain typed, while a healthy
  source can keep a Schedule Situation runnable during another source's failure.
- Source handles hash provider-native calendar identifiers. Credential values, external
  IDs and calendar names do not enter the common snapshot.
- Calendar failure observation time is now persisted with aggregate and per-source state,
  cleared by a successful import and preserved across reopen.

## Automated evidence

Run from the repository root:

```sh
cargo test -p floe-core --test connected_calendar
cargo test -p floe-agent --test connected_context
cargo check --workspace
cargo test --workspace
```

The Calendar integration suite covers four cases:

1. A reopened durable mirror produces a conforming provider-neutral snapshot.
2. A per-calendar permission failure reports degraded state while the healthy Calendar
   View keeps the explicit foreground Situation runnable.
3. Disconnect survives reopen, clears grants and Views, and reconnect starts pending at a
   new revision without restoring old evidence.
4. A projection at the five-minute freshness boundary becomes typed stale/unavailable
   evidence and cannot satisfy a required Situation View.

## Remaining gate

The snapshot is a Core API and has not yet been exposed through the shared Connections UI
or consumed as the discovery source for every adapter. Only Calendar implements this
production projection, and no signed live EventKit lifecycle was rerun for this checkpoint.
Therefore S5.5-C1 and all other S5.5 criteria remain pending.
