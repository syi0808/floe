# S5.5 Personal Context View Foundation

> Date: 2026-09-10  
> Acceptance status: provider-neutral projection contracts only; live adapters pending

## Delivered boundary

- Added bounded Personal View contracts for Contacts-backed identity, next-event feasibility,
  coarse attention and derived wellbeing state.
- People identity includes only a bounded display name, aliases, confidence and opaque evidence
  handles. It has no contact notes or address-book object projection.
- Feasibility carries event/evidence handles, travel duration, leave-by, coarse weather impact and
  confidence. It has no coordinates or location history.
- Attention carries only an availability/focus/interruption-pressure category and provenance; it has
  no app, domain, notification or raw Screen Time records.
- Wellbeing carries only coarse capacity/recovery categories and provenance; it has no raw HealthKit
  samples, metrics or diagnosis.
- Every View is versioned, Personal-class, five-minute bounded, byte-limited and source-attributed.
  Unknown states require zero confidence; derived states require evidence.

## Automated evidence

```sh
cargo test -p floe-agent --test personal_context
```

The focused contract tests pass 2/2, including stale/duplicate/unproven rejection and strict unknown
field denial for representative precise-location, app-activity, raw-health and diagnosis fields.

## Remaining gate

These are contracts and synthetic projections, not live provider evidence. Apple Contacts,
Core Location/MapKit/WeatherKit, physical-device Screen Time and HealthKit adapters still need to
produce them, and domain Experts do not consume them yet. S5.5-C2 and S5.5-E1/E4/E5/E6 remain
pending.
