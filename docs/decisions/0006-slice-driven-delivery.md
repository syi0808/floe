# ADR 0006: Deliver through connected vertical slices

- **Date:** 2026-09-04
- **Status:** accepted as historical product delivery rationale; delivery sequence amended by [ADR 0012](0012-memory-and-expert-first-slices.md) and [ADR 0013](0013-conversational-agent-learning-and-voice-sequence.md). For the current refactor, execution order is superseded by [Stage 2 — Canonical Internal Runtime](../refactoring/stage-2.md), with [Stage 1](../refactoring/stage-1.md) recording completed ownership and [Stage 3](../refactoring/stage-3.md) defining the final product-boundary cutover.

> Current refactor: one coding agent completes canonical internal owner cutovers in Stage 2 before Stage 3 closes the outer product boundary and end-to-end product validation. The Stage 2 Current checkpoint, not this historical ADR, owns the active sequence. The historical decision below does not require a live demonstration after every structural change or declare any old acceptance gate passed. Product requirements and safety conditions remain in force.

## Context

The Personal Day foundation already connects Flutter, Rust, and embedded Turso.
Finishing every Phase 1 feature before starting connectors and intelligence would
delay validation of Floe's central assistant experience. Phase percentages also
mix implementation breadth with evidence that components work together.

## Decision

- Keep roadmap phases as a product capability map, not sequential delivery gates.
- Use user-visible, end-to-end slices as the implementation and acceptance unit.
- Deliver S1 Calendar read and S3 approved execution before expanding the same
  scenario. ADRs 0012 and 0013 subsequently put conversational Agent/Expert,
  governed Memory/learning, voice and wake-up validation before the renamed S8
  cross-device/server and S9 intervention slices.
- Limit the first connected loop to macOS, one Person, one calendar connector,
  and one calendar-create action.
- Use fixtures to establish contracts, then validate real integrations before
  accepting a slice. A mock-only demonstration is not integration completion.
- The historical delivery process tracked status and acceptance evidence separately
  from requirements. Current refactoring status is owned only by Stage 2.
- Keep at most one slice in Implementing, Integrated, or Verified at a time.
  An earlier slice may remain in Dogfooding while the next is implemented.

The historical scenario, acceptance criteria, transition rules and phase coverage
were recorded in the delivery plan that is now preserved only in Git history.

## Relationship to existing scope

ADR 0004 remains the definition of the first local Personal Day slice. Its
unfinished acceptance criteria remain unfinished; this decision does not declare
that slice complete or retroactively expand it. Non-blocking UI breadth work is
deferred while connected delivery is prioritized.

The Personal Day product hypothesis remains historical context for this decision.
Connected slices did not silently redefine its acceptance.

Flutter presentation, Rust-owned canonical mutations, native Rust/Go connectors,
and intelligence proposing rather than directly executing actions remain intact.
S8 must validate identity, authorization, and sync boundaries before using real
multi-device personal data. S7 wake-up remains local and consented; S9 does not
imply always-listening recording.

## Consequences

- Integration risks surface earlier, at the expense of postponing non-blocking
  editing, folding, and other Personal Day polish.
- Small bounded capability implementations take precedence over complete generic
  connector frameworks, expert SDKs, and platform parity.
- Phase coverage and delivered slices are reported separately. Partial boundary
  validation is never equivalent to a completed phase.
- Later slices are refinement points, not permission to bypass security PoCs.

## References

- [Roadmap](../planning/00-overview/roadmap.md)
- [First local slice](0004-personal-day-first-slice.md)
- [Native connectors and experts](0003-native-connectors-and-experts.md)
