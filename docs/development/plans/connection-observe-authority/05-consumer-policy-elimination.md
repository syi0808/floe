# 05: Eliminate ConsumerPolicyAuthority and duplicate policy storage

Prerequisite: 04 complete.

Status: Not started.

Planning base: `main` at `2a3b8d27029fdee7f5846f5bbe41f0bc044f80be` on 2026-09-29.

Checkpoints 01-04 have already separated the two real authority dimensions:

~~~
standing permission
  DataAccessGrant
  GrantScope
  GrantAuthority

current source truth
  SourceConnection / producer source
  SourceAuthority
  exact source_resources in ContextDependency
~~~

The remaining `ConsumerPolicyAuthority` is now a duplicate permission epoch. It is copied into `ContextDependency`, exact-recipient processing scopes, Calendar policy storage, remote View mapping storage, personal source records, remote admission proofs, review DTOs, and durable inline interactions even though the current `DataAccessGrant` already owns every standing permission fact it attempts to fence.

Checkpoint 05 removes that second epoch and converges the final model to:

~~~
GrantAuthority
  = standing permission/state/scope epoch

SourceAuthority
  = current source/resource/subject epoch

policy_digest
  = compare-only prospective review identity
  = no authority
~~~

This checkpoint is intentionally destructive. Old local rows, serialized dependencies/interactions, policy side tables and same-snapshot wire shapes are not backward-compatible. Do not add deprecated aliases, serde defaults, dual schemas, compatibility decoders or mapping fallbacks.

Line numbers below are planning-base anchors on `2a3b8d27`. Re-resolve every symbol on the actual execution HEAD before editing.

## 1. Exit state

Checkpoint 05 is complete only when all of the following are true.

1. `ConsumerPolicyAuthority` does not exist in production Rust code.
2. `ConsumerPolicyAuthority` is not re-exported by `floe-access` or any other crate.
3. `ContextDependency` has no `consumer_policy` field, constructor argument, accessor, serializer member or validation rule.
4. `ProcessingSourceScope` has no `policy_authority`.
5. Exact-recipient consent identity remains bound to:
   - stable source identity;
   - logical grant resources;
   - exact source resources;
   - categories;
   - operation;
   - purpose;
   - consumer;
   - GrantId;
   - GrantAuthority;
   - SourceAuthority.
6. `CalendarReadAccessAdmission` has no consumer-policy field.
7. Remote View read binding has no consumer-policy field.
8. Personal read records expose no consumer-policy lookup.
9. Grant scope/consumer/purpose/processing drift is fenced only by the current `DataAccessGrant` and `GrantAuthority`.
10. Source/resource/subject drift is fenced only by current source state and `SourceAuthority`.
11. Observation/query freshness remains Context evidence/provenance.
12. Exact-recipient model transfer remains the existing contextual recipient authority.
13. `calendar_grant_policy.rs` is deleted.
14. `calendar_grant_policy_schema` and `calendar_grant_policies` are not created or read by current code.
15. Native Calendar review/authorization uses only `DataAccessGrant`, `GrantAuthority`, current source facts, and compare-only policy digest.
16. `remote_view_grant_mappings` is deleted.
17. `remote_view_grant_schema` is deleted.
18. `remote_view_grants.rs` is deleted if no real non-duplicative storage boundary remains.
19. Remote View lookup is derived directly from stable source + exact logical View resource in `data_access_grants`.
20. Two non-revoked grants for one stable source + logical View resource fail closed as ambiguity.
21. A revoked historical grant does not prevent a new explicitly reviewed grant for the same source/View.
22. Multi-member remote Observe activation remains atomic after mapping deletion.
23. Expected absence and expected GrantAuthority are rechecked inside the activation transaction.
24. The owner-key remote signer validates the current `DataAccessGrant` directly and does not query a remote policy mapping.
25. Remote admission/release proof contains no policy incarnation/epoch.
26. Go `PolicyReference`, challenge `policy`, and `ViewAdmission.policy` are deleted.
27. Remote admission/release continues to bind exact GrantId/GrantAuthority, SourceAuthority/source identity, logical resource, consumer, purpose, query digest, bounds, principal and producer.
28. `personal_grant_policies` remains only as a bounded interim personal source/review record until 06.
29. `personal_grant_policies` contains no `policy_incarnation`, `policy_epoch`, or duplicated consumer list.
30. Personal source authority columns `source_incarnation` / `source_epoch` remain until 06.
31. Personal reviewed subject and selected handles remain until 06.
32. Personal consumer changes are represented by `GrantScope.consumers` and advance `GrantAuthority`.
33. Personal subject/selected-handle changes advance `SourceAuthority` and do not require a second permission epoch.
34. An exact personal source no-op preserves SourceAuthority.
35. Feasibility query review remains contextual and explicit.
36. Changing a feasibility query while the same active grant/scope remains advances `GrantAuthority` so older dependencies become stale without a policy epoch.
37. Exact no-op feasibility re-review preserves GrantAuthority.
38. `PersonalGrantRecords` and `PersonalGrantStore` expose no policy-authority API.
39. `policy_fingerprint` production naming is replaced by `policy_digest`.
40. The final policy digest is compare-only and is never accepted as authorization input.
41. The digest input is the canonical intended logical View policy:
    - View ID;
    - trusted consumers;
    - categories;
    - operation;
    - purpose;
    - actual standing `ProcessingRestriction`.
42. The digest excludes:
    - Expert assignment/binding selection;
    - current leaf/source resources;
    - SourceAuthority;
    - GrantId;
    - GrantAuthority;
    - producer revision;
    - native subject fingerprint;
    - provider identity.
43. `SourceProcessingPolicy` is deleted if, as on the planning base, it exists only to perturb the old fingerprint.
44. Remote paired-server transport identity remains reviewed separately through producer/source evidence; it is not encoded as standing grant processing.
45. App internal digest representation is fixed 32-byte SHA-256 where practical.
46. Product/durable wire represents the digest as canonical lowercase 64-character hex.
47. `RemoteObserveMemberExpectation` has no `expected_policy`.
48. `ConnectionObserveMemberDto` has no `expected_policy`.
49. Flutter `ConnectionObserveMember` has no `expectedPolicy`.
50. Native Calendar access overview/DTO/Flutter model has no `consumer_policy`.
51. `ConnectionObserveMember` overview has no `consumer_policy`.
52. Durable `ReviewedBundleMember` has no `policy_authority`.
53. Durable `ReviewedBundleMember` uses `policy_digest`.
54. One connection/source revision is represented at bundle/target level rather than copied into every member.
55. `InlineObserveTarget` owns the reviewed source revision for its connection.
56. `LiveInlineState` owns current source revision for its connection.
57. Members contain only member identity/resource, policy digest and expected grant state.
58. Review resolution fails on policy digest drift, grant drift, source drift, connection revision, producer/native subject or member-set drift as applicable.
59. No review resolution branch refers to policy authority.
60. Vault release/commit validation reloads the current `DataAccessGrant` and validates the dependency against it; it does not query Calendar/personal/remote policy epochs.
61. Provider/Connections I/O remains outside Vault transactions.
62. Context source reauthorization remains separate from Vault grant-authority validation.
63. Old serialized `ContextDependency`, `ProcessingSourceScope`, review expectation or durable interaction shapes are rejected; no defaulted missing fields/aliases.
64. Old policy side-table profiles are not migrated.
65. A fresh profile creates no Calendar/remote policy side-table schemas.
66. The personal source/review schema advances to its new direct shape and rejects the old local version.
67. `ConsumerPolicyAuthority`, `consumer_policy`, `policy_authority`, `expected_policy`, `policy_incarnation`, `policy_epoch` have zero production matches.
68. `calendar_grant_policy_schema`, `calendar_grant_policies`, `remote_view_grant_schema`, `remote_view_grant_mappings`, `evolve_calendar_consumer_policy` have zero production matches.
69. `policy_fingerprint` has zero production matches after the `policy_digest` rename.
70. Architecture/current product documentation describes exactly two standing/live authorities: GrantAuthority and SourceAuthority.
71. Parent README marks 05 Complete and 06 remains Not started.

## 2. Checkpoint boundary with 06/07/09

### 2.1 What 05 owns

05 owns the complete deletion of the duplicate policy authority across:

- shared contracts;
- Context dependency/provenance;
- Access admission/release;
- Calendar grants;
- remote View grants;
- personal read plumbing;
- Vault side tables;
- remote key-holder proof;
- Go authorization protocol;
- App pending review;
- durable Conversation interaction identity;
- protocol/FFI/Flutter fields that directly encode this removed authority.

A `ConsumerPolicyAuthority` residual is not allowed merely because 07 also changes product wire.

### 2.2 What remains for 06

Do not move standing Contacts/Attention/Wellbeing source ownership to Connections in 05.

The following bounded personal source state remains:

~~~
personal_grant_policies
  grant_id
  stable source key columns
  reviewed_subject_fingerprint
  selected_handles
  source_incarnation
  source_epoch
~~~

Checkpoint 06 moves the standing personal source/resource owner to Connections and removes that interim source-authority ownership.

Feasibility remains a contextual exception. Do not widen it into standing Connection Observe.

### 2.3 What remains for 07

05 removes fields whose type/meaning disappears now, but 07 still owns final product-shape simplification such as:

- optional outer `RemoteAccess ConnectionObserve.resource`;
- selected/granted resource projections;
- final generic ConnectionObserve DTO/gateway/UI convergence;
- Calendar IDs in review/change wire where 07 explicitly owns their product cleanup.

Do not preserve `consumer_policy`, `expected_policy` or `policy_authority` until 07; those concepts cease to exist in 05.

### 2.4 What remains for 09

Current architecture docs change in 05 because the runtime changes.

The final durable rationale/ADR convergence remains 09 unless implementation reveals a new durable decision beyond the already-frozen two-authority design.

## 3. Final authority model after 05

### 3.1 Standing permission

~~~
DataAccessGrant
  GrantId
  GrantSourceBinding
  GrantScope
    logical resources
    categories
    operations
    purposes
    consumers
    processing
  GrantAuthority
  state
  review_required
~~~

Any reviewed permission semantic change is represented by a changed grant scope/state and therefore a changed `GrantAuthority`.

### 3.2 Current source

~~~
current source owner
  stable source identity
  current source resources/subject
  SourceAuthority
~~~

Changing current source resources/subject stales prior dependencies through `SourceAuthority`; it does not mutate standing permission.

### 3.3 Dependency

Target:

~~~
ContextDependency
  person_id
  grant_id
  grant_authority
  source
  resources
  source_authority
  source_resources
  categories
  operation
  purpose
  consumer
  processing
  observation_id
  query_fingerprint
  lease_invocation_id
  process_incarnation_id
  observed_at
  expires_at
~~~

No policy epoch.

### 3.4 Pending review

Target member semantics:

~~~
member
  member_id
  resource
  policy_digest
  expected_grant
~~~

Target/bundle owner semantics:

~~~
InlineObserveTarget
  connection/source identity
  source_revision
  connection_revision
  producer/native-subject evidence
  members[]
~~~

`policy_digest` answers only:

> Is the product asking the Person to review the same intended policy as before?

It never answers:

> Is this operation currently authorized?

Authorization comes from current grant/source owners.

## 4. Planning-base code map

All anchors refer to `2a3b8d27`.

### 4.1 Contract type and dependency

`crates/contracts/context/src/lib.rs`:

- `GrantAuthority` around lines ~440-480 is the standing permission epoch and remains.
- `ConsumerPolicyAuthority` around ~590-635 is the duplicate type to delete.
- `ContextDependency` around ~637+ still contains `consumer_policy`.
- `ContextDependency::try_new`, `validate`, accessor and canonical serialization all carry it.
- test fixtures across the workspace instantiate `ConsumerPolicyAuthority::new()`.

### 4.2 Exact-recipient processing scope

`crates/contracts/context/src/processing.rs` around ~82:

~~~
ProcessingSourceScope
  grant_authority
  source_authority
  policy_authority
~~~

`from_dependency` copies `dependency.consumer_policy()`.

Delete only policy authority. Keep exact logical/physical resources and both real authorities.

### 4.3 GrantAuthority already fences permission facts

`crates/modules/access/src/application/dependency.rs`:

`validate_grant_dependency` already compares:
- GrantId;
- active state/review-required;
- GrantAuthority;
- stable source;
- permission resources;
- categories;
- operation;
- purpose;
- consumer;
- processing.

That is the canonical post-05 permission validation. Do not add another replacement epoch.

`crates/modules/access/src/data_access_grant.rs`:
- `activate_review` advances GrantAuthority when active scope changes;
- exact active same-scope activation is a no-op;
- pause/revoke advance GrantAuthority;
- `review_active` can explicitly advance an active grant where a contextual grant fact changes outside `GrantScope`.

### 4.4 Native Calendar policy side table is now pure duplication

`crates/adapters/vault/src/vault/calendar_grant_policy.rs`:
- whole file exists only for `ConsumerPolicyAuthority`;
- creates `calendar_grant_policy_schema`;
- creates `calendar_grant_policies`;
- `evolve_calendar_consumer_policy` compares source/scope and advances the second epoch.

`crates/adapters/vault/src/vault/calendar_grants.rs`:
- `CalendarGrantAdmission` contains `consumer_policy`;
- `authorize_current_native_calendar_grant` loads `calendar_grant_policy`;
- `review_native_calendar_grant` initializes/upserts/evolves policy rows.

Native subject state already lives in Connections. The table has no surviving owner fact.

### 4.5 Remote View mapping duplicates the grant

`crates/adapters/vault/src/vault/remote_view_grants.rs`:

~~~
RemoteViewGrantMapping
  grant_id
  view_id
  source
  scope
  policy_incarnation
  policy_epoch
~~~

Every standing fact except the second epoch duplicates `DataAccessGrant`.

The file also:
- finds grants through the mapping;
- returns `RemoteViewGrantBinding { grant, consumer_policy }`;
- validates dependency policy in transaction;
- stores the policy epoch;
- owns atomic multi-View activation.

After 05, lookup and atomic activation must move to generic `data_access_grants` transaction primitives, then this file/table can be deleted.

### 4.6 Generic access grant storage already has the right base

`crates/adapters/vault/src/vault/access_grants.rs`:

- current schema stores stable source, GrantAuthority, state and canonical payload;
- `data_access_grants_for_source` returns all grants for stable source;
- transaction create/mutate primitives already exist;
- `find_data_access_grant_by_source_in_transaction` exists but assumes one grant per source and therefore is not sufficient for generic remote multi-View sources.

05 needs an exact stable-source + logical-resource lookup helper with ambiguity detection.

### 4.7 Remote grant port still carries policy

`crates/modules/access/src/ports/remote_grants.rs`:

- `RemoteGrantBinding { grant, consumer_policy }`;
- `find_view_grant`;
- `activate_view_grant`;
- `view_grant_binding`.

After mapping deletion:
- no binding wrapper with one field;
- no policy lookup;
- exact current grant is derived from stable source + logical resource.

### 4.8 Remote grant activation still expects policy epoch

`crates/modules/access/src/application/remote_grants.rs`:

- `RemoteViewGrantExpectation.expected_policy`;
- `RemoteViewGrantActivation.expected_policy`;
- preparation checks coherent `(expected_grant, expected_policy)` pairs.

After 05 only reviewed grant absence or exact `(GrantId, GrantAuthority)` remains.

### 4.9 Remote dependency matching still compares policy

`crates/modules/access/src/application/remote_view.rs`:
- `remote_dependency_binding_matches(consumer_policy, authority, dependency)` compares both epochs.

After 05 it compares current GrantAuthority only; source currentness remains `remote_dependency_source_admits`.

### 4.10 Context remote read copies mapping policy

`crates/modules/context/src/application/remote_sources.rs`:
- read path checks `current_binding.consumer_policy`;
- dependency builder receives policy;
- tests assert policy equality.

`crates/modules/context/src/application/remote_views.rs`:
- `remote_view_dependency` takes `ConsumerPolicyAuthority`.

Remove all of it.

### 4.11 Personal source record mixes two different epochs

`crates/adapters/vault/src/vault/personal_grants.rs` schema version 5:

~~~
personal_grant_policies
  grant_id
  person_id
  connector
  connection_id
  execution_owner
  reviewed_subject_fingerprint
  policy_incarnation
  policy_epoch
  consumers
  selected_handles
  source_incarnation
  source_epoch
~~~

Post-05 target:

~~~
personal_grant_policies
  grant_id
  person_id
  connector
  connection_id
  execution_owner
  reviewed_subject_fingerprint
  selected_handles
  source_incarnation
  source_epoch
~~~

The table name can remain until 06 to avoid a rename immediately before ownership migration, but comments must state that it is bounded personal source/review state, not a policy authority table.

### 4.12 Personal read port still exposes policy

`crates/modules/context/src/ports/personal_source.rs`:
- `PersonalGrantRecords::consumer_policy`.

`crates/adapters/vault/src/repositories/personal_grants.rs` implements it through `personal_grant_consumer_policy`.

Delete both.

`crates/modules/context/src/application/personal_sources.rs`:
- `CompletedRead.policy`;
- acquisition re-reads policy;
- dependency construction copies it;
- every reauthorization path compares it.

GrantAuthority/current grant and SourceAuthority/source state already cover those facts.

### 4.13 Feasibility query requires an explicit post-policy-epoch fence

`crates/adapters/vault/src/vault/personal_grants.rs`:
- `review_personal_grant_with_feasibility_query`;
- `personal_feasibility_queries`.

Current active same-scope re-review may preserve GrantAuthority even when the contextual feasibility query changes.

Once the second policy epoch is removed, a changed query must advance GrantAuthority atomically or old evidence would not be fenced by the standing grant epoch.

### 4.14 Vault release has a second policy validation pass

`crates/adapters/vault/src/vault/personal_grants.rs` around ~500:

`validate_grant_policy_authority_in_transaction`:
- validates current grant;
- branches to Calendar policy table;
- personal policy table;
- remote mapping policy.

After 05 the canonical transaction check is simply:
- read current DataAccessGrant;
- `validate_grant_dependency`.

Source owner checks remain outside the Vault transaction.

### 4.15 Vault initialization explicitly owns obsolete policy stores

`crates/adapters/vault/src/vault.rs`:
- line ~24 module `calendar_grant_policy`;
- line ~37 module `remote_view_grants`;
- fresh open lines ~233-234 initialize both;
- reopen lines ~322-323 validate both.

Delete these current-store hooks.

Add one fail-closed obsolete-table probe so old local profiles are rejected rather than partially accepted or migrated.

### 4.16 Remote key-holder signer depends on mapping

`crates/adapters/vault/src/vault/remote_authority.rs` around ~904:

`validate_remote_view_grant_in_transaction` currently:
1. requires challenge policy;
2. queries `remote_view_grant_mappings`;
3. verifies mapped View/source/policy;
4. then loads `data_access_grants`;
5. verifies GrantAuthority/scope/consumer/purpose/processing.

After 05 steps 1-3 disappear. The current grant itself is authoritative.

The signer must:
- read the grant by challenged GrantId;
- validate source;
- validate GrantAuthority;
- validate one logical connection/View resource;
- validate operation/purpose/consumer/processing;
- compare challenge resources exactly.

### 4.17 Provider admission wire carries redundant policy

`crates/adapters/providers/src/sources/server.rs`:
- `AuthorizedViewRead.consumer_policy`;
- constructs `policy_incarnation` / `policy_epoch`.

`crates/adapters/providers/src/control/authorization.rs`:
- `RemoteViewAuthorizationRequest.policy_incarnation`;
- `.policy_epoch`;
- serializes `policy`.

`RemoteViewAuthorizationExpectation` in Access already carries no policy epoch; it contains GrantAuthority and SourceAuthority facts.

### 4.18 Go challenge wire carries redundant policy

`server/internal/authorization/source_service.go`:
- `viewPolicyWire`;
- `ViewAdmission.Policy`;
- `AdmitView` copies it into `Request.Policy`.

`server/internal/authorization/authorization.go`:
- `PolicyReference`;
- `Request.Policy`;
- challenge `policyWire`;
- `makeChallenge` serializes it.

Remove the policy field end-to-end while retaining source/grant/proof/replay semantics.

### 4.19 App remote review carries both digest and policy epoch

`crates/app/src/remote_services.rs`:
- `RemoteObserveMemberExpectation.policy_fingerprint`;
- `expected_policy`.

`crates/app/src/vault_host/remote_observe.rs`:
- review loads `remote_view_grant_policy`;
- enable echoes `expected_policy`;
- prepare checks it.

After 05:
- digest remains;
- exact expected grant remains;
- expected policy disappears.

### 4.20 App inline interaction duplicates both review identities

`crates/app/src/vault_host/review_snapshot.rs`:

~~~
SnapshotMember
  policy_fingerprint
  source_revision
  expected_grant
  policy_authority
```

`InlineReviewSnapshot` already has bundle-level connection/producer/native identity.

`crates/app/src/vault_host/interaction_resolution.rs`:

~~~
LiveMember
  policy_fingerprint
  source_revision
  live_grants
  policy_authority
```

05 removes policy authority and moves the single connection's source revision to bundle level.

### 4.21 Conversation durable target stores policy authority

`crates/modules/conversation/src/domain/interaction.rs` around ~187:

~~~
ReviewedBundleMember
  member_id
  policy_fingerprint
  resource
  source_revision
  expected_grant
  policy_authority
```

Its canonical target digest serializes both source revision and policy authority per member.

Post-05:

~~~
ReviewedBundleMember
  member_id
  policy_digest
  resource
  expected_grant

InlineObserveTarget
  source_revision
  connection_revision
  producer/native subject...
  members
~~~

No old decoder.

### 4.22 Product wire and Flutter still expose removed authority

`crates/bindings/protocol/src/dto/access.rs`:
- `policy_fingerprint`;
- `expected_policy`.

`apps/client/lib/features/connections/domain/remote_owner_models.dart`:
- `policyFingerprint`;
- `expectedPolicy`.

`crates/bindings/protocol/src/dto/local_access.rs`:
- Calendar `consumer_policy`.

`apps/client/lib/features/connections/domain/native_calendar_access.dart`:
- `consumerPolicy`.

These are 05 fields because their concepts disappear now.

### 4.23 Current fingerprint has one stale pseudo-policy dimension

`crates/app/src/first_party_observe.rs`:
- `SourceProcessingPolicy::{LocalOnly, PairedSourceRecipient}`;
- `policy_fingerprint` hashes it.

The enum is only used by policy construction/fingerprinting on the planning base.

Actual native, personal and remote standing Observe grants use `ProcessingRestriction::LocalOnly`; paired producer audience is separately reviewed source transport identity.

Post-05 digest must hash actual intended permission processing, not a duplicate transport classification.

## 5. 05-A — baseline and full residual inventory

Before edits:

~~~
git fetch origin
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
git log -1 --oneline
python3 tools/architecture/check_boundaries.py
```

Read:

~~~
AGENTS.md
.agents/skills/architecture-change/SKILL.md
.agents/skills/code-change-verification/SKILL.md
docs/development/plans/connection-observe-authority/README.md
docs/development/plans/connection-observe-authority/05-consumer-policy-elimination.md
docs/architecture/invariants.md
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
server/README.md
```

Capture before edits:

~~~
rg -n "ConsumerPolicyAuthority|consumer_policy" crates apps server
rg -n "policy_authority|expected_policy" crates apps server
rg -n "policy_incarnation|policy_epoch" crates apps server
rg -n "calendar_grant_policy|calendar_grant_policies|calendar_grant_policy_schema" crates
rg -n "remote_view_grant_mappings|remote_view_grant_schema|remote_view_grants" crates
rg -n "policy_fingerprint|SourceProcessingPolicy|source_processing" crates apps
rg -n "ContextDependency::try_new" crates
rg -n "ProcessingSourceScope::try_new|ProcessingSourceScope" crates
rg -n "personal_grant_consumer_policy|fn consumer_policy" crates
rg -n "PolicyReference|viewPolicyWire|\"policy\"" server/internal/authorization
```

Classify every result by owner before editing. There should be no legitimate production `ConsumerPolicyAuthority` residual after this checkpoint.

## 6. 05-B — delete `ConsumerPolicyAuthority` from the context contract

Primary:

~~~
crates/contracts/context/src/lib.rs
```

Delete:
- type definition;
- constructor;
- accessors;
- `advance`;
- `Default`;
- re-exports.

### `ContextDependency`

Remove:
- `consumer_policy` field;
- constructor argument;
- validation;
- accessor;
- serde member.

Target constructor order:

~~~
ContextDependency::try_new(
  person_id,
  grant_id,
  grant_authority,
  source,
  resources,
  source_authority,
  source_resources,
  categories,
  operation,
  purpose,
  consumer,
  processing,
  observation_id,
  query_fingerprint,
  lease_invocation_id,
  process_incarnation_id,
  observed_at,
  expires_at,
)
```

Do not retain an overloaded old constructor.

### Identity/canonical bytes

Keep dependency identity bound to:
- observation;
- GrantId/GrantAuthority;
- SourceAuthority.

Full canonical serialized bytes still conflict when source resources/query/etc differ.

Old JSON containing `consumer_policy` must fail due `deny_unknown_fields`. Missing-field defaults are unnecessary because the field no longer exists.

### Size budget

Re-run dependency serialized-size tests. The shape shrinks; do not change the max merely because of this checkpoint.

## 7. 05-C — remove policy authority from exact-recipient processing scopes

Modify:

~~~
crates/contracts/context/src/processing.rs
crates/modules/access/src/application/model_dispatch.rs
crates/modules/access/src/application/recipient_consent.rs
related protocol/conversation tests
```

Target `ProcessingSourceScope`:

~~~
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
```

Delete:
- `policy_authority`;
- constructor input;
- accessor;
- validation;
- canonical consent identity component.

`ProcessingSourceScope::from_dependency` maps the two real authorities only.

Required tests:
1. GrantAuthority change changes recipient scope identity.
2. SourceAuthority change changes identity.
3. physical source resource change changes identity.
4. logical grant resource change changes identity.
5. removed policy epoch has no replacement field.

Exact-recipient consent remains separately required.

## 8. 05-D — make GrantAuthority the only standing permission epoch

No new epoch type.

Strengthen tests around:

~~~
crates/modules/access/src/data_access_grant.rs
crates/modules/access/src/application/grants.rs
crates/modules/access/src/application/dependency.rs
```

Required semantics:

### Scope change

Changing any canonical scope permission fact:
- resources;
- categories;
- operations;
- purposes;
- consumers;
- processing;

advances `GrantAuthority`.

### Exact active no-op

Re-reviewing an active grant with byte/semantic-equivalent canonical scope:
- does not advance GrantAuthority.

### Pause/re-enable/revoke

Existing semantics remain:
- pause advances;
- re-enable from paused advances;
- revoke advances/terminal.

### Dependency

`validate_grant_dependency` is the canonical permission-currentness check.

Do not add product-policy digest to dependency authorization.

## 9. 05-E — preserve contextual Feasibility fencing through GrantAuthority

Primary:

~~~
crates/adapters/vault/src/vault/personal_grants.rs
```

Feasibility query is explicitly contextual and remains stored in:

~~~
personal_feasibility_queries
```

It is not a source resource and not a `ConsumerPolicyAuthority`.

### Transaction algorithm

Before mutating an existing feasibility grant:

1. load current grant under expected GrantAuthority;
2. load current persisted feasibility query if one exists;
3. compare canonical query with the reviewed requested query;
4. classify:
   - same query + same scope + active -> exact no-op;
   - changed scope -> normal grant scope review advances GrantAuthority;
   - changed query + active same scope -> explicitly advance GrantAuthority using the existing `ReviewActive` transition or an equally direct Access mutation;
   - paused -> normal reactivation already advances GrantAuthority;
   - revoked -> new GrantId as current rules require;
5. persist the new query in the same transaction.

Do not make query change advance `SourceAuthority` unless source/subject facts also changed.

### Acceptance

- query A -> dependency at GrantAuthority G1;
- review query B under same source/scope;
- grant becomes G2;
- old dependency G1 is rejected;
- exact query B re-review while active leaves G2 unchanged.

This closes a semantic hole that a second policy epoch must not be used to mask.

## 10. 05-F — delete native Calendar policy storage

Delete:

~~~
crates/adapters/vault/src/vault/calendar_grant_policy.rs
```

Modify:

~~~
crates/adapters/vault/src/vault/calendar_grants.rs
crates/adapters/vault/src/vault.rs
crates/app/src/vault_host/calendar_access.rs
```

### `CalendarGrantAdmission`

Remove `consumer_policy`.

### Authorization

`authorize_current_native_calendar_grant`:
- finds exact current active DataAccessGrant;
- validates consumer/scope/operation/purpose/processing;
- returns GrantAuthority/source/scope only.

No policy row read.

### Review

`review_native_calendar_grant`:
- does not initialize policy schema;
- does not evolve policy;
- does not upsert policy row;
- scope changes are represented by normal GrantAuthority advancement.

### Vault initialization

Delete Calendar policy module/init/reopen validation.

### Legacy profile

Do not silently ignore old policy tables.

Add a small obsolete-schema probe at Vault open/init that rejects presence of:
- `calendar_grant_policy_schema`;
- `calendar_grant_policies`.

Return `UnsupportedVersion` or the repository's exact current old-profile failure.

This probe is a fail-closed conformance check, not a compatibility reader.

### Tests

Port only meaningful tests:
- fresh grant;
- same-scope no-op;
- consumer set change advances GrantAuthority;
- stale expected GrantAuthority conflicts;
- old policy table profile rejected.

Delete policy-epoch-only tests.

## 11. 05-G — replace remote mapping lookup with direct DataAccessGrant lookup

Primary storage owner:

~~~
crates/adapters/vault/src/vault/access_grants.rs
```

Add a generic exact-source-resource lookup usable both outside and inside a transaction.

Conceptual API:

~~~
data_access_grant_for_source_resource(
  source,
  logical_resource,
  include_revoked?
)
```

and transaction equivalent.

Rules:
- stable source must match exactly;
- grant scope must name the exact logical resource;
- for remote View grants, scope resources length must be exactly one;
- ignore revoked grants when deciding current authority;
- zero -> absence;
- one -> exact current grant;
- more than one non-revoked -> Conflict/PolicyDenied, never pick newest;
- payload/index corruption -> VaultUnavailable.

Do not add a `view_id` column to `data_access_grants`; logical resource already carries the View identity.

### Centralize duplicate source query

Move the current transaction stable-source scan out of Calendar-private code if needed so Calendar/remote implementations do not maintain duplicate SQL.

## 12. 05-H — delete `remote_view_grant_mappings`

Delete current storage schema:

~~~
remote_view_grant_schema
remote_view_grant_mappings
```

Preferred final result:

~~~
crates/adapters/vault/src/vault/remote_view_grants.rs
  deleted
```

If a small file survives, it may contain no table/schema/payload duplication and should represent a real repository boundary. Prefer moving remaining generic transaction helpers to `access_grants.rs` and the `repositories/remote_grants.rs` adapter.

### Old profile rejection

At Vault open, reject presence of:
- `remote_view_grant_schema`;
- `remote_view_grant_mappings`.

No migration/read fallback.

### `RemoteGrantStore`

Simplify:

~~~
crates/modules/access/src/ports/remote_grants.rs
crates/adapters/vault/src/repositories/remote_grants.rs
```

Delete:
- `RemoteGrantBinding`;
- `consumer_policy`;
- `view_grant_binding`.

`find_view_grant` should derive the canonical logical resource from `(view_id, source.connection_id)` and locate the exact DataAccessGrant directly.

Do not filter lookup by consumer before identifying the existing grant. A currently reviewed grant with a different consumer set is a grant that needs re-review, not reviewed absence.

### Read-time binding

Context already has the selected grant.

After fresh signed source preview:
- refetch exact source/resource grant;
- require same GrantId/GrantAuthority/source/scope;
- use `admit_remote_view_binding`.

No mapping wrapper.

## 13. 05-I — preserve atomic remote Observe bundle activation without mapping storage

Current remote mapping code owns the multi-member transaction. Deleting the table must not make Gmail/etc enable partially commit.

Move atomic grant activation to generic Access-grant persistence.

### Prepared activation

Reduce remote prepared activation to:

~~~
grant_id
expected: Option<GrantAuthority>
source
scope
```

`view_id` may remain only while validating the expected logical resource; it is not persisted.

Delete `expected_policy`.

### Transaction requirements

For every prepared member, before any commit:

- validate source/scope;
- derive exact logical resource;
- re-read current non-revoked grant for source/resource;
- expected absence -> require no current grant;
- expected `(id, authority)` -> require exact grant and authority;
- reject duplicate current grants.

Then apply all member mutations in one Immediate transaction.

If any member fails, none commit.

Revoked historical rows may coexist; fresh reviewed absence can create a new GrantId.

### API placement

Prefer a generic `access_grants` batch primitive rather than retaining a remote-policy storage file.

`remote_observe::enable_bundle` still receives all prepared activations and performs one atomic batch.

## 14. 05-J — simplify remote Access contracts

Modify:

~~~
crates/modules/access/src/application/remote_grants.rs
crates/modules/access/src/application/remote_view.rs
crates/modules/access/src/ports/remote_grants.rs
```

Delete:
- `RemoteViewGrantExpectation.expected_policy`;
- `RemoteViewGrantActivation.expected_policy`;
- coherent policy/grant pair validation;
- `remote_dependency_binding_matches` consumer-policy parameter.

`remote_dependency_binding_matches` becomes a GrantAuthority/current-grant check or is deleted if `admit_remote_view_binding` + `validate_grant_dependency` make it redundant.

Review preparation still binds:
- producer fingerprint;
- SourceAuthority;
- connection revision;
- provider identity;
- producer recipient/audience;
- expected grant absence or exact GrantAuthority.

Those are not policy epochs.

## 15. 05-K — simplify Context remote read/provenance

Modify:

~~~
crates/modules/context/src/application/remote_sources.rs
crates/modules/context/src/application/remote_views.rs
```

Delete:
- binding consumer-policy comparison;
- consumer-policy dependency argument;
- tests asserting policy equality.

Dependency creation uses:
- exact current grant;
- current GrantAuthority;
- current signed SourceAuthority;
- exact signed source resources.

Reauthorization:
- current grant/source/scope via Access;
- current signed source evidence;
- no policy mapping.

## 16. 05-L — trim personal source/review storage to source facts

Modify:

~~~
crates/adapters/vault/src/vault/personal_grants.rs
crates/adapters/vault/src/repositories/personal_grants.rs
crates/modules/context/src/ports/personal_source.rs
crates/modules/context/src/application/personal_sources.rs
```

### Schema

Advance personal schema from planning-base version 5 to the new direct shape.

Fresh table:

~~~
personal_grant_policies
  grant_id TEXT PRIMARY KEY
  person_id
  connector
  connection_id
  execution_owner
  reviewed_subject_fingerprint
  selected_handles
  source_incarnation
  source_epoch
  UNIQUE(person_id, connector, connection_id, execution_owner)
```

Delete columns:
- `policy_incarnation`;
- `policy_epoch`;
- `consumers`.

Consumers live in `DataAccessGrant.scope`.

No v5 migration.

### Upsert

Rename `upsert_personal_policy_in_transaction` to a source/review-state name, e.g.:

~~~
upsert_personal_source_review_in_transaction
```

Inputs:
- grant;
- reviewed subject;
- selected handles.

Rules:
- same subject + same handles -> SourceAuthority preserved;
- changed subject or handles -> SourceAuthority advances;
- changed grant consumers alone -> SourceAuthority preserved;
- new stable source -> new SourceAuthority;
- new grant for same stable source after revocation may reuse current source authority if source facts are unchanged.

### Port deletion

Delete:
- `PersonalGrantRecords::consumer_policy`;
- repository implementation;
- `personal_grant_consumer_policy`.

### Context personal read

Delete:
- `CompletedRead.policy`;
- pre/post read policy reload;
- dependency policy field;
- policy comparison in every personal reauthorization branch.

Keep:
- exact current grant/GrantAuthority;
- source authority;
- reviewed subject;
- selected handles;
- trusted observation;
- feasibility query.

### 06 removal note

Retain/update the code comment:

~~~
Standing Contacts/Attention/Wellbeing source authority moves to Connections in
connection-observe-authority checkpoint 06.
```

## 17. 05-M — simplify Vault release/current-authority validation

Modify:

~~~
crates/adapters/vault/src/vault/personal_grants.rs
crates/adapters/vault/src/vault.rs
```

Delete:
- `validate_calendar_dependency_policy_in_transaction`;
- `validate_personal_dependency_policy_in_transaction`;
- `validate_remote_view_dependency_policy_in_transaction`;
- policy-specific dispatch logic.

Target transaction validation:

~~~
for dependency:
  verify person/freshness
  current_grant = read_data_access_grant_in_transaction(dependency.grant_id)
  validate_grant_dependency(current_grant, dependency)
```

No provider/Connections/source I/O inside the transaction.

Source currentness is still checked by:
- native Calendar dependency resolver;
- generic remote signed-preview resolver;
- personal source resolver;
before model/history/output use.

Update comments so Vault transaction authority is described as grant/state authority, not source/policy authority.

## 18. 05-N — remove policy from remote owner-key and provider protocol

Modify:

~~~
crates/modules/access/src/ports/remote_authorization.rs
crates/adapters/vault/src/vault/remote_authority.rs
crates/adapters/providers/src/sources/server.rs
crates/adapters/providers/src/control/authorization.rs
```

### `AuthorizedViewRead`

Delete `consumer_policy`.

### `RemoteViewAuthorizationRequest`

Delete:
- `policy_incarnation`;
- `policy_epoch`.

Request JSON has no `policy`.

### Key-holder signer

`validate_remote_view_grant_in_transaction`:
- no challenge policy;
- no remote mapping query.

Directly validate challenged current DataAccessGrant:
- GrantId;
- GrantAuthority;
- stable source;
- active/review state;
- one exact logical resource;
- View/connection resource format;
- consumer;
- purpose;
- processing.

The existing expected challenge already binds GrantAuthority and SourceAuthority separately.

### Logical resource validation

Use the canonical connection/View resource parser against the grant source connection. Do not recreate the deleted mapping merely to remember `view_id`.

### Rust challenge parsing

Delete policy wire parsing/validation from remote challenge parts.

Unknown old `policy` field must be rejected by same-snapshot strict decoder.

## 19. 05-O — remove policy from Go admission/release proof

Modify:

~~~
server/internal/authorization/source_service.go
server/internal/authorization/authorization.go
server/internal/authorization/*_test.go
server/internal/application/* source tests as affected
```

Delete:
- `viewPolicyWire`;
- `PolicyReference`;
- `ViewAdmission.Policy`;
- `Request.Policy`;
- `challengeWire.Policy`;
- `policyWire`.

`AdmitView` sends:

~~~
Request {
  Audience
  Purpose
  Consumer
  Source
  Grant
  Resources
  QueryDigest
  MaxItems
  MaxBytes
}
```

### Preserve proof security

Tests must still prove challenge/release are bound to:
- principal/client/device;
- audience;
- purpose/consumer;
- source identity + source epoch;
- grant ID + GrantAuthority;
- logical resources;
- query digest;
- item/byte bounds;
- result digest/admission id;
- key/signature;
- replay state.

Add explicit stale GrantAuthority and stale SourceAuthority tests after policy removal.

Do not replace policy epoch with another Go-only counter.

## 20. 05-P — define one compare-only `policy_digest`

Primary:

~~~
crates/app/src/first_party_observe.rs
```

### Delete pseudo-authority processing class

Delete `SourceProcessingPolicy` if caller audit still shows it is fingerprint-only.

Replace `FirstPartyObservePolicy.source_processing` with actual intended permission facts:

~~~
operation: GrantOperation::Read
processing: ProcessingRestriction::LocalOnly
```

`purpose` remains explicit.

Remote producer audience/paired transport class stays in source review evidence, not policy digest.

### Digest input

Canonical SHA-256 over a versioned representation equivalent to:

~~~
(
  "floe.first-party-observe-policy.sha256.v2",
  view_id,
  sorted trusted consumers,
  sorted categories,
  operation,
  purpose,
  processing,
)
```

Do not include member connection/resource because `InlineObserveTarget` separately binds connection and member logical resource. The View ID is included.

Do not include:
- leaf resources;
- binding selection;
- source authority;
- grant authority;
- producer fingerprint;
- provider identity;
- native subject.

### Internal representation

Prefer:

~~~
[ u8; 32 ]
```

or a tiny App-private newtype.

Convert to lowercase hex only at durable/product boundaries.

No authorization API accepts the digest.

### Rename

Production rename:
- `policy_fingerprint` -> `policy_digest`;
- `member_policy_fingerprint` -> `member_policy_digest`;
- `native_member_policy_fingerprint_for_target` -> digest naming;
- `remote_member_policy_fingerprint_for_target` -> digest naming;
- `DriftReason::PolicyFingerprint` -> `PolicyDigest`.

No deprecated forwarding names.

## 21. 05-Q — App Calendar/remote/personal overview cleanup

Modify:

~~~
crates/app/src/local_access_services.rs
crates/app/src/connection_observe.rs
crates/app/src/vault_host/calendar_access.rs
crates/app/src/vault_host/remote_observe.rs
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_owners.rs
```

### Calendar access overview

Delete `consumer_policy`.

State derives from:
- grant existence/state/review_required;
- GrantAuthority;
- current source/system status.

### ConnectionObserve overview

Delete `ConnectionObserveMember.consumer_policy`.

### Remote expectation

`RemoteObserveMemberExpectation` target:

~~~
view_id
policy_digest
resource
producer_fingerprint
source_authority
connection_revision
provider_identity
recipient
expected_grant_id
expected_grant_authority
```

Delete `expected_policy`.

Expectation coherence becomes:
- both grant ID + authority absent;
- or both present.

### Review member

Do not query remote policy mapping.

`live_member_grants` already supplies the exact current grant expectation.

### Enable

`prepare_remote_view_grant_activation` receives only expected grant.

### Policy digest

Review and decision re-read recompute current digest from current product policy and compare.

## 22. 05-R — collapse inline source revision to bundle level

Modify:

~~~
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_resolution.rs
crates/app/src/vault_host/interaction_owners.rs
crates/app/src/vault_host/conversation_turn/interaction_publication.rs
crates/modules/conversation/src/domain/interaction.rs
```

All current inline Observe bundles describe one stable source connection.

### Snapshot

Target:

~~~
SnapshotMember
  member_id
  policy_digest
  resource
  expected_grant

InlineReviewSnapshot
  source_revision
  connection_revision
  producer_fingerprint
  native_subject
  members
```

### Live state

Target:

~~~
LiveMember
  member_id
  policy_digest
  resource
  live_grants

LiveInlineState
  source_revision
  connection_revision
  producer_fingerprint
  native_subject
  connection_usable
  members
```

### Capture rules

- blocked requirement source revision is the reviewed bundle source revision;
- native/personal one-member bundles use that exact revision;
- remote multi-View bundle must prove all fresh member previews describe the same source authority;
- disagreement fails closed instead of storing per-member divergence.

### Durable target

`ReviewedBundleMember`:

~~~
member_id
policy_digest
resource
expected_grant
```

`InlineObserveTarget` adds:

~~~
source_revision: Option<AuthorityRevision>
```

`AuthorityRevision` comment becomes source authority only.

### Canonical target bytes

Serialize source revision once at target level.

Delete per-member policy/source authority bytes.

Old durable targets are intentionally incompatible.

## 23. 05-S — Conversation review resolution without policy epoch

Update comparison:

1. connection/source/producer/native identity;
2. bundle source revision;
3. exact canonical member set;
4. member resource;
5. `policy_digest`;
6. expected grant absence or exact GrantId/GrantAuthority.

Delete:
- `PolicyStore`;
- policy record reads;
- `policy_key`;
- `reviewed_policy`;
- `DriftReason::PolicyAuthority`.

### Mutation behavior

If pending review digest differs before mutation:
- supersede/fail review.

If the reviewed grant authority differs:
- grant drift.

After owner mutation:
- verify exact member set/digest and exactly one expected live grant per member;
- the owner mutation may legitimately advance GrantAuthority.

No policy epoch comparison.

## 24. 05-T — protocol / FFI / Flutter cutover

Modify:

~~~
crates/bindings/protocol/src/dto/access.rs
crates/bindings/protocol/src/dto/local_access.rs
crates/bindings/protocol/tests/*
crates/bindings/ffi/src/remote_wire.rs
crates/bindings/ffi/src/conversion/owners.rs
crates/bindings/ffi/tests/*
apps/client/lib/features/connections/domain/remote_owner_models.dart
apps/client/lib/features/connections/domain/native_calendar_access.dart
affected Flutter tests
```

### Remote bundle DTO

Rename field:

~~~
policy_fingerprint
  -> policy_digest
```

Delete:

~~~
expected_policy
```

Coherence:
- expected grant ID/authority both absent or both present.

### Flutter

Rename:
- `policyFingerprint` -> `policyDigest`.

Delete:
- `expectedPolicy`;
- parsing/serialization of the removed authority.

### Native Calendar access

Delete:
- `consumer_policy` DTO/model field.

### Same-snapshot rule

No aliases for:
- `policy_fingerprint`;
- `expected_policy`;
- `consumer_policy`.

Old JSON is rejected.

Do not bump protocol solely for compatibility.

## 25. 05-U — persistence and fresh-profile discipline

05 changes:
- personal policy schema;
- removal of Calendar policy tables;
- removal of remote View mapping tables;
- ContextDependency serialized shape;
- processing scope serialized shape;
- Conversation inline target shape;
- protocol/FFI product fields;
- server admission challenge shape.

### Fresh profile

Use a fresh Floe development profile for final validation.

Do not migrate old local rows.

### Obsolete table fail-closed check

On reopen, an old profile containing removed tables must fail clearly rather than silently running with stale second-authority data.

At minimum detect:
- `calendar_grant_policy_schema`;
- `calendar_grant_policies`;
- `remote_view_grant_schema`;
- `remote_view_grant_mappings`.

Personal schema marker v5 is rejected by new marker.

Keep this detection small and one-way; it is not a compatibility implementation.

### No speculative schema bump

Do not bump unrelated App protocol/version numbers solely because old local clients are incompatible.

## 26. Test migration matrix

### 26.1 Context contract

Add/modify tests:

1. `ConsumerPolicyAuthority` no longer compiles/exports.
2. dependency JSON has no policy field.
3. old dependency JSON with `consumer_policy` is rejected.
4. dependency canonicalization still covers GrantAuthority + SourceAuthority.
5. ProcessingSourceScope has exactly two authority revisions.
6. recipient consent changes on GrantAuthority/SourceAuthority/resource drift.
7. recipient consent does not require a third policy epoch.

### 26.2 Access grant semantics

1. consumer change -> scope change -> GrantAuthority advances.
2. processing change -> GrantAuthority advances.
3. exact active same-scope review -> no authority change.
4. source authority change alone -> GrantAuthority unchanged.
5. grant dependency rejects old GrantAuthority.
6. grant dependency rejects consumer/scope mismatch.
7. no policy authority fixture exists.

### 26.3 Native Calendar

1. same policy/scope active review is GrantAuthority no-op.
2. shipped consumer change in reviewed scope advances GrantAuthority.
3. source/resource change leaves GrantAuthority unchanged.
4. read dependency contains only GrantAuthority + SourceAuthority.
5. old Calendar policy tables fail old-profile reopen.
6. no `calendar_grant_policy.rs`.

### 26.4 Remote View

1. direct source+logical-resource lookup finds exact current grant.
2. duplicate non-revoked source/resource grants fail closed.
3. revoked historical + one current grant works.
4. expected absence races with concurrent grant -> conflict.
5. multi-member activation is all-or-nothing.
6. same scope re-review preserves authority.
7. scope/consumer change advances GrantAuthority.
8. remote mapping tables absent.
9. old mapping profile rejected.
10. dependency old GrantAuthority/source authority rejected.

### 26.5 Remote owner-key / server

1. challenge contains no policy object.
2. owner-key signer validates current grant directly.
3. stale GrantAuthority denied.
4. stale SourceAuthority denied.
5. wrong source/resource/consumer/purpose denied.
6. challenge/release replay protection unchanged.
7. exact resource/query/bounds/result digest unchanged.
8. Mail/Work/Logistics/Calendar generic routes all pass.

### 26.6 Personal

1. schema has no policy/consumer columns.
2. current source authority survives consumer-only grant scope change.
3. consumer-only change advances GrantAuthority.
4. subject change advances SourceAuthority, not GrantAuthority when scope unchanged.
5. selected handle change advances SourceAuthority.
6. exact no-op preserves both.
7. old policy epoch schema rejected.
8. Context personal dependency has no policy field.
9. current source/subject/handles still reauthorize.
10. feasibility query change advances GrantAuthority.
11. exact feasibility query re-review is no-op.
12. 06 removal note remains.

### 26.7 Policy digest / interactions

1. digest stable under consumer/category input order.
2. digest changes for consumer/category/operation/purpose/processing change.
3. digest unchanged by extension install/binding.
4. digest unchanged by leaf resource/source authority/grant authority change.
5. native vs remote source transport identity does not invent a different permission digest when intended GrantScope/View policy is identical.
6. pending review digest drift fails before mutation.
7. grant drift fails independently.
8. source drift fails independently.
9. durable target has one source revision at bundle level.
10. old `policy_fingerprint` / `policy_authority` JSON is rejected.

### 26.8 Wire / Flutter

1. remote member uses `policy_digest`.
2. no `expected_policy`.
3. native Calendar access has no `consumer_policy`.
4. old wire fields rejected.
5. echo review/enable still round-trips exact digest + grant/source expectations.
6. product UX unchanged aside from removed internal authority fields.

## 27. Residual / deletion audit

Run after implementation:

~~~
rg -n "ConsumerPolicyAuthority|consumer_policy" crates apps server
rg -n "policy_authority|expected_policy" crates apps server
rg -n "policy_incarnation|policy_epoch" crates apps server
rg -n "calendar_grant_policy|calendar_grant_policies|calendar_grant_policy_schema" crates
rg -n "remote_view_grant_mappings|remote_view_grant_schema|remote_view_grants" crates
rg -n "evolve_calendar_consumer_policy" crates
rg -n "policy_fingerprint" crates apps
rg -n "SourceProcessingPolicy|source_processing" crates/app
rg -n "personal_grant_consumer_policy|fn consumer_policy" crates
rg -n "PolicyReference|viewPolicyWire" server/internal
rg -n "\"policy\"" server/internal/authorization
```

### Required zero production matches

All exact concepts above are zero in production.

Historical plan/ADR text may remain only where clearly historical and current docs no longer describe them as live.

### Required positive checks

~~~
rg -n "policy_digest" crates apps
rg -n "GrantAuthority" crates/contracts/context crates/modules/access crates/app
rg -n "SourceAuthority" crates/contracts/context crates/modules/context crates/app
rg -n "source_incarnation|source_epoch" crates/adapters/vault/src/vault/personal_grants.rs
```

The last search is expected until 06 and must be documented as the bounded personal-source owner.

## 28. Architecture/documentation convergence

Update current-state docs:

~~~
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
docs/product/integrations-and-privacy.md
server/README.md if the admission challenge shape is documented
```

After 05 they must state:

- GrantAuthority is the only standing permission epoch.
- SourceAuthority is the only current source/resource epoch.
- no `ConsumerPolicyAuthority` exists.
- Context dependencies store GrantAuthority + SourceAuthority and exact provenance.
- exact-recipient scopes use those two authorities only.
- policy digest is compare-only review identity.
- digest is derived from canonical intended View permission policy, not assignment/source/grant state.
- remote key-holder/server admission signs/verifies GrantAuthority and SourceAuthority without policy epoch.
- native Calendar has no policy side table.
- remote View has no duplicated mapping table.
- personal source review row temporarily owns source epoch/subject/handles only until 06.
- feasibility query-specific review remains explicit and query changes fence old evidence through GrantAuthority.
- source transport audience is separate from model recipient consent.

Remove stale current claims that mention policy authority or policy fingerprint as runtime authority.

## 29. Suggested implementation slices

### 05-A — contract deletion

- delete ConsumerPolicyAuthority;
- remove dependency policy field;
- remove ProcessingSourceScope policy field;
- migrate compile surface/tests.

Suggested commit:

~~~
context: remove consumer policy authority
```

### 05-B — GrantAuthority-only native/personal semantics

- delete Calendar policy table/file;
- trim personal schema;
- remove personal policy port;
- feasibility query GrantAuthority fence.

Suggested commit:

~~~
access: make grant authority the sole permission epoch
```

### 05-C — remote mapping deletion

- exact source/resource grant lookup;
- atomic generic grant batch activation;
- delete remote mapping table/file;
- simplify RemoteGrantStore.

Suggested commit:

~~~
vault: remove duplicate remote grant mapping
```

### 05-D — remote proof cutover

- remove policy from owner-key signer;
- provider request;
- Go admission/challenge/release;
- tests.

Suggested commit:

~~~
remote: bind source reads to grant and source authority only
```

### 05-E — policy digest and durable review

- delete SourceProcessingPolicy;
- introduce policy digest;
- App review/interaction cutover;
- bundle-level source revision;
- Conversation target serialization.

Suggested commit:

~~~
app: replace policy epoch with review digest
```

### 05-F — wire/client cutover

- protocol/FFI fields;
- Flutter model;
- negative old-shape tests.

Suggested commit:

~~~
client: remove observe policy authority fields
```

### 05-G — docs/closure

- residual audit;
- current architecture docs;
- execution evidence;
- README 05 Complete.

Suggested commit:

~~~
docs: complete connection observe checkpoint 05
```

Combine slices where a direct cutover yields a smaller final system. Do not add temporary wrappers to preserve intermediate compilation.

## 30. Verification

Run affected crates during iteration.

Minimum Rust close gate:

~~~
cargo test -p floe-context-contract
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-provider-adapters
cargo test -p floe-conversation
cargo test -p floe-inference
cargo test -p floe-agent-runtime
cargo test -p floe-app
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
```

If a named package/filter does not exist or matches zero tests, run the actual nearest target/full crate and record the command.

### Go

Because admission challenge shape changes:

~~~
cd server
go test ./...
go test -race ./...
go vet ./...
```

Run focused authorization/application/HTTP suites during iteration.

Use `gofmt` on changed Go.

### FFI / Flutter

Because review/access DTOs change:

~~~
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/connections
flutter test test/features/conversation
flutter test
flutter build macos
```

Do not preserve old JSON just to keep old fixtures green; rewrite same-snapshot fixtures.

### Broad Rust

Final broad gate:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
```

If default parallel is also run and exposes an unrelated known race, report it separately.

### Native/provider live smoke

05 changes authority representation but not native/provider I/O.

No external account/device mutation is required.

If an already-authorized disposable native/remote source is available, a smoke may be run; otherwise record SKIPPED and rely on deterministic owner/provider fixtures.

## 31. Close procedure

Before marking 05 Complete:

1. rerun every residual search;
2. confirm `ConsumerPolicyAuthority` has zero production matches;
3. confirm policy epoch/incarnation has zero production matches;
4. confirm Calendar policy file/tables are absent;
5. confirm remote mapping file/tables are absent;
6. confirm personal row keeps only bounded source/review facts;
7. confirm old side-table profile fails closed;
8. confirm policy digest is compare-only;
9. confirm SourceProcessingPolicy is gone;
10. confirm remote challenge contains no policy;
11. confirm stale GrantAuthority and stale SourceAuthority each fail independently;
12. confirm feasibility query change advances GrantAuthority;
13. confirm exact-recipient processing still binds GrantAuthority + SourceAuthority + exact resources;
14. run Rust/Go/FFI/Flutter/architecture/broad gates;
15. update current architecture/product/server docs;
16. append execution evidence to this document;
17. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Complete
    - 05 Complete
    - 06 Not started
18. commit closure;
19. stop. Do not begin checkpoint 06.

Execution evidence must record:

- date;
- start local HEAD / fetched origin/main;
- implementation/closure SHAs;
- final ContextDependency shape;
- final ProcessingSourceScope shape;
- GrantAuthority-only permission semantics;
- feasibility query fencing behavior;
- deleted Calendar policy store;
- deleted remote mapping store;
- final RemoteGrantStore surface;
- final remote owner-key validation path;
- final Go admission challenge shape;
- final personal source/review row schema;
- retained personal source authority and 06 removal condition;
- final policy digest representation/input;
- final durable inline target/member shape;
- protocol/FFI/Flutter field changes;
- old-profile rejection behavior;
- residual audit;
- architecture docs changed;
- all executed commands/results;
- skipped live smoke reason;
- final clean worktree.

## 32. Required agent report

Report:

1. start HEAD / origin-main / final HEAD;
2. final `ContextDependency` contract;
3. final `ProcessingSourceScope` contract;
4. proof that GrantAuthority is the only standing permission epoch;
5. feasibility query change/no-op GrantAuthority behavior;
6. Calendar policy store deletion;
7. remote View mapping deletion and exact source/resource lookup;
8. atomic remote bundle activation after mapping deletion;
9. final `RemoteGrantStore`/remote read path;
10. personal source/review row schema and retained SourceAuthority;
11. personal `consumer_policy` port/API deletion;
12. remote key-holder current-grant validation;
13. Rust provider request/challenge shape;
14. Go admission/challenge/release shape;
15. final policy digest input/representation;
16. deleted SourceProcessingPolicy result;
17. App review/enable expectation shape;
18. durable Conversation inline target/member shape;
19. protocol/FFI/Flutter field cutover;
20. tests moved/rewritten/deleted;
21. residual audit and 06/07 bounded matches;
22. current architecture/product/server docs updated;
23. Rust verification commands/results;
24. Go verification commands/results;
25. Flutter/FFI verification commands/results;
26. live source smoke or SKIPPED reason;
27. checkpoint commit SHA(s);
28. clean worktree confirmation;
29. confirmation that checkpoint 06 was not started.
