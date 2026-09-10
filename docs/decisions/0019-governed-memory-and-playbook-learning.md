# ADR 0019: Govern self-improvement through staged knowledge changes

- **Status:** accepted
- **Date:** 2026-09-10
- **Scope:** S5 Personal Memory and Playbook learning lifecycle
- **Extends:** ADR 0017 context assembly

## Context

S4 produces the first trustworthy learning inputs: durable conversations, explicit
user corrections, Tool and Expert outcomes, Review decisions and immutable Session
Archive snapshots. S5 must turn that evidence into useful Personal Memory and
procedural Playbooks without letting a model silently change identity, policy,
authority or its own future instructions.

Hermes Agent is the primary implementation reference for this lifecycle. Its current
implementation demonstrates several useful mechanisms:

- a post-turn background review fork with a restricted Tool set and an explicit
  `Nothing to save` result;
- separate factual Memory and procedural Skill updates;
- a write gate that either applies, blocks or durably stages the exact requested
  operation for later approval;
- origin tracking for foreground and background writes;
- atomic Skill batches and notifications derived from applied results rather than
  requested operations;
- usage records, pin protection and deterministic `active → stale → archived`
  Curator transitions before optional model-assisted consolidation;
- recoverable archive as the maximum autonomous destructive operation.

Floe should adopt those operational lessons, but not Hermes' authoritative Markdown
files, session-start-only Memory snapshot, direct background mutation path or
untyped replay payload. Floe has a Person-scoped encrypted vault, shared Review and
Activity surfaces, typed context boundaries and stronger provenance/deletion goals.

## Decision

### Knowledge classes and ownership

The learning loop may propose changes to exactly two learnable classes:

| Class | Captures | Authoritative representation |
| --- | --- | --- |
| Personal Memory | facts, preferences, relationships and commitments | typed temporal records |
| Playbook | reusable guidance for a recurring task class | immutable versioned body and index metadata |

Session Archive is immutable evidence, not learned knowledge. Persona and User Model
remain user-controlled configuration/profile concepts. Host Policy, Behavior Kernel,
Role, grants, Action Authority, Expert trust and model configuration are never
learning targets.

### Durable records

Core owns the following versioned, Person-scoped records in the encrypted Agent vault:

```text
LearningObservation {
  id, personId, sessionId, turnIds[], evidenceRefs[], outcomeRefs[],
  kind, digest, observedAt, contentHash
}

LearningRun {
  id, observationIds[], extractorVersion, promptVersion, modelPolicy,
  budget, status, startedAt, finishedAt, failure?
}

KnowledgeCandidate {
  id, kind: memory | playbook, operation: create | revise | retire,
  targetId?, baseRevision?, proposedValue, beforeHash?, afterHash,
  sourceRefs[], factOrInference, confidence, actor, runId,
  state: pending | approved | rejected | superseded | withdrawn,
  createdAt, decidedAt?, decidedBy?
}

KnowledgeRevision {
  id, revision, value, sourceRefs[], validFrom?, validUntil?,
  state: active | superseded | stale | archived | tombstoned,
  createdBy, createdAt
}

KnowledgeMutation {
  id, candidateId?, targetId, fromRevision?, toRevision?, actor,
  operation, beforeHash?, afterHash?, rollbackRevision?, createdAt
}
```

`contentHash + extractor/prompt version + candidate kind + targetId` forms the
idempotency key for an extraction result. Replaying a session or retrying a model call therefore
cannot create another pending or active copy of the same proposal. Unknown schema or
extractor versions fail closed rather than being reinterpreted.

### Observation and trigger policy

The foreground Agent never pauses its answer to run speculative learning. After a
primary Manager turn reaches a durable terminal outcome, Core may enqueue a review
only when one of these signals exists:

- the user explicitly asks Floe to remember or forget something;
- the user corrects a fact, preference or prior procedure;
- a Tool/Expert outcome contradicts a loaded Memory or Playbook revision;
- a successful multi-step outcome contains a reusable procedural delta;
- a configurable bounded interval is reached for eligible completed turns.

Subagent, cancelled, unresolved-failure, fixture and privacy-ineligible turns do not
produce autonomous candidates. A run may conclude with no candidate; absence of a
write is a normal result, not a model failure. One-off narratives, transient setup
failures and unsuccessful guesses must not become Playbooks.

### Isolated Learner

The Learner runs outside the foreground conversation with a separate model policy,
deadline, call/token/cost budget and cancellation token. Its input is a bounded digest
plus immutable evidence and outcome references, not the live mutable session object.
A same-model implementation may reuse a frozen prefix; a different-model
implementation receives a newly assembled least-privilege digest.

Unlike Hermes' background fork, Floe's Learner receives no Memory or Playbook mutation
Tool. It can only:

```text
evidence.read_bounded
knowledge.current_projection
candidate.propose_memory
candidate.propose_playbook_change
```

Candidate proposal validates schema, scope, size, source existence and base revision.
It cannot activate, edit or delete authoritative knowledge, invoke domain Tools,
delegate to Experts, access the network or read credentials. A new foreground turn
may defer or cancel local inference without changing the source session or leaving a
partially written candidate batch.

### Review and activation

All inferred background changes are durably staged as `pending` and enter the shared
Review surface. Pending, rejected, superseded and withdrawn candidates are excluded
from model context and retrieval indexes.

A Review item shows:

- Memory type or Playbook target and proposed operation;
- source session/turn labels and outcome;
- fact versus inference, confidence and extractor version;
- before/after diff and expected context impact;
- approve, edit-and-approve, reject and inspect-source actions.

An explicit foreground user instruction may be confirmed inline. Confirmation still
creates a candidate, decision and mutation ledger entry atomically; it never bypasses
provenance. If no interactive Review channel is available, the change remains pending.

Approval is a single encrypted transaction that revalidates Person, source, candidate
state and `baseRevision`, writes the new immutable revision, updates the active
projection and retrieval index, records rollback material, then marks the candidate
approved. A stale base revision yields conflict and a new diff; it never overwrites a
newer decision. UI notifications and Activity entries are derived only from committed
mutation rows.

### Retrieval and context use

Only active, confirmed, non-tombstoned revisions may enter `ContextEnvelope`.
Retrieval applies Person, knowledge class, scope, time validity, source availability,
grant and freshness filters before ranking. The `ContextManifest` records record ID,
revision, source class, score, token estimate and exclusion reason.

Memory is rendered as structured evidence, never instructions. Current explicit user
intent wins over an older Memory and creates correction evidence. Playbook summaries
and bodies remain governed instructions below the current request and use ADR 0017's
nested progressive-disclosure limits.

Each included revision records a usage event linked to model attempt and outcome.
Usage is evidence that the record was considered, not proof that it helped. Evaluation
compares later comparable outcomes and explicit corrections before proposing a
revision or rollback.

### Rollback, pinning and curation

Every activation preserves the prior immutable revision and records a reversible
mutation. Rollback creates another revision pointing to the selected prior value; it
does not erase history. Pinning blocks every autonomous change, including edits,
retirement, stale marking and archival. Foreground user edits remain possible after
an explicit confirmation.

The initial Curator is deterministic and opportunistic rather than a resident daemon:
it runs only after an idle/interval gate and seeds its first-run timestamp without
mutating knowledge. It may:

```text
active → stale → active
active | stale → archived
archived → active (explicit user recovery)
```

It skips pinned, user-owned and currently referenced revisions. Automatic hard delete,
content consolidation and model-authored patching are disabled. A future optional
model-assisted curator must emit ordinary candidates through the same Review boundary;
it receives no special mutation authority.

### Forgetting and deletion propagation

User forget/delete is an authoritative foreground operation. Core first writes a
tombstone and denial index entry, then follows provenance through active projections,
search indexes, compaction summaries, candidate inputs and derived manifests. Raw
immutable evidence is removed or cryptographically made inaccessible according to its
retention contract. Re-extraction checks tombstones before candidate creation, so a
deleted fact cannot reappear after restart, compaction or extractor upgrade.

If complete propagation cannot finish atomically, a durable deletion job keeps the
record excluded from retrieval while cleanup retries. Recovery and replay respect the
tombstone before reading archived material.

### Controls and observability

Person and chat controls are separate:

| Control | Default | Effect |
| --- | --- | --- |
| use confirmed Memory | on | permits eligible active Memory retrieval |
| use Playbooks | on | permits eligible reviewed Playbook discovery |
| generate learning candidates | on | permits staged extraction only |
| background Learner | on when local policy permits | schedules isolated review |
| inferred write Review | always on | cannot be disabled in S5 |

Inspection exposes candidates, active revisions, source labels, usage/outcome history,
ledger actions and tombstones without exposing hidden reasoning or raw highly
sensitive evidence. Metrics cover candidate precision, approval/rejection/edit rate,
duplicate rate, retrieval precision, correction rate, outcome delta, rollback rate,
deletion completion and foreground latency impact.

## Hermes mechanisms adopted and changed

| Hermes implementation mechanism | Floe decision |
| --- | --- |
| restricted post-turn background review fork | adopt with typed digest, independent budget and cancellation |
| Memory versus procedural Skill split | adopt as Personal Memory versus Playbook |
| allow/block/stage write gate | adopt staging semantics; inferred activation is never directly allowed |
| exact pending operation persisted for replay | adapt to typed encrypted candidate plus base-revision CAS |
| foreground/background origin tracking | adopt as required actor and run provenance |
| notifications derived from applied Tool results | adopt from committed mutation ledger only |
| bounded `MEMORY.md` and `USER.md` prompt snapshot | reject as authority; use typed temporal records and per-call retrieval |
| background memory/skill mutation Tools | reject; Learner can only propose candidates |
| usage sidecar and pin guards | adapt into encrypted revision/usage ledger with stronger background pin semantics |
| deterministic stale/archive Curator | adopt; no hard delete and no autonomous consolidation |
| optional model consolidation with direct Skill edits | defer; future model curator must submit reviewed candidates |

## Implementation sequence

1. Add encrypted observation, candidate, decision and mutation-ledger tables plus
   idempotent repository contracts.
2. Implement explicit correction/preference extraction with a deterministic fixture
   and shared Review projection; no background scheduling yet.
3. Activate approved typed Memory revisions transactionally and retrieve them through
   `ContextEnvelope` with manifest evidence.
4. Add the isolated bounded Learner and cancellation/defer behavior using the same
   candidate port.
5. Add immutable Playbook revisions, nested loading, staged diff activation and
   rollback.
6. Add usage/outcome evaluation, pin/stale/archive Curator and full deletion
   propagation.
7. Validate versioned corpus precision, restart/replay, key-unavailable behavior and
   controlled real-data dogfood before claiming S5 acceptance.

## Consequences

- Hermes' proven fork, staging, pin and curation patterns reduce lifecycle risk.
- Floe pays additional schema and transaction complexity to gain provenance,
  explainability, same-turn safety and deletion consistency.
- No model process can directly change durable knowledge, even when the user enables
  background learning.
- The first vertical path remains small: one explicit correction becomes one reviewed
  Memory revision and is retrieved in the next relevant model call.
- Playbook self-improvement reuses the same candidate, Review, ledger and rollback
  machinery instead of creating a second trust boundary.

## References

Hermes revision `f97a4102dd3864eed0c85132850ce7e06f13e09a` was reviewed on
2026-09-10 so later upstream changes cannot silently alter the reference.

- [Hermes background review implementation](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/agent/background_review.py)
- [Hermes write approval and pending store](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/tools/write_approval.py)
- [Hermes Memory tool](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/tools/memory_tool.py)
- [Hermes Curator implementation](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/agent/curator.py)
- [Hermes Skill usage and pin state](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/tools/skill_usage.py)
- [Hermes Skill mutation guards](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/tools/skill_manager_tool.py)
- [Hermes Memory provider lifecycle](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/agent/memory_provider.py)
- [Hermes persistent Memory guide](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/website/docs/user-guide/features/memory.md)
- [Hermes Curator guide](https://github.com/NousResearch/hermes-agent/blob/f97a4102dd3864eed0c85132850ce7e06f13e09a/website/docs/user-guide/features/curator.md)
