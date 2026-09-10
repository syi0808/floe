# S5.5 Floe-native Context in Schedule Expert

> Date: 2026-09-10  
> Acceptance status: implementation increment only; S5.5 remains pending

## Delivered boundary

- Durable Floe-native Tasks and Notes project into separate, provider-neutral Personal Views.
- The projections exclude completed or deleted Tasks, deleted Notes, cross-person records and
  domain source metadata. Items expose only opaque evidence handles and bounded untrusted text.
- Views enforce five-minute freshness, item and byte limits, unique evidence handles and strict
  Task/Note item separation before entering Agent context.
- The production Calendar Agent path loads these Views for Personal turns and passes them to the
  Schedule Expert model as bounded evidence. Synthetic fixture turns do not receive Personal data.
- Existing inference placement and transfer consent still govern the resulting Personal context;
  Floe-native sources are not presented as external Connections.
- Agent context now applies aggregate evidence count and byte bounds and rejects empty, duplicate
  or oversized source handles.

## Automated evidence

Run from the repository root:

```sh
cargo test -p floe-core --test native_context
cargo test -p floe-core agent_calendar::tests::schedule_expert_consumes_bounded_floe_native_tasks_and_notes --lib
cargo check --workspace
cargo test --workspace
```

The focused tests cover durable reopen, person isolation, completed-record exclusion, stable
evidence handles, Task/Note shape, and rejection when item or byte limits are exceeded. The
Calendar Agent test observes both native Views in every Schedule Expert model request.

## Remaining gate

This increment connects only Floe-native Task/Note evidence to the existing Calendar-backed
Schedule Expert. Gmail, Contacts, location/ETA/weather, health and attention production Views are
still absent, and no Commitments or Communication Expert consumes them. Implementation-order item
2 and all S5.5 acceptance criteria therefore remain pending.
