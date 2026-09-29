# Connection-owned Observe authority simplification

## Authority, baseline and scope

This is the authoritative execution plan for replacing Floe's current resource-by-resource Observe/grant model with a connection-owned source scope and connection/View permission model. It is a target and ordered cutover, not a claim that the target is already implemented.

- Source baseline: the original plan was authored from main at 1af55ea6264cb536f458717a8727c784431e4918. Checkpoint 00 line-level anchors were re-resolved on 2026-09-28 against main at eb55389dc66ce30ad9693f739569dc1b58f2b2f3.
- Intervening convergence: eb55389dc66ce30ad9693f739569dc1b58f2b2f3 removed fixed Calendar selection-count limits and added >4/>128 regression coverage. Checkpoint 00 treats that as existing code to verify, not work to re-implement.
- Evidence at plan creation: current Rust, Go, Flutter, architecture docs, accepted ADRs and the September 27 first-party Observe history were inspected. No runtime/code verification is claimed by the planning commits.
- Compatibility policy: no internal backward compatibility is required. Disposable development profiles may be recreated when persisted meaning changes. Do not add legacy decoders, vNext paths, dual schemas, forwarding adapters or old/new runtime branches.
- Destructive scope: obsolete files, types, fields, tests, fixtures, storage tables, protocol operations and package-private abstractions may be deleted when the checkpoint's canonical replacement is complete.
- External boundaries remain real: provider OAuth contracts, OS APIs, cryptographic proof, external side effects and exact-recipient consent are not weakened by this plan.

Read AGENTS.md, .agents/skills/architecture-change/SKILL.md, this file, the first incomplete checkpoint, docs/architecture/invariants.md and the relevant current owner documents. Authority/recovery work must also read docs/architecture/authority-recovery.md. Code changes finish with the code-change-verification skill.

## Why this replacement exists

The current Calendar failure is not one isolated invalid_input. The present model combines four independent facts:

1. a Connection owns a current resource set;
2. an Expert assignment selects a source;
3. Access grants source use to consumers;
4. Context acquires current evidence.

Today Calendar leaf resources still leak through all four layers. first_party_observe::native_calendar_policy_for_target computes the intersection of Expert consumers selected for every Calendar resource, and GrantScope rejects the resulting empty consumer set. The former fixed >4 Calendar authorization/read caps were removed by eb55389dc66ce30ad9693f739569dc1b58f2b2f3; that cleanup is useful but does not change the incorrect permission ownership or leaf-scoped Expert binding model.

The same representation also created:

- Calendar resource candidates per calendar rather than per connection;
- GrantSourceBinding carrying live SourceAuthority;
- a separate ConsumerPolicyAuthority epoch;
- CalendarGrantPolicy storage;
- remote Calendar grant/preview/admission code parallel to generic remote Views;
- Day-owned Calendar connection authority despite Connections being the lifecycle owner;
- selected_resources and granted_resources as duplicated UI projections;
- remote Flutter resource plumbing solely for Calendar.

The plan removes these causes rather than patching the observed intersection.

## Final decisions

| Concern | Final owner and rule |
|---|---|
| Connection identity and lifecycle | Connections. Account/source replacement gets a new connection identity. |
| Current connector resource set | Connections. One canonical current resource set; no Floe-owned duplicate selection list. |
| Live source/resource revision | Connections or the producer that Connections observes, expressed as SourceAuthority. Resource/subject change advances SourceAuthority, not the standing Observe grant. |
| Standing Observe permission | Access. A grant binds stable connection identity plus logical View resource, consumer set, operation, purpose and processing. |
| Expert source configuration | Experts. Bind capability + exact connection/View; Calendar leaf IDs are not Expert configuration. |
| First-party consumer policy | App product policy over trusted shipped manifests/capabilities. Assignment resource selection never computes grant consumers. |
| Leaf resource mechanics | Provider/source adapters. Context resolves the connection's current resources at acquisition time. |
| Evidence/provenance | Context. Dependency records the logical grant scope, current SourceAuthority and exact source resources actually observed. |
| Review drift identity | A canonical policy digest plus source/grant expectations. It is not a second authorization epoch. |
| External-model transfer | Existing exact-recipient Access authority. Standing Observe remains separate. |
| Actions | Existing Actions authority and durable uncertain-write recovery. Observe simplification never implies Act. |

## Target topology

~~~
Connection
  stable identity
  current resources
  SourceAuthority
       |
       +----------------------+
       |                      |
       v                      v
Access standing grant       Context acquisition
  logical View                resolve current resources
  consumers                   provider read
  GrantAuthority              exact provenance
       |                      |
       +----------+-----------+
                  v
             Expert / Manager
~~~

The defining invariant is:

~~~
Connection resource change
  != Grant change
  != Expert binding change
~~~

A resource change advances SourceAuthority and invalidates/re-authorizes affected evidence. It does not require a new grant ceremony while Use with Floe remains enabled.

## Canonical contract decisions

These are frozen unless checkpoint 00 finds a contradiction in current source that makes the target unsafe.

1. GrantSourceBinding remains the stable grant source identity but loses SourceAuthority. It names Person, Connection, Connector and execution owner only.
2. GrantScope.resources names logical permission resources such as calendar.timeline:<connection-id>, mail.communication:<connection-id> and work.context:<connection-id>; leaf calendar/contact/folder IDs do not belong there by default.
3. ContextDependency gains an explicit current SourceAuthority and exact source_resources observed during acquisition. Its existing grant resources remain the logical Access scope.
4. Connection resource scope is the only standing leaf-resource list. There is no second Floe-visible list to reconcile.
5. ConsumerPolicyAuthority is removed. GrantAuthority is the standing permission epoch; SourceAuthority is the current source epoch.
6. Pending review uses an opaque deterministic policy_digest derived from the exact intended GrantScope/View policy. A digest detects review drift but grants no authority.
7. Built-in first-party consumers are derived from trusted shipped manifests/capabilities, not Registry assignment selection. Third-party/extension packages never receive automatic first-party authority.
8. Calendar Expert selection is connection/View-level. Context reads all current resources exposed by that connection unless a future explicit RestrictedSubset product feature is deliberately designed.
9. A future RestrictedSubset, if ever required, is an explicit Access/Connection policy mode. This plan does not preserve today's accidental per-leaf Expert narrowing as that feature.
10. Feasibility contextual review, exact-recipient consent and Act approval are not standing Connection Observe and are not collapsed into it.

## Scope boundaries

In scope:

- native and remote Calendar Observe;
- generic remote View Observe where required for unification;
- DataAccessGrant/ContextDependency source authority semantics;
- ConsumerPolicyAuthority deletion;
- first-party policy composition and Expert source candidates/bindings;
- standing personal Observe for Contacts, Attention and Wellbeing where the same connection-wide model applies;
- Rust/Go/FFI/Flutter product wire and Use with Floe UI;
- Vault storage simplification;
- tests, architecture docs and ADR convergence.

Not forced into the same abstraction:

- Feasibility query-specific approval;
- model exact-recipient consent;
- Actions approval/execution/reconciliation;
- credentials or OAuth;
- dynamic third-party connector/plugin loading.

No new workspace crate is expected. Existing Connections, Access, Context, Experts, Day, Vault, Providers and App ownership is sufficient.

## Checkpoint order and status

| Checkpoint | Responsibility | Status |
|---|---|---|
| 00 | [Baseline, executable regressions and contract freeze](00-baseline-and-contract-freeze.md) | Complete |
| 01 | [Connection-owned source/resource authority](01-connection-resource-authority.md) | Complete |
| 02 | [Stable grant source and dependency semantics](02-grant-and-dependency-cutover.md) | Complete |
| 03 | [Native Calendar vertical cutover](03-native-calendar-vertical.md) | Complete |
| 04 | [Remote Calendar -> generic remote View](04-remote-observe-unification.md) | Complete |
| 05 | [ConsumerPolicyAuthority elimination](05-consumer-policy-elimination.md) | Complete |
| 06 | [Standing personal Observe convergence](06-personal-observe-convergence.md) | Complete |
| 07 | [Product wire, FFI and UI convergence](07-product-wire-and-ui.md) | In progress |
| 08 | [Legacy purge and conformance closure](08-deletion-and-conformance.md) | Not started |
| 09 | [Full verification and documentation/ADR convergence](09-verification-and-docs.md) | Not started |

Order is 00 -> 01 -> 02 -> 03 -> 04 -> 05 -> 06 -> 07 -> 08 -> 09.

A checkpoint closes only when its canonical path is implemented, all in-scope callers use it, obsolete code/tests for that checkpoint are deleted, residual searches are classified, affected architecture docs match the actual code, and required checks pass. A short compile break inside one checkpoint is acceptable. A compatibility wrapper retained only to split commits is not.

## Cross-checkpoint invariants

Every checkpoint must preserve:

- no source read from a foreign Person/connection/execution owner;
- no grant expansion from provider drift or sync success;
- third-party Experts receive no automatic first-party authority;
- source pause/revoke blocks later admission and release;
- evidence records exact source/grant/provenance needed for reauthorization;
- external model transfer still requires exact-recipient consent;
- Observe never implies Act;
- durable external-write intent and uncertain-outcome recovery remain unchanged;
- no global Vault transaction across provider/model I/O;
- user-visible Use with Floe remains connection-level.

## Execution discipline

Before each checkpoint:

1. fetch/recheck main and record start HEAD/worktree;
2. re-resolve every symbol/path named by the checkpoint because line offsets are not stable;
3. preserve unrelated user changes;
4. inspect current tests before editing production;
5. use a fresh development profile rather than writing migration compatibility when persisted meaning changes.

During each checkpoint:

1. establish the new owner contract;
2. cut all in-scope callers directly;
3. rewrite tests to the final semantic model;
4. delete the replaced path in the same checkpoint;
5. run concept-level residual searches, not only exact symbol searches.

Do not advance after a failed deletion gate merely because the new path works.

## Required implementation report

Every checkpoint report records:

1. start HEAD and resulting commit SHA(s);
2. exact substeps completed;
3. changed owners/contracts and the one canonical runtime path;
4. migrated callers;
5. deleted files/types/routes/fields/tests/fixtures;
6. targeted and broad checks actually run;
7. residual matches and their disposition;
8. architecture/ADR/product docs changed;
9. next checkpoint blocker, if any.

No invented test counts. A skipped Apple/native prerequisite is reported as skipped/unavailable, not passed.

## Agent start instruction

~~~
Read AGENTS.md,
docs/development/plans/connection-observe-authority/README.md,
and the first incomplete checkpoint document.
Recheck current main and worktree. Execute only that checkpoint.
Optimize for the smallest final system, not the smallest diff.
Migrate all in-scope callers and delete obsolete code/tests in the same checkpoint.
Do not add compatibility paths or preserve disposable local schema formats.
Preserve Access/Context provenance, exact-recipient consent, Actions recovery and
third-party Expert denial. Report using the plan format and stop at a failed gate.
~~~

## Plan lifecycle

This bundle is temporary execution context. Current architecture documents change when implementation changes, not in advance. The durable rationale change should be captured by an ADR during checkpoint 09 (or earlier when the decision first lands).

After checkpoint 09 and explicit user acceptance, delete this plan bundle and its docs/README pointer. Git history is the archive; completed migration plans must not remain default agent context.
