# ADR 0006: Deliver through connected vertical slices

- **Date:** 2026-09-04
- **Status:** accepted

## Context

The Personal Day foundation already connects Flutter, Rust, and embedded Turso.
Finishing every Phase 1 feature before starting connectors and intelligence would
delay validation of Floe's central assistant experience. Phase percentages also
mix implementation breadth with evidence that components work together.

## Decision

- Keep roadmap phases as a product capability map, not sequential delivery gates.
- Use user-visible, end-to-end slices as the implementation and acceptance unit.
- Deliver S1 Calendar read, S2 contextual suggestion, and S3 approved execution
  before expanding the same scenario to S4 cross-device/server and S5 interventions.
- Limit the first connected loop to macOS, one Person, one calendar connector,
  one built-in Schedule Expert, and one calendar-create action.
- Use fixtures to establish contracts, then validate real integrations before
  accepting a slice. A mock-only demonstration is not integration completion.
- Track status, acceptance evidence, integration mode, blockers, and the next
  demonstration in `PROGRESS.md`; define requirements in the delivery plan.
- Keep at most one slice in Implementing, Integrated, or Verified at a time.
  An earlier slice may remain in Dogfooding while the next is implemented.

The scenario, acceptance criteria, transition rules, and phase coverage are
defined in [the vertical slice delivery plan](../planning/08-engineering/vertical-slice-delivery.md).

## Relationship to existing scope

ADR 0004 remains the definition of the first local Personal Day slice. Its
unfinished acceptance criteria remain unfinished; this decision does not declare
that slice complete or retroactively expand it. Non-blocking UI breadth work is
deferred while connected delivery is prioritized.

The Personal Day MVP remains a separate product hypothesis with its existing
scope and two-week dogfood requirement. Connected slices add an integration
validation track; they do not silently redefine MVP acceptance.

Flutter presentation, Rust-owned canonical mutations, native Rust/Go connectors,
and intelligence proposing rather than directly executing actions remain intact.
S4 must validate identity, authorization, and sync boundaries before using real
multi-device personal data. S5 does not imply wake word or always-listening support.

## Consequences

- Integration risks surface earlier, at the expense of postponing non-blocking
  editing, folding, and other Personal Day polish.
- Small bounded capability implementations take precedence over complete generic
  connector frameworks, expert SDKs, and platform parity.
- Phase coverage and delivered slices are reported separately. Partial boundary
  validation is never equivalent to a completed phase.
- Later slices are refinement points, not permission to bypass security PoCs.

## References

- [Progress](../../PROGRESS.md)
- [Roadmap](../planning/00-overview/roadmap.md)
- [Personal Day MVP](../mvp.md)
- [First local slice](0004-personal-day-first-slice.md)
- [Native connectors and experts](0003-native-connectors-and-experts.md)
