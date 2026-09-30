# ADR 0032: Retire the dedicated Schedule Feasibility vertical

- **Status:** accepted
- **Date:** 2026-09-30
- **Amends:** [ADR 0014](0014-s4-connected-agent-sources.md), [ADR 0020](0020-ambient-assistant-expert-connector-model.md), [ADR 0021](0021-s5-5-connected-domain-expansion.md), [ADR 0024](0024-device-context-collection-and-convergence.md), [ADR 0031](0031-connection-owned-source-scope-and-logical-observe.md)
- **Extends:** [ADR 0029](0029-pre-stable-architecture-convergence.md)

## Context

Floe implemented a dedicated `schedule.feasibility` vertical that combined a reviewed Calendar event and destination with current location, MapKit directions/ETA and WeatherKit event-window weather. To authorize that one composite read it also introduced a Feasibility-only query review, contextual grant/source authority, Vault schema, native acquisition request fields, App/FFI wire, Flutter management UI and Apple package.

The Manager no longer owns source-backed domain Tools. The shipped Schedule Expert uses Calendar as its declared source and does not own the Feasibility contextual contract. Moving the existing vertical into Schedule would make one Expert responsible for Calendar, location, routing and weather authority and would preserve the special query-bound grant model merely to keep an existing implementation alive.

The pre-stable architecture policy prefers removing that unsupported vertical over generalizing an exception with no canonical owner.

## Decision

1. Retire the dedicated `schedule.feasibility` View and `schedule.feasibility.read` capability.
2. Retire the `feasibility.apple` connector/provider surface and the bundled `FloeFeasibilityProvider`.
3. Remove the Feasibility-specific contextual Access review, Vault persistence and local product/wire API rather than converting them into a standing source or generic Expert binding.
4. Schedule remains responsible for Calendar/time judgment: events, conflicts, available time, priorities and realistic schedule changes. It does not acquire current location, directions, ETA or weather under this contract.
5. This decision does not assign travel/mobility context to another Expert. A future travel, mobility or logistics capability must define its own domain owner, source composition and authority semantics rather than restoring this vertical by compatibility.
6. When no other current product owner requires them, remove Floe's iOS location usage description and WeatherKit entitlement together with the provider.
7. Old local development Vaults containing the retired Feasibility review schema are unsupported development state. The implementation may fail them explicitly; it does not require an in-place migration or automatic destructive reset.
8. Generic uses of the word feasibility, including provider/OS feasibility gates and Screen Time availability, are unaffected.

## Consequences

- One special contextual data-authority model disappears from Access and Vault.
- Personal native acquisition becomes a smaller shared path for the standing People/Wellbeing device sources.
- The Apple client no longer needs current-location permission or WeatherKit solely for this feature.
- Schedule loses travel/leave-by/weather evidence from this implementation; it must state that limitation rather than infer those facts.
- Future travel assistance can still be designed, but it starts from a fresh domain contract instead of inheriting the retired event/destination query shape.
- Historical ADR text remains useful evidence of why the vertical existed. Current product and architecture documents must follow this decision as the implementation is retired.
