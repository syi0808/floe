# 06: Standing personal Observe convergence

Prerequisite: 05 complete.

Status: In progress.

Planning base: `main` at `5d5cdd75c3fddf452300009be5acfb46d261cf51` on 2026-09-29.

Checkpoints 01-05 established the final authority split for Calendar and remote Views:

~~~
Connections / producer
  stable source identity
  current source resources
  trusted native subject where applicable
  SourceAuthority

Access
  stable source + logical View permission
  GrantAuthority

Context
  current acquisition
  exact source_resources
  SourceAuthority provenance
~~~

Contacts, Attention and Wellbeing are the remaining standing Observe sources that do not yet follow this ownership model. Their current source authority, reviewed native subject and — for Contacts — selected provider handles still live in `personal_grant_policies`, while Context reconstructs source identity from Access-owned constants and Expert candidates are synthesized even when no serving `SourceConnection` exists.

Checkpoint 06 moves the standing personal source truth to Connections and deletes the personal source side table. Feasibility is intentionally not collapsed into this model: its reviewed event/destination/query remains a contextual Access review with its own bounded source evidence record.

This is a direct cutover. Old local personal policy/source rows and old personal dependency shapes are disposable. Do not add compatibility readers, vNext rows, dual source authorities, fallback synthetic candidates or old/new grant scopes.

Line numbers below are planning-base anchors on `5d5cdd75`. Re-resolve symbols on the actual execution HEAD before editing.

## 1. Scope

### Converge in 06

- Contacts / `people.identity`;
- Attention / `attention.coarse`;
- Wellbeing / `wellbeing.derived`;
- their Connections source records;
- standing grants;
- Context acquisition/reauthorization;
- Expert source candidates/bindings;
- pending Observe review source evidence;
- source-configuration meaning of the existing personal access UI.

### Keep explicitly separate

- Feasibility / `schedule.feasibility`;
- exact-recipient model consent;
- Actions authority;
- raw provider/OS acquisition mechanics;
- final generic ConnectionObserve DTO/UI cleanup owned by 07.

## 2. Exit state

Checkpoint 06 is complete only when all of the following are true.

1. Contacts, Attention and Wellbeing each use a durable `SourceConnection`.
2. Connections is the only owner of their standing current `SourceAuthority`.
3. Connections is the only owner of their standing trusted native subject fingerprint.
4. Connections is the only owner of Contacts selected resource handles.
5. `personal_grant_policies` no longer exists in the current Vault schema.
6. `personal_grant_schema` no longer exists in the current Vault schema.
7. Standing personal source authority is never looked up by GrantId.
8. Standing personal native subject is never looked up by GrantId.
9. Standing Contacts selected handles are never looked up by GrantId.
10. A Contacts handle is stored losslessly as a `ConnectionResource.handle`; no truncation/hash remapping is introduced.
11. Standing Contacts input is validated through `ResourceHandle`, therefore respects the canonical 256-byte source-resource contract.
12. Apple and dormant Android generated contact identity handles fit that bound; provider-native raw IDs may keep looser provider-bound limits internally.
13. Contacts source resource count remains bounded by the existing personal domain maximum of 64, even though generic Connections can store more.
14. Attention source resources contain exactly the canonical bounded attention source resource.
15. Wellbeing source resources contain exactly the canonical bounded wellbeing source resource.
16. Each standing personal source is Ready only when it has a valid trusted native subject.
17. A reviewed resource+subject change is one Connections CAS transition.
18. A combined resource+subject change advances `SourceAuthority` exactly once.
19. Exact resource+subject no-op does not advance source revision/authority.
20. Label-only resource metadata changes do not advance `SourceAuthority`.
21. Explicit source disconnect advances `SourceAuthority` and blocks later standing reads.
22. Native subject drift observed during a read fails closed and is not silently adopted.
23. A new reviewed subject becomes current source truth only through an explicit Connections mutation/review.
24. Standing personal `GrantSourceBinding` is built from the actual current `SourceConnection`.
25. Standing personal `GrantScope.resources` contains one logical `connection_view_resource(view_id, connection_id)`.
26. Contacts grant permission resource is `people.identity:<connection-id>`.
27. Attention grant permission resource is `attention.coarse:<connection-id>`.
28. Wellbeing grant permission resource is `wellbeing.derived:<connection-id>`.
29. Standing grant scope contains no contact handle and no native subject fingerprint.
30. Contacts resource edits do not change GrantId or GrantAuthority when the intended permission policy is otherwise unchanged.
31. Attention/Wellbeing subject review does not change GrantAuthority when permission scope is unchanged.
32. A first-party consumer/purpose/processing policy change still advances GrantAuthority.
33. Pause/re-enable changes GrantAuthority according to existing grant semantics without changing source resources/SourceAuthority.
34. Standing first-party personal consumers come from the same App shipped-manifest policy rules as Calendar/remote Views.
35. An extension install/binding cannot add itself to a standing personal grant.
36. Standing personal policy digest uses the canonical View ID, not connector ID.
37. `SourceCandidateRequest` no longer has a Calendar-only `calendar_connection` field.
38. Candidate discovery consumes current `SourceConnection` records for Calendar and standing native personal sources through one common input.
39. A standing personal candidate exists only for a serving current `SourceConnection`.
40. Personal candidate resource is the logical connection/View resource.
41. Contacts candidate ID/binding revision remains unchanged across handle-set changes.
42. Attention/Wellbeing candidate ID/binding revision remains unchanged across subject/source-authority changes.
43. No source candidate is fabricated solely from hard-coded personal connector constants.
44. A saved binding to a disconnected/missing source remains unavailable and is never redirected to another provider.
45. Context standing personal reads load current source state from Connections.
46. Context standing personal reads load current grant state from Access/Vault independently.
47. Contacts acquisition uses the exact current `SourceConnection.resources`.
48. Attention/Wellbeing acquisition uses their exact current singleton source resource.
49. Context uses `SourceConnection.native_subject_fingerprint` as the expected native subject.
50. After provider I/O, Context reloads current source state and rejects SourceAuthority/resource/subject drift.
51. After provider I/O, Context reloads current grant and rejects GrantId/GrantAuthority drift.
52. Standing personal dependency `resources` contains only the logical View permission.
53. Standing personal dependency `source_resources` contains the exact current Connections resources.
54. Standing personal dependency `source_authority` equals the exact Connections authority observed for the read.
55. Old dependency becomes stale after Contacts handle-set change.
56. Old dependency becomes stale after an explicitly reviewed personal native-subject change.
57. Source/config drift never causes Context to silently choose a different source.
58. `PersonalGrantRecords` or its replacement exposes no standing `selected_handles`, `current_source_authority` or `reviewed_subject` lookup.
59. App personal dependency resolver receives a Connections reader/source owner in addition to grant records and native driver.
60. Manager direct personal tools use the same Connections-owned standing source path as Expert reads.
61. Expert personal readers validate the selected logical View against the exact current SourceConnection.
62. `PersonalReadRequirement.same_authority` is deleted; after 02, `GrantSourceBinding` equality already is stable-source equality.
63. Caller-zero `active_resource_grant` / `active_resource_grants` helpers are deleted if the execution-head audit confirms they remain test-only.
64. Access no longer constructs standing personal source identity from connector/device constants.
65. Access no longer owns high-level Contacts/Attention/Wellbeing source review orchestration.
66. App owns the cross-owner standing personal review composition, analogous to Calendar.
67. Connections mutates source truth; Access mutates standing permission; neither transaction pretends to cover both stores.
68. If source configuration succeeds and a later grant CAS conflicts, source configuration is not rolled back.
69. `PersonalAccessOverview.source_authority` for standing sources comes from Connections.
70. `PersonalAccessOverview.native_subject_fingerprint` for standing reviewed sources reflects Connections current trusted subject.
71. Existing personal access wire may remain until 07 where it is still useful, but its semantics reflect Connections source ownership.
72. Contacts `selected_handles` may remain in the 06 command as source-configuration input; it is not a grant-scope field.
73. Changing Contacts selected handles while Observe is active does not require GrantAuthority change when policy is unchanged.
74. `personal_grant_policies` old-profile presence fails closed; it is not migrated or silently ignored.
75. Feasibility remains explicit contextual review.
76. Feasibility retains a bounded contextual review record containing exact query, reviewed native subject and contextual SourceAuthority.
77. Feasibility query-only change advances GrantAuthority as established in 05, not SourceAuthority.
78. Feasibility subject change advances its contextual SourceAuthority.
79. Exact Feasibility no-op preserves both authorities.
80. Feasibility is not exposed as a standing `SourceConnection` candidate merely to unify code.
81. Use with Floe for standing personal sources remains connection/View permission and does not authorize arbitrary future Feasibility destinations.
82. Exact-recipient consent remains unchanged.
83. Observe still does not imply Act.
84. Current architecture/product docs no longer say the bounded personal Vault row owns standing source authority.
85. Parent README marks 06 Complete and 07 remains Not started.

## 3. Final topology after 06

### 3.1 Contacts

~~~
Connections SourceConnection
  connector          contacts.apple
  connection         contacts.apple.local   # current stable local ID may remain
  execution owner    apple:<device>
  resources          [person.identity:<opaque>, ...]
  native subject     reviewed provider subject
  SourceAuthority

Access DataAccessGrant
  source             exact stable SourceConnection identity
  resource           people.identity:<connection>
  category           Derived
  operation          Read
  purpose            Assistant
  consumers          product-approved first-party readers
  processing         LocalOnly
  GrantAuthority

Context
  load current connection
  acquire exact current handles
  record:
    resources         [people.identity:<connection>]
    source_resources  exact contact handles
    SourceAuthority
~~~

### 3.2 Attention

~~~
SourceConnection
  connector          attention.macos
  connection         attention.macos.local
  execution owner    macos:<device>
  resources          [attention.coarse]
  native subject     reviewed attention subject
  SourceAuthority

Grant
  resource           attention.coarse:<connection>
~~~

### 3.3 Wellbeing

~~~
SourceConnection
  connector          health.apple
  connection         health.apple.local
  execution owner    apple:<device>
  resources          [wellbeing.derived]
  native subject     reviewed health subject
  SourceAuthority

Grant
  resource           wellbeing.derived:<connection>
~~~

### 3.4 Feasibility exception

~~~
No standing SourceConnection migration in 06.

DataAccessGrant
  stable contextual source
  schedule.feasibility permission
  GrantAuthority

FeasibilityReviewRecord
  stable contextual source identity
  current grant id
  reviewed subject
  contextual SourceAuthority
  exact reviewed event/destination/query
~~~

The query is not a reusable connection resource set.

## 4. Important contract decision: personal native source changes are atomic

The current `SourceConnection` APIs separate:

~~~
configure(resources)
update_native_subject(subject)
~~~

and `validate_successor` rejects some simultaneous subject/config transitions.

That is correct for existing incremental Calendar operations but insufficient for Contacts review, because the contact set and its subject fingerprint describe one source truth.

06 introduces one direct native source transition at Connections, with semantics equivalent to:

~~~
establish_reviewed_native(
  stable source identity,
  resource_mode,
  resources,
  native_subject_fingerprint,
)

configure_reviewed_native(
  expected_revision,
  resource_mode,
  resources,
  native_subject_fingerprint,
)
~~~

Exact naming may differ. Required semantics:

- one expected local revision;
- one successor validation;
- one repository CAS;
- one SourceAuthority advance if either resource handles or subject changed;
- no double epoch for resources+subject changing together;
- label-only change may advance local revision only;
- exact no-op returns unchanged;
- source remains/enters Ready only with valid native subject;
- stable identity cannot change.

Do not implement this as `configure()` followed by `update_native_subject()`.

## 5. Planning-base code map

All anchors below refer to `5d5cdd75`.

### 5.1 Connections source contract

`crates/modules/connections/src/source.rs`:

- ~13 `ConnectionResource`;
- ~65 `SourceConnection`;
- ~157 `validate_successor`;
- ~257 `configure`;
- ~291 `update_native_subject`;
- ~351 `requires_native_subject`.

Current `requires_native_subject` is Calendar-only.

06 extends native-subject ownership to standing:
- Contacts;
- Attention;
- Wellbeing.

`SourceConnectionService` in `crates/modules/connections/src/application/source_connections.rs` already owns create/configure/update/disconnect and repository CAS.

### 5.2 Current personal standing identity is Access-owned

`crates/modules/access/src/application/personal_sources.rs`:

- `ATTENTION_CONNECTION`;
- `WELLBEING_CONNECTION`;
- `contacts_connection`;
- execution-owner helpers;
- `attention_source`;
- `contacts_source`;
- `wellbeing_source`;
- generic `source_binding`.

After 06 these standing helpers are not permission-owner responsibilities.

Feasibility helpers may remain in a clearly contextual module.

### 5.3 High-level standing review is currently inside Access

`crates/modules/access/src/application/personal_grants.rs`:

- ~82 `PersonalAccessOverview`;
- ~212 `scope_for`;
- ~230 `source_and_scope`;
- ~300 `wellbeing_scope`;
- ~355 `apply_attention`;
- ~573 `apply_wellbeing`;
- ~665 `admitted_handles`;
- ~682 `apply_contacts`.

These functions currently:
- probe the device;
- read Vault source state;
- mutate grant;
- synthesize source identity.

06 moves the cross-owner orchestration to App.

### 5.4 App worker currently calls Access high-level review

`crates/app/src/vault_host.rs`:

- ~2481 `WorkerAction::PersonalAccess`;
- ~2504 `WorkerAction::ContactsAccess`.

Both derive first-party consumers then call Access `apply_*`.

Target: route standing connectors to an App `personal_access` composition owning Connections + native probe + Access grant operations.

Feasibility remains on its contextual Access/Vault path.

### 5.5 Personal Vault row is the final standing source side table

`crates/adapters/vault/src/vault/personal_grants.rs`:

- schema version 6;
- ~611 standing review;
- ~628 selection review;
- ~647 Feasibility review;
- ~667 combined review helper;
- ~792 selected handles getter;
- ~903 personal source mapping;
- ~934 source review upsert;
- ~1040 `personal_grant_policies` creation;
- ~1047 `personal_feasibility_queries` creation;
- ~1170 source authority getter;
- ~1197 subject getter.

After 06 `personal_grant_policies` has no legitimate standing fact and is deleted.

### 5.6 Context personal records still expose source truth by grant

`crates/modules/context/src/ports/personal_source.rs`:

`PersonalGrantRecords` currently exposes:
- grants;
- reviewed subject;
- current SourceAuthority;
- Feasibility query;
- selected handles.

Standing source methods are deleted.

### 5.7 Context personal acquisition still reads source state from Vault

`crates/modules/context/src/application/personal_sources.rs`:

- ~84 `CompletedRead`;
- ~115 `acquire_personal_source`;
- ~166 `personal_dependency`;
- ~288 `read_manager_people`;
- ~324 `read_feasibility`;
- ~397 `read_wellbeing`;
- ~459 `admit_attention`;
- ~563 `personal_dependency_holds`;
- ~700 `authorize_personal_dependency`;
- ~979 `PersonalSourceIdentity`;
- ~1346+ outcome helpers.

This is the major read-path migration.

### 5.8 Personal source candidates are synthesized

`crates/modules/context/src/application/source_candidates.rs`:

- top-level validation hard-codes Attention/Contacts/Wellbeing source identity;
- `SourceCandidateRequest` has a Calendar-only `calendar_connection`;
- ~192 Attention candidate fabricated;
- ~201 Contacts candidate fabricated;
- ~208 Wellbeing candidate fabricated.

Target: candidates derive from current serving `SourceConnection`.

### 5.9 Candidate App caller special-cases Calendar only

`crates/app/src/vault_host/expert_binding_settings.rs`:

`discover_live_candidates` loads current `SourceConnection` only for Calendar, then lets Context fabricate personal sources.

06 generalizes current connection input without adding one field per personal source type.

### 5.10 Expert readers use hard-coded local selection validation

`crates/app/src/vault_host/conversation_turn/expert_host.rs`:

- ~1123 Attention reader;
- ~1187 Wellbeing reader;
- ~1239 People reader.

They validate `SourceSelectionReference` against hard-coded device/source names and pass only Vault records + native driver.

Target: load exact selected SourceConnection and validate logical View identity.

### 5.11 Manager tools also lack Connections input

`crates/modules/context/src/application/tools.rs`:

~~~
ContextToolService<Records, Driver, Remote>
  records
  driver
  remote
~~~

App constructs it at multiple sites in `conversation_turn.rs`, currently with:
- `VaultGrantRecords`;
- native driver;
- optional remote reader.

06 adds the current Connections source reader to the canonical Manager personal path.

### 5.12 Current first-party personal policy still names connector as digest View

`crates/app/src/first_party_observe.rs`:

- `native_consumers_for_target` already derives trusted shipped personal consumers correctly;
- `native_member_policy_digest_for_target` uses connector IDs as the View ID for Attention/Wellbeing.

06 switches digest/grant policy to canonical:
- `PEOPLE_VIEW_ID`;
- `ATTENTION_VIEW_ID`;
- `WELLBEING_VIEW_ID`.

### 5.13 Review capture reads personal SourceAuthority from requirement/Vault path

`crates/app/src/vault_host/review_snapshot.rs`:

- ~278 `capture_personal`.

`crates/app/src/vault_host/interaction_owners.rs`:

- ~359 `read_personal`;
- around ~409 currently reads `personal_grant_source_authority`;
- around ~1070 standing personal enable calls Access high-level review.

All standing source revision/subject truth moves to Connections.

### 5.14 Contacts provider handles fit Connections resource contract

Contract:

~~~
MAX_RESOURCE_HANDLE_BYTES = 256
~~~

Apple generated identity handle:
- HMAC-derived;
- provider validator <=128 bytes.

Dormant Android generated identity handle:
- namespace + 16-byte digest rendered hex;
- comfortably under 256 bytes.

Provider-native raw contact IDs may be <=512 internally, but they are not Floe `ResourceHandle`s.

06 validates standing selected handles through `ResourceHandle::try_new`. Do not widen the shared handle budget and do not hash/truncate an already opaque provider handle.

### 5.15 Personal read Access residuals

`crates/modules/access/src/application/personal_read.rs`:

`PersonalReadRequirement.same_authority` is now redundant because `GrantSourceBinding` contains only stable identity.

`active_resource_grant` and `active_resource_grants` appear production caller-zero on the planning base.

Audit again and delete caller-zero abstractions/tests.

### 5.16 Flutter personal UI is still a dedicated pre-07 surface

`apps/client/lib/app/runtime/local_owner_gateways.dart`:
- Attention inspect/review/enable;
- Wellbeing inspect/review/enable;
- Contacts inspect/review sends `selected_handles`;
- Feasibility contextual review.

`connector_screen.dart` maps current local IDs:
- `contacts.apple.local`;
- `attention.macos.local`;
- `health.apple.local`;
- `feasibility.apple.local`.

06 may retain these stable IDs and dedicated personal commands. 07 owns final generic product-wire/UI convergence.

## 6. 06-A — baseline and residual inventory

Before edits:

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
docs/development/plans/connection-observe-authority/06-personal-observe-convergence.md
docs/architecture/invariants.md
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
docs/product/integrations-and-privacy.md
~~~

Capture:

~~~
rg -n "personal_grant_policies|personal_grant_schema" crates
rg -n "personal_grant_source_authority|personal_grant_subject_fingerprint|personal_grant_selected_handles" crates
rg -n "selected_handles" crates/modules crates/adapters/vault crates/app apps/client
rg -n "ATTENTION_CONNECTION|WELLBEING_CONNECTION|contacts_connection|attention_source|contacts_source|wellbeing_source" crates
rg -n "source_and_scope|apply_personal_access|apply_contacts" crates
rg -n "same_authority|active_resource_grant" crates
rg -n "calendar_connection:" crates
rg -n "PersonalGrantRecords" crates
rg -n "native_member_policy_digest_for_target" crates
~~~

Classify Feasibility matches separately from standing personal matches.

## 7. 06-B — extend Connections native-source semantics

Modify:

~~~
crates/modules/connections/src/source.rs
crates/modules/connections/src/application/source_connections.rs
crates/adapters/vault/src/repositories/connections.rs tests
~~~

### Native-subject connectors

The standing personal connectors now require a native trusted subject.

At minimum current Apple-priority sources:

~~~
attention.macos
contacts.apple
health.apple
~~~

Keep existing dormant `contacts.android` behavior coherent if shared code touches it, but do not add Android platform work or verification.

### Atomic reviewed native mutation

Add a single-domain method/API for reviewed native configuration.

Required input:
- expected revision for existing source;
- ResourceMode;
- canonical `ConnectionResource`s;
- native subject fingerprint.

For creation, allow source establishment with the reviewed subject in the first persisted source state.

The reviewed creation path must persist the source directly as:

- revision `1`;
- one freshly created valid `SourceAuthority`;
- `Ready`;
- canonical resources;
- reviewed native subject.

Do not implement creation as ordinary `establish()` followed by `update_native_subject()`: that would manufacture a synthetic Pending state and an unnecessary second revision/SourceAuthority epoch before the first usable source ever existed.

### Successor validation

Permit resource-handle and subject changes in the same reviewed native successor.

SourceAuthority advancement:

~~~
handles changed || subject changed || mode changed || lifecycle changed
  -> advance exactly once

labels only
  -> preserve SourceAuthority

exact no-op
  -> preserve revision/authority
~~~

Do not weaken stable identity or CAS validation.

### Tests

1. create reviewed Contacts source -> revision 1, Ready, one initial SourceAuthority, subject/resources present, with no synthetic Pending persistence.
2. `[A] + subject S1` -> `[A,B] + subject S2` in one revision/one SourceAuthority advance.
3. same exact state -> no-op.
4. label-only -> revision may advance, SourceAuthority unchanged.
5. subject-only -> one advance.
6. resource-only -> one advance.
7. disconnected source cannot be silently re-bound.
8. Calendar existing transitions remain green.

## 8. 06-C — define App-owned standing personal source specs

Introduce one private App-level mapping or equivalent composition, not Access-owned source constructors.

For each standing connector, define:

~~~
connector
stable local connection ID
execution owner from device
View ID
resource mode
initial/current physical resource rule
native subject required
~~~

Planning-base stable IDs may remain:

~~~
contacts.apple.local
attention.macos.local
health.apple.local
~~~

These become actual persisted `SourceConnection.connection_id` values, not strings reconstructed as authority at read time.

### Canonical resource modes and physical resources

Freeze the mode instead of leaving it caller-selected:

- Contacts (`contacts.apple`, and shared dormant `contacts.android` logic where touched): `ResourceMode::Selected`. The Person explicitly chose a bounded identity set.
- Attention (`attention.macos`): `ResourceMode::AllAvailable`. The singleton coarse attention source is the source's complete current resource set, not a user-selected subset.
- Wellbeing (`health.apple`): `ResourceMode::AllAvailable`. The singleton derived wellbeing source is the source's complete current resource set.

A standing personal source with the wrong mode is not equivalent current source state. App review, candidate discovery and Context read validation must require the mode defined by the source spec rather than accepting either mode.

Physical resources:

Contacts:
- exact selected opaque identity handles;
- <=64;
- each `ResourceHandle` valid;
- canonical sorted unique;
- labels may honestly use the opaque handle until richer provider metadata exists.

Attention:
- exactly `[attention.coarse]`.

Wellbeing:
- exactly `[wellbeing.derived]`.

### Feasibility

Do not include `feasibility.apple` in this standing-source mapping.

## 9. 06-D — move standing personal review orchestration to App

Create/expand an App owner module such as:

~~~
crates/app/src/vault_host/personal_access.rs
~~~

or refactor `personal_grants.rs` if the final naming remains honest.

The composition owns:
- `FloeCore.source_service()`;
- Vault grant operations;
- native personal subject inspector;
- product first-party policy.

Access no longer orchestrates all four.

### Inspect

Inspect is read-only.

For Contacts:
- validate intended selected handles;
- fresh native subject probe for those handles;
- load existing SourceConnection/grant if present;
- do not create/configure source.

For Attention/Wellbeing:
- fresh subject probe;
- load existing current source/grant;
- do not mutate source.

Overview reports current connection/grant if present and the fresh preview fingerprint.

### Review — standing source

Algorithm:

1. validate source spec and command.
2. canonicalize intended physical resources.
3. fresh native subject probe using those exact resources.
4. require before == after.
5. require fresh subject == reviewed fingerprint.
6. load current SourceConnection by stable local connection ID.
7. if absent:
   - establish reviewed native source atomically;
8. if present:
   - validate Person/connector/connection/execution owner;
   - apply atomic reviewed native configuration under current revision;
9. derive stable `GrantSourceBinding` from resulting SourceConnection.
10. derive logical permission resource:
    `connection_view_resource(view_id, connection.connection_id())`.
11. derive exact first-party `GrantScope` from App policy.
12. recheck reviewed grant expectation:
    - absence or exact GrantId/GrantAuthority;
13. activate/review through generic access-grant transaction.
14. exact same permission scope is a GrantAuthority no-op.
15. return overview composed from current SourceConnection + grant.

### Cross-store atomicity

Do not build a fake transaction across Connections and Vault.

Ordering:
- source configuration is committed first from explicit reviewed source facts;
- grant mutation follows under its own CAS.

If grant mutation conflicts:
- return conflict;
- keep the independently valid source configuration;
- do not roll source state back to make permission mutation appear atomic.

This is the defining ownership split.

### SetEnabled(false)

- load exact current SourceConnection;
- locate exact logical standing grant;
- pause grant;
- leave source resources/subject/SourceAuthority unchanged.

### SetEnabled(true)

1. load exact current serving SourceConnection;
2. use its current resources and stored native subject;
3. re-probe those exact resources;
4. require fresh subject == stored subject before/after;
5. derive current first-party scope;
6. activate current paused grant under exact GrantAuthority;
7. do not change source on exact proof.

If subject differs:
- `AccessReviewRequired`;
- do not silently call Connections update.

## 10. 06-E — standing personal grants become logical connection/View grants

Use one App/Access scope construction based on exact first-party policy.

Target:

~~~
resources
  [connection_view_resource(view_id, connection_id)]

categories
  [Derived]

operations
  [Read]

purposes
  [Assistant]

consumers
  assistant where Manager-direct
  + trusted shipped capability consumers

processing
  LocalOnly
~~~

### Remove plain View grant resource

Standing grants must not use bare:
- `people.identity`;
- `attention.coarse`;
- `wellbeing.derived`.

Those may remain physical source resources/domain View IDs, not permission resource handles.

### Old grants

Old bare-resource personal grants are disposable local state.

Do not accept both:
- `attention.coarse`;
- `attention.coarse:<connection>`.

Fresh profile / explicit review is the cutover.

## 11. 06-F — simplify Access personal code

Modify:

~~~
crates/modules/access/src/application/personal_grants.rs
crates/modules/access/src/application/personal_sources.rs
crates/modules/access/src/application/personal_read.rs
crates/modules/access/src/ports/personal_grants.rs
crates/modules/access/src/lib.rs
~~~

### Remove standing source construction

Delete/move from Access when caller-zero:
- `ATTENTION_CONNECTION`;
- `WELLBEING_CONNECTION`;
- `contacts_connection`;
- `attention_source`;
- `contacts_source`;
- `wellbeing_source`;
- standing execution-owner source constructors;
- `source_and_scope`.

Retain Feasibility-specific source helpers in a clearly contextual module if required.

### Remove high-level standing review

Delete Access-owned:
- `apply_attention`;
- `apply_wellbeing`;
- `apply_contacts`;
- any generic `apply` branch for those connectors.

Feasibility review can remain as Access/Vault contextual policy logic.

### Pure grant/read validation

Keep/narrow only true Access semantics:
- `DataAccessGrant` validation;
- active standing grant selection by exact stable source + logical resource + consumer;
- grant unchanged checks;
- subject fingerprint syntax helper if it is genuinely shared.

### Remove stale `same_authority`

`GrantSourceBinding` equality is already stable-source equality.

Delete:
- `PersonalReadRequirement.same_authority`;
- `binds_source` duplication if ordinary equality suffices;
- tests based on the old distinction.

### Delete caller-zero generic helpers

Planning-base production audit shows:
- `active_resource_grant`;
- `active_resource_grants`;

are test-only.

Re-audit then delete if still caller-zero.

## 12. 06-G — replace the personal standing Vault row with a Feasibility-only contextual record

Modify/rename:

~~~
crates/adapters/vault/src/vault/personal_grants.rs
crates/adapters/vault/src/repositories/personal_grants.rs
crates/adapters/vault/src/vault.rs
~~~

### Delete standing table

Delete current:
- `personal_grant_schema`;
- `personal_grant_policies`;
- selected-handle storage;
- standing source authority storage;
- standing reviewed subject storage;
- standing grant mapping by stable source.

No migration.

### Feasibility contextual store

Keep one explicit contextual record, preferably a new schema/table with an honest name.

Suggested shape:

~~~
personal_feasibility_review_schema

personal_feasibility_reviews
  person_id
  connector
  connection_id
  execution_owner
  grant_id
  reviewed_subject_fingerprint
  source_incarnation
  source_epoch
  event_handle
  evidence_handles
  destination_latitude
  destination_longitude
  event_start_unix_ms
  event_end_unix_ms
  travel_mode

UNIQUE(person_id, connector, connection_id, execution_owner)
UNIQUE(grant_id, person_id)
~~~

This is a real contextual review record, not a standing source owner.

### Feasibility authority rules

Same stable source:

- subject unchanged + query unchanged -> SourceAuthority unchanged;
- subject changed -> SourceAuthority advances;
- query changed only -> SourceAuthority unchanged;
- query changed -> GrantAuthority advances as established in 05;
- permission scope changed -> GrantAuthority advances;
- exact no-op -> both unchanged;
- revoked permission may create a new GrantId while the contextual source record may preserve SourceAuthority when subject is unchanged.

### Old profile rejection

Reject current v6:
- `personal_grant_schema`;
- `personal_grant_policies`;
- `personal_feasibility_queries`.

Do not read/migrate them.

Fresh profile creates only the new Feasibility contextual schema for this domain.

## 13. 06-H — narrow Context records and add a Connections source reader

Modify:

~~~
crates/modules/context/src/ports/personal_source.rs
crates/adapters/vault/src/repositories/personal_grants.rs
~~~

### Standing connection port

Add a Context-facing source reader owned by Connections, e.g.:

~~~
trait PersonalConnectionReader {
  load(person_id, connection_id) -> Option<SourceConnection>
  list_current(person_id, connector_id) -> Vec<SourceConnection>  # only if live callers need it
}
~~~

Exact API may be narrower.

App implements it over `FloeCore.source_service()`.

### Vault record port

`PersonalGrantRecords` or its replacement keeps only Vault-owned facts:

- list current DataAccessGrants;
- read Feasibility contextual review/query.

Delete standing:
- `reviewed_subject`;
- `current_source_authority`;
- `selected_handles`.

Do not hide Connections reads inside a Vault adapter.

### Feasibility typed record

Return one typed record containing:
- query;
- reviewed subject;
- contextual SourceAuthority.

Do not expose three unrelated Vault getters that callers can race independently.

## 14. 06-I — standing Context acquisition uses Connections

Refactor:

~~~
crates/modules/context/src/application/personal_sources.rs
~~~

Split standing source acquisition from Feasibility where that clarifies ownership.

### Current source validation

For standing read:
- load exact selected/expected SourceConnection;
- require same Person;
- expected connector;
- exact connection ID;
- exact execution owner;
- exact canonical ResourceMode from the standing personal source spec;
- serving/Ready;
- valid native subject;
- expected physical source-resource shape.

### Permission resource

Derive:
`connection_view_resource(view_id, connection.connection_id())`.

Use exact current grant with:
- same stable source;
- exact logical View resource;
- active state;
- expected consumer;
- Derived / Read / Assistant / LocalOnly.

### Contacts read

`selected_handles` comes only from:

~~~
connection.resources[].handle
~~~

Require:
- non-empty;
- <=64;
- canonical unique;
- every handle valid.

Pass exact handles to native driver.

### Attention/Wellbeing read

Require the exact singleton current source resource.

### Native subject

Expected subject comes only from:

~~~
connection.native_subject_fingerprint()
~~~

Pass it to driver.

The acquisition must return before/after same subject.

### Post-I/O continuity

Reload SourceConnection.

Reject if:
- stable identity differs;
- source disconnected;
- SourceAuthority changed;
- resource handles changed;
- native subject changed.

Do not reject label-only revision drift when authority/resources/subject remain equivalent.

Reload grant and require:
- same GrantId;
- same GrantAuthority;
- same stable source;
- same logical permission facts.

### Dependency

Record:

~~~
resources
  [logical connection/View]

source_authority
  current SourceConnection authority

source_resources
  exact current ConnectionResource handles
~~~

No Vault side-table source data.

## 15. 06-J — standing personal dependency reauthorization

Refactor `authorize_personal_dependency`.

### Contacts / Attention / Wellbeing

1. parse/validate the logical connection/View resource.
2. load exact current SourceConnection by dependency connection.
3. verify stable source identity.
4. verify source is serving.
5. compare dependency SourceAuthority.
6. compare exact current resource handles with `source_resources`.
7. compare current trusted native subject to the trusted observation subject where required.
8. reload exact current DataAccessGrant.
9. validate GrantId/GrantAuthority/scope/consumer/etc.
10. validate observation/query/freshness lineage.

No personal Vault source row.

### Feasibility

Keep a separate branch:
- contextual stable source identity;
- Feasibility review record SourceAuthority;
- reviewed subject;
- exact query record;
- current DataAccessGrant;
- trusted observation.

Do not force a synthetic `SourceConnection` into this branch.

### Test-only liveness

`personal_dependency_holds` / `PersonalDependencyLiveness` is test-only on the planning base.

If the canonical async resolver supersedes it and production caller audit is zero:
- delete it;
- port its meaningful lineage assertions to resolver tests.

Do not maintain a second stale source-currentness algorithm solely for tests.

## 16. 06-K — Manager tools use the same standing Connections source truth

Modify:

~~~
crates/modules/context/src/application/tools.rs
crates/app/src/vault_host/conversation_turn.rs
~~~

Current:

~~~
ContextToolService<Records, Driver, Remote>
~~~

Target includes current personal Connections source reader.

Example:

~~~
ContextToolService<Connections, Records, Driver, Remote>
~~~

or an equally direct composition.

Update all construction sites in `conversation_turn.rs`.

Manager personal tools:
- Contacts;
- Attention;
- Wellbeing;

must call the same standing Context acquisition used by Expert readers.

Feasibility continues through contextual record path.

No hidden source lookup through Vault records.

## 17. 06-L — source candidates and Expert binding converge on current SourceConnection

Modify:

~~~
crates/modules/context/src/application/source_candidates.rs
crates/app/src/vault_host/expert_binding_settings.rs
relevant registry/binding tests
~~~

### Generalize request

Replace Calendar-only:

~~~
calendar_connection: Option<&SourceConnection>
~~~

with a general current source collection, e.g.:

~~~
source_connections: &[SourceConnection]
~~~

Use it for:
- Calendar;
- Contacts;
- Attention;
- Wellbeing.

Remote Mail/Work/Logistics may keep their current server `ConnectorSnapshot` input.

Intrinsic `floe.tasks` / `memory.confirmed` remain device-local non-Connection targets.

### Standing personal candidate

For one serving matching source:

~~~
connector_id
  = connection.connector_id

connection_id
  = connection.connection_id

execution_owner
  = connection.execution_owner_id

capability_id
  = requested capability

resource
  = connection_view_resource(capability, connection.connection_id)

contract_version
  = requirement version
~~~

No leaf handles.

### Candidate source lookup

App loads current source connections by the connector(s) that can satisfy the capability.

If multiple current sources are valid:
- expose distinct candidates;
- do not auto-select/switch.

### Validation

Replace hard-coded `validate_local_source_selection(selected, device_id)` for standing personal sources with validation against the actual current `SourceConnection`.

Split intrinsic task/memory validation if necessary.

### Stability tests

Contacts `[A] -> [A,B]`:
- same reference;
- same candidate ID;
- same saved binding revision.

Subject-only change:
- same candidate ID/binding.

## 18. 06-M — Expert standing personal readers use selected current connection

Modify:

~~~
crates/app/src/vault_host/conversation_turn/expert_host.rs
~~~

Add core/current connection access to:
- `PersonalAttentionReader`;
- `PersonalPeopleReader`;
- `PersonalWellbeingReader`.

Flow:
1. load `selected.connection_id` from Connections;
2. require serving exact SourceConnection;
3. validate selected connector/owner/capability/logical resource against it;
4. invoke Context standing personal read with this exact connection;
5. never reconstruct a source from `device_id`.

People reader must not query selected handles from Vault.

## 19. 06-N — first-party personal policy uses canonical View identity

Modify:

~~~
crates/app/src/first_party_observe.rs
~~~

Create/reuse one exact standing personal policy helper.

Canonical View IDs:
- `people.identity`;
- `attention.coarse`;
- `wellbeing.derived`.

Consumers:
- Manager direct `assistant` where current product advertises direct access;
- trusted shipped Experts declaring the exact capability.

Permission:
- Derived;
- Read;
- Assistant;
- LocalOnly.

Delete connector-ID-as-View digest semantics.

Contacts should have a policy digest even if Contacts resource selection keeps review/navigation UX separate.

An extension installation/binding does not alter policy digest.

## 20. 06-O — App review snapshot/interaction owner reload Connections

Modify:

~~~
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_owners.rs
crates/app/src/vault_host/conversation_turn/interaction_publication.rs tests
~~~

### Capture standing personal

Load current SourceConnection.

Review target:
- member_id = canonical View ID;
- resource = logical connection/View resource;
- source_revision = current Connections SourceAuthority at bundle level;
- expected grant = exact current logical standing grant/absence;
- policy_digest = canonical personal policy digest;
- native subject = fresh probe.

Fresh probe must equal current trusted connection subject for an inline-enable review.

### Contacts review UX

Do not invent an inline contact resource picker.

If a blocker requires selecting/changing Contacts handles:
- keep navigation/resource-picker flow.

A paused existing Contacts grant may only be inline-enabled if the current source resources are already fully configured and the existing product interaction can prove the exact current source without asking for a new selection; otherwise navigate.

### Live owner read

Use Connections SourceAuthority, not:
`personal_grant_source_authority(grant.id)`.

### Enable

Route standing personal enable through the new App personal-access orchestrator.

Do not call the deleted Access high-level `apply_personal_access`.

## 21. 06-P — delete standing personal Vault/API surfaces

Required caller-zero targets after migration:

~~~
personal_grant_source_authority
personal_grant_subject_fingerprint
personal_grant_selected_handles
personal_grant_mapping_in_transaction
upsert_personal_source_review_in_transaction
review_personal_grant_with_selection
~~~

Delete `personal_grant_policies`.

Rename remaining Feasibility-specific functions/files/ports so “personal policy” does not describe current standing source ownership.

No forwarding aliases.

## 22. 06-Q — product/Flutter bounded cutover

06 changes semantics, not the final product shape.

Modify only what is necessary in:

~~~
crates/bindings/protocol/src/dto/agent.rs
crates/bindings/ffi/src/conversion/owners.rs
apps/client/lib/features/settings/domain/agent_personal_access.dart
apps/client/lib/app/runtime/local_owner_gateways.dart
apps/client/lib/features/connections/presentation/personal_access_cards.dart
related tests
~~~

### Allowed retained pre-07 surface

May remain:
- dedicated personal access inspect/review/set-enabled commands;
- Contacts `selected_handles` in source configuration command;
- PersonalAccessOverview projection;
- existing stable local connection IDs.

### Semantic changes

Contacts `selected_handles` now mean:
> configure this Connection's current source resources.

They do not mean:
> put these handles in the grant.

### Contacts validation

Dart/backend review selection:
- non-empty;
- <=64;
- unique/canonical;
- each within `ResourceHandle` 256-byte identifier rules.

Do not hash/truncate.

### Overview

Standing source authority returned by backend is Connections authority.

No new compatibility field.

07 owns:
- generic ConnectionObserve DTO/gateway;
- selected/granted resource projection cleanup;
- final UI control convergence.

## 23. 06-R — Feasibility explicit contextual exception

Do not let broad personal cleanup swallow Feasibility semantics.

### Keep

- explicit user-reviewed event;
- evidence handles;
- destination;
- time window;
- travel mode;
- reviewed native subject;
- contextual SourceAuthority;
- query change -> GrantAuthority advance;
- exact query re-review -> no-op.

### Do not

- create `SourceConnection` for Feasibility merely for symmetry;
- turn destination into Connection resources;
- expose Feasibility as a standing Expert candidate;
- make Use with Floe authorize arbitrary future destinations;
- reuse Contacts/Wellbeing source configuration commands.

### Documentation

Current architecture must name Feasibility as the explicit contextual exception.

## 24. Permanent acceptance tests

### 24.1 Connections personal source

1. reviewed Attention creation -> Ready SourceConnection.
2. reviewed Wellbeing creation -> Ready.
3. Contacts `[A]`, subject S1 -> Ready.
4. Contacts `[A] -> [A,B]`, S1->S2 -> one revision and one SourceAuthority advance.
5. exact Contacts no-op -> no authority/revision mutation.
6. label-only change -> SourceAuthority unchanged.
7. subject-only review -> one SourceAuthority advance.
8. disconnect -> later read fails.

### 24.2 Contacts standing resource invariant

1. configure source `[A]`;
2. standing grant G on `people.identity:<connection>`;
3. Expert binding candidate X;
4. edit source `[A,B]`;
5. source authority changes;
6. GrantId/GrantAuthority unchanged;
7. candidate ID/binding revision unchanged;
8. next read requests A+B;
9. dependency source_resources A+B;
10. old dependency stale.

### 24.3 Attention/Wellbeing

1. source subject S1 + logical grant G;
2. subject drift during read -> fail closed; no grant mutation;
3. explicit review S2 -> SourceAuthority changes;
4. G/GrantAuthority unchanged if permission policy unchanged;
5. next dependency records S2 authority.

### 24.4 Grant policy

For all standing personal views:
- logical connection/View resource only;
- no physical handle in grant;
- trusted shipped consumers only;
- arbitrary extension receives no authority;
- pause/re-enable does not alter source state.

### 24.5 Candidate/binding

- no candidate before serving SourceConnection;
- one candidate per usable connection/View;
- resource/subject edits preserve candidate ID;
- selected missing connection is unavailable, not rerouted;
- multiple valid sources are distinct candidates.

### 24.6 Context provenance

- permission resource logical;
- exact source_resources physical;
- Connections SourceAuthority;
- current native subject;
- grant/source drift independently fail;
- label-only revision drift does not stale evidence when source authority/resources/subject unchanged.

### 24.7 Vault deletion

Fresh profile:
- no `personal_grant_policies`;
- no `personal_grant_schema`.

Old v6 profile:
- fails closed; no migration.

New Feasibility contextual table:
- query/subject/source authority round-trip;
- corruption fail closed;
- query-only vs subject-only authority behavior correct.

### 24.8 Feasibility exception

- different destination/query still requires explicit contextual review;
- query change advances GrantAuthority;
- subject change advances contextual SourceAuthority;
- exact no-op preserves both;
- no standing SourceConnection/candidate created.

## 25. Residual/deletion audit

Run:

~~~
rg -n "personal_grant_policies|personal_grant_schema" crates
rg -n "personal_grant_source_authority|personal_grant_subject_fingerprint|personal_grant_selected_handles" crates
rg -n "review_personal_grant_with_selection|upsert_personal_source_review" crates
rg -n "selected_handles" crates/modules crates/adapters/vault crates/app
rg -n "ATTENTION_CONNECTION|WELLBEING_CONNECTION|contacts_connection|attention_source|contacts_source|wellbeing_source" crates
rg -n "source_and_scope|apply_personal_access|apply_contacts" crates
rg -n "same_authority|active_resource_grant" crates
rg -n "calendar_connection:" crates
rg -n "validate_local_source_selection" crates
rg -n "PersonalGrantRecords" crates
rg -n "source_incarnation|source_epoch" crates/adapters/vault/src/vault
~~~

### Required zero production matches

- standing `personal_grant_policies`;
- standing personal source authority/subject/handle getters by GrantId;
- standing selected-handle persistence in Vault;
- standing personal source constructors in Access;
- `PersonalReadRequirement.same_authority`;
- Calendar-only `SourceCandidateRequest.calendar_connection`;
- hard-coded synthetic standing personal candidate creation;
- Vault source authority used by App review/interaction for standing personal sources.

### Allowed classified matches

- Contacts command/acquisition `selected_handles` as source configuration/provider input;
- Feasibility contextual review record `source_incarnation/source_epoch`;
- provider-native selected-handle mechanics;
- source constants used only as physical View/resource IDs;
- 07-owned dedicated product wire/UI;
- dormant Android platform code not touched beyond shared contract compatibility.

Every surviving `selected_handles` match must be classified as:
- source configuration input;
- provider acquisition input;
- query/provenance;
not grant-owned standing state.

## 26. Architecture documentation convergence

Update:

~~~
docs/architecture/modules.md
docs/architecture/runtime.md
docs/architecture/authority-recovery.md
docs/product/integrations-and-privacy.md
~~~

After 06 they must say:

- Connections owns Calendar + standing Contacts/Attention/Wellbeing source truth.
- Contacts current handles are Connection resources.
- trusted personal native subject is SourceConnection state.
- standing personal SourceAuthority is Connections-owned.
- standing personal grants name logical connection/View resources.
- physical handles/source classes live only in connection resources/dependency provenance/provider mechanics.
- personal resource/subject changes stale evidence but do not inherently mutate grant or Expert binding.
- Expert personal source selection is connection/View configuration.
- Feasibility remains a contextual review exception, not standing Connection Observe.
- exact-recipient consent/Actions remain separate.
- no personal standing Vault side table owns source state.

Do not claim 07 UI/wire cleanup is already complete.

## 27. Suggested implementation slices

### 06-A — Connections native personal source contract

- native-subject connector support;
- atomic reviewed native configuration;
- source transition tests.

Suggested commit:

~~~
connections: own standing personal source truth
~~~

### 06-B — App standing personal review orchestration

- source specs;
- Contacts/Attention/Wellbeing review;
- logical grant activation/pause;
- overview composition.

Suggested commit:

~~~
app: compose standing personal observe from connections and access
~~~

### 06-C — remove personal standing Vault side state

- delete `personal_grant_policies`;
- add Feasibility contextual store;
- narrow Access/Vault ports;
- Feasibility tests.

Suggested commit:

~~~
vault: retain only contextual feasibility review state
~~~

### 06-D — Context read/reauthorization cutover

- personal connection reader;
- standing acquisition/replay;
- Manager tools;
- dependency resolver.

Suggested commit:

~~~
context: read standing personal sources from connections
~~~

### 06-E — Expert candidate/binding convergence

- general SourceConnection candidate input;
- logical personal selections;
- Expert readers.

Suggested commit:

~~~
experts: bind personal sources by connection view
~~~

### 06-F — review/product caller convergence

- policy digest canonical View IDs;
- interaction source truth;
- Flutter bounded source-config semantics;
- tests.

Suggested commit:

~~~
app: converge personal review on connection source state
~~~

### 06-G — docs/closure

- residual audit;
- architecture/product docs;
- execution evidence;
- README 06 Complete.

Suggested commit:

~~~
docs: complete connection observe checkpoint 06
~~~

Combine slices when a direct cutover makes the final system smaller. Do not add compatibility wrappers to make intermediate commits compile.

## 28. Verification

During iteration run targeted crates.

Minimum Rust close gate:

~~~
cargo test -p floe-connections
cargo test -p floe-access personal
cargo test -p floe-context personal
cargo test -p floe-vault personal
cargo test -p floe-provider-adapters personal
cargo test -p floe-app personal
cargo test -p floe-app interaction_resolution
cargo test -p floe-app registered_runner
cargo test -p floe-conversation
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
~~~

If a filter matches zero tests, run the actual full crate/nearest test target and record the command.

### Apple/native

Because Contacts/Attention/Wellbeing source semantics cross native bridges, run current available deterministic native checks:

- Apple Contacts package tests;
- relevant macOS Attention fixture/validation;
- Health/Wellbeing fixture/bridge validation where available;
- Flutter macOS build.

Do not modify signing/account/device state.

Android is not a 06 verification requirement.

### FFI / Flutter

Because personal access product callers still carry source configuration:

~~~
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/connections
flutter test test/features/conversation
flutter test
flutter build macos
~~~

Distinguish the known unrelated Expert-registry golden mismatch if it remains.

### Broad Rust

Final:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
~~~

Default-parallel `floe-app` may separately expose the known global runner race; report it separately, do not weaken tests.

### Live source smoke

If already-authorized disposable Apple Contacts/Attention/Health sources exist:
- source resource/subject change;
- SourceAuthority continuity;
- grant continuity;
- next-read provenance.

Otherwise report SKIPPED with reason.

Do not request permissions, pair accounts, or change external credentials solely for the smoke.

## 29. Close procedure

Before marking 06 Complete:

1. rerun all residual searches;
2. prove no standing personal source fact remains in Vault side state;
3. prove Contacts handles come from Connections;
4. prove standing native subject comes from Connections;
5. prove standing SourceAuthority comes from Connections;
6. prove personal standing grants use logical connection/View resources;
7. prove resource/subject edit does not change grant or binding;
8. prove old dependency stales;
9. prove candidate IDs stay stable across source edits;
10. prove no synthetic fallback candidate exists without a source connection;
11. prove old personal v6 table profile fails closed;
12. prove Feasibility remains contextual and separate;
13. run targeted + broad + Flutter/FFI/native deterministic gates;
14. update current architecture/product docs;
15. append execution evidence to this file;
16. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Complete
    - 05 Complete
    - 06 Complete
    - 07 Not started
17. commit closure;
18. stop. Do not begin checkpoint 07.

Execution evidence must record:

- date;
- start local HEAD / fetched origin/main;
- implementation/closure SHAs;
- final standing personal SourceConnection identities;
- final Connections resource/subject mutation API;
- one-advance combined Contacts transition proof;
- final Contacts resource representation/bounds;
- final standing personal logical GrantScope;
- final first-party consumer policy/digest;
- deleted Access standing source constructors/orchestrators;
- deleted personal Vault standing table/schema;
- final Feasibility contextual record schema;
- final Context standing personal read path;
- final dependency example for Contacts/Attention/Wellbeing;
- final candidate/binding shape;
- App review/interaction source owner;
- bounded 07 product-wire residual;
- Feasibility exception proof;
- residual audit;
- architecture/product docs;
- exact verification commands/results;
- live native smoke or SKIPPED reason;
- clean worktree.

## 30. Required agent report

Report:

1. start HEAD / origin-main / final HEAD;
2. final Contacts/Attention/Wellbeing SourceConnection identities;
3. final atomic native source mutation semantics;
4. Contacts resource-handle representation and bounds;
5. source resource/subject change -> SourceAuthority behavior;
6. standing personal logical GrantScope shape;
7. proof resource/subject edits preserve standing grant authority where permission is unchanged;
8. standing personal Access API final surface/deletions;
9. deleted personal Vault standing schema/table/APIs;
10. final Feasibility contextual record and authority behavior;
11. final Context connection/grant/source reader composition;
12. Contacts read/dependency example;
13. Attention read/dependency example;
14. Wellbeing read/dependency example;
15. dependency reauthorization/source-drift behavior;
16. Manager direct personal tool cutover;
17. Expert candidate/reference final shape;
18. binding stability across source edits;
19. first-party consumers/policy digest final shape;
20. App review/interaction source-owner cutover;
21. Flutter/product semantic changes and bounded 07 residuals;
22. tests moved/rewritten/deleted;
23. residual audit, including legitimate selected_handles matches;
24. architecture/product docs updated;
25. targeted Rust verification;
26. native deterministic verification;
27. FFI/Flutter verification;
28. broad Rust verification;
29. live source smoke or SKIPPED reason;
30. checkpoint commit SHA(s);
31. clean worktree confirmation;
32. confirmation that checkpoint 07 was not started.
