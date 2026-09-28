# 02: Stable grant source and dependency semantics

Prerequisite: 01 complete.

Status: Complete.

Planning base: `main` at `582d1548d0ed8b9d63bc723076f6ed0dbda21cae` on 2026-09-28.

Checkpoint 01 established the owner boundary this checkpoint depends on: Connections now owns current Calendar resources, local source configuration revision, `SourceAuthority`, and native subject identity; Day owns mirror state only. Checkpoint 02 now removes live source epoch from standing grant identity and makes provenance state explicit in `ContextDependency`.

This checkpoint is a contract and persistence cutover. It changes the meaning of `GrantSourceBinding`, `DataAccessGrant` storage, dependency serialization, current-source reauthorization, and exact-recipient processing scopes. It must not reintroduce source authority into Access grant identity under another field or table.

Line numbers below are planning-base anchors on `582d1548`. Re-resolve symbols on the actual start HEAD before editing.

## Exit state

Checkpoint 02 is complete only when all of the following hold:

1. `GrantSourceBinding` contains only stable source identity:
   - Person;
   - Connection;
   - Connector;
   - execution owner.
2. `GrantSourceBinding` contains no `SourceAuthority`, source incarnation, source epoch, or compatibility alias.
3. A `DataAccessGrant` source is immutable after grant creation. Re-review/activate may change grant scope/state/GrantAuthority but may not replace stable source identity.
4. Source/account/execution-owner replacement requires a new `GrantId`; it cannot mutate an existing grant into another source.
5. `data_access_grants` persistence contains no `source_incarnation` or `source_epoch` columns, indexes, predicates, or payload/index consistency checks.
6. Grant lookup by source uses only stable source identity; current `SourceAuthority` is checked by the source owner/read path, not by grant identity.
7. `ContextDependency` stores:
   - stable `GrantSourceBinding`;
   - logical/permission `resources`;
   - explicit current `source_authority`;
   - explicit canonical `source_resources` representing the exact provider/leaf resources actually observed.
8. `ContextDependency.resources` means the Access grant resources the dependency was admitted under. It is never reused as the source leaf/provenance list.
9. `ContextDependency.source_resources` is provenance/current-source evidence, not a second grant scope and not authority to widen a future read.
10. `ContextDependency` serialization/canonical identity deliberately changes; the prior shape is rejected rather than decoded.
11. generic connection/View resource formatting has one shared contract helper. Equivalent `remote_view_resource` formatters do not remain duplicated.
12. Generic remote View grants use stable source + logical connection/View resource and survive a producer `SourceAuthority` change without becoming a different grant.
13. Native Calendar and remote Calendar special stacks are converted to the new stable-source/dependency contract, but their existing leaf grant resources are allowed as explicit bounded transitions until 03 and 04 respectively.
14. Native Calendar dependency provenance records exact Calendar IDs in `source_resources`, even while the 02-to-03 transitional grant scope still contains leaf Calendar IDs.
15. Remote Calendar dependency provenance records the exact remote Calendar leaf in `source_resources`, even while the special grant remains leaf-scoped until 04.
16. Personal dependency provenance records explicit `source_authority` and exact source resources. Because standing personal sources are not Connections-owned until 06, their current source epoch has one temporary owner outside `DataAccessGrant`.
17. `ConsumerPolicyAuthority` remains in `ContextDependency` and policy side tables until checkpoint 05. This checkpoint does not remove it.
18. exact-recipient `ProcessingSourceScope` preserves both logical grant/View identity and exact observed source resources, plus current `SourceAuthority`.
19. model/history reauthorization reloads current grant authority and current source facts independently. A source epoch change can stale evidence without rewriting or pausing the standing grant.
20. Vault transaction authority validates grant/policy state only. Provider/current-source I/O is not moved into a Vault transaction.
21. current-source checks remain in owner-specific dependency resolvers and existing pre-handoff/post-response fences.
22. `data_access_grant_cleanup.invalidated_incarnation/invalidated_epoch` remains because it is `GrantAuthority` cleanup state, not source authority.
23. remote signed authorization/source-preview structures may still carry source incarnation/epoch as current-source proof; those fields are not standing grant identity and must not be removed merely by name.
24. no old access-grant/dependency decoder, dual schema, fallback query, vNext type, or source-epoch compatibility path remains.
25. current architecture/provenance documentation matches the implemented split.
26. parent README marks 02 Complete and 03 remains Not started.

## Checkpoint boundary with 03/04/05/06

The final plan says standing Observe permission uses a logical connection/View resource. The ordered checkpoint bundle intentionally reaches that final state in stages.

### Completed in 02

- stable grant source identity;
- source epoch removed from standing grant persistence and mapping identity;
- explicit dependency `source_authority`;
- explicit dependency `source_resources`;
- one shared connection/View resource formatter;
- generic remote View paths use that formatter immediately;
- processing/recipient provenance is split correctly;
- current-source reauthorization is independent from grant identity.

### Intentionally retained for 03

Native Calendar still has the 00-proven leaf-selection architecture:

- per-leaf Expert candidates;
- per-leaf first-party consumer intersection;
- `CalendarAccessChange` review carrying Calendar IDs;
- native Calendar grant scope using leaf Calendar IDs.

02 migrates those paths to stable source + explicit source provenance without completing the native logical-View vertical. Checkpoint 03 changes native Calendar grant/candidate/review/read semantics to one `calendar.timeline:<connection>` permission resource and current Connections resources at read time.

### Intentionally retained for 04

Remote Calendar special preview/grant/read APIs and leaf grant resource remain until 04. 02 removes source epoch from their `GrantSourceBinding` and dependency identity, but does not prematurely replace the special stack.

### Intentionally retained for 05

`ConsumerPolicyAuthority`, Calendar policy rows, personal consumer-policy fields, remote mapping policy epochs, and `ContextDependency.consumer_policy` remain until 05.

### Intentionally retained for 06

Contacts / Attention / Wellbeing current-source ownership is not yet Connections-owned. 02 must remove source epoch from their `DataAccessGrant`, so the current personal source epoch needs one bounded interim owner. 06 moves standing personal source state to Connections and removes that interim authority. Feasibility remains an explicit contextual exception as defined by 06.

Do not start 03, 04, 05, or 06 semantics early merely to make 02 compile.

## Final contract after 02

### Stable grant source

Target:

~~~
GrantSourceBinding
  person_id
  connection_id
  connector
  execution_owner
~~~

`SourceAuthority` is not a grant source field.

There should be no `same_identity()` method whose only purpose is to work around equality including source epoch. Prefer direct equality of the now-stable value. If a named helper still adds semantic clarity, it must be exactly equivalent to equality and have real callers; otherwise delete it.

### Dependency split

Target:

~~~
ContextDependency
  person_id

  grant_id
  grant_authority

  source                  # stable GrantSourceBinding
  resources               # exact Access grant permission resource(s)

  source_authority        # source epoch observed/admitted for this evidence
  source_resources        # exact physical/provider resources actually observed

  categories
  operation
  purpose
  consumer
  processing

  consumer_policy         # temporary until 05

  observation_id
  query_fingerprint
  lease_invocation_id
  process_incarnation_id
  observed_at
  expires_at
~~~

Rules:

- `resources` and `source_resources` are separately canonical, sorted, unique, non-empty for source-backed evidence.
- `resources` must remain admitted by the current `GrantScope`.
- `source_resources` do not need to be members of `GrantScope.resources`.
- `source_resources` must never be interpreted as permission to acquire a new resource later.
- `source_authority` must be valid.
- exact source resources used by the acquisition must be recorded after the read is admitted and before evidence is published.
- a dependency whose grant is still current but source authority is stale is stale evidence, not a reason to rewrite the grant.
- old serialized dependencies without explicit `source_authority` / `source_resources` fail closed.

### Dependency identity/canonicalization

Current `ContextDependency::identity()` uses:

~~~
(observation_id, grant_id, grant_authority, source.source_authority)
~~~

After source epoch leaves `GrantSourceBinding`, use the explicit dependency `source_authority` instead.

Keep canonical dependency bytes as the full serialized dependency. If two dependencies have the same identity but different `source_resources`, query fingerprint, consumer policy, or other payload, `DependencyCoverage` merge must continue to treat that as a conflict rather than silently coalescing it.

### Processing review scope

Target `ProcessingSourceScope` must distinguish permission identity from physical provenance. Prefer explicit names:

~~~
ProcessingSourceScope
  connection_id
  connector_id
  grant_resources
  source_resources
  categories
  operation
  purpose
  consumer
  grant_id
  grant_authority
  source_authority
  policy_authority       # temporary until 05
~~~

If retaining `resources` for compatibility with internal naming would make the meaning ambiguous, rename it now. There is no need to preserve the old local serialization shape.

The exact-recipient consent identity must change if either:

- logical permission resource changes; or
- exact observed `source_resources` changes; or
- `SourceAuthority` changes.

A logical View alone is not proof that every current/future leaf was reviewed for external transfer.

## Planning-base code anchors

### Contract: stable source and dependency

`crates/contracts/context/src/lib.rs` on `582d1548`:

| Lines | Symbol | Current coupling |
|---|---|---|
| ~242-267 | `ResourceHandle` | Shared resource value; best location for connection/View resource helper. |
| ~383-439 | `GrantSourceBinding` | Still stores `source_authority`, constructor takes it, accessor exposes it. |
| ~647-865 | `ContextDependency` | Has only one `resources` vector and derives source epoch from `GrantSourceBinding`. |
| ~850-865 | dependency `identity()` | Includes `source.source_authority()`. |
| ~870 onward | `DependencyCoverage` | Canonical conflict/merge behavior depends on dependency identity and bytes. |

### DataAccessGrant source mutation

`crates/modules/access/src/data_access_grant.rs`:

- `DataAccessGrant` stores one `GrantSourceBinding`.
- `activate_review`, `review_active`, and `review` currently accept a replacement source.
- `validate_transition_source` calls `same_identity` so source epoch can currently change while the grant remains the same grant.

`crates/modules/access/src/application/grants.rs`:

- `AccessGrantMutation::{Review, Activate, ReviewActive}` each carries `source`.
- `apply_grant_mutation` forwards the source into the existing grant.

After 02 an existing grant source is immutable. Narrow these mutation contracts rather than passing the same stable source redundantly.

### Vault standing grant persistence

`crates/adapters/vault/src/vault/access_grants.rs`:

| Anchor | Current state |
|---|---|
| schema creation around line 61 | `data_access_grants` contains `source_incarnation`, `source_epoch`. |
| create/insert paths | write source incarnation/epoch into indexed columns. |
| `find_data_access_grant_by_source_in_transaction` | already queries stable source columns but decodes a payload that still has source epoch. |
| `data_access_grants_for_source` around ~291 | query includes source incarnation/epoch and docs call full source+authority identity. |
| mutation update around ~586 | rewrites source incarnation/epoch from the grant source. |
| `grant_values` / `decode_grant` around ~860+ | serialize and cross-check source epoch columns. |

`data_access_grant_cleanup.invalidated_incarnation/invalidated_epoch` refers to invalidated **GrantAuthority**. Keep it.

### Calendar grant SQL outside access_grants

`crates/adapters/vault/src/vault/calendar_grants.rs` has direct `data_access_grants` SQL around the current source-specific lookup. It selects/predicates `source_incarnation/source_epoch` independently of the shared helper. Migrate those queries too.

`calendar_scope()` still maps Calendar IDs directly to `GrantScope.resources`. This is an intentional 02-to-03 transition.

### Remote Calendar special stack

`crates/adapters/vault/src/vault/remote_calendar_grants.rs` searches stable source + leaf resource, but decodes the global `data_access_grants` schema containing source epoch.

`crates/modules/access/src/application/remote_calendar.rs`:
- `RemoteCalendarSourceReference` carries live source authority from the signed producer.
- `remote_calendar_source()` currently embeds that authority into `GrantSourceBinding`.
- `remote_calendar_scope()` is leaf-resource grant scope.
- dependency source admission compares `reference.source_authority` against `dependency.source().source_authority()`.

After 02 the signed reference keeps source authority, but `GrantSourceBinding` does not. Compare against `dependency.source_authority()` instead.

### Generic remote View mapping

`crates/adapters/vault/src/vault/remote_view_grants.rs`:

- `RemoteViewGrantMapping.source` is `GrantSourceBinding`.
- table `remote_view_grant_mappings` additionally indexes `source_incarnation` / `source_epoch`.
- unique/source lookup identity includes source epoch.
- `find_remote_view_grant`, `remote_view_grant_binding`, `remote_view_grant_policy`, and dependency-policy validation all query or compare source epoch.
- `policy_incarnation` / `policy_epoch` are `ConsumerPolicyAuthority` and remain until 05.

02 removes the source epoch columns/identity while preserving the consumer-policy epoch.

### Generic remote View resource formatting

`crates/modules/context/src/application/remote_views.rs:31-46` currently defines:

- `remote_view_resource(view_id, connection_id) -> String`;
- `split_remote_view_resource`.

This is already the desired `<view-id>:<connection-id>` semantic form but it lives in Context and is remote-specific. Move the canonical formatting/validation to the shared context contract and delete the duplicate formatter once callers migrate.

### Dependency builders

Current builders that must gain explicit source state:

- `crates/modules/access/src/application/calendar_lease.rs`
  - `CalendarLeaseKey.calendar_ids` already contains exact native Calendar leaves;
  - `calendar_lease_dependency` currently copies `admission.scope.resources` into dependency resources and has no separate provenance resources.
- `crates/modules/context/src/application/remote_views.rs`
  - `remote_view_dependency` builds `ContextDependency`.
- `crates/modules/context/src/application/personal_sources.rs`
  - `personal_dependency`;
  - Attention dependency construction;
  - people / wellbeing / feasibility paths.
- tests/fixtures across Context, Conversation, runtime, Vault, App.

### Native dependency reauthorization

`crates/app/src/vault_host/calendar_access.rs:112+`:
`NativeCalendarDependencyResolver` currently converts `dependency.resources()` back into Calendar IDs. After 02 it must use `dependency.source_resources()`.

`crates/modules/context/src/application/native_calendar_view.rs:174+`:
`authorize_native_calendar_dependency` similarly derives `selected_calendar_ids` from `dependency.resources()`. Switch to source resources.

`crates/modules/access/src/application/calendar_read.rs`:
`admits_native_calendar_read` currently compares current source authority with `admission.source.source_authority()` and compares grant scope resources with Calendar IDs. Move current source authority to explicit admission/dependency state. The grant-resource/Calendar-ID equality remains only as the 03 transition.

### Remote dependency reauthorization

`crates/modules/context/src/application/remote_sources.rs:711+`:
`authorize_remote_dependency`:
- loads grant by dependency grant ID;
- derives one resource through `remote_dependency_resource`;
- obtains a fresh signed preview;
- verifies producer/source;
- currently compares current preview authority against source authority embedded in dependency source.

After 02:
- grant comparison uses stable source + logical grant resource;
- signed preview current authority compares to `dependency.source_authority()`;
- exact source leaf/provider resource compares to `dependency.source_resources()`.

### Personal dependency reauthorization

`crates/modules/context/src/application/personal_sources.rs`:
- dependency builders currently copy grant source, which carries source epoch;
- `authorize_personal_dependency` compares dependency source == grant source;
- subject fingerprint and selected handles are already separate policy/source evidence.

After 02 personal source epoch comes from the bounded personal source state described below, not from the grant.

### Processing / recipient consent

`crates/contracts/context/src/processing.rs:82+`:
`ProcessingSourceScope` currently has one `resources` vector plus `source_authority`.

`ProcessingSourceScope::from_dependency` uses:
- `dependency.resources()`;
- `dependency.source().source_authority()`.

`crates/modules/access/src/application/model_dispatch.rs` derives exact-recipient review requirements through `ProcessingSourceScope::from_dependency`, and `RecipientConsent` hashes/persists those scopes. This is the critical external-transfer boundary for exact `source_resources`.

### SourceView / consumed lineage

`crates/modules/context/src/application/source_view.rs::validate_source_scope` correctly compares dependency permission resources to the `GrantScope`. Keep this behavior on `dependency.resources()`.

`crates/modules/context/src/application/consumed.rs` compares stored `GrantScope.resources()` with `dependency.resources()`. That remains logical grant-scope validation; do not change it to `source_resources`.

### Actions / Expert proposal provenance

`crates/adapters/vault/src/vault/agent_actions.rs` currently checks admitted `SourceSelectionReference.resource` against `dependency.resources()`.

During the 02-to-03 native Calendar transition, selected Expert resource is still a leaf Calendar, while `source_resources` is the physical leaf list. Use `source_resources` where a check is explicitly about the selected physical leaf. Checkpoint 03 changes the selected Calendar source reference to connection/View level and updates this check accordingly.

`crates/modules/actions/src/application/expert.rs::validate_context_calendar_source` also must compare:
- `dependency.source_authority()` to current Connections source authority;
- action destination Calendar ID to `dependency.source_resources()`.

### Release / current authority

`crates/modules/access/src/application/release.rs` delegates grant authority validation to `CurrentAuthority`.

`crates/adapters/vault/src/vault.rs::VaultTransactionAuthority` calls `validate_current_authority_in_transaction`.

`crates/adapters/vault/src/vault/personal_grants.rs:397+` currently validates:
- current DataAccessGrant;
- Calendar/personal/remote consumer policy.

Source epoch is only indirectly coupled today through grant/source equality. After 02 Vault must not become a source owner.

Keep source/provider/Connections I/O outside the Vault write transaction. Owner-specific `DependencyResolver` paths remain responsible for current source authority and exact source-resource reauthorization before model/history/output use. Vault's final storage transaction continues to fence current grant/policy state. Existing no-I/O `DependencyLiveness` checks may still run inside the transaction where they already represent in-process observation liveness; do not add provider or Connections storage I/O there.

## 02-A — baseline and residual inventory

Before code edits:

~~~
git fetch origin
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
git log -1 --oneline
python3 tools/architecture/check_boundaries.py
~~~

Read:

~~~
AGENTS.md
.agents/skills/architecture-change/SKILL.md
.agents/skills/code-change-verification/SKILL.md
docs/development/plans/connection-observe-authority/README.md
docs/development/plans/connection-observe-authority/02-grant-and-dependency-cutover.md
docs/architecture/invariants.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
```

Capture and classify before edits:

~~~
rg -n "GrantSourceBinding::try_new|GrantSourceBinding" crates
rg -n "\.source_authority\(\)" crates
rg -n "source_incarnation|source_epoch" crates
rg -n "ContextDependency::try_new" crates
rg -n "dependency\.resources\(\)|scope\(\)\.resources\(\)" crates
rg -n "ProcessingSourceScope" crates apps/client
rg -n "remote_view_resource\(" crates
rg -n "AccessGrantMutation::(Review|Activate|ReviewActive)" crates
rg -n "data_access_grants_for_source|find_data_access_grant_by_source" crates
```

Not every `source_incarnation/source_epoch` match is obsolete:

- `RemoteAuthorizationKeys` / signed producer admission may retain source epoch as live source proof;
- SourceAuthority serialization itself remains;
- personal bounded interim source state added by 02 is allowed until 06;
- grant cleanup `invalidated_incarnation/invalidated_epoch` is GrantAuthority cleanup.

Classify by semantic owner, not name alone.

## 02-B — cut `GrantSourceBinding` to stable identity

Primary file:

~~~
crates/contracts/context/src/lib.rs
```

Change `GrantSourceBinding` to:

~~~
pub struct GrantSourceBinding {
    person_id: PersonId,
    connection_id: ConnectionId,
    connector: ConnectorId,
    execution_owner: ExecutionOwnerId,
}
```

### Constructor

`try_new` takes exactly four arguments. Remove `SourceAuthority`.

Validation continues to prove:
- Person is valid;
- connection ID valid;
- connector valid;
- execution owner valid.

Delete:
- `source_authority` field;
- accessor;
- serialization field;
- validation of source epoch from this type.

### Equality / same_identity

With live epoch removed, stable source identity is ordinary value equality.

Delete `same_identity()` if callers can use `==` / `!=` directly. If a real caller needs a named predicate for readability, it must be a trivial stable-source equality and must not mask different semantics.

### Constructor migration

Update every constructor in:
- Access;
- Context;
- Vault;
- Actions;
- Conversation tests;
- runtime tests;
- App fixtures;
- provider tests.

Do not pass a dummy `SourceAuthority::new()` merely to preserve signatures.

### Contract test

Add a serialization regression proving:
- the serialized source has exactly the stable fields;
- adding `source_authority` is rejected by `deny_unknown_fields`;
- a source can be equal across independent current `SourceAuthority` values held elsewhere.

## 02-C — split `ContextDependency` permission and provenance

Primary file:

~~~
crates/contracts/context/src/lib.rs
```

Add explicit fields:

~~~
source_authority: SourceAuthority
source_resources: Vec<ResourceHandle>
```

Keep `resources` as grant permission resources.

### Constructor shape

Prefer an explicit parameter order that makes accidental swapping difficult:

~~~
ContextDependency::try_new(
  person_id,
  grant_id,
  grant_authority,
  source,
  resources,             // grant permission resources
  source_authority,
  source_resources,      // exact observed provider/leaf resources
  categories,
  operation,
  purpose,
  consumer,
  processing,
  consumer_policy,
  ...
)
```

If a small typed sub-struct reduces argument mistakes without adding a migration wrapper, use it. Do not preserve the old constructor.

### Validation

Require:

- stable source Person == dependency Person;
- valid grant and grant authority;
- valid explicit source authority;
- non-empty canonical permission `resources`;
- non-empty canonical `source_resources`;
- both vectors sorted and unique;
- every handle valid and non-wildcard;
- both vectors bounded by dependency size and an explicit maximum compatible with Connections' current resource bound;
- no requirement that source resources belong to grant resources;
- categories canonical/non-empty;
- existing consumer/operation/purpose/processing/freshness/UUID rules unchanged.

For a source whose exact observed resource is already a logical View, the two vectors may contain equal values. Their semantic roles remain distinct.

### Accessors

Add:

~~~
source_authority()
source_resources()
```

Retain:

~~~
resources()
```

with docs explicitly naming it as permission/grant resources.

### Canonical identity

Update `identity()` to use explicit `self.source_authority`.

Keep complete serialized bytes for conflict detection. Do not drop `source_resources` from canonical serialization.

### Persistence compatibility

No old decoder.

`DependencyCoverage::from_persisted_bytes` must reject dependencies missing the new fields. Tests should serialize an old-shaped JSON fixture and assert corrupt/rejected.

### Size budgets

Re-measure serialized dependency size tests and adjust the dependency max only if the new required provenance makes a currently valid bounded case exceed it. Do not loosen size budgets speculatively.

## 02-D — introduce one canonical connection/View resource helper

Primary owner:

~~~
crates/contracts/context/src/lib.rs
```

Add a helper such as:

~~~
connection_view_resource(view_id: &str, connection_id: &ConnectionId)
    -> Result<ResourceHandle, GrantValidationError>
```

Canonical form:

~~~
<view-id>:<connection-id>
```

Validate both components and final resource length. Do not return an unchecked String.

Add a split/parser helper only where a real current caller needs to recover the two parts. Keep parsing strict and round-trip through the formatter.

Migrate generic remote View callers from:

~~~
remote_view_resource(...)
```

to the shared helper.

Delete `crates/modules/context/src/application/remote_views.rs::remote_view_resource` and equivalent duplicate formatting once caller-zero.

Examples:

~~~
mail.communication:<connection>
work.context:<connection>
life.logistics:<connection>
calendar.timeline:<connection>
```

02 does not force the native/remote Calendar special stacks to use `calendar.timeline:<connection>` yet; 03/04 own that runtime cutover.

## 02-E — make DataAccessGrant source immutable

Modify:

~~~
crates/modules/access/src/data_access_grant.rs
crates/modules/access/src/application/grants.rs
```

### DataAccessGrant transitions

A `DataAccessGrant` receives its stable source at creation. Existing-grant review/activation cannot change that source.

Narrow methods conceptually to:

~~~
activate_review(expected, scope)
review_active(expected, scope)
review(expected, scope)
```

or equivalent internal methods.

Remove `source` argument from transition validation.

Delete `validate_transition_source` if stable source replacement can no longer occur.

### AccessGrantMutation

Change:

~~~
Review { source, scope }
Activate { source, scope }
ReviewActive { source, scope }
```

to scope-only forms:

~~~
Review { scope }
Activate { scope }
ReviewActive { scope }
```

or an equally narrow representation.

Creation still takes stable source.

### Source replacement

If a review resolves to a different Person/Connection/Connector/execution owner:
- do not mutate the old grant;
- create a new grant under the new stable source;
- leave/revoke the old grant according to the owner flow.

Do not overload `GrantAuthority` advancement to represent source replacement.

### Tests

Add/modify tests proving:
- source is byte-identical across review/activate;
- an attempted different source cannot be passed to mutation because the API has no such input;
- scope/state change still advances GrantAuthority according to current rules;
- exact no-op preserves authority where current grant semantics say so.

## 02-F — cut Vault access-grant schema to stable source

Primary file:

~~~
crates/adapters/vault/src/vault/access_grants.rs
```

This is a real persisted meaning change. Local development profiles are disposable. Change the schema marker deliberately and require a fresh profile; do not add migration SQL.

### New `data_access_grants` columns

Target:

~~~
grant_id
person_id
authority_owner
connection_id
connector
execution_owner
grant_incarnation
access_epoch
state
payload
```

Delete:

~~~
source_incarnation
source_epoch
```

### Schema marker

Advance `ACCESS_GRANT_SCHEMA_VERSION` only because the stored schema meaning actually changes. Fresh creation writes the new marker. Opening an old marker fails `UnsupportedVersion`/current repo error policy rather than attempting migration.

### SQL migration in code

Update every SELECT / INSERT / UPDATE column list and tuple index in `access_grants.rs`.

Update:
- create;
- create-in-transaction;
- get;
- list;
- source lookup;
- mutate;
- cleanup validation;
- full-store validation;
- tests/corruption fixtures.

### `data_access_grants_for_source`

Change lookup to stable:

~~~
person_id
authority_owner
connection_id
connector
execution_owner
```

No source authority predicate.

Because multiple grants for one stable source may legitimately represent different logical Views/resources, this method must not assume stable source alone means exactly one grant unless the caller's operation really expects one. Prefer resource/member-aware lookup where ambiguity exists.

Audit every caller currently using `data_access_grants_for_source(..., 2)` as a uniqueness assertion.

### Decode/index validation

`decode_grant` validates:
- indexed stable source fields == payload stable source;
- indexed GrantAuthority == payload grant authority;
- state == payload state.

It no longer reconstructs `SourceAuthority` from columns.

### Cleanup table

Do not alter:

~~~
data_access_grant_cleanup.invalidated_incarnation
data_access_grant_cleanup.invalidated_epoch
```

Those are invalidated grant authority values.

### Direct SQL outside file

Run:

~~~
rg -n "data_access_grants .*source_incarnation|source_incarnation.*data_access_grants|source_epoch.*data_access_grants" crates/adapters/vault
```

Migrate direct Calendar/remote queries in the same checkpoint.

## 02-G — remove source epoch from remote View grant mapping identity

Modify:

~~~
crates/adapters/vault/src/vault/remote_view_grants.rs
```

Keep the mapping until 05 because it still owns `ConsumerPolicyAuthority` bookkeeping.

Remove from mapping table/index:

~~~
source_incarnation
source_epoch
```

Stable uniqueness becomes something equivalent to:

~~~
person_id
view_id
connector
connection_id
execution_owner
```

plus whatever stable key is genuinely required.

Advance the remote View mapping schema marker for this real schema change; no migration.

### Mapping payload

`RemoteViewGrantMapping.source` becomes the new stable `GrantSourceBinding`.

It does not persist live source authority.

### Lookup APIs

Functions such as:

~~~
find_remote_view_grant
remote_view_grant_binding
remote_view_grant_policy
```

must locate the standing grant/mapping by stable source + View, not current source epoch.

Where current source authority is required to admit a read, pass/compare it separately against a fresh signed `RemoteViewSourceReference`, not as mapping identity.

### Dependency policy validation

`validate_remote_view_dependency_policy_in_transaction` should validate consumer-policy epoch only. It is a Vault policy check, not a source-currentness check.

Current remote source authority is revalidated by the remote dependency resolver from a fresh signed preview.

### Acceptance

Test:
1. review remote logical View under source authority A;
2. source preview later reports authority B for the same stable source;
3. stored `GrantId` / `GrantAuthority` remain unchanged solely due to B;
4. old dependency with A fails source reauthorization;
5. new dependency records B;
6. policy mapping still identifies the same standing grant.

## 02-H — bounded personal-source authority transition

Standing personal sources are not moved to Connections until 06. After source epoch leaves `GrantSourceBinding`, Access/Vault must not lose the current personal source epoch.

Use one explicit, bounded interim owner in the existing personal policy/source record. Do not create a second parallel table solely for migration if `personal_grant_policies` already owns reviewed subject/selected-source facts.

Primary files:

~~~
crates/modules/access/src/ports/personal_grants.rs
crates/modules/access/src/application/personal_grants.rs
crates/adapters/vault/src/vault/personal_grants.rs
crates/modules/context/src/ports/personal_source.rs
crates/modules/context/src/application/personal_sources.rs
```

### Personal policy state

Persist an explicit valid `SourceAuthority` alongside the current reviewed personal source facts.

Suggested fresh schema additions to `personal_grant_policies`:

~~~
source_incarnation
source_epoch
```

These columns are NOT `DataAccessGrant` identity. They are the current personal-source epoch until 06 moves standing source ownership to Connections.

Advance the personal policy schema marker because this is a real stored meaning change; no migration.

### Owner rules

For the same stable personal source:

- first review -> new `SourceAuthority`;
- exact no-op review -> preserve authority;
- trusted native subject change -> advance authority;
- Contacts selected handle set change -> advance authority;
- consumer-only policy change -> do not advance source authority;
- pause/re-enable without source change -> preserve authority;
- different stable source identity -> new grant/policy/source authority.

Feasibility query changes are contextual query changes, not automatically source truth changes. Keep their query identity in the existing feasibility review record/query fingerprint. Subject/source drift still changes source authority.

### Store ports

Expose a narrow method such as:

~~~
current_source_authority(grant_id)
```

from personal policy/records.

Review operations must accept or atomically compute/persist the exact source authority separately from stable `GrantSourceBinding`.

Do not reconstruct it from the `DataAccessGrant`.

### PersonalAccessOverview

Populate `source_authority` from the current personal policy/source state, not `grant.source()`.

### Context dependencies

Personal read builders record:
- stable grant source;
- grant permission resources;
- current personal policy `SourceAuthority`;
- exact source resources:
  - Contacts: exact selected handles actually read;
  - Attention: exact current attention source/view handle;
  - Wellbeing: exact provider/view resource;
  - Feasibility: exact source/provider evidence resource(s) required by the read, while query fingerprint remains contextual.

### 06 removal condition

Document in code near the temporary personal source authority owner:

~~~
Standing Contacts/Attention/Wellbeing source authority moves to Connections in
connection-observe-authority checkpoint 06. Do not use this record as grant identity.
```

This is a bounded owner transition, not backward compatibility.

Checkpoint 06 must delete this source-authority ownership for standing personal sources. Feasibility may retain a contextual source record if its explicit-query model still requires it.

## 02-I — update source binding helpers

Modify:

~~~
crates/modules/access/src/application/personal_sources.rs
crates/modules/access/src/application/remote_view.rs
crates/modules/access/src/application/remote_calendar.rs
crates/adapters/vault/src/vault/calendar_grants.rs
App native grant-source helpers
```

### Personal source helpers

Change:

~~~
source_binding(..., authority)
attention_source(..., authority)
contacts_source(..., authority)
wellbeing_source(..., authority)
feasibility_source(..., authority)
```

to build stable source identity only.

Current source authority is separate policy/source state.

### Remote source helpers

`remote_view_source(reference)` and `remote_calendar_source(...)` build stable `GrantSourceBinding`.

The reference's signed `source_authority` remains on `RemoteViewSourceReference` / `RemoteCalendarSourceReference`.

### Native Calendar grant source

`native_grant_source` / Calendar Vault helpers build stable source without epoch.

Current `SourceConnection.source_authority()` is passed separately into admission/dependency creation.

## 02-J — update native Calendar admission and dependency construction

Modify:

~~~
crates/modules/access/src/application/calendar_read.rs
crates/modules/access/src/application/calendar_lease.rs
crates/modules/context/src/application/native_calendar.rs
crates/modules/context/src/application/native_calendar_view.rs
crates/app/src/vault_host/calendar_access.rs
```

### CalendarReadAccessAdmission

Add explicit current source authority to the admission, or otherwise return it in an equally typed admission value. Do not recover it from `GrantSourceBinding`.

A useful shape:

~~~
CalendarReadAccessAdmission
  grant_id
  grant_authority
  source                 # stable
  source_authority       # current source epoch
  scope                  # permission/grant scope
  consumer_policy
  ...
```

### Grant admission

`admits_native_calendar_read` validates:
- stable source identity against current Connections source;
- explicit admission source authority against current Connections source authority;
- current exact Calendar IDs against source read evidence;
- current grant scope/consumer/category/processing.

During 02, native Calendar grant scope may still equal the Calendar leaf list. Keep that as a documented 03 transition.

### Dependency

`calendar_lease_dependency` records:

~~~
resources = admission.scope.resources()
source_authority = admission.source_authority
source_resources = CalendarLeaseKey.calendar_ids
```

Canonicalize `source_resources`.

### Native dependency resolver

Change all code that turns `dependency.resources()` into Calendar IDs to use:

~~~
dependency.source_resources()
```

Specifically:
- `NativeCalendarDependencyResolver`;
- `authorize_native_calendar_dependency`;
- lease/read continuity helpers;
- Actions validation of Calendar leaf provenance.

### Acceptance regression

Create a targeted regression:

1. Connections source resources = A;
2. standing grant/dependency read under source authority S1;
3. Connections resources change to A+B -> S2;
4. stored grant ID/GrantAuthority remain unchanged by the source change itself;
5. old dependency S1 fails current-source reauthorization;
6. new read records S2 and exact `[A,B]` source resources.

Because native grant scope is still leaf-based until 03, step 4 may require using the existing grant before 03's scope update semantics only when no grant review is triggered. Do not fake final 03 behavior in 02. The essential 02 proof is that the standing grant's source identity no longer changes merely because source epoch changes.

## 02-K — update generic remote View dependency construction/replay

Modify:

~~~
crates/modules/access/src/application/remote_view.rs
crates/modules/context/src/application/remote_views.rs
crates/modules/context/src/application/remote_sources.rs
crates/adapters/vault/src/vault/remote_view_grants.rs
crates/app/src/vault_host/remote_views.rs
```

### Grant

Use shared logical connection/View `ResourceHandle`.

Standing grant source is stable and mapping lookup ignores source epoch.

### Read

Fresh signed preview returns current:
- source authority;
- provider identity;
- connection revision;
- exact source resource.

Validate those as current source evidence.

### Dependency

Record:

~~~
resources = grant.scope.resources()       # logical view
source_authority = reference.source_authority
source_resources = [reference.resource]
```

If the generic remote source resource already equals the logical View handle, both vectors may be equal but remain semantically separate.

### Replay

`authorize_remote_dependency`:
1. validates dependency freshness;
2. loads exact recorded grant;
3. proves current GrantAuthority/consumer policy;
4. fetches fresh signed source preview;
5. compares preview `SourceAuthority` with `dependency.source_authority()`;
6. compares exact current source resource with `dependency.source_resources()`;
7. proves producer/provider/connection revision;
8. revalidates binding.

Do not query grant mappings by source epoch.

## 02-L — migrate remote Calendar special path without completing 04

Modify:

~~~
crates/modules/access/src/application/remote_calendar.rs
crates/adapters/vault/src/vault/remote_calendar_grants.rs
crates/modules/context/src/application/remote_sources.rs
crates/app/src/vault_host/remote_observe.rs
```

### Stable source

`remote_calendar_source` returns stable `GrantSourceBinding`.

### Current source evidence

Signed `RemoteCalendarSourceReference.source_authority` remains current source proof.

### Dependency

Record:
- grant resources: current special leaf grant resource for now;
- explicit source authority from signed reference;
- `source_resources = [exact calendar leaf read]`.

### Lookup

Remote Calendar grant lookup uses stable source + its current special leaf grant resource, not source epoch.

### Replay

`remote_calendar_dependency_source_admits` compares fresh reference authority with `dependency.source_authority()` and exact resource with `source_resources`.

Do not:
- delete the special remote Calendar APIs;
- replace them with generic remote View;
- change the product route.

Those are 04.

## 02-M — migrate personal dependency creation/replay

Modify all `ContextDependency::try_new` calls in:

~~~
crates/modules/context/src/application/personal_sources.rs
```

and associated test support.

### Contacts

Grant resource remains the current `PEOPLE_RESOURCE` standing permission in 02.

Dependency:
- `resources = [PEOPLE_RESOURCE]`;
- `source_authority = current personal source policy authority`;
- `source_resources = exact selected handles actually acquired`.

Reauthorization:
- stable source matches grant;
- grant authority/policy current;
- personal source authority current;
- trusted subject current;
- exact selected handle/source set still admits recorded evidence.

### Attention

Dependency:
- grant permission resource = `ATTENTION_RESOURCE`;
- explicit current personal source authority;
- source resource = exact attention source/view handle represented by the trusted observation.

Keep observation/process-incarnation checks.

### Wellbeing

Same split:
- grant permission resource = `WELLBEING_RESOURCE`;
- explicit source authority;
- exact source/provider resource.

### Feasibility

Keep explicit query review and fingerprint semantics.

Use stable source + explicit current source authority. Do not turn its contextual event/destination/query approval into standing connection Observe.

### Personal store/replay helpers

Any function that currently calls:

~~~
grant.source().source_authority()
```

must instead read current source authority from the personal source policy record.

Do not synthesize a new authority merely because it is no longer on the grant.

## 02-N — simplify replay admission contract

`crates/modules/access/src/application/admission.rs` currently has `ReplayRequest` and `ReplayTrust` carrying source authority separately but still comparing it to `dependency.source().source_authority()`.

First run a caller audit:

~~~
rg -n "ReplayRequest|ReplayTrust|admit_replay" crates
```

If production caller-zero remains true:
- delete `ReplayRequest`;
- delete `ReplayTrust`;
- delete `admit_replay`;
- delete tests that only preserve an unused abstraction.

Do not migrate a caller-zero abstraction to the new model.

If a live caller exists on the execution HEAD, update it to explicit:
- current source authority;
- exact source resources;
- grant permission resources;
and make stable source equality separate.

Owner-specific `DependencyResolver` is the preferred canonical reauthorization path.

## 02-O — exact-recipient processing and consent identity

Modify:

~~~
crates/contracts/context/src/processing.rs
crates/modules/access/src/application/model_dispatch.rs
crates/modules/access/src/application/recipient_consent.rs
protocol/FFI tests that serialize ProcessingSourceScope
conversation interaction tests
```

### ProcessingSourceScope contract

Represent both:

~~~
grant_resources
source_resources
```

plus explicit `source_authority`.

`from_dependency` maps them directly.

### Validation

Require both vectors canonical/non-empty.

They may be equal.

Keep:
- stable connection;
- connector;
- grant identity;
- GrantAuthority;
- consumer;
- categories;
- operation;
- purpose;
- temporary ConsumerPolicyAuthority.

### Recipient consent

Because `RecipientConsent` derives deterministic ID from `source_scopes`, a different exact source leaf set or source authority must derive a different consent identity/review requirement.

Add tests:

1. same logical View + same grant, source leaf A -> consent X;
2. same logical View + same grant, source leaf B -> consent Y;
3. same leaf set but advanced SourceAuthority -> different reviewed scope/consent;
4. order-only difference canonicalizes to same identity;
5. grant resource change changes identity.

Do not broaden prior recipient consent to future resources under the same logical View.

## 02-P — grant/dependency validation and SourceView semantics

Modify:

~~~
crates/modules/access/src/application/dependency.rs
crates/modules/context/src/application/source_view.rs
crates/modules/context/src/application/consumed.rs
```

### `validate_grant_dependency`

Validate:
- grant ID;
- GrantAuthority;
- stable source equality;
- dependency permission resources are within grant scope;
- categories/operation/purpose/consumer/processing;
- existing ConsumerPolicyAuthority semantics.

Do NOT validate source authority here. A `DataAccessGrant` no longer owns it.

Do NOT require `source_resources` to appear in grant scope.

### SourceView

`validate_source_scope` continues to compare dependency `resources()` against `GrantScope`.

Do not change it to `source_resources`.

### ConsumedLineage

Grant-scope binding stays:

~~~
binding.scope.resources() == dependency.resources()
```

or current equivalent.

Add a separate source-provenance comparison only where consumed evidence is expected to bind exact observed leaves. Do not conflate the two vectors.

## 02-Q — current-source reauthorization and release fences

The 02 contract removes the accidental guarantee obtained by embedding source epoch in the grant. Preserve real source checks explicitly.

### DependencyResolver is the source-current authority path

Owner-specific resolvers:

- native Calendar -> Connections + native subject/source adapter + Access grant;
- remote -> fresh signed producer preview + grant;
- personal -> personal source policy + trusted subject/observation.

Each must compare:
- stable source identity;
- dependency `source_authority`;
- dependency `source_resources`;
- current observation/query/freshness;
- current grant authority/policy.

### Vault transaction authority

`VaultTransactionAuthority` remains responsible for:
- current stored grant;
- GrantAuthority;
- state/revocation;
- consumer policy until 05;
- recipient/storage target identity.

It must not fetch Connections/provider state while an Immediate Vault transaction is open.

Update docs/comments so `validate_current_authority_in_transaction` is not described as source-current authority after 02.

### Model dispatch / external release

Keep the existing `DependencyResolver` checks:
- before provider handoff;
- immediately before transport consume;
- post-response before response release.

These are the source-current fences for model transmission.

### Conversation/history

History projection already reauthorizes dependencies through `DependencyResolver`. Keep that as the source-current gate before derived history is shown again.

### Session storage release

The Vault commit path may continue to:
- validate grant/policy inside the transaction;
- run only no-I/O `DependencyLiveness` checks inside the transaction.

Do not introduce provider/Connections I/O into that transaction.

Audit every production source-dependent session commit path. Ensure it has already passed the canonical resolver/source fence for the output being committed. If a path can commit newly derived source-dependent output without any resolver/source check, fix the composition before closing 02.

## 02-R — update Actions / proposal provenance

Modify:

~~~
crates/modules/actions/src/application/expert.rs
crates/adapters/vault/src/vault/agent_actions.rs
crates/adapters/vault/src/vault/expert_actions.rs
```

### Current source

Use:

~~~
dependency.source_authority()
```

for comparison with current Connections source.

Use:

~~~
dependency.source_resources()
```

for exact Calendar/provider leaf provenance while Calendar Expert selection is still leaf-level.

### Grant

`validate_grant_dependency` validates stable source + grant permission scope.

### Existing selected source

In 02, native Calendar selected `SourceSelectionReference.resource` is still a leaf. Where this code checks the actual physical leaf used, compare to `source_resources`.

Checkpoint 03 changes Calendar Expert selection to connection/View resource and should then update this selection check to grant/logical resource semantics.

Preserve all action write approval/recovery invariants.

## 02-S — Vault context dependency and coverage persistence

Inspect/update:

~~~
crates/adapters/vault/src/vault/context_dependencies.rs
conversation/task/evidence repositories that serialize ContextDependency
```

No special migration decoder.

Freshly written dependency bytes include explicit source authority/resources.

Open/validation of old development data must fail closed or require a fresh profile according to current repository policy.

Update corruption tests:
- missing `source_authority`;
- missing `source_resources`;
- unsorted/duplicate source resources;
- source resources too large;
- old source field containing `source_authority`.

Do not add default serde values for required new authority/provenance fields.

## 02-T — schema version and fresh-profile discipline

02 changes multiple real stored meanings:

- `data_access_grants`;
- `remote_view_grant_mappings`;
- personal policy source authority;
- persisted `ContextDependency`.

For tables with explicit schema markers:
- advance the marker deliberately;
- create only the new schema for fresh profiles;
- reject old marker/state;
- do not migrate or dual read.

Do not bump unrelated product protocol versions merely to preserve old local clients. Rust bindings/Flutter are built from the same snapshot.

Document in execution evidence which profile/state requires reset. Do not automatically delete user data or keychain entries.

## Test migration matrix

### Context contract

Add tests proving:

1. stable `GrantSourceBinding` JSON contains no source authority.
2. source authority cannot be supplied as an unknown source field.
3. dependency permission resources and source resources may differ.
4. both resource vectors canonicalize/validate independently.
5. old dependency JSON shape is rejected.
6. dependency identity conflicts if the same observation/grant/source authority carries different source resources.
7. dependency coverage merge remains deterministic.
8. connection/View resource helper validates and round-trips.

### Access / DataAccessGrant

Tests:

1. same stable grant source survives SourceAuthority A -> B external change unchanged.
2. grant source cannot be mutated through review API.
3. source replacement requires new grant.
4. grant revocation/pause/GrantAuthority semantics unchanged.
5. `validate_grant_dependency` ignores current source epoch but rejects wrong stable source/scope/authority.
6. caller-zero replay abstraction deleted or migrated.

### Vault access grant

Tests:

1. fresh schema has no source epoch columns.
2. reopen validates stable source fields and grant authority.
3. old access-grant schema marker is rejected; no migration.
4. stable source lookup returns same grant independent of current source epoch.
5. corruption in stable indexed fields fails closed.
6. cleanup still uses invalidated GrantAuthority correctly.
7. Calendar direct SQL contains no source epoch columns.

### Remote generic View

Tests:

1. source authority rotation does not create another grant/mapping.
2. old dependency stale against fresh signed authority.
3. new dependency records fresh authority/source resource.
4. mapping schema contains no source epoch.
5. logical View helper is the only formatter.

### Native Calendar transition

Tests:

1. dependency `source_authority` equals current Connections source authority.
2. dependency `source_resources` equals exact Calendar IDs acquired.
3. resolver uses `source_resources`, not grant resources, for native acquisition.
4. source authority rotation stales old dependency without changing stable source identity.
5. existing >128 Calendar real-budget behavior remains.
6. leaf grant scope is explicitly retained only as 03 transition.

### Remote Calendar transition

Tests:

1. stable grant source omits authority.
2. exact remote Calendar leaf stored in source resources.
3. fresh signed source authority reauthorizes explicit dependency authority.
4. special leaf grant remains until 04 without source-epoch identity.

### Personal bounded transition

Tests:

1. personal policy stores valid current source authority separately from grant.
2. same subject/handle set preserves source authority.
3. subject drift advances personal source authority but does not replace grant source.
4. Contacts selected-handle change advances source authority.
5. consumer-only policy change does not advance source authority.
6. personal dependency records exact selected handles/source resource.
7. old personal dependency stales on policy source-authority change.
8. `PersonalAccessOverview.source_authority` comes from personal source state, not grant.

### Recipient consent

Tests described in 02-O for logical vs exact source resources and source authority.

### Actions / Conversation

Tests:

1. stale source authority blocks Expert proposal/dispatch even if grant remains active.
2. wrong physical Calendar leaf is rejected via `source_resources`.
3. history projection drops stale dependencies.
4. model dispatch pre/post-response source resolver fences remain.
5. grant revocation blocks regardless of source authority.
6. uncertain action recovery remains unchanged.

## Residual audit

Run at close:

~~~
rg -n "GrantSourceBinding::try_new" crates
rg -n "\.source\(\)\.source_authority\(\)|source\(\)\.source_authority\(\)" crates
rg -n "grant\.source\(\)\.source_authority\(\)" crates
rg -n "source_incarnation|source_epoch" crates
rg -n "ContextDependency::try_new" crates
rg -n "dependency\.resources\(\)" crates
rg -n "dependency\.source_resources\(\)" crates
rg -n "ProcessingSourceScope" crates apps/client
rg -n "remote_view_resource\(" crates
rg -n "AccessGrantMutation::(Review|Activate|ReviewActive)" crates
rg -n "data_access_grants_for_source" crates
```

### Required zero matches

- `GrantSourceBinding` source-authority accessor.
- `grant.source().source_authority()`.
- source incarnation/epoch columns in `data_access_grants`.
- source incarnation/epoch mapping identity in `remote_view_grant_mappings`.
- duplicate Context `remote_view_resource` formatter.
- old ContextDependency constructor shape.
- old dependency decoder/defaulting.

### Allowed classified source epoch matches

- `SourceAuthority` itself;
- Connections source records;
- signed remote source/authorization proof structures;
- current provider preview/admission values;
- bounded personal-source policy epoch until 06;
- tests explicitly proving old schema rejection.

### Resource-use classification

Every surviving `dependency.resources()` use must be classified as grant/permission scope.

Every physical/provider/leaf check must use `dependency.source_resources()`.

A Calendar leaf `GrantScope.resources` use is allowed only with explicit 03/04 ownership.

## Suggested implementation slices

Checkpoint 02 may use several commits. Suggested boundaries:

### 02-A — contract split

- stable `GrantSourceBinding`;
- explicit dependency source authority/resources;
- canonical connection/View resource helper;
- contract tests.

Suggested commit:

~~~
context: split grant source from observed source
```

### 02-B — immutable grant source

- narrow DataAccessGrant transitions;
- narrow `AccessGrantMutation`;
- migrate Access callers/tests.

Suggested commit:

~~~
access: make standing grant source immutable
```

### 02-C — Vault stable-source schema

- remove source epoch from `data_access_grants`;
- remove source epoch from remote View mapping;
- direct SQL cleanup;
- schema tests.

Suggested commit:

~~~
vault: decouple grants from source epoch
```

### 02-D — dependency provenance cutover

- native/remote/personal builders;
- owner-specific resolvers;
- SourceView/ConsumedLineage;
- Actions/proposal paths.

Suggested commit:

~~~
context: record exact observed source resources
```

### 02-E — personal bounded source authority

- personal policy source epoch;
- ports/overview/replay;
- tests;
- explicit 06 removal note.

Suggested commit:

~~~
access: separate personal source epoch from grant
```

### 02-F — processing / recipient lineage

- `ProcessingSourceScope`;
- model dispatch requirement;
- consent identity/tests.

Suggested commit:

~~~
access: bind recipient consent to exact source provenance
```

### 02-G — deletion/docs/closure

- residual deletion;
- architecture/provenance docs;
- execution evidence;
- README 02 Complete.

Suggested commit:

~~~
docs: complete connection observe checkpoint 02
```

Combine slices when it produces a smaller coherent architecture. Do not add temporary wrappers merely to preserve compilation between commits.

## Verification

During iteration use affected crates first.

Minimum close gate:

~~~
cargo test -p floe-context-contract
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-actions
cargo test -p floe-conversation
cargo test -p floe-agent-runtime
cargo test -p floe-app
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo check --workspace
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
```

Because `ContextDependency` is re-exported through agent/runtime boundaries, compilation failures may surface widely. Migrate the whole snapshot rather than adding old/new constructors.

Run Flutter tests only where protocol/interaction `ProcessingSourceScope` or access overview serialization is affected:

~~~
cd apps/client
flutter analyze
flutter test test/features/connections
flutter test test/features/conversation
```

If those exact directories differ on the execution HEAD, run the nearest relevant tests and record the actual commands.

Broad Rust gate recommended before closure:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
```

Use serialized workspace verification if the known parallel shared-counter test remains flaky. Record any default-parallel result separately rather than hiding it.

Checkpoint 02 does not require a live EventKit read solely to prove the contract cutover, but if native Calendar manual smoke is available, verify:
- source authority change does not create a different standing source identity;
- new dependency records the new authority;
- old dependency is rejected.

Report unavailable native prerequisites as SKIPPED.

## Architecture documentation convergence

Update the current-state docs touched by the new semantics, at minimum:

~~~
docs/architecture/modules.md
docs/architecture/authority-recovery.md
```

After 02 they must say:

- Access grant source identity is stable connection/connector/execution-owner identity.
- standing grants do not own `SourceAuthority`.
- Connections/current source owner owns source epoch.
- Context dependency records grant permission resources separately from exact observed source resources.
- source epoch drift invalidates evidence without inherently mutating standing grant.
- generic remote View grants survive source epoch changes.
- exact-recipient processing review includes current source authority and exact source resources.
- Vault grant transactions do not perform provider/source I/O.
- Calendar leaf grant scope is a bounded pre-03/pre-04 implementation detail, not the final permission model.
- standing personal source epoch is temporarily outside the grant until 06 and must not be described as final Connections convergence yet.

Do not amend durable ADR history unless implementation reveals a genuinely new durable decision not already represented by the plan. Final rationale convergence remains checkpoint 09.

## Close procedure

Before marking 02 complete:

1. rerun all residual searches;
2. classify every source-epoch match by semantic owner;
3. classify every `dependency.resources()` use as permission scope;
4. verify physical resource checks use `source_resources`;
5. verify no old grant/dependency decoder or fallback query remains;
6. verify no provider/current-source I/O was moved into a Vault transaction;
7. run targeted + broad verification;
8. append execution evidence to this file;
9. update parent README:
   - 00 Complete
   - 01 Complete
   - 02 Complete
   - 03 Not started
10. commit closure;
11. stop. Do not begin checkpoint 03.

Execution evidence must record:

- date;
- start local HEAD / origin/main;
- final commit SHA(s);
- final `GrantSourceBinding` shape;
- final `ContextDependency` shape;
- canonical View resource helper;
- DataAccessGrant mutation API;
- exact new Vault grant schema columns;
- remote View mapping schema columns;
- personal bounded source authority location/removal condition;
- native/remote/personal dependency source-resource semantics;
- processing/recipient scope shape;
- release/current-source reauthorization design actually implemented;
- old surfaces deleted;
- bounded 03/04/05/06 transitions;
- residual search results;
- architecture docs changed;
- commands and actual outcomes;
- skipped native checks;
- final clean worktree.

## Required agent report

Report:

1. start HEAD / origin-main / final HEAD;
2. `GrantSourceBinding` final stable contract;
3. `ContextDependency` final permission/provenance split;
4. canonical connection/View resource helper and migrated callers;
5. immutable DataAccessGrant source/mutation semantics;
6. Vault access-grant schema and source lookup changes;
7. remote View mapping source-epoch removal;
8. native Calendar dependency transition and exact `source_resources`;
9. remote Calendar bounded transition;
10. personal interim source-authority owner and 06 removal condition;
11. processing/recipient consent provenance changes;
12. resolver/release source-vs-grant reauthorization boundary;
13. Actions/Conversation caller migration;
14. tests moved/rewritten/deleted;
15. residual audit and bounded later-checkpoint matches;
16. architecture docs updated;
17. commands with actual outcomes;
18. skipped checks with reason;
19. checkpoint commit SHA(s);
20. clean worktree confirmation;
21. confirmation that checkpoint 03 was not started.

## Execution evidence — 2026-09-28

- Start local HEAD and fetched `origin/main`: `e014cb150daf56fdf2c40a94325889782c9caa6f`; implementation commit: `4637ba2d9548ecd3dbb8fac26d2a3439fe0fc4e9`.
- `GrantSourceBinding` is exactly Person, Connection, Connector and execution owner. `DataAccessGrant` review/activation mutations accept scope, never a replacement source; a different source requires a new grant.
- `ContextDependency` stores stable source, logical grant `resources`, explicit `source_authority`, canonical exact `source_resources`, and retained `consumer_policy`. Old serialized dependencies fail closed; provenance participates in canonical conflict detection.
- `connection_view_resource` and its strict parser in `floe-context-contract` are the sole generic View resource formatter/parser. Generic remote View candidates, grants, reads, review and App callers use the shared helper; their grants survive source-epoch changes while existing evidence stales.
- Fresh `data_access_grants` columns are `grant_id`, `person_id`, `authority_owner`, `connection_id`, `connector`, `execution_owner`, `grant_incarnation`, `access_epoch`, `state`, `payload`. Source lookup predicates use stable source fields only. Fresh `remote_view_grant_mappings` columns are `grant_id`, `person_id`, `view_id`, `connector`, `connection_id`, `execution_owner`, `policy_incarnation`, `policy_epoch`, `payload`. Both schemas use marker 2, with no old decoder or fallback query.
- Native Calendar dependencies record the current Connections authority and exact selected Calendar IDs in `source_resources`; the leaf grant scope remains until 03. Remote Calendar dependencies record signed source authority and the exact remote Calendar leaf; its special leaf grant stack remains until 04.
- Personal source authority is temporarily owned by `personal_grant_policies.source_incarnation/source_epoch` (marker 5), advanced by source subject/selection changes rather than consumer-only policy changes. Checkpoint 06 removes this interim owner when standing personal sources move to Connections.
- `ProcessingSourceScope` carries `grant_resources`, exact `source_resources`, `source_authority` and retained policy authority. Exact-recipient consent identity changes with either resource set or source authority; the FFI consent projection shows physical source resources.
- Context owner-specific resolvers reload current source facts independently of grant authority at read, projection, history and model fences. Final Vault transactions validate grant/policy state only, with no provider/current-source I/O inside them. Action physical-resource checks use `source_resources`.
- Deleted `ReplayRequest`, `ReplayTrust`, `admit_replay`, grant-source `same_identity`, and duplicated `remote_view_resource` helpers. Consumer policy remains until 05; native/remote leaf grants remain until 03/04; personal interim authority remains until 06. No 03 implementation was started.
- Residual audit: 30 `source_incarnation/source_epoch` matches in crates belong to signed remote challenge/proof, personal interim policy, provider transport, Access remote-auth DTOs, or test names; none is standing grant or generic View mapping identity. All 19 `dependency.resources()` uses are grant permission-scope checks, propagation or assertions. Physical-resource comparisons use `source_resources`. Removed-symbol search returned no matches.
- Current architecture updated in `docs/architecture/modules.md` and `docs/architecture/authority-recovery.md`.
- PASS: each of the ten targeted crate `cargo test -p` commands in Verification, `cargo check --workspace`, `cargo check --workspace --lib`, `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1`, both architecture scripts, `git diff --check`, `cargo build -p floe-ffi`, `flutter analyze`, `flutter test test/features/connections`, `flutter test test/features/conversation`, and `flutter build macos`.
- Full `flutter test` has 364 passes and one reproducible 29-pixel (0.02%) golden mismatch in the unchanged `agent_registry_dialog_test.dart` at width 520.0. No Flutter source or golden changed in this checkpoint; the failure is reported rather than altering an unrelated assertion. Live EventKit smoke was SKIPPED because no authorized device/read was available; it is not required for 02.
- Closure commit records this evidence and the parent README status. The worktree is clean after closure; checkpoint 03 remains Not started.
