# ADR 0027: Separate connection authority, AI grants and observations

- **Status:** proposed; common design direction, runtime not implemented
- **Date:** 2026-09-12
- **Extends:** ADR 0024 observation/lease semantics, ADR 0025 Person ownership and ADR 0026 credential ownership

## Context

The local Calendar mirror increments its connection revision during ordinary synchronization.
The durable Calendar Expert binding pins that same revision, and conversation dispatch compares
them exactly. A pre-turn refresh can therefore invalidate an otherwise unchanged access scope.
Reloading a conversation does not repair the authority mismatch.

The Go connection record already uses revision for a different purpose: protecting source-scope
changes. Other connectors expose lifecycle, freshness and provider revisions through different
paths. A global revision rename or automatic rebinding would either keep conflating data and
authority or risk silently enlarging user consent.

## Proposed decision

1. Keep one Person-owned connection identity for a specific source account and execution owner.
   Replacing account, tenant or owner creates a new identity rather than inheriting old grants.
2. Use the existing `DataAccessGrant` vocabulary for AI scope, categories, uses, consumers and
   processing policy. Connection availability does not itself authorize AI use or mutations.
3. Separate owner-issued source epochs, grant access epochs, processing/consumer/action policy epochs,
   immutable observation IDs, storage row versions and provider-native revision/cursor values.
   Never compare or substitute counters across these namespaces.
4. Authorize the intersection of current source authority, connection scope, grant, consumer
   permissions and purpose/processing policy. Scope changes are not inferred from sync success.
5. Acquire bounded Views at tool invocation, pin immutable evidence, and preserve source/grant/
   policy dependencies through model inputs, artifacts, output, continuation and learning.
   Ordinary refresh does not invalidate access; actual authority changes invalidate affected work.
6. Put authorization in Core and the executing source host, not Flutter or model instructions.
   Remote producers require verifiable owner authority in addition to pairing; opaque IDs or
   self-asserted epochs are not credentials.
7. Serialize authority mutations with short admission/output fences. Revocation blocks later
   admissions/releases, but cannot recall already transmitted data or a provider side effect.
   Offline remote execution without current authority verification is not enabled by this design.
8. Keep Act policy, exact-target approval, provider preconditions and durable attempt identity
   separate from Observe leases. Unknown outcomes are reconciled, never blindly replayed.
9. Degrade unavailable source capabilities rather than disabling general conversation. Distinguish
   source recovery, consent review, conversation conflicts and uncertain action outcomes.
10. Migrate verified identities and consent without scope expansion; preserve deny/tombstone and
    action-attempt records. Unknown legacy authority requires review. Mixed versions cannot fall
    back to a less protected source path.

## Alternatives not selected

- **Stop every revision increment:** loses storage conflict and data-change detection.
- **Rewrite Expert revision after every sync:** conflates synchronization with authorization and
  remains race-prone; it can silently rebind broader or different source scopes.
- **Ignore revision checks:** admits revoked/replaced sources and stale action preconditions.
- **Catch all `StaleContext` as empty data:** hides missing evidence and encourages false conclusions.
- **One global lock/version for all connectors:** unrelated sources block each other and remote
  provider operations still cannot participate in an atomic local transaction.
- **A universal raw snapshot store:** violates domain retention and device-only data boundaries.

## Consequences and delivery

The common boundary is identity, authorization, evidence and lifecycle—not a single storage or sync
implementation. Domain adapters still own scope interpretation, freshness, coverage and action
preconditions. Existing descriptor, retention, encrypted vault and action-ledger mechanisms remain.

The first vertical implementation targets native Calendar and headless tool-time access, followed
by server Calendar/Mail and the remaining context domains. The design does not claim that all
connectors have the reported Calendar bug, nor that cross-device authorization has shipped.

- [Semantic contract](../product/integrations-and-privacy.md)
- [Runtime, migration and conformance plan](../architecture/authority-recovery.md)

Runtime changes, live acceptance evidence and rollout approval remain separate work.
