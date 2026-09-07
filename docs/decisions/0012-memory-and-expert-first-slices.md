# ADR 0012: Validate memory and expert judgment before distribution

- **Date:** 2026-09-07
- **Status:** accepted
- **Amends:** [ADR 0006](0006-slice-driven-delivery.md)

## Context

The original connected delivery sequence moved from calendar read and approved
execution directly to cross-device/server and resident intervention work. That
would validate distribution and lifecycle before proving two more central Floe
hypotheses:

- source-backed Personal Memory becomes more useful without creating false or
  irrecoverable memory;
- one Manager can obtain bounded domain judgment from Experts without allowing
  them to bypass data permissions, review or action authority.

Server and cross-device work is important, but it multiplies identity, sync and
operational concerns around domain contracts that are still unvalidated. A local
vertical slice can invalidate the Memory and Expert designs sooner and more
cheaply.

## Decision

- Insert **S4 Reviewable Personal Memory** after the approved Calendar action.
  It validates one Note-based Preference/Commitment lifecycle from immutable
  evidence through candidate review, authoritative storage, inspection, edit and
  complete deletion.
- Insert **S5 Manager and Expert Advice Loop** after S4. It validates a local
  Manager, a built-in Schedule Expert and a deterministic declarative fixture
  Expert behind the same bounded invocation/result contract.
- Re-number the former S4 cross-device/server slice to **S6** and the former S5
  intervention slice to **S7**. S6 begins only after S5 is accepted.
- Reuse the existing Calendar scenario: confirmed Memory and today's Timeline
  inform a Schedule Expert; the Manager may surface one grounded focus-time
  proposal; any external write still passes through S3.
- Keep S4 and S5 device-local and foreground-capable. They must not depend on the
  future sync server, resident Device Agent, arbitrary code Expert sandbox or
  Marketplace.
- Require the local at-rest/key-unavailable portion of the encrypted Personal Store
  PoC before S4 uses real personal data; self-host and cross-device key design stays
  outside the S4 gate.
- Treat model output as untrusted candidate data. Fixtures establish deterministic
  contracts; a fixed evaluation corpus measures false memory, false merge,
  grounding and unnecessary advice before live-model evidence is accepted.

The detailed scope and acceptance criteria live in the
[vertical slice delivery plan](../planning/08-engineering/vertical-slice-delivery.md).

## Consequences

- Floe tests its differentiating Memory and judgment loop before investing in
  distribution topology.
- The first Memory slice deliberately covers only Floe-owned Note evidence and
  two memory types; it does not claim the full Personal Memory phase.
- The first Expert slice proves semantic parity with a declarative fixture but
  does not require Wasm, SDK or Marketplace delivery.
- Server and cross-device delivery moves later and must preserve the accepted
  local Memory, Expert permission and action-authority contracts rather than
  inventing server-specific shortcuts.
- Event-driven resident behavior moves to S7; S5's manual trigger does not validate
  background lifecycle or proactive intervention quality.

## References

- [Roadmap](../planning/00-overview/roadmap.md)
- [Personal Memory](../planning/02-domain/personal-memory.md)
- [Manager and Experts](../planning/03-intelligence/manager-and-experts.md)
- [Expert Runtime](../planning/09-implementation/expert-runtime.md)
- [Progress](../../PROGRESS.md)
