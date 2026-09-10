# S5.5 Provider-neutral Context Routing Foundation

> Date: 2026-09-11
> Acceptance status: route arbitration and calendar provider contracts; source adapters pending

## Delivered boundary

- Added an explicit logical-source route table that maps provider connector IDs to common View IDs.
  The model prompt never selects providers and does not receive provider routing policy.
- Arbitration admits only conforming, fresh ready/degraded snapshots. It prefers ready over degraded,
  then fresher observation, configured provider priority and stable connector ID order.
- One View is selected per logical source. Repeated physical source handles are suppressed across
  logical routes, and unavailable logical sources remain explicit instead of becoming empty data.
- Fixtures cover Google/Microsoft mail arbitration and Android/Health Connect-shaped candidates,
  including degraded fallback, invalid bounds and duplicate-source suppression.
- The durable calendar provider enum, registry binding and Schedule Expert setup now represent
  Google Calendar, Microsoft Calendar and Android Calendar explicitly. Every provider gets a
  distinct provider-pinned package identifier and personal-data classification.
- Durable mirror projection emits the same `calendar.timeline` capability/View descriptor for the
  three parity providers while preserving distinct connector/provider IDs and opaque source handles.
  Unsupported FFI access returns `CapabilityUnavailable` until its real adapter is installed rather
  than accidentally invoking EventKit or fixture data.
- Google and Microsoft calendar snapshots share one logical route in a focused arbitration test.
  Equal-health/equal-freshness candidates follow configured provider priority without model input,
  and the losing physical candidate is counted as deduplicated.

## Automated evidence

```sh
cargo test -p floe-agent --test context_routing
cargo test -p floe-agent --test calendar_setup
cargo test -p floe-core --test connected_calendar
cargo test -p floe-ffi --lib vault_host::tests::calendar_experts
```

The focused routing tests pass 2/2.

## Remaining gate

This is deterministic route/dedup and calendar contract infrastructure, not complete provider
parity. Microsoft Mail has a product adapter path, but Google Calendar, Microsoft Calendar, Android
Calendar/Contacts and Health Connect still need real source adapters and live conforming snapshots.
S5.5-C3 remains pending.
