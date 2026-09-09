# S4 lightweight Schedule Expert subagent

Date: 2026-09-08. Runtime integration with synthetic automated evidence; S4 remains
**0/14**.

> Historical checkpoint: the mandatory free-window behavior below was superseded by
> [the 2026-09-09 generalization](s4-agent-context-generalization.md). Isolation,
> authority and bounded execution evidence remains applicable.

## Implemented boundary

- The Manager remains the only owner of user conversation history. A Schedule Expert
  invocation creates an ephemeral turn containing only the typed `ExpertInput` task.
- The built-in Schedule implementation runs a lightweight loop of at most ten model
  calls. It must call `schedule.find_free_windows` at least once, may call it up to
  nine times, and must finish with a non-empty bounded summary.
- Each Tool call may cover the whole authorized View with `{}` or one explicit
  subrange of at most 24 hours; the Expert contract caps a View at 14 days. This
  supports comparing several dates without letting the model expand the host-issued
  View or alter exact interval calculations.
- `schedule.find_free_windows` is the existing deterministic Rust interval analysis.
  The model cannot alter its insights or focus proposal timestamps.
- The loop derives a `fast`/`schedule-summary` policy while preserving the Calendar
  turn's model placement, data classes and transfer consent. It enforces separate
  limits of ten model calls, nine Tool calls, 40,960 reported tokens, 50,000
  micro-cost units, 4 KiB per model output and the parent deadline/cancellation.
- Only the final bounded summary and `model_calls` count join the structured
  `ExpertResult`. The internal task/tool messages are not committed to the Manager
  session. Old results without a summary remain readable.
- Declarative Experts retain deterministic execution without a model. Expert output
  remains advisory; Calendar mutation still requires the existing S3 policy and
  review/action path.

The current app Calendar turn still issues a one-day View. The loop and Tool contract
can compare dates contained in a broader bounded View, but the native/Dart transport
does not yet request or hydrate that multi-day View.

## Automated evidence

- The Calendar Core suite exercises Manager model → Schedule subagent → deterministic
  Tool → subagent summary → Manager synthesis, including proposal publication,
  cancellation, stale access, key loss and registry revocation boundaries.
- Rust Expert tests continue to cover the model-free declarative and built-in semantic
  contract. Flutter parsing accepts both the new bounded summary and legacy results.

Validation run:

```text
cargo test --workspace
All Rust workspace tests passed.

cargo clippy -p floe-agent -p floe-core -p floe-ffi --all-targets --
  -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
  -A clippy::large_enum_variant
Passed.

flutter test test/agent_expert_result_test.dart test/agent_proposal_test.dart
  test/agent_calendar_controller_test.dart test/agent_calendar_turn_test.dart
14 passed; 0 failed.

flutter analyze
No issues found.
```

The full Flutter suite still has existing failures in the fixture retry and registry
settings tests. The focused Calendar/Expert tests above pass; this increment does not
modify those failing controller or settings surfaces.

## Remaining live gates

Automated models and injected Calendar access do not prove real EventKit reads,
Foundation Models behavior, remote-provider privacy, signed-app key lifecycle or
prompt-injection resistance. Those live S4 gates remain unchanged.
