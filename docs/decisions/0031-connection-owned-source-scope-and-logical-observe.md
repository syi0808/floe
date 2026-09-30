# ADR 0031: Connection-owned source scope and logical standing Observe

- **Status:** accepted
- **Date:** 2026-09-30
- **Amends:** [ADR 0027](0027-connection-authority-and-observation.md) connection authority/observation, [ADR 0028](0028-pairing-integrated-authority-and-connection-permissions.md) connection-scoped permission presentation, [ADR 0030](0030-durable-interaction-and-linked-resume.md) Observe reviewed-target semantics
- **Extends:** [ADR 0029](0029-pre-stable-architecture-convergence.md) pre-stable architecture convergence

## Context

Calendar leaf resources leaked into four distinct concerns: connection source scope,
Expert selection, standing permission and acquisition. Composing consumers across
per-leaf Expert selections could collapse the reader set or overgrant it. Copying
the current leaves into standing grants made an ordinary source-resource edit look
like a permission change. A separate consumer-policy authority duplicated the
actual source and permission epochs without introducing a distinct owner.

The connection identifies a source; permission identifies who may use its logical
Views; acquisition establishes what was actually observed. These facts need
separate authority and exact provenance, not duplicate leaf selection lists.

## Decision

1. Connections owns stable source identity, lifecycle, the current physical resource
   set and trusted native subject. Account or execution-owner replacement creates a
   new connection identity rather than inheriting another source's grants.
2. `SourceAuthority` is the source/resource/subject epoch. Local configuration CAS,
   producer revisions and Day mirror revisions remain separate namespaces.
3. Access standing Observe binds stable Person/Connection/Connector/execution-owner
   identity plus one or more logical connection/View resources. Calendar uses one
   `calendar.timeline:<connection>` resource per connection, native or hosted.
4. `GrantAuthority` is the only standing permission/state/scope epoch. Grant consumers,
   operations, purpose, categories and processing restrictions remain Access authority.
5. There is no `ConsumerPolicyAuthority`. Deterministic `policy_digest` compares the
   exact intended logical View policy; it grants no authority and is not an epoch.
6. Provider leaves belong to source configuration, acquisition and provenance, not
   standing permission identity. Context reloads the current source resources at
   acquisition and retains the exact observed `source_resources` separately from
   logical grant `resources`.
7. Experts owns connection/View binding as configuration. Provider leaves are not
   Expert selection, and binding is never source permission.
8. Resource changes advance source authority without inherently mutating `GrantId`,
   `GrantAuthority`, candidate identity or Expert binding. Sync success cannot widen
   permission or restore a removed source.
9. Old dependency and review evidence fails closed through current source authority,
   exact physical resources, native subject or signed producer identity, and grant
   checks. Current state must not be substituted under a stale reviewed expectation.
10. First-party consumers derive only from trusted shipped capability declarations
    plus explicit Manager direct-read policy. Installed third-party packages, Registry
    assignments and bindings cannot widen those consumers or change their policy digest.
11. **Use with Floe** is one connection-level Access permission control. Inspect is
    read-only; Off pauses Observe; On performs fresh review. Source editors are separate
    Connections operations, even when presented on the same connection detail screen.
12. Feasibility query review, exact-recipient processing consent and Act authority remain
    separate contextual authorities. Observe does not imply model transfer or Act.

Hosted acquisition retains the generic signed View preview/admission/read/release path.
Provider routing and leaves stay at the provider/server boundary. Pairing authenticates
the client/producer relationship but creates no connector data permission. Native
and hosted sources preserve exact evidence, release fences and provider-drift denial.
No global Vault transaction spans provider or model I/O.

## Amendments

### ADR 0027 — authority namespaces

The wording about processing/consumer/action policy epochs is historical insofar as
it implies a standing consumer-policy epoch. Standing Observe uses `GrantAuthority`
for permission and `SourceAuthority` for current source truth; `policy_digest` is
compare-only evidence. Identity, pairing, revocation, observation and uncertain-write
recovery foundations remain valid.

### ADR 0028 — source resources versus permission presentation

Connection UI may display the exact current physical source set, but that set is not
copied into standing `GrantScope.resources`. Editing resources while Use with Floe
is active updates Connections and advances source authority; changing leaves alone
does not expand or re-review the logical grant. Earlier evidence becomes stale.
Off/On or an actual permission-policy change invokes fresh grant review. System
permission, source configuration and Access permission remain distinct; the global
overview is not a second source or grant editor.

This amends the resource-edit coordination and Calendar grant-resource wording in
ADR 0028, including its corresponding resource-edit acceptance criterion. It does
not change the pairing ceremony, identity repair or revocation decision.

### ADR 0030 — immutable durable Observe review

Durable Observe targets no longer carry a separate policy authority. Review binds
the applicable source/connection revision, native subject or signed producer evidence,
canonical logical member resources, compare-only policy digest and exact expected
grant state or absence. Physical source resources belong to acquisition/provenance
and exact-recipient review where applicable, not copied standing permission scope.

Conversation still owns immutable review, durable decision intent, CAS resolution,
identical-command recovery and origin-linked fresh resume. Material drift supersedes
review before mutation; recovery never silently widens a recorded decision.

## Alternatives rejected

- **Union/intersection of per-leaf Expert consumers:** configuration cannot define
  permission; intersection can erase legitimate readers and union can overgrant.
- **Copy current leaves into every standing grant:** duplicates source truth and
  turns source configuration edits into permission ceremonies.
- **Advance or review grants on every source edit:** conflates source and permission
  epochs; exact stale-evidence checks already fence changed sources.
- **Automatically rebind Experts after edits:** creates a second mutation authority
  over configuration and risks silently rerouting pending work.
- **Keep a separate consumer-policy authority:** duplicates standing permission
  authority; deterministic policy comparison expresses review drift without an epoch.
- **Treat Expert binding as permission:** Registry configuration must not admit reads
  or grant third-party access.

## Consequences

Standing Observe has two authority namespaces: source and grant. Many physical leaves
can sit behind one logical permission while exact provenance survives every read and
later reauthorization. Source edits stale evidence without grant/binding churn; provider
drift remains fail closed and third-party authorization is never implicit.

Exact-recipient consent still binds both logical and physical resources and both
authorities, so changed source evidence requires a new contextual approval. Actions
retain exact-destination/source fences, durable pre-dispatch intent and lookup-only
uncertain-write recovery. Observer timeout and disposal never cancel recorded work.

RestrictedSubset, if ever needed, must be a deliberate future product/authority mode,
not accidental per-leaf Expert narrowing or a compatibility path.

Current ownership and runtime details live in [modules](../architecture/modules.md),
[runtime](../architecture/runtime.md) and [authority/recovery](../architecture/authority-recovery.md).
