# ADR 0030: Conversation-owned durable interactions and origin-linked resume

- **Status:** accepted
- **Date:** 2026-09-24
- **Amended by:** [ADR 0034](0034-gateway-reasoning-and-source-processing-authority.md) for model-review retirement and durable automatic resume; ADR 0031 continues to govern standing Observe reviewed-target semantics

## Amendment by ADR 0034

Conversation's durable interaction ownership, immutable reviewed target, CAS, durable owner-operation intent, no live user wait and fresh linked-Run semantics remain. Exact model-recipient interactions, consent-specific lineage and synthetic model-consent limitation replies in the original text below are superseded. Source-processing mismatch is an owner-derived SourceAccess review on the connection, not another model approval. ExpertBinding remains configuration navigation, not permission.

After an origin's interaction group is terminal with at least one Resolved member, the resume-required state must be durable and idempotently reconciled to one linked child Run, including crash after mutation/resolution, lost acknowledgement and duplicate/racing decisions. The existing unique slot alone does not prove durable scheduling. Routine Resolved-to-Continue product action is removed only with that recovery cutover; internal resume and exact-batch continuation machinery remain distinct.

A blocked Run need not invent an Assistant answer. Project the durable card separately from generated output, without waiting live for the person. Unknown/forged provenance and hard identity failures remain fail-closed; a known source processing-policy mismatch may be reviewed through its owner. The historical recipient-specific non-goals below do not override this amendment. The [convergence plan](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/development/plans/reasoning-source-processing-convergence.md) owns implementation and acceptance; this amendment is not a claim that automatic resume is already crash-durable.

## Context

A turn can reach a recoverable owner requirement: a source read needs
Observe review, a system permission needs attention, a resource must be
picked, or a model recipient needs processing consent. Before this decision
the requirement had no durable Conversation-owned home. The practical
consequences were:

- Nothing bound the person's decision to the exact source, resource set,
  bundle, consumer, revision and grant expectation they reviewed, so a
  re-read of latest state could silently swap the authority under an old
  Allow button.
- A Run that needed a person had nowhere honest to go: either it waited on
  the user (holding execution state hostage) or the requirement evaporated
  with the turn.
- Model-safe artifacts could only carry an opaque pending/allowed flag with
  no lifecycle, so resumed or retried work could not distinguish approval,
  denial, dismissal, supersede or expiry.
- Resuming after a decision risked becoming budget continuation (claiming an
  old validated batch) or a duplicated user message, instead of a fresh Run
  with recomputed context.

Access owns Observe and exact-recipient authority; Connections, providers
and native owners own connection, resource and system changes; Inference
owns route selection and model attempts. None of them owns the person's
review lifecycle. That lifecycle needs one owner with one durable record.

## Decision

Conversation owns the durable user interaction: identity, origin,
immutable reviewed descriptor, lifecycle, decision intent and resume
linkage.

An interaction records the admitted origin (a Tool call, Delegation Task
or Model attempt verified against the origin Run's durable journal), the
owner-produced semantic requirement, and the immutable reviewed target:
exact connection/device/source, selected resources, affected capability
bundle, requesting consumer/purpose, source revision, grant expectation
(including expected absence) and policy authority where applicable. The
record carries no credentials, tokens, source payloads, prompts, provider
errors or display labels.

Publication identity is deterministic: the admitted origin plus the
canonical requirement/target digests. Set-valued scope fields are
canonicalized; display text never enters the security digest. Replaying
the same publication settles the same row exactly once; unique
origin+digest, message-reference and decision-command constraints make
crash replay converge instead of duplicating.

Lifecycle is Pending to Resolving to Resolved, with Pending also reaching
Denied, Cancelled, Superseded or Expired. The original Run never waits for
the person: it records its limited response plus the interaction reference
and completes. Resolving is durable owner-operation recovery, not a Run
wait. Decisions persist intent (interaction id/revision, kind, reviewed
digest, stable command/operation identity, principal) before any owner
mutation, race through compare-and-swap, and rejoin on the identical
command id. Material drift invalidates review; a decision never re-reads
latest state to widen what was approved.

An authority review resolves through the canonical source/Access owner operation with
the reviewed target binding, re-reading and re-verifying current
authority; the interaction then records a semantic receipt. An Expert
binding blocker uses the same durable lifecycle but is navigation, not
approval: its target names the exact Task/assignment/requirement and
selection revision/digest, not a candidate or grant. A separate Experts
settings command saves the person's choice, and explicit Refresh resolves
only after re-reading the current binding. An Expert-origin SourceAccess
review also binds the admitted Task selection and supersedes before grant
mutation if that assignment was rebound. A linked
resume is a fresh Run, not budget continuation: it recomputes context,
claims no old batch takeover, duplicates no user text, and admits through
one atomically bound per-origin auto-resume slot. No global run-id stash,
no Flutter-double-click-only exactly-once. Grant consumption and fresh
review use different lineage: the child dispatches under origin-carried
lineage so a grant reviewed under the origin still scopes it, but a
fresh blockage is the child's own review and re-scopes to the attempting
Run before publication, so each generation carries the review it
actually attempted.

Model-safe artifacts and session messages carry only the opaque
interaction reference (id, kind, historical status at most). Reference
shape is never authorization: only Conversation's trusted lookup of the
durable row decides meaning and actionability, and a syntactically valid
reference to another Session stays forbidden. A bare interaction message
is source-independent; it never makes surrounding derived content
independent.

## Consequences

- Approval is bound to user-reviewed state with compare-and-swap
  semantics; stale approvals conflict instead of widening authority.
- Original Runs stay bounded and completable; person latency lives in
  Pending interactions, not in Working runs or held transactions.
- Crash recovery rejoins recorded publications and decisions instead of
  regenerating approvals or repeating owner mutations.
- Multi-source reads keep one concrete requirement per blocked source;
  Context never manufactures an aggregate grant or drops a dependency.
- Flutter presents backend projections and sends explicit decisions; it
  owns no grant, authority or resume semantics.
- Compaction may archive origin intent but must preserve a validated
  retrieval path and must not destroy a pending interaction's target.

## Non-goals

- Interactions never authorize source reads, model processing or external
  effects by themselves; owner admission still decides every use.
- Not every PolicyDenied becomes a card: corrupt storage, forged or
  foreign authority, unknown provenance, prohibited classes,
  LocalOnly-to-external and mismatched recipients stay fail-closed.
- Recipient consent stays Access-owned, exact, time-bounded and
  lineage-scoped; it never revives saved global allow flags or changes
  source processing restrictions, and a first-model blockage yields a
  typed blocked outcome plus a deterministic limitation, never an
  unapproved model call.
- Action proposals are not executed from an interaction, and uncertain
  external writes are reconciled, never blindly retried.

## Alternatives not selected

- **Run waits for the user** (`WaitingForUser` Run state): couples person
  latency to Run lifecycle, journal recovery and budget accounting, and
  strands ValidatedBatch ownership across the wait. Rejected.
- **Chat-local permission writer**: a second grant/consent path beside the
  canonical owner API would duplicate authority and CAS semantics.
  Rejected; fixes land in the canonical owner API.
- **Display-text-bound or random-per-replay identity**: breaks crash replay
  (duplicated cards) or binds decisions to unreviewed presentation.
  Rejected; identity commits to origin plus canonical semantic digests.
- **Budget-continuation resume**: reuses stale context and risks old-batch
  takeover for work that must recompute under new authority. Rejected;
  resume is a fresh Run with its own admission.
