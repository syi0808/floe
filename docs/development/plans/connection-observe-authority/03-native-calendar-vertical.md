# 03: Native Calendar logical-View vertical

Prerequisite: 02 complete.

Status: Not started.

Planning base: `main` at `2e6a0a79d9b391eaba2a813b8f4ed43ef6ab06aa` on 2026-09-28.

Planning reconciliation: `5ebedec41362f1b3d8ea784f65cd9f530fe83b40` was a documentation-only expansion of this checkpoint plan on top of that code baseline. The production source tree was unchanged; the caller map and line anchors below were rechecked against the same `2e6a0a79` implementation on 2026-09-28.

Checkpoint 01 moved native Calendar source/resource authority to Connections. Checkpoint 02 made standing grant source identity stable and split `ContextDependency` permission resources from current `SourceAuthority` and exact observed `source_resources`.

Checkpoint 03 is therefore no longer a foundational authority migration. It is the first complete product vertical that consumes those foundations: native EventKit Calendar must stop treating leaf Calendar IDs as Expert configuration or Access permission and instead use one `calendar.timeline:<connection-id>` View permission while Context acquires every current Calendar resource from Connections at read time.

The defect reproduced in checkpoint 00 is still present on the planning base:

~~~
Connection resources
  -> one Expert source candidate per Calendar leaf
  -> first-party consumers intersect exact leaf bindings
  -> Calendar grant scope contains Calendar leaf IDs
  -> Expert read passes selected leaf IDs
~~~

Checkpoint 03 deletes that native path rather than patching the intersection.

Line numbers below are planning-base anchors on `2e6a0a79`. Re-resolve symbols on the actual execution HEAD before editing.

## 1. Exit state

Checkpoint 03 is complete only when all of the following are true.

1. One serving native EventKit `SourceConnection` produces exactly one `calendar.timeline` `SourceSelectionReference`.
2. The native candidate resource is exactly `connection_view_resource("calendar.timeline", connection_id)`.
3. Native Calendar candidate ID is stable when only the Connection resource set changes.
4. Existing native Calendar Expert bindings select the connection/View, never Calendar leaf IDs.
5. Adding/removing/renaming Calendar resources does not mutate an Expert binding solely because the resource set changed.
6. First-party Observe consumers are derived from the compile-time trusted shipped manifest set and declared source capability, not Registry installation/assignment/binding state.
7. Calendar first-party consumers are exactly the shipped built-ins that declare `calendar.timeline`.
8. Installing, enabling, disabling, binding or rebinding a third-party/extension Expert cannot change the default Calendar first-party consumer set or Calendar policy fingerprint.
9. Manager `assistant` is not added to Calendar merely as a fallback. It is present only for Views/Connectors backed by an actual Manager direct-read tool.
10. `selected_shipped_consumers` and `native_calendar_policy_for_target` are deleted.
11. Native Calendar standing grant scope contains one logical resource:
    `calendar.timeline:<connection-id>`.
12. Native Calendar grant scope contains no Calendar leaf IDs.
13. Native Calendar grant identity and `GrantAuthority` do not change merely because Connections changes its Calendar resources.
14. Native Calendar consumer-policy authority does not change merely because Connections changes its Calendar resources. It may still exist until 05.
15. Enabling Use with Floe reviews the current source/native subject, but leaf Calendar IDs are review evidence only and are not copied into grant scope.
16. A native read always resolves the full current sorted resource set from Connections; an Expert-selected subset cannot narrow it.
17. `NativeCalendarViewRead` carries no `selected_calendar_ids`.
18. `admit_current_native_calendar_read` carries no `expected_calendar_ids` argument.
19. `NativeCalendarGrantReader` does not require Calendar leaf IDs or native subject fingerprint to select/admit the standing grant.
20. Native grant admission validates one logical View resource and one requesting consumer; provider resource membership is validated by Connections/source acquisition, not `GrantScope`.
21. A successful native dependency records:
    - `resources = [calendar.timeline:<connection-id>]`;
    - current Connections `source_authority`;
    - exact full Calendar IDs actually acquired in `source_resources`.
22. `admission_matches_dependency` does not require `source_resources` to be members of logical grant resources.
23. Old dependencies stale when current Connections `SourceAuthority` changes.
24. The next read after resources `[A] -> [A,B]` requests both A and B without re-reviewing the grant or rebinding the Expert.
25. The checkpoint-00 eleven-Calendar failure becomes a permanent positive regression: enable/review succeeds for 11 current Calendars without manual Expert binding expansion.
26. The prior `>4`/`>128` fixed-count cleanup remains green; 03 does not reintroduce a permission count cap.
27. Native inline Observe review contains one logical Calendar member, not one reviewed member per Calendar leaf.
28. Native interaction revalidation probes the full current Connections resource set and compares one logical View member.
29. Resource edits while Use with Floe is active do not trigger an automatic fresh grant review in Flutter/App product flow.
30. `selected_resources` / `granted_resources` may remain on the temporary Calendar DTO until 07, but they are presentation-only and never standing permission authority.
31. Existing product copy must not render a logical one-resource grant as “Using 1 of 11 calendars”.
32. The Access-owned `calendar_lease.rs` dependency builder is deleted if its only remaining role is constructing Context provenance.
33. Native Calendar query fingerprint/provenance construction lives with Context acquisition/lease ownership.
34. Actions still require exact destination Calendar provenance through `ContextDependency.source_resources`.
35. Agent action execution validates the Expert’s native Calendar selection against dependency permission `resources` after that selection becomes a logical View resource.
36. Hosted Google/Microsoft Calendar special source selection/grant/read semantics remain explicitly deferred to 04.
37. `ConsumerPolicyAuthority` remains explicitly deferred to 05.
38. Final generic `ConnectionObserve` DTO/gateway removal remains explicitly deferred to 07.
39. Current architecture docs describe native Calendar as connection/View-level permission and Expert configuration.
40. Required residual searches and verification pass.
41. Parent README marks 03 Complete and 04 remains Not started.

## 2. Checkpoint boundary

### 2.1 Native EventKit is the complete vertical in 03

03 completes the native Apple Calendar path:

~~~
SourceConnection
  current Calendar resources
  SourceAuthority
       |
       +--> Expert binding
       |      calendar.timeline:<connection>
       |
       +--> Access grant
       |      calendar.timeline:<connection>
       |      shipped built-in consumers
       |
       +--> Context read
              load current resources
              read all current Calendar IDs
              dependency.resources = logical View
              dependency.source_resources = exact IDs read
~~~

`calendar.event_kit` is the product target for this checkpoint.

Common native abstractions may continue to support `calendar.android` if the change naturally stays platform-neutral, but do not add Android-specific work or validation. Apple remains the target per `AGENTS.md`.

### 2.2 Hosted Calendar remains bounded until 04

The planning-base `SourceCandidateRequest.calendar_connection` branch also handles:

- `calendar.google`;
- `calendar.microsoft`;
- `calendar.fixture`.

The hosted Calendar selected-read path still iterates leaf resources and calls the special remote Calendar protocol once per selected Calendar. Converting those candidates to logical View resources in 03 would force the remote Calendar stack cutover that belongs to 04.

Therefore:

- EventKit/native candidate becomes connection/View-level in 03.
- Google/Microsoft hosted Calendar candidate/read may remain leaf-scoped until 04.
- Do not create two permanent Calendar models. This is a bounded one-checkpoint transition with removal owner 04.
- Do not add compatibility abstractions around the hosted branch; keep the existing branch explicit and small.

`calendar.fixture` should follow the boundary required by its actual caller on the execution HEAD. If it exercises local native acquisition semantics, use the new logical native model. If it only supports non-native test/mirror paths, keep it as test-only and document the classification. Do not use fixture behavior to justify retaining leaf-native production semantics.

### 2.3 Consumer-policy epoch remains until 05

03 fixes what the policy means:

- trusted shipped manifest capability declarations;
- no Registry assignment selection;
- logical View scope.

It does not delete `ConsumerPolicyAuthority`, `CalendarGrantPolicy`, or `policy_authority` fields. 05 owns that deletion.

Keep the current `policy_fingerprint` symbol name if renaming it in 03 would only create churn. 05 owns the final `policy_digest` convergence. What must change in 03 is its semantic input: no Expert assignment/binding state and no leaf Calendar resource state.

### 2.4 Product wire remains until 07, but product behavior must already be correct

The native Calendar access DTO may still carry:

- `calendar_ids`;
- `expected_source_authority`;
- `selected_resources`;
- `granted_resources`.

Until 07, `calendar_ids` on a review is only an exact reviewed-source expectation. It is not a grant scope list.

03 must remove product behavior that automatically re-reviews a standing grant after a resource edit. Otherwise the core invariant would still be violated even though Access uses a logical View.

07 later deletes the Calendar-specific DTO/gateway fields and unifies the product wire.

## 3. Canonical native contracts after 03

### 3.1 Native Expert source selection

One `SourceSelectionReference`:

~~~
connector_id        = calendar.event_kit
connection_id       = current SourceConnection.connection_id
execution_owner_id  = current SourceConnection.execution_owner
capability_id       = calendar.timeline
resource            = calendar.timeline:<connection-id>
contract_version    = 1
~~~

This reference identifies the usable connection/View. It never identifies Calendar A/B/C.

### 3.2 Native standing grant

One `GrantScope`:

~~~
resources   = [calendar.timeline:<connection-id>]
categories  = [metadata, content]
operations  = [read]
purposes    = [assistant]
consumers   = trusted first-party Calendar consumers
processing  = local_only
~~~

The source is the stable `GrantSourceBinding` created in 02.

No source epoch or Calendar ID is present in the grant.

### 3.3 Native read provenance

At acquisition:

~~~
connection = reload current SourceConnection
calendar_ids = all current sorted connection.resources handles
source_authority = connection.source_authority
~~~

After provider/native subject checks and grant admission:

~~~
ContextDependency.resources
  = [calendar.timeline:<connection-id>]

ContextDependency.source_authority
  = current SourceAuthority

ContextDependency.source_resources
  = exact calendar_ids passed to provider/read
~~~

The provider source stamp must quote exactly the same Calendar IDs.

### 3.4 First-party consumer policy

Canonical helper:

~~~
trusted_shipped_consumers(capability)
~~~

or an equivalently named function owned by App product policy.

It derives consumers from:

~~~
floe_experts_builtin::manifests()
  -> manifest validates
  -> manifest.source_requirements contains capability
  -> GrantConsumer::builtin(manifest.package.id)
  -> sort + dedupe
~~~

It does not read:

- `RegistrySnapshot`;
- installation enabled state;
- assignment enabled state;
- Expert binding selections;
- source candidate IDs;
- Connection resource leaves.

For `calendar.timeline`, planning-base shipped declarations imply exactly:

~~~
floe.builtin.schedule
floe.builtin.commitments
floe.builtin.focus-attention
floe.builtin.wellbeing
~~~

Verify this from the actual execution HEAD instead of hard-coding an unexplained list. Tests should assert the product declaration remains the source of truth.

Manager `assistant` is added only by an explicit Manager direct-read policy. `tools.rs` currently has no direct Calendar timeline Manager tool, so Calendar must not receive `assistant` as a fallback.

## 4. Planning-base code map

### 4.1 Source candidate leak

`crates/modules/context/src/application/source_candidates.rs` on `2e6a0a79`:

| Lines | Symbol | Current behavior |
|---|---|---|
| ~90 | `discover_source_candidates` | Generic candidate owner. |
| ~154 | `calendar.timeline` branch | Loads one `SourceConnection`. |
| ~173 | `for calendar in connection.resources()` | Emits one candidate per Calendar leaf. |
| ~321 | `calendar_addition_produces_a_new_candidate_without_changing_the_old_reference` | Explicitly preserves the old leaf-candidate model. |

Generic remote mail/work/logistics already use `connection_view_resource`; native Calendar should converge to the same connection/View representation.

### 4.2 Expert settings/default binding

`crates/app/src/vault_host/expert_binding_settings.rs`:

- ~138-188 loads at most one current Calendar product connection.
- ~190-213 derives remote execution owner and calls `discover_source_candidates`.
- ~290+ `bind_initial_defaults` selects every live `calendar.timeline` candidate.
- because native candidates are currently per-leaf, setup writes every Calendar leaf into every shipped Calendar-capable Expert binding.

After 03 a native connection yields one candidate, so the same default-binding logic should naturally select one stable connection/View reference. Do not add Calendar-resource-specific binding logic.

### 4.3 First-party consumer intersection

`crates/app/src/first_party_observe.rs`:

| Lines | Symbol | Current problem |
|---|---|---|
| ~22 | `selected_shipped_consumers` | Restores Registry and requires exact active assignment/binding target. |
| ~174 | `calendar_policy` | Base Calendar policy starts with no consumers. |
| ~182 | `native_calendar_policy_for_target` | Loops Calendar resources and intersects exact selected shipped consumers. |
| ~255 | `remote_policies_for_target` | Also augments remote policies from Registry target selections. |
| ~529 | `shipped_consumer_requires_active_exact_binding` | Test freezes the wrong authority coupling. |

`native_consumers_for_target` also calls `selected_shipped_consumers` for personal sources. If 03 deletes the old helper globally, migrate those callers to the same trusted shipped manifest capability policy while preserving 06's separate source-ownership work.

### 4.4 Calendar review currently copies leaf IDs into the grant

`crates/app/src/vault_host/calendar_access.rs`:

| Lines | Current behavior |
|---|---|
| ~462 | `apply_calendar_access` owner operation. |
| ~489 | Review branch accepts `calendar_ids`. |
| ~518 | Reads current Connection resources. |
| ~523 | Only requires reviewed IDs to be a subset of current resources. |
| ~533 | Native subject preview probes the reviewed leaf list. |
| ~552 | Calls `native_calendar_policy_for_target`. |
| ~580 | Passes Calendar IDs and consumers into Vault grant review. |
| ~674 | Overview derives `granted_resources` from `grant.scope.resources`. |

For connection-wide Use with Floe, a fresh enable review should compare the reviewed Calendar list with the full current Connection resource set, not accept a narrowed subset.

After the review, later source resource changes do not mutate/review the grant.

### 4.5 Vault Calendar grant is leaf-scoped

`crates/adapters/vault/src/vault/calendar_grants.rs`:

| Lines | Symbol | Current behavior |
|---|---|---|
| ~25 | `authorize_current_native_calendar_grant` | Builds requested Calendar-ID scope and subsets it against stored grant leaf scope. |
| ~111 | `review_native_calendar_grant` | Takes `calendar_ids`, source authority, subject fingerprint and consumers. |
| ~345 | source lookup | Stable source after 02. |
| ~416 | `calendar_binding` | Converts all Calendar IDs to grant resources. |
| ~447 | `calendar_scope` | Builds a per-consumer Calendar-ID scope. |

The source authority/fingerprint arguments no longer belong to Vault standing-grant identity after 02. App/Connections/native source owns current source proof.

### 4.6 Native acquisition still accepts a caller-selected subset

`crates/modules/context/src/application/native_calendar.rs`:

- ~108 `admit_current_native_calendar_read`.
- ~115 `expected_calendar_ids: Option<&[String]>`.
- ~136 chooses caller subset or current connection resources.
- ~188 calls grant admission with those Calendar IDs.
- current connection is re-read after admission.

`crates/modules/context/src/application/native_calendar_view.rs`:

- ~22 `NativeCalendarViewRead`.
- ~26 `selected_calendar_ids`.
- ~31 read path forwards that subset.
- ~174 dependency reauthorization currently gets exact prior physical resources from `dependency.source_resources`.

The 02 provenance split is already correct; 03 removes the caller subset from initial acquisition.

### 4.7 Access still compares grant resources to Calendar IDs

`crates/modules/access/src/application/calendar_read.rs`:

- ~21 `CalendarReadAccessRequest` carries physical Calendar IDs for provider check; keep this.
- ~32 `CalendarReadAccessAdmission` contains stable source, explicit current source authority, scope and policy.
- ~196 `admits_native_calendar_read` requires grant scope resources to equal Calendar IDs.
- ~278 `admission_matches_dependency` requires dependency `source_resources` to be contained in admission grant resources.

Both grant-resource/leaf relationships become invalid once grant resources are logical.

Physical Calendar IDs remain in provider/source request and dependency `source_resources`.

### 4.8 Access owns a Calendar-specific dependency builder

`crates/modules/access/src/application/calendar_lease.rs`:

- ~14 `CalendarLeaseKey` carries query identity plus Calendar IDs.
- ~29 `calendar_lease_dependency` constructs `ContextDependency`.

Only Context native acquisition needs this provenance/query builder. Access should own grant admission, not Context dependency construction.

### 4.9 Expert host still turns selected refs into Calendar IDs

`crates/app/src/vault_host/conversation_turn/expert_host.rs`:

- ~599 `calendar_views` passes selected refs to the Calendar reader.
- ~807 `calendar_access_requirement` publishes blocker resources from selected refs.
- ~940 native/hosted selected validation requires each selected resource to exist in `connection.resources`.
- ~974 native branch maps `selected[*].resource` to Calendar IDs.
- ~999 passes them as `selected_calendar_ids`.
- ~1020+ hosted Google/Microsoft path loops selected leaf refs and reads remote Calendar once per leaf.

After 03:
- native branch validates one logical selection and never derives provider IDs from it;
- hosted branch remains leaf-oriented until 04.

### 4.10 Inline review is one member per Calendar leaf

`crates/app/src/vault_host/review_snapshot.rs`:

- ~170+ native capture.
- ~190 reads full current source resources.
- ~200 derives reviewed leaves from `requirement.resources`.
- ~240 recomputes target-specific Registry policy.
- ~270 emits one `SnapshotMember` per reviewed Calendar leaf.

`crates/app/src/vault_host/interaction_owners.rs`:

- ~280+ native live-state reader.
- ~300 verifies every reviewed member resource is a current Calendar leaf.
- ~311 turns members back into Calendar IDs.
- ~343 recomputes target policy by leaf list.
- ~630 `probed_member` has Calendar-specific per-resource policy logic.
- ~790 `calendar_scope_satisfied` loops every Connection leaf and requires one covering grant per leaf.

All of those must become one logical Calendar member/grant.

### 4.11 Actions currently bridge leaf Expert selection to source provenance

Checkpoint 02 correctly changed physical action validation to `source_resources`:

- `crates/modules/actions/src/application/expert.rs:547+` compares destination Calendar ID to dependency `source_resources`.
- `crates/adapters/vault/src/vault/agent_actions.rs:482+` currently compares the selected Expert resource to dependency physical source resources because selected Calendar refs are still leaf IDs.

After native Expert selection becomes logical in 03:
- action destination remains checked against `source_resources`;
- Expert execution selection is checked against dependency permission `resources`.

This keeps configuration and physical provenance distinct.

### 4.12 Product UI automatically re-reviews after Calendar resource edits

`apps/client/lib/features/connections/presentation/connector_screen.dart`:

- line ~103 stores `reconcileCalendarAfterExplicitChange`.
- ~154-159 `didUpdateWidget` triggers automatic review after connection changes.
- ~771 displays `Using <granted> of <selected> selected calendars`.
- ~846+ `_reviewObserve` previews current Calendar IDs and reviews.
- ~877-913 `_calendarChangedExplicitly` / `_reconcileExplicitCalendarChange` deliberately re-review an active grant after resource edits.

This behavior directly contradicts the final invariant and must be removed in 03, even though DTO deletion belongs to 07.

## 5. 03-A — baseline and semantic inventory

Before production edits:

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
docs/development/plans/connection-observe-authority/03-native-calendar-vertical.md
docs/architecture/invariants.md
docs/architecture/modules.md
docs/architecture/runtime.md
docs/architecture/authority-recovery.md
~~~

Record a pre-edit inventory:

~~~
rg -n "selected_shipped_consumers|native_calendar_policy_for_target" crates
rg -n "selected_calendar_ids|expected_calendar_ids" crates
rg -n "calendar_addition_produces_a_new_candidate" crates
rg -n "for calendar in connection\.resources\(\)" crates/modules/context crates/app
rg -n "calendar_scope\(|calendar_binding\(" crates/adapters/vault/src/vault/calendar_grants.rs
rg -n "calendar_ids.*GrantScope|GrantScope.*calendar_ids" crates
rg -n "source_resources\(\).*selected\.resource|selected\.resource.*source_resources" crates
rg -n "reconcileCalendarAfterExplicitChange|_reconcileExplicitCalendarChange" apps/client
rg -n "granted_resources|grantedResources" crates apps/client
~~~

Classify every native Calendar leaf-semantic match. Hosted Calendar matches assigned to 04 and temporary DTO matches assigned to 07 are allowed only when explicitly documented.

## 6. 03-B — native Calendar candidate becomes one connection/View

Modify:

~~~
crates/modules/context/src/application/source_candidates.rs
~~~

### Native candidate construction

For a serving EventKit `SourceConnection` with correct Person and execution owner:

~~~
let resource = connection_view_resource(
    CALENDAR_CONTEXT_VIEW_ID,
    connection.connection_id(),
)?;
add(
    connector,
    connection.connection_id(),
    connection.execution_owner_id(),
    resource,
    connection/provider title,
    "Connected calendar account/device"
)
~~~

Use the shared contract helper from 02. Do not manually format the resource string.

Emit exactly one candidate regardless of:

- resource count;
- resource handles;
- resource labels;
- resource mode.

The candidate may require the connection to have a non-empty usable current resource set if an empty native source is considered non-serving by current Connections invariants. Follow the existing source usability contract rather than inventing a new Calendar-specific state.

### Hosted branch

Keep Google/Microsoft leaf candidates in an explicit deferred branch until 04 because hosted `SelectedCalendarContextReader` still reads one leaf per selected reference.

Do not let the native logical branch accidentally make hosted refs logical while their read path remains leaf-based.

### Tests

Replace:

~~~
calendar_addition_produces_a_new_candidate_without_changing_the_old_reference
~~~

with a regression such as:

~~~
native_calendar_resource_change_preserves_single_connection_view_candidate
~~~

Required assertions:

1. resources `[A]` -> one candidate X;
2. X.resource == `calendar.timeline:<connection>`;
3. resources `[A,B,...]` -> still one candidate X;
4. exact candidate and candidate ID unchanged;
5. label-only resource changes leave X unchanged;
6. foreign Person rejected;
7. foreign device rejected;
8. disconnected/non-serving connection rejected.

Keep a hosted Calendar test proving its leaf candidate behavior remains intentionally deferred to 04, so the boundary is executable rather than implicit.

## 7. 03-C — stabilize native Expert bindings

Primary files:

~~~
crates/app/src/vault_host/expert_binding_settings.rs
crates/app/src/vault_host/tests/registered_runner.rs
relevant Expert registry/binding tests
~~~

### Initial defaults

`bind_initial_defaults` should not need a native Calendar special case beyond selecting the one live candidate.

If the existing branch selects every Calendar live candidate, it naturally selects one native candidate after 03-B. Keep that simple behavior.

Do not add a second list of current Calendar resources to Expert binding state.

### Existing binding stability

Add an App-level regression:

1. install shipped Calendar-capable Expert;
2. bind native candidate X;
3. capture assignment binding revision;
4. mutate Connections resources `[A] -> [A,B]`;
5. inspect candidates;
6. assert X is still selected and candidate ID unchanged;
7. assert assignment binding revision unchanged;
8. invoke/re-resolve current selection successfully.

A source replacement with a new Connection ID should produce a different candidate and require explicit reconfiguration. Resource changes on the same Connection should not.

### No compatibility migration

Old local profiles with per-leaf saved native Calendar refs are disposable.

Do not:
- translate old leaf refs at runtime;
- accept either leaf or logical native refs;
- add a fallback candidate;
- keep an old native source picker branch.

Use a fresh development profile for execution/manual validation if needed.

## 8. 03-D — replace Registry-selected consumers with trusted shipped capability policy

Primary file:

~~~
crates/app/src/first_party_observe.rs
~~~

### Canonical helper

Replace `selected_shipped_consumers` with a helper based only on `floe_experts_builtin::manifests()`.

Required algorithm:

1. obtain the compile-time shipped manifest set;
2. validate the manifests or rely on the validated shipped registration invariant;
3. include each manifest whose `source_requirements` contains the exact capability;
4. map package ID to `GrantConsumer::Builtin`;
5. sort/dedupe;
6. never inspect Registry assignment/binding state.

The helper receives a capability/View, not connector/connection/resource identity.

### Calendar policy

`calendar_policy()` should include the shipped Calendar consumers directly.

Planning-base expected set, verified from `BuiltinExpertKind::required_sources()`:

~~~
floe.builtin.commitments
floe.builtin.focus-attention
floe.builtin.schedule
floe.builtin.wellbeing
~~~

Canonical sort order is whatever `GrantConsumer` ordering produces; tests should compare canonical IDs rather than relying on declaration order unless the contract explicitly sorts.

Do not add `assistant`: `manager_direct_remote_view("calendar.timeline")` is false and there is no Manager direct native Calendar read tool.

### Remove target-specific native policy

Delete:

~~~
native_calendar_policy_for_target
~~~

All native Calendar policy callers use `calendar_policy()` / the canonical capability policy.

Calendar resource list, connection ID, device ID and Registry snapshot are no longer policy inputs.

### Migrate remaining `selected_shipped_consumers` callers

The old helper is also used by:
- generic remote target policy;
- native personal target consumer calculation.

Migrate those callers to the same trusted shipped manifest capability rule so the obsolete helper can be deleted rather than retained only for later checkpoints.

This is a policy-source convergence, not 04/06 source-ownership work:
- remote transport/grant stack remains unchanged until 04;
- personal source/resource ownership remains unchanged until 06.

Where an existing wrapper becomes a forwarding-only target-specific function after Registry removal, delete it and call the canonical policy directly.

### Tests

Delete/rewrite:

~~~
shipped_consumer_requires_active_exact_binding
~~~

Add:

1. shipped Calendar consumer set equals packages declaring `calendar.timeline`;
2. disabling/unbinding a shipped assignment does not change the set;
3. rebinding a shipped assignment from one connection/resource to another does not change the set;
4. installing/binding an extension manifest declaring `calendar.timeline` does not change the set;
5. package ID spoofing from a non-shipped manifest cannot enter the compile-time shipped set;
6. Calendar policy fingerprint is unchanged across Registry binding changes;
7. Calendar policy fingerprint changes when actual shipped product policy fields change;
8. Manager assistant is absent from Calendar;
9. remote/personal direct Manager consumer behavior still follows explicit Manager tool policy.

Do not weaken Expert callability checks. A grant consumer being allowed by product policy does not make an Expert installed, enabled, configured or callable.

## 9. 03-E — one logical native Calendar grant

Modify:

~~~
crates/adapters/vault/src/vault/calendar_grants.rs
crates/app/src/vault_host/calendar_access.rs
crates/modules/access/src/application/calendar_read.rs
~~~

### Canonical logical resource

Use:

~~~
connection_view_resource(
  CALENDAR_CONTEXT_VIEW_ID,
  &ConnectionId::try_new(connection_id)?
)
~~~

Do not create a Calendar-specific formatter.

### `calendar_binding`

Replace the leaf-ID builder with a logical builder:

~~~
native_calendar_binding(
  person,
  provider,
  device,
  connection_id,
  consumers
) -> (GrantSourceBinding, GrantScope)
~~~

The resulting scope has one resource: `calendar.timeline:<connection>`.

Remove Calendar IDs and `SourceAuthority` from this standing-grant builder.

### `review_native_calendar_grant`

Narrow the Vault API.

Vault review needs:
- stable connection/source identity;
- trusted consumers;
- expected Grant ID/authority;
- logical scope.

It does not need:
- current Calendar leaf list;
- current `SourceAuthority`;
- native subject fingerprint.

Those have already been verified by App/Connections/native source before the Vault transaction. Do not duplicate source authority in Vault.

### Existing leaf grant on old local profile

An old leaf-scoped grant is not silently converted through a compatibility branch.

On a fresh profile only logical grants are created.

If old local data is encountered and current code can structurally open it, the native logical admission/review must fail closed rather than treating leaf scope as equivalent. Record the fresh-profile requirement; do not add migration logic or dual semantics.

### `authorize_current_native_calendar_grant`

Stop constructing a Calendar-ID requested scope.

Authorization should validate:
- exactly one active standing grant for the stable native source;
- stored grant resource is exactly `calendar.timeline:<connection>`;
- metadata/content categories;
- Read/Assistant;
- requested consumer is in stored grant consumer set;
- LocalOnly;
- GrantAuthority/state/policy valid.

Return a per-consumer admission scope if the current API requires one, but that scope must also contain the one logical View resource, never the Calendar IDs.

Remove redundant `grant_scope` from `CalendarGrantAdmission` if it becomes caller-zero outside tests.

### `CalendarReadAccessAdmission`

Keep explicit `source_authority` from 02.

Its `scope` becomes the logical per-consumer permission scope.

### Consumer policy

`CalendarGrantPolicy` remains until 05.

Because stable source + logical grant scope + trusted consumer set are unchanged across Connection resource changes, `evolve_calendar_consumer_policy` should preserve `ConsumerPolicyAuthority` across `[A] -> [A,B]`.

Add an explicit regression for that bounded pre-05 behavior.

## 10. 03-F — native Use with Floe review uses current Connection resources, not grant resources

Modify:

~~~
crates/app/src/vault_host/calendar_access.rs
crates/app/src/local_access_services.rs only if comments/contracts need correction
~~~

### Review input semantics until 07

`CalendarAccessChange::Review.calendar_ids` remains only as reviewed source evidence until 07.

For a native connection-wide grant, require exact canonical equality with the current Connections resource handles:

~~~
reviewed_calendar_ids == current_connection_resources
~~~

after sorting/deduping.

Do not accept a subset.

This proves the subject preview corresponds to the entire current connection source the user is enabling.

### Review flow

Target order:

1. load usable current Connection;
2. verify reviewed connection ID and expected `SourceAuthority`;
3. canonicalize current full resource set;
4. require review `calendar_ids` exactly equal that set;
5. preview native subject over that full set;
6. compare expected reviewed subject;
7. get assignment-independent `calendar_policy()` consumers;
8. re-read/revalidate Connection;
9. update trusted native subject in Connections if needed;
10. review/activate logical Calendar grant;
11. return overview.

No Registry read is required to compute consumers.

### Inspect after resource update

If an active logical Calendar grant exists on the stable source:
- `Inspect` returns Active;
- grant ID unchanged;
- GrantAuthority unchanged;
- review_required remains false solely because resources changed;
- current `source_authority` reflects the Connections update.

Source drift invalidates prior evidence, not the standing grant.

### Pause/remove

Pause/revoke continue to target the standing grant by Grant ID/GrantAuthority and stable source.

Current `SourceAuthority` need not be a Vault grant argument. Keep current connection ownership checks in App before mutation.

## 11. 03-G — temporary Calendar projection must not reinterpret logical grant as one selected Calendar

Files:

~~~
crates/app/src/vault_host/calendar_access.rs
crates/app/src/connection_observe.rs
apps/client/lib/features/connections/presentation/connector_screen.dart
~~~

07 deletes `selected_resources` / `granted_resources`; 03 only prevents them from becoming false authority/UI semantics.

### Backend projection

Do not populate `CalendarAccessOverview.granted_resources` by reading `grant.scope.resources()` after the grant becomes logical, because that would expose:

~~~
["calendar.timeline:<connection>"]
~~~

as if one Calendar leaf were granted.

Until 07, choose the smallest non-authoritative projection compatible with current UI/tests. Preferred behavior:

- `selected_resources` = current Connections Calendar handles;
- `granted_resources` = current selected resources only when the standing grant is currently Active and not review-required;
- otherwise empty or the smallest state-consistent projection required by current UI.

Document in code that this is display-only and must never be consumed by Access/Context admission.

`ConnectionObserveOverview::from_calendar` must not turn this compatibility projection back into permission authority.

If `ConnectionObserveMember.resources` needs the actual logical grant resource for review/authority logic, supply it from the actual grant separately rather than overloading the temporary leaf display list.

### Flutter copy

The existing:

~~~
Using ${grantedResources.length} of ${selectedResources.length} selected calendars.
~~~

must not be allowed to render “1 of 11” from a logical grant.

Either:
- derive the temporary compatibility projection as above; and/or
- replace the count copy with connection-level wording such as current selected Calendars are used while the grant is active.

Do not redesign the whole ConnectionObserve UI in 03. 07 owns final DTO/gateway cleanup.

## 12. 03-H — inline native review becomes one logical member

Modify:

~~~
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_owners.rs
relevant Conversation interaction tests
~~~

### Capture

`capture_native_calendar` should:

1. load current native `SourceConnection`;
2. verify connection/execution owner/source authority;
3. derive the one logical View resource;
4. require the source access requirement to identify that logical resource;
5. obtain full current Calendar IDs from Connections for subject preview;
6. preview native subject over the full current set;
7. compute assignment-independent Calendar policy fingerprint;
8. update trusted Connections subject as today;
9. verify current logical grant expectation;
10. emit exactly one `SnapshotMember`:
    - member_id = `calendar.timeline`;
    - resource = `calendar.timeline:<connection>`;
    - source_revision = current SourceAuthority;
    - expected_grant = current logical grant expectation;
    - policy_authority = current Calendar policy authority until 05.

No `for resource in reviewed` leaf-member loop remains.

### Live revalidation

`read_native_calendar` / equivalent live owner path should:
- require exactly one logical member;
- verify member resource equals current connection/View resource;
- probe native subject using the full current Connections Calendar set;
- compare current source authority/revision/native subject;
- recompute assignment-independent policy fingerprint;
- read current logical grant once.

Delete Calendar-specific per-leaf `probed_member` policy recomputation.

### Scope satisfied

Replace `calendar_scope_satisfied` behavior that loops every Connection Calendar and asks for a covering grant per leaf.

Target:

~~~
one current usable Connection
+ one active logical calendar.timeline grant
+ one current native subject probe over all current resources
= satisfied
~~~

A resource update should not make this false merely because the grant does not list the new leaf.

## 13. 03-I — native read always resolves the full current Connections resource set

Modify:

~~~
crates/modules/context/src/application/native_calendar.rs
crates/modules/context/src/application/native_calendar_view.rs
crates/app/src/vault_host/conversation_turn/expert_host.rs
crates/app/src/vault_host/calendar_access.rs
~~~

### `admit_current_native_calendar_read`

Delete:

~~~
expected_calendar_ids: Option<&[String]>
~~~

Always:

~~~
calendar_ids = connection_calendar_ids(&connection)
~~~

Require:
- non-empty;
- canonical/unique;
- valid identifiers;
- current connection serving;
- current subject exists.

The Connection owner already canonicalizes resources; keep defensive boundary validation without creating a second source selection.

### `NativeCalendarViewRead`

Delete:

~~~
selected_calendar_ids
~~~

Callers provide:
- Person;
- device;
- consumer;
- Calendar query;
- deadline/cancellation.

### Native provider check/read

Both `source.check` and `source.observe` receive the full current Calendar ID set.

Keep:
- native subject fingerprint validation;
- provider generation/double-read semantics;
- exact stamp equality;
- post-I/O current Connection re-read;
- timeout/cancellation;
- item/byte/query budgets.

No fixed Calendar count permission cap.

### `NativeCalendarGrantReader`

Narrow the trait to grant semantics.

A target shape is:

~~~
admit(
  connection,
  person_id,
  consumer
) -> CalendarReadAccessAdmission
~~~

or equivalent.

Do not pass:
- Calendar leaf IDs;
- native subject fingerprint;

merely so Vault can re-check provider/source facts it does not own.

`VaultNativeCalendarGrants` can derive the logical resource from `connection.connection_id()` and the current explicit `source_authority` from the Connection when constructing the admission.

### Access admission

Update `admits_native_calendar_read`:

Validate:
- Person;
- stable source connection/connector/execution owner;
- admission explicit `source_authority` == current Connections authority;
- one logical `calendar.timeline:<connection>` scope;
- metadata/content;
- Read/Assistant;
- exact requesting consumer;
- LocalOnly.

Do not compare grant resources with Calendar IDs.

Provider Calendar IDs are validated through:
- Connections current resource set;
- native source request/stamp;
- exact acquisition provenance.

### Dependency construction

A native successful read records:

~~~
resources = admission.scope.resources()
source_authority = admission.source_authority
source_resources = exact calendar_ids used in source.check/source.observe
~~~

After logical grant cutover `resources` has one connection/View resource.

## 14. 03-J — delete Access-owned Calendar dependency construction

Current:

~~~
crates/modules/access/src/application/calendar_lease.rs
~~~

owns `CalendarLeaseKey` and `calendar_lease_dependency`, despite the key describing Context query/acquisition provenance.

Search callers first:

~~~
rg -n "CalendarLeaseKey|calendar_lease_dependency" crates
~~~

On the planning base the production callers are both Context-owned:

- `crates/modules/context/src/application/native_calendar_view.rs` constructs dependency provenance for the direct native Calendar View read;
- `crates/modules/context/src/application/calendar_timeline.rs` uses `CalendarLeaseKey` as its governed timeline lease/cache key and constructs the same dependency shape.

Migrate both callers in one cutover:

1. move the query/lease key to Context as one private/internal Context-owned type shared by `native_calendar_view.rs` and `calendar_timeline.rs`, or place it in the existing Context lease module if that avoids a duplicate key;
2. keep exactly one canonical serialization for the query fingerprint used by both paths;
3. construct `ContextDependency` in Context using the admitted logical grant + exact source resources;
4. migrate the `calendar_timeline.rs` lease map/key type in the same change;
5. remove `calendar_lease_dependency`;
6. remove `CalendarLeaseKey` from Access public surface;
7. delete `crates/modules/access/src/application/calendar_lease.rs`;
8. remove exports/imports/tests that exist only for the Access-owned helper.

Do not duplicate one key/fingerprint type per Context caller merely to delete the Access module.

Keep the query fingerprint deterministic and bound to:
- invocation;
- Person;
- connection/view identity as appropriate;
- exact Calendar IDs acquired;
- range;
- cursor;
- timezone offsets;
- item/byte limits.

Do not move Access grant judgments into Context while deleting the helper.

## 15. 03-K — native Expert read consumes one logical selected reference

Modify the native branch of:

~~~
crates/app/src/vault_host/conversation_turn/expert_host.rs
~~~

### Selection validation

For `calendar.event_kit`:

Require:
- exactly one selected ref for this requirement;
- capability `calendar.timeline`;
- contract version 1;
- connector/connection/execution owner match the current Connection;
- selected resource == `calendar.timeline:<connection>`.

Do not require selected resource to appear in `connection.resources()`.

### Acquisition

Delete:

~~~
selected -> calendar_ids
~~~

Do not construct provider access from selected resources.

Construct the native provider reader using current Connection identity; Context then loads the current full resource set and passes it to source check/read.

If `NativeCalendarReadAccess::new` currently requires Calendar IDs, pass the current Connection resources only at the provider boundary or narrow that adapter constructor if Context already supplies IDs on each request. Do not derive them from Expert binding.

### Blockers

`calendar_access_requirement` for native selection should publish the one logical selected resource in the source access requirement.

Current source authority remains Connections-derived.

### Hosted branch

Preserve current Google/Microsoft leaf-selected iteration until 04. Keep the branch explicit.

### Tests

Rewrite `selected_calendar()` fixtures in `expert_host` and App tests to logical connection/View references for EventKit.

Do not update hosted Calendar fixtures to logical refs until 04.

## 16. 03-L — align Actions with new native selection semantics

Modify:

~~~
crates/adapters/vault/src/vault/agent_actions.rs
crates/modules/actions/src/application/expert.rs
~~~

### Expert execution selection

After native selected refs become logical, this check:

~~~
selected.resource ∈ dependency.source_resources
~~~

is wrong.

For native Calendar, selection is permission/configuration identity, so validate:

~~~
selected.resource ∈ dependency.resources
~~~

With the bounded 04 transition this also works for hosted Calendar while its dependency permission resource remains the special leaf resource.

Do not use this change to broaden an action destination.

### Exact action destination

Keep `validate_context_calendar_source` checking:

~~~
dependency.source_resources contains destination calendar_id
current SourceConnection.resources contains destination calendar_id
dependency.source_authority == current SourceAuthority
~~~

Observe still does not imply Act. Preserve all proposal approval/durable intent/idempotency/uncertain-result recovery gates.

## 17. 03-M — remove resource-change auto re-review in Flutter

Modify:

~~~
apps/client/lib/features/connections/presentation/connector_screen.dart
relevant connector_screen tests
~~~

Delete the behavior represented by:

~~~
reconcileCalendarAfterExplicitChange
_reconcileExplicitCalendarChange
~~~

A successful Calendar source configuration change should:

1. update the Connections source through its existing source gateway;
2. refresh current connection/Observe inspection;
3. if the logical standing grant was Active, it remains Active;
4. not call `reviewCalendarAccess` automatically.

The next source-dependent read will use the new `SourceAuthority` and exact current resource set.

A resource edit may make old dependencies stale; that is expected and must not surface as grant review.

When Use with Floe is Off/NeedsReview, changing Calendar resources also must not silently enable it.

### Review when user explicitly turns On

The explicit toggle-on flow may continue until 07 to:
- preview current subject over `connection.selectedCalendarIds`;
- send those IDs as compare-only reviewed source evidence;
- send expected source/grant authority.

Backend requires exact full current source equality and writes only a logical grant.

## 18. 03-N — rewrite permanent acceptance tests

### 18.1 Source candidates

`crates/modules/context/src/application/source_candidates.rs`:

- `[A] -> candidate X`;
- `[A,B] -> same candidate X`;
- candidate resource is canonical connection/View;
- candidate stable across label changes;
- foreign identity denied;
- hosted Calendar leaf candidate remains marked 04 transition.

### 18.2 First-party policy

`crates/app/src/first_party_observe.rs`:

- Calendar trusted shipped consumers exactly match shipped manifest capability declarations;
- Registry assignment/binding changes do not alter consumer set/fingerprint;
- extension install/bind does not alter it;
- Calendar has no fallback assistant;
- existing Manager direct remote/native tools retain explicit assistant policy.

### 18.3 Vault logical grant

`crates/adapters/vault/src/vault/calendar_grants.rs`:

1. fresh review creates one logical Calendar grant;
2. grant scope resources length == 1;
3. resource == `calendar.timeline:<connection>`;
4. all canonical shipped Calendar consumers are admitted under the same grant;
5. extension consumer denied;
6. resource set/source-authority changes do not by themselves change grant ID/GrantAuthority/policy authority;
7. pause/revoke semantics unchanged;
8. wrong stable source denied;
9. wrong logical resource/corrupt old leaf grant denied;
10. no-op review keeps current authority/policy per current semantics.

Delete/rewrite tests whose expected behavior is “scope expands from home to home+work”.

### 18.4 App eleven-Calendar regression

Promote checkpoint-00 temporary reproduction into a permanent positive test in:

~~~
crates/app/src/vault_host/tests/native_calendar_access.rs
~~~

Required setup:

- one EventKit SourceConnection;
- 11 current Calendar resources;
- no manual per-leaf Expert binding expansion;
- current native subject fingerprint for all 11.

Required result:

- `Inspect` -> NeedsReview initially;
- explicit review of full current source succeeds;
- state Active;
- one grant;
- logical grant resource;
- shipped Calendar consumer set present;
- no empty consumer error.

The fixture should not need to install/bind every Calendar leaf. Prefer removing per-leaf registry setup from the fixture entirely if no other test needs it.

### 18.5 Resource update invariant

Permanent integration:

1. start with `[A]`;
2. bind native Expert to candidate X;
3. enable logical grant G;
4. record:
   - candidate X ID;
   - binding revision;
   - Grant ID;
   - GrantAuthority;
   - ConsumerPolicyAuthority while it still exists;
5. Connections configure `[A,B]`;
6. assert SourceAuthority changed;
7. inspect Observe;
8. assert G still Active;
9. assert Grant ID/GrantAuthority/policy authority unchanged;
10. inspect Expert candidate/binding;
11. assert X and binding revision unchanged;
12. perform next read;
13. provider receives A+B;
14. dependency resources == logical View;
15. dependency source_resources == A+B;
16. old dependency from A authority fails reauthorization.

This is the defining 03 regression.

### 18.6 Native read

Rewrite `crates/modules/context/tests/native_calendar_read.rs` fixture grant reader to logical grant scope.

Update assertions:
- admission scope resource is logical View;
- >128 current source resources are still sent to provider/read where byte/budget permits;
- grant scope remains one logical resource;
- no selected-subset test remains;
- two canonical consumers use the same Grant ID;
- foreign source/subject/generation/current-authority checks remain.

Delete tests asserting subset reads selected by Expert.

### 18.7 Inline interaction review

Add/update tests proving:
- native Calendar inline review contains one member, not N leaves;
- resource field is logical View;
- source authority drift invalidates pending decision;
- new Connection resource alone does not require a different standing grant expectation;
- policy fingerprint unaffected by Registry rebinding;
- reviewed native subject drift still blocks resolution.

### 18.8 Flutter

Update `connector_screen_test.dart`:

1. active Use with Floe + Calendar resource edit does not call review mutation;
2. UI remains Active after refreshed backend inspection;
3. no “1 of 11” logical-resource count regression;
4. explicit Off pauses;
5. explicit On still performs subject preview + review;
6. 11 selected Calendars remain supported;
7. source edit does not silently enable a previously off grant.

Do not delete the whole native Calendar access gateway in 03; 07 owns that.

## 19. Deletion and residual gate

After implementation run:

~~~
rg -n "selected_shipped_consumers|native_calendar_policy_for_target" crates
rg -n "selected_calendar_ids|expected_calendar_ids" crates
rg -n "calendar_addition_produces_a_new_candidate" crates
rg -n "for calendar in connection\.resources\(\)" crates/modules/context crates/app
rg -n "calendar_scope\(|calendar_binding\(" crates/adapters/vault/src/vault/calendar_grants.rs
rg -n "source_resources\(\).*selected\.resource|selected\.resource.*source_resources" crates
rg -n "reconcileCalendarAfterExplicitChange|_reconcileExplicitCalendarChange" apps/client
rg -n "Using .*grantedResources|grantedResources\.length" apps/client
rg -n "CalendarLeaseKey|calendar_lease_dependency" crates
rg -n "\"calendar.timeline\".*ResourceHandle::try_new\(\"(home|primary|calendar-)" crates
~~~

### Required zero matches in production

- `selected_shipped_consumers`;
- `native_calendar_policy_for_target`;
- native `selected_calendar_ids`;
- native `expected_calendar_ids`;
- old candidate-addition test;
- native grant builder converting Calendar IDs to `GrantScope.resources`;
- EventKit Expert binding refs using leaf Calendar resource;
- native interaction member-per-leaf loop;
- Flutter automatic grant re-review after source resource edit;
- Access-owned Calendar dependency builder if caller audit confirms it is obsolete.

### Allowed classified matches

- hosted Google/Microsoft Calendar leaf source selection/read: owner 04;
- `CalendarAccessChange::Review.calendar_ids`: compare-only product review evidence until 07;
- `selected_resources` / `granted_resources`: non-authoritative presentation fields until 07;
- `ConsumerPolicyAuthority`: owner 05;
- provider/native requests/stamps containing exact Calendar IDs;
- ContextDependency `source_resources`;
- Day/provider event provenance carrying Calendar IDs;
- historical plan/docs evidence.

Every surviving native Calendar leaf match must be classified by semantic role.

## 20. Architecture documentation convergence

The planning-base `docs/architecture/runtime.md` is explicitly stale: it currently says native Calendar selected reads carry an exact Calendar subset from Expert binding and newly added resources do not widen that subset.

Update in the same implementation change:

~~~
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
~~~

After 03 current docs must state:

- native Calendar Expert binding selects connection/View;
- one logical Calendar standing grant covers the connection View;
- first-party consumers come from trusted shipped manifest capability declarations;
- Registry assignment/binding controls configuration/callability, not grant consumer authority;
- Context resolves every current Connections Calendar resource at acquisition time;
- exact provider leaves live only in `source_resources`;
- resource changes advance SourceAuthority and stale old dependencies;
- resource changes do not re-review the grant or mutate Expert binding;
- native inline Observe review has one logical View member;
- hosted Calendar special leaf semantics remain until 04;
- ConsumerPolicyAuthority remains until 05;
- product DTO duplication remains until 07.

Do not rewrite ADR history solely for checkpoint progress. The final rationale ADR convergence remains 09 unless implementation reveals a genuinely new durable decision.

## 21. Suggested implementation slices

Multiple commits are acceptable. Suggested coherent slices:

### 03-A — native connection/View candidate

- source candidate change;
- candidate tests;
- Expert binding/default stability tests.

Suggested commit:

~~~
experts: bind native calendar by connection view
~~~

### 03-B — trusted first-party policy

- shipped manifest consumer helper;
- Calendar policy;
- remote/personal callers off Registry-selected consumer authority;
- policy tests;
- delete `selected_shipped_consumers` and target-specific Calendar policy.

Suggested commit:

~~~
access: derive first party consumers from shipped manifests
~~~

### 03-C — logical native Calendar grant

- logical scope builder;
- review/authorize API narrowing;
- App review flow;
- policy stability tests;
- temporary projection correction.

Suggested commit:

~~~
access: grant native calendar logical view
~~~

### 03-D — current-resource native acquisition

- remove selected subset;
- narrow `NativeCalendarGrantReader`;
- dependency logical resources + exact source resources;
- delete Access Calendar lease helper;
- native read tests.

Suggested commit:

~~~
context: read all current native calendars
~~~

### 03-E — Expert/interaction/Action convergence

- Expert host one logical native ref;
- source blockers;
- one-member inline review;
- scope satisfaction;
- action selection permission-resource check.

Suggested commit:

~~~
app: converge native calendar view authority
~~~

### 03-F — product invariant and acceptance

- remove auto re-review;
- fix temporary UI projection/copy;
- eleven-Calendar acceptance;
- resource-update acceptance;
- Flutter tests.

Suggested commit:

~~~
client: keep calendar grant stable across resource edits
~~~

### 03-G — residual/docs/closure

- deletion audit;
- architecture docs;
- execution evidence;
- README status.

Suggested commit:

~~~
docs: complete connection observe checkpoint 03
~~~

Combine slices where a smaller coherent change results. Do not create compatibility wrappers between slices.

## 22. Verification

Run targeted checks during implementation.

Minimum Rust close gate:

~~~
cargo test -p floe-context source_candidates
cargo test -p floe-context --test native_calendar_read
cargo test -p floe-context --test calendar_timeline
cargo test -p floe-access calendar
cargo test -p floe-vault calendar
cargo test -p floe-actions calendar
cargo test -p floe-app native_calendar_access
cargo test -p floe-app first_party_observe
cargo test -p floe-app registered_runner
cargo test -p floe-conversation
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
~~~

If Cargo test filters match zero tests, use the nearest exact target/test command and record it. A zero-match filter is not evidence.

Because the change crosses App/FFI-visible product behavior, run:

~~~
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/connections/native_calendar_access_test.dart
flutter test test/features/connections/connector_screen_test.dart
flutter build macos
~~~

Run full affected Flutter connection tests when the exact paths differ.

Broad Rust gate before closure:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
~~~

If the known default-parallel shared-counter flake is also run, report its result separately. Do not hide it and do not weaken the test.

### Apple-native validation

Per the verification skill, run the repository's available EventKit/native Calendar fixture or native integration checks that do not require unauthorized external account changes.

Required real behavior when environment permits:

1. source configured with >4 Calendars;
2. logical grant active;
3. native check/read receives all current resources;
4. source resource edit does not trigger a grant review;
5. subsequent read uses new resource set;
6. native subject/generation drift fails closed.

If an authorized EventKit device/read is unavailable, report the live smoke as SKIPPED with the concrete reason. Do not modify signing, permissions or account state to manufacture coverage.

### Existing unrelated golden mismatch

The prior checkpoints recorded one unchanged `agent_registry_dialog_test.dart` 29-pixel golden mismatch.

If full Flutter tests are run and the same unchanged mismatch reproduces:
- report it separately;
- verify no related file/golden changed;
- do not modify unrelated golden output as part of 03.

A new connections/Calendar golden or functional failure is in-scope and must be resolved.

## 23. Close procedure

Before marking 03 complete:

1. rerun all residual searches;
2. classify every hosted Calendar leaf residual as 04 or eliminate it;
3. classify every Calendar DTO/presentation residual as 07 or eliminate it;
4. ensure native production has one candidate and one logical grant resource;
5. ensure no Registry binding state affects first-party consumer policy;
6. ensure no resource edit path automatically re-reviews the grant;
7. ensure no native Expert selected ref contains a Calendar leaf;
8. ensure physical Calendar IDs appear only in Connections/provider/source/provenance boundaries;
9. run targeted and broad verification;
10. update architecture docs;
11. append execution evidence to this file;
12. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Not started
13. commit closure;
14. stop. Do not begin checkpoint 04.

Execution evidence must record:

- execution date;
- start local HEAD / fetched origin/main;
- implementation and closure commit SHAs;
- final native candidate reference shape;
- final trusted Calendar consumer set and derivation;
- final Calendar GrantScope;
- final native read request path;
- final dependency resources/source_resources example;
- inline review member shape;
- resource-update grant/binding invariants;
- exact eleven-Calendar acceptance result;
- third-party denial result;
- deleted functions/files/tests;
- hosted Calendar residuals deferred to 04;
- policy residuals deferred to 05;
- DTO/UI residuals deferred to 07;
- architecture docs changed;
- residual search results;
- commands and actual outcomes;
- native smoke result or SKIPPED reason;
- final clean worktree.

## 24. Required agent report

Report:

1. start HEAD / origin-main / final HEAD;
2. native Calendar candidate final shape and candidate-ID stability;
3. Expert binding resource-change stability;
4. trusted shipped consumer derivation and exact Calendar consumers;
5. proof Registry/third-party binding cannot alter first-party grant consumers;
6. native Calendar logical GrantScope and Vault API changes;
7. Calendar access review semantics and temporary DTO projection;
8. native read path and removal of selected Calendar subset;
9. final `ContextDependency.resources` / `source_resources` example;
10. Calendar lease/dependency builder deletion or justified residual;
11. inline review one-member convergence;
12. Actions provenance/selection updates;
13. Flutter resource-change behavior and removed auto re-review;
14. eleven-Calendar acceptance result;
15. SourceAuthority-change / grant ID / GrantAuthority / binding-revision regression result;
16. stale old dependency reauthorization result;
17. tests moved/rewritten/deleted;
18. residual audit with explicit 04/05/07 matches;
19. architecture docs updated;
20. commands with real pass/fail/skip outcomes;
21. native smoke result or skip reason;
22. checkpoint commit SHA(s);
23. clean worktree confirmation;
24. confirmation that checkpoint 04 was not started.
