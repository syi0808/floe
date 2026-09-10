# S5.5 Provider-neutral Context Routing Foundation

> Date: 2026-09-10  
> Acceptance status: route arbitration contract only; provider adapters pending

## Delivered boundary

- Added an explicit logical-source route table that maps provider connector IDs to common View IDs.
  The model prompt never selects providers and does not receive provider routing policy.
- Arbitration admits only conforming, fresh ready/degraded snapshots. It prefers ready over degraded,
  then fresher observation, configured provider priority and stable connector ID order.
- One View is selected per logical source. Repeated physical source handles are suppressed across
  logical routes, and unavailable logical sources remain explicit instead of becoming empty data.
- Fixtures cover Google/Microsoft mail arbitration and Android/Health Connect-shaped candidates,
  including degraded fallback, invalid bounds and duplicate-source suppression.

## Automated evidence

```sh
cargo test -p floe-agent --test context_routing
```

The focused routing tests pass 2/2.

## Remaining gate

This is deterministic route/dedup infrastructure, not provider parity. Google Calendar,
Microsoft Calendar/Mail, Android Calendar/Contacts and Health Connect adapters still need to emit
live conforming snapshots and use the route table. S5.5-C3 remains pending.
