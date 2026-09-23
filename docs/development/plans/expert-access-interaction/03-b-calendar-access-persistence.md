# Checkpoint 03-B — Calendar Access persistence convergence

- **Execution baseline:** Checkpoint 03-A completion on top of main bbce2fa2ae2da4bffb05185f39d8d50aa3e6b82a
- **Depends on:** 03-A source-independent Registry
- **Goal:** make DataAccessGrant the canonical Calendar Observe record and replace native/remote Calendar mapping tables with minimal grant-scoped policy metadata.

Do not modify Flutter/product permission ceremony here. 03-C owns the public surface cutover; Checkpoint 04 owns the final Use with Floe UX.

---

## 1. Baseline anchors

| File | Current symbol / responsibility |
|---|---|
| crates/adapters/vault/src/vault/access_grants.rs | canonical data_access_grants store; create/get/list/mutate; source lookup helper |
| crates/adapters/vault/src/vault/calendar_grants.rs | CalendarGrantMapping; native Calendar mapping schema; setup/install/configure coupling |
| crates/adapters/vault/src/vault/remote_calendar_grants.rs | RemoteCalendarGrantMapping; remote Calendar mapping schema/binding/review |
| crates/adapters/vault/src/vault/personal_grants.rs | Calendar dependency policy reauthorization via UNION over native/remote mapping tables |
| crates/adapters/vault/src/vault/remote_authority.rs | remote Calendar policy/grant authority serialization and verification |
| crates/adapters/vault/src/repositories/remote_grants.rs | RemoteGrantStore adapter for Calendar activation/binding |
| crates/modules/access/src/application/remote_calendar.rs | remote Calendar grant scope/admission |
| crates/modules/context/src/application/native_calendar.rs | native current-source read admission |
| crates/modules/context/src/application/remote_sources.rs | remote Calendar read and ContextDependency construction |
| crates/app/src/vault_host/calendar_access.rs | canonical Calendar first-party consumer policy and native subject/connection composition |

Symbols may move after 03-A. Re-find by semantic name rather than preserving old file layout.

---

## 2. Canonical data model

### 2.1 DataAccessGrant remains source/scope/state authority

The canonical grant already stores:

~~~text
grant_id
person_id
authority_owner
GrantSourceBinding
  connection_id
  connector
  execution_owner
  source_authority
GrantScope
  resources
  categories
  operations
  purposes
  consumers
  processing
GrantAuthority
GrantState
review_required
~~~

Do not copy these fields into another Calendar mapping.

### 2.2 CalendarGrantPolicy stores only Calendar-specific authority metadata

Introduce one Calendar policy record shared by native and remote Calendar grants.

Conceptual contract:

~~~text
CalendarGrantPolicy
  grant_id
  person_id
  consumer_policy_authority
  reviewed_native_subject_fingerprint?  # Some only for native providers
~~~

Storage may include columns needed to enforce uniqueness/size, but the serialized semantic record must not duplicate:

- setup_id;
- Registry instance/revision;
- Calendar view handle;
- Expert/tool installation id;
- Expert/tool assignment id;
- connection id / connector / execution owner;
- source authority;
- resource list;
- GrantScope;
- GrantAuthority.

Those values are read from DataAccessGrant.

### 2.3 One policy table

Replace both Calendar mapping stores with one adapter schema, for example:

~~~text
calendar_grant_policy_schema
calendar_grant_policies
  grant_id TEXT PRIMARY KEY
  person_id TEXT NOT NULL
  policy_incarnation TEXT NOT NULL
  policy_epoch INTEGER NOT NULL
  reviewed_native_subject_fingerprint TEXT NULL
  payload TEXT NOT NULL
~~~

The exact SQL naming can differ, but there must be one Calendar policy authority path for both native and remote grants.

Do not rename calendar_grant_mappings while keeping its old payload.

---

## 3. Generic DataAccessGrant source lookup

### 3.1 Current problem

The current helper find_data_access_grant_by_source_in_transaction assumes at most one grant per source identity. Remote Calendar may legitimately hold one grant per Calendar resource under the same connection/source authority.

### 3.2 Add bounded exact-source enumeration

Add an adapter query with semantics such as:

~~~text
data_access_grants_for_source(
  person_id,
  authority_owner,
  connection_id,
  connector,
  execution_owner,
  source_authority
) -> Vec<DataAccessGrant>
~~~

Requirements:

- exact current source_authority match, not only same_identity;
- bounded result count;
- decode and validate every grant;
- reject duplicate grant ids / corrupt rows;
- no caller-provided wildcard;
- no selection by Registry setup id.

Create an index including current source authority if the existing index is insufficient.

### 3.3 Selection rules

Native current Calendar:

- select exactly one current grant whose source matches the current native Calendar connection;
- its scope resources equal or contain the selected Calendar resource set according to the current native policy;
- it admits the exact consumer/purpose/processing requested;
- zero matches -> AccessReviewRequired;
- more than one semantically matching current grant -> fail closed Conflict/VaultUnavailable according to corruption semantics.

Remote Calendar:

- enumerate exact-source grants;
- select exactly one active grant containing the requested resource and exact consumer/purpose/processing;
- multiple resources may therefore have separate grants under one connection;
- ambiguous matching grants fail closed.

Do not select a grant because it was once attached to a CalendarExpertSetup.

---

## 4. ConsumerPolicyAuthority ownership

ConsumerPolicyAuthority is not a substitute for DataAccessGrant authority. It identifies the reviewed consumer policy that a ContextDependency was released under.

### 4.1 Policy lifecycle

For a fresh Calendar review:

- create a new ConsumerPolicyAuthority;
- persist it with the grant policy record;
- construct ContextDependency with that authority.

For a semantic no-op review:

- same grant source/scope/consumers;
- same reviewed native subject when native;
- keep ConsumerPolicyAuthority stable.

For a consumer-policy change:

- DataAccessGrant activation/mutation uses normal GrantAuthority CAS;
- advance ConsumerPolicyAuthority;
- persist the new authority atomically with the grant mutation.

For source replacement or a new grant id:

- create/rebind policy only after the new grant exists in the same transaction;
- stale dependencies on the old source/grant/policy must fail.

### 4.2 Exact consumer policy

Use the canonical first-party consumer set from Checkpoint 02 product composition. Do not duplicate strings in Access/Vault.

Do not store a second consumer list in CalendarGrantPolicy; the current approved consumers are already in GrantScope. ConsumerPolicyAuthority is the revision identity of that policy.

---

## 5. Native Calendar review and read

### 5.1 Review input

The Access-owned native Calendar review operation is based on current owner facts:

~~~text
Person from CallerContext/Vault
current CalendarConnection
selected calendar resources
canonical first-party consumers
fresh native subject observation
expected current GrantAuthority?  # CAS on update
~~~

No Registry setup id, assignment id, installation id or view handle is an input.

### 5.2 Review sequence

Required sequence:

1. load current CalendarConnection;
2. validate native provider/device/source authority;
3. obtain a fresh native subject observation through Context/native adapter;
4. compare before/after subject continuity;
5. construct exact GrantSourceBinding from current connection;
6. construct GrantScope using selected resources and canonical consumers;
7. locate current source grant if updating;
8. create/activate grant under CAS;
9. persist/update CalendarGrantPolicy with reviewed native fingerprint and consumer-policy authority in the same transaction;
10. re-read/check current source after I/O where the existing Context sequence requires it.

Do not hold a DB transaction while waiting for EventKit/native I/O. Perform observation first, then commit against the expected source authority and revalidate current connection as required.

### 5.3 Native read

authorize_current_native_calendar_grant must be replaced or rewritten to:

- load current grant by current source identity;
- load CalendarGrantPolicy by grant_id;
- validate exact consumer/resource/purpose/processing;
- validate policy authority;
- validate reviewed native subject fingerprint;
- never inspect Registry snapshot/setup/view state.

Rename it if the old name hides the new generic source-owned semantics.

---

## 6. Remote Calendar review and read

### 6.1 Signed source review remains

Keep existing producer/pairing/source preview verification. Do not weaken:

- pinned producer;
- signed descriptor;
- exact pairing/person/device;
- connector/connection/resource;
- source authority;
- current connection revision.

### 6.2 Activation

review_and_activate_remote_calendar_grant must:

- build source/scope from verified preview + current connection + canonical consumers;
- select/create the exact DataAccessGrant;
- persist CalendarGrantPolicy in the same transaction;
- not persist a second copy of source/scope.

### 6.3 Binding/read

Replace remote_calendar_grant_binding mapping lookup with source/resource lookup from data_access_grants plus CalendarGrantPolicy by grant_id.

Remote Context read remains:

~~~text
current source preview
-> exact DataAccessGrant
-> exact resource/consumer admission
-> CalendarGrantPolicy
-> provider read
-> re-fetch grant/source/policy
-> ContextDependency
~~~

Delete RemoteCalendarGrantMapping and remote_calendar_grant_mappings when the new path is green.

---

## 7. Dependency reauthorization

### 7.1 personal_grants.rs

Replace validate_calendar_dependency_policy_in_transaction SQL UNION over legacy mapping tables.

New validation:

1. load DataAccessGrant by dependency.grant_id;
2. validate current GrantAuthority/source/scope against dependency;
3. load CalendarGrantPolicy by grant_id/person;
4. compare dependency.consumer_policy to policy authority;
5. for native dependencies, ensure the policy has a valid reviewed subject and the current observation/source reauthorization path still validates it;
6. for remote dependencies, native subject must be absent.

Do not infer policy from Registry state.

### 7.2 remote_authority.rs / provider wire

Any server authorization payload that carries policy incarnation/epoch must source it from CalendarGrantPolicy.

Do not keep remote_calendar_grant_mappings only because the provider wire wants policy authority.

### 7.3 action proposals

/focus proposal ContextDependency reauthorization must continue to work after mapping deletion.

Action publication/dispatch stands on:

- Expert/Task identity;
- exact ContextDependency observation;
- current DataAccessGrant;
- current CalendarGrantPolicy;
- current Calendar connection/provider preconditions;
- ActionAuthority.

No CalendarExpertSetup or Registry Calendar view is consulted.

---

## 8. Delete old persistence

Once native and remote new paths pass targeted tests, delete:

~~~text
CalendarGrantMapping
calendar_grant_schema
calendar_grant_mappings
RemoteCalendarGrantMapping
remote_calendar_grant_schema
remote_calendar_grant_mappings
calendar_grant_connection_id
install_calendar_expert_with_connection
apply_calendar_access_with_grant
setup/install/assignment validation inside Calendar grant authorization
~~~

calendar_grants.rs may be deleted or reduced to Access-owned Calendar policy persistence. Prefer a new file name such as calendar_grant_policy.rs rather than leaving misleading Expert-setup naming.

remote_calendar_grants.rs may be deleted if all Calendar-specific remote persistence moved to the shared Calendar policy store. Do not modify generic remote_view_grants.rs except mechanical trait fallout.

---

## 9. Persistence compatibility policy

Do not migrate old authorization state.

### Fresh profile

Fresh Vault initialization creates:

- data_access_grants schema;
- calendar_grant_policy schema;
- no calendar_grant_mappings;
- no remote_calendar_grant_mappings.

### Existing incompatible development profile

If old Calendar mapping tables/schema are present without the new supported shape:

- fail explicitly as UnsupportedVersion/VaultUnavailable according to the repository's storage convention;
- surface the documented development reset requirement;
- never auto-create a new active grant from old rows;
- never silently drop unknown user/provider data;
- unresolved external Action records remain untouched.

Do not add a migration that translates CalendarExpertSetup or legacy mapping rows into current grants.

---

## 10. Tests

### DataAccessGrant lookup

- exact source authority required;
- native unique current grant selected;
- remote separate resource grants coexist;
- wrong resource ignored/denied;
- duplicate semantic grant ambiguity fails closed;
- foreign Person/owner rejected.

### Native

- fresh review creates active grant + CalendarGrantPolicy;
- exact first-party Schedule/Commitments/Focus/Wellbeing consumers admitted;
- third-party consumer denied;
- no-op review preserves ConsumerPolicyAuthority;
- consumer change advances policy authority;
- pause/review/re-enable uses GrantAuthority CAS;
- source authority drift requires review;
- subject fingerprint drift requires review;
- reopen preserves grant/policy;
- no Registry lookup occurs.

### Remote

- signed preview + review creates grant/policy;
- same source with two Calendar resources can have two exact grants;
- Schedule read chooses correct resource grant;
- producer/source/connection change rejected;
- policy authority survives reopen;
- pause invalidates later admission;
- no remote mapping table lookup remains.

### Dependency/history/action

- old dependency fails after grant authority change;
- old dependency fails after consumer-policy authority change;
- old dependency fails after source authority change;
- valid dependency reauthorizes after reopen;
- Schedule history projection and /focus proposal still use current dependency;
- no setup/view/install/assignment id is needed.

### Legacy profile

- fixture containing old Calendar mapping schema is not promoted;
- opening/using it returns the documented incompatible/reset outcome;
- fresh profile contains no old tables.

---

## 11. Residual gate

At 03-B exit, production search must be zero for:

~~~text
CalendarGrantMapping
RemoteCalendarGrantMapping
calendar_grant_mappings
remote_calendar_grant_mappings
calendar_grant_connection_id
install_calendar_expert_with_connection
authorize_calendar_grant(  # old setup-bound form
setup_id                   # inside Calendar access persistence
expert_assignment_id       # inside Calendar access persistence
tool_assignment_id         # inside Calendar access persistence
expert_installation_id     # inside Calendar access persistence
tool_installation_id       # inside Calendar access persistence
~~~

Search policy_incarnation/policy_epoch. Calendar matches must belong to CalendarGrantPolicy, not duplicated source mapping rows.

Search remote_view_grant_mappings. It is allowed and out of scope.

CalendarExpertSetup types may still exist in App/wire/UI until 03-C, but must have no persistence/authorization effect.

---

## 12. Verification

Targeted first:

- access grant tests;
- Vault access/calendar policy tests;
- Context native Calendar tests;
- Context remote Calendar tests;
- remote authority tests;
- proposal/dependency reauthorization tests.

Then:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Do not proceed to 03-C if a Calendar source read or dependency reauthorization still consults a Registry/setup/mapping identity.