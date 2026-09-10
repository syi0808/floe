# S5 Idle Learner Worker

Date: 2026-09-10

## Scope

- Connected explicit conversation discovery, durable queue claim, the device-local
  structured Learner, candidate staging and durable settlement inside the existing
  encrypted Person-vault worker.
- Kept the work invisible to the foreground protocol: background review does not add
  foreground events, alter its result, or hold completion open.

## Scheduling and preemption

- The first review window starts 750 milliseconds after foreground work completes.
- A successful claimed review drains the next job after the same idle delay. An empty
  queue backs off for 30 seconds; a worker error backs off for 5 seconds.
- A foreground submission publishes a pending flag before enqueue and cancels the
  currently registered Learner token. The worker rechecks the flag before starting a
  timed-out idle slice, closing the enqueue/timeout race.
- Worker shutdown cancels both foreground and background tokens.

## Settlement guarantees

- A successful no-change review completes without a candidate ID; a successful proposal
  completes with the exact candidate ID bound to the Learner run and source turns.
- Cancellation, deadline, local-model unavailability, quota and interruption defer the
  job. Policy, schema, budget and stale-source failures are terminal.
- When a transient failure occurs on the final leased attempt, the vault records a
  terminal failed job rather than rejecting settlement or leaving another lease.
- Runtime candidate staging remains source-revision CAS protected. If the candidate
  commit wins a cancellation race, the job is completed with that durable candidate.

## Validation

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- Focused coverage verifies that accepted foreground work synchronously cancels the
  active Learner token, retry classification is fail-closed, and final-attempt deferral
  becomes a durable terminal record.

## Acceptance

This completes S5-A2: the background Learner has a bounded read-only input, a
candidate-only write boundary, a separate budget and cancellation scope, and is safely
deferred/preempted by new foreground work without mutating the foreground session.

Remaining S5 work includes Playbook candidates, Persona management, full knowledge
inspection/edit/delete and rollback, Curator/pinning/deletion propagation, replay
evaluation, and on-device dogfood evidence.
