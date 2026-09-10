# S5.5 Connected Context Conformance Foundation

> Date: 2026-09-10  
> Acceptance status: foundation only; S5.5-E1–E8 and S5.5-C1–C6 remain pending

## Delivered boundary

- Added strict versioned descriptors for connector execution location, Observe/Act/Interact
  capabilities, provider-neutral Views and explicit foreground Situations.
- Added retention, freshness, size, data-class and provenance constraints to each View.
- Added connection snapshots with granted scopes, last success, typed last failure and
  pending/ready/degraded/unavailable/disconnected/revoked/unsupported lifecycle states.
- Added a provider-neutral conformance harness that rejects missing scopes, credential
  projections, Observe/Act authority mixing, stale or oversized Views and incomplete
  provenance.
- Added cross-source Situation evaluation. A failed optional source remains visible in
  the report while a conforming required source can keep the foreground scenario runnable.

`SituationTrigger` intentionally supports only an explicit foreground request. The
contract does not enable background detection, interruption or proactive delivery, which
remain S9 work.

## Automated evidence

Run from the repository root:

```sh
cargo test -p floe-agent --test connected_context
cargo check --workspace
cargo test --workspace
```

The focused suite covers five cases:

1. Observe capability output remains separate from Act authority.
2. Missing grants and incomplete per-item provenance fail conformance.
3. A degraded optional source does not block an available required View.
4. Expired and nonconforming View snapshots are not reported as available.
5. Unknown wire fields, including an attempted background-delivery flag, fail closed.

The full Rust workspace check and test suites also pass. Warnings-denied Clippy remains
blocked by the existing Agent runtime/model-journal lints outside this increment.

## Remaining gate

This checkpoint supplies an in-process contract and fixture harness only. It does not
yet connect a live provider, an Expert production path or the shared Connections UI,
and it does not demonstrate disconnect/reconnect transitions over durable state.
Therefore it does not satisfy S5.5-C1 or any Expert acceptance criterion.
