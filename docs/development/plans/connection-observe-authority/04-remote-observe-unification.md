# 04: Remote Calendar into the generic remote View path

Prerequisite: 03 complete.

Status: Complete (2026-09-29).

Planning base: `main` at `b3f4bb06053c62cf7965e8f718b0e7d167d25f84` on 2026-09-28.

Checkpoint 03 completed the native Calendar logical-View vertical. A native EventKit connection now has one stable `calendar.timeline:<connection-id>` Expert selection and standing grant, while Context resolves every current Calendar resource at acquisition time and records the exact provider leaves in `source_resources`.

Remote Google/Microsoft Calendar still has the old parallel transport/grant/admission model:

~~~
hosted Calendar SourceConnection
  -> one Expert candidate per Calendar leaf
  -> RemoteCalendarGrantRequest / leaf grant
  -> calendar-specific signed preview
  -> calendar-specific Vault mapping
  -> RemoteCalendarViewRead
  -> Go CalendarAdmission / calendarAdmissionState / ReadCalendar
  -> one server connection scope calendar_id
~~~

Checkpoint 04 deletes that parallel architecture. Hosted Calendar becomes a normal remote `calendar.timeline` View:

~~~
SourceConnection / server connection
  current Calendar resource set
  SourceAuthority
        |
        +--> one Expert candidate
        |      calendar.timeline:<connection>
        |
        +--> one generic remote View grant
        |      calendar.timeline:<connection>
        |
        +--> generic signed View source preview
        |      current SourceAuthority
        |      exact source_resources = current Calendar IDs
        |
        +--> generic AdmitView / ReadView / Release
               Calendar runtime resolves current configured resources
               Context dependency records logical grant + exact leaves
~~~

This is a direct cutover. Do not preserve the Calendar-specific remote protocol as a compatibility path. Old local server/client development state is disposable.

Line numbers below are planning-base anchors on `b3f4bb06`. Re-resolve every symbol on the actual execution HEAD before editing.

## 1. Exit state

Checkpoint 04 is complete only when all of the following are true.

1. `calendar.timeline` is a canonical generic remote View alongside Mail, Work Context and Life Logistics.
2. A serving hosted Google or Microsoft Calendar connection exposes exactly one Calendar source candidate.
3. Hosted Calendar candidate resource is exactly `connection_view_resource("calendar.timeline", connection_id)`.
4. Hosted Calendar candidate ID and saved Expert binding stay unchanged when only the connection Calendar resource set changes.
5. Hosted Calendar Expert bindings never contain provider Calendar leaf IDs.
6. `RemoteCalendarViewRead` and `read_remote_calendar_view` are deleted.
7. Hosted Calendar Expert reads use `read_selected_remote_view` or the same generic remote View acquisition path used by other remote Views.
8. Generic remote View query validation understands `CalendarViewQuery`.
9. Generic remote View response validation understands `CalendarContextView` and checks it against the exact query.
10. Generic remote View category semantics support a canonical category set, not one category. Calendar uses `[Metadata, Content]`.
11. Generic remote standing grants for Calendar contain exactly one resource:
    `calendar.timeline:<connection-id>`.
12. Remote Calendar grant ID / GrantAuthority do not change solely because the server Calendar resource set changes.
13. `RemoteCalendarGrantRequest`, `RemoteCalendarGrantPreview`, `RemoteCalendarGrantReviewExpectation` and `RemoteCalendarSourceReference` are deleted.
14. `remote_calendar_scope`, `remote_calendar_source`, `preview_remote_calendar_grant`, `review_and_activate_remote_calendar_grant`, `admit_remote_calendar_read`, `remote_calendar_dependency_source_admits` and equivalent special Access helpers are deleted.
15. `crates/modules/access/src/application/remote_calendar.rs` is deleted.
16. `RemoteCalendarQuery` and `SignedCalendarPreview` are deleted.
17. `RemoteGrantTransport` exposes only generic View source preview for remote source grants.
18. `RemoteGrantStore` exposes only generic remote View grant/source operations; Calendar-specific methods are deleted.
19. `crates/adapters/vault/src/vault/remote_calendar_grants.rs` is deleted.
20. Calendar uses `remote_view_grant_mappings` only as the temporary pre-05 `ConsumerPolicyAuthority` record.
21. No Calendar-specific columns or duplicate mapping table are added to generic remote View storage.
22. Generic signed source preview carries:
    - logical View `resource`;
    - current `SourceAuthority`;
    - current connection revision;
    - exact canonical `source_resources`.
23. For hosted Calendar, signed preview `source_resources` equals the server connection's exact configured Calendar resource set.
24. For existing generic remote Views, signed preview `source_resources` is at least the canonical exact source resource already used by that View; the field is required rather than nullable migration state.
25. `RemoteViewSourceReference` stores canonical exact `source_resources`.
26. Remote dependency creation writes:
    - logical View into `ContextDependency.resources`;
    - signed current leaves into `ContextDependency.source_resources`;
    - signed current `SourceAuthority`.
27. Remote dependency reauthorization compares exact current `source_resources`, not only the logical View.
28. A hosted Calendar server resource edit advances producer connection revision / source epoch and stales old dependencies without changing the standing grant.
29. App remote Observe review/enable treats hosted Calendar exactly like another logical remote View member.
30. App derives hosted Calendar review member resource from `connection_view_resource`; no leaf Calendar resource is an Access review input.
31. `review_calendar_member`, `enable_calendar_member`, hosted `verified_remote_calendar_resource`, Calendar-specific remote pause/status branches and equivalent App special paths are deleted.
32. Remote inline review / interaction resolution uses generic remote View grant policy lookup for hosted Calendar.
33. The outer `RemoteAccess` product `resource: Option<String>` field may remain structurally until 07, but 04 core authorization never reads it as Calendar authority.
34. Same-snapshot Flutter callers pass no hosted Calendar Observe leaf `resource`; a non-`None` legacy leaf must not silently influence grant/review behavior.
35. Hosted Calendar source configuration is connection resource state, not Observe permission state.
36. Server Calendar connector scope is a canonical non-empty `calendar_ids` set, not one `calendar_id`.
37. Reordering the same server `calendar_ids` set is a semantic no-op and does not advance server source revision/epoch.
38. Adding/removing a server Calendar resource advances server connection revision and source epoch exactly once.
39. The server resource-set bound is a real encoded-byte/provider/protocol budget, not a restored small permission count cap such as 4.
40. The Rust local remote `SourceConnection.resources` mirrors the server Calendar resource set and advances its own local `SourceAuthority` on actual resource changes.
41. Server Google/Microsoft Calendar runtime reads the full current canonical `calendar_ids` set for one connection.
42. Provider leaf mechanics remain adapter-owned. One leaf client may still talk to one provider Calendar internally.
43. The connection-level Calendar runtime deterministically merges/pages those leaf clients into one `CalendarContextView`.
44. Multi-resource pagination uses one opaque bounded composite cursor tied to the current resource-set identity.
45. Provider event evidence handles cannot collide merely because two Calendars expose the same provider event ID.
46. Server `calendar.timeline` source preview uses generic `/v1/views/calendar.timeline/source-preview`.
47. Special `/v1/authority/calendar/source` is deleted.
48. Go `CalendarAdmission`, `calendarAdmissionState`, `AdmitCalendar`, `ReadCalendar` and the calendar admission map are deleted.
49. `calendar.timeline` goes through generic `ViewAdmission`, one generic admission map, generic `ReadView`, and generic release.
50. Generic `ViewAdmission.Resources` remains the one logical permission resource. Exact Calendar leaves are not copied into grant/admission resources.
51. The generic server read rechecks current connection revision/source epoch before provider I/O exactly as other Views do.
52. Calendar provider identity preflight remains fail-closed before a Calendar admission is issued.
53. Server scope change while Use with Floe is active does not automatically run another Observe review/enable.
54. Explicit connection completion may still perform the product's current first enable flow; 04 does not remove that user-initiated behavior.
55. `ConsumerPolicyAuthority`, remote View mapping policy epochs and expected-policy fields remain until checkpoint 05.
56. Final removal of the outer `resource` field, expected policy wire and other `ConnectionObserve` product DTO duplication remains checkpoint 07 work.
57. Exact-recipient model consent remains separate. The paired source server is transport/source identity, not a standing model-recipient permission.
58. Observe continues not to imply Act.
59. Current architecture docs describe hosted Calendar as generic remote View authorization with connection-owned source resources.
60. Required Rust, Go, Flutter, FFI, architecture, residual and broad verification gates pass.
61. Parent README marks 04 Complete and 05 remains Not started.

## 2. Checkpoint boundaries

### 2.1 What 04 completes

04 completes the remote Calendar authorization/acquisition vertical:

- one connection/View Expert candidate;
- one logical remote View grant;
- one generic signed source descriptor;
- one generic source/grant store port;
- one generic Context remote View read;
- one generic server admission/read/release route;
- one server connection-owned Calendar resource set;
- one provider runtime that resolves all current connection Calendars;
- exact physical provenance in `source_resources`.

After 04 there is no remote Calendar-specific authorization protocol.

### 2.2 What remains for 05

Do not delete or bypass:

- `ConsumerPolicyAuthority`;
- `consumer_policy`;
- `policy_incarnation`;
- `policy_epoch`;
- `remote_view_grant_mappings` while it still owns the temporary policy authority;
- `expected_policy`;
- policy authority checks in dependency/release paths.

Calendar must use the generic remote View policy record after 04. Checkpoint 05 then deletes the policy epoch/mapping duplication for every View at once.

### 2.3 What remains for 07

The following product-wire shapes may remain until 07:

- `RemoteAccessOperationDto::ConnectionObserve.resource`;
- `RemoteAccessOperationDto::ConnectionObserveReview.resource`;
- `RemoteAccessGateway.resource`;
- temporary `selected_resources` / `granted_resources`;
- expected-policy fields still present because 05 has not run yet.

However, 04 must make the outer Calendar leaf `resource` caller-zero as authority:

- Flutter same-snapshot calls pass `null`;
- App derives logical member resource itself;
- backend does not use a non-null leaf to find/review/activate a hosted Calendar grant.

Do not add a compatibility path that accepts both leaf and logical outer resource.

### 2.4 Connection source configuration is in scope now

Server `calendar_id` -> `calendar_ids` is not a 07 Observe UI cleanup. It is the source-owner correction required for a connection to actually own multiple current resources.

The Connection configuration UI may therefore be updated in 04 to represent the canonical set. The final Observe toggle/wire simplification still belongs to 07.

### 2.5 Exact-recipient consent remains separate

Remote source transport to the paired Floe server is not model-recipient consent.

Preserve current semantics:

- the standing remote source grant remains source-use permission under the current local processing restriction;
- producer `audience` remains signed source/review identity;
- model dispatch still requires its existing exact-recipient authority.

Do not make `calendar.timeline` integration a reason to broaden `ProcessingRestriction`.

## 3. Canonical remote Calendar path after 04

### 3.1 Expert/configuration identity

~~~
SourceSelectionReference
  connector_id        = calendar.google | calendar.microsoft
  connection_id       = SourceConnection.connection_id
  execution_owner_id  = paired server execution owner
  capability_id       = calendar.timeline
  resource            = calendar.timeline:<connection-id>
  contract_version    = 1
~~~

One source connection yields one candidate.

### 3.2 Standing remote grant

~~~
GrantSourceBinding
  stable Person / Connection / Connector / execution owner

GrantScope
  resources   = [calendar.timeline:<connection-id>]
  categories  = [metadata, content]
  operations  = [read]
  purposes    = [assistant]
  consumers   = trusted shipped Calendar consumers
  processing  = existing canonical remote-source processing restriction
~~~

No Calendar leaf IDs are in `GrantScope`.

### 3.3 Signed source preview

Generic descriptor semantics:

~~~
RemoteViewSourceReference
  view_id
  stable source identity
  connection_revision
  source_authority
  resource                # logical connection/View
  source_resources[]      # exact current physical/provider resources
  provider_identity
  producer/pairing identity
~~~

For hosted Calendar:

~~~
resource
  = calendar.timeline:<connection>

source_resources
  = sorted canonical server connection calendar_ids
~~~

The producer signature covers both.

### 3.4 Remote read

~~~
Expert/Manager selected logical View
  -> generic grant selection
  -> generic source preview
  -> exact current SourceAuthority/source_resources
  -> generic ViewAdmission
  -> Go ReadView
  -> CalendarRuntime reads current configured calendar_ids
  -> CalendarContextView
  -> client validates response/query
  -> ContextDependency
       resources        = logical View
       source_authority = signed current epoch
       source_resources = signed exact Calendar IDs
~~~

### 3.5 Resource edit

~~~
calendar_ids [A] -> [A,B]

server:
  connection revision      advances
  source epoch              advances

client Connections mirror:
  revision                  advances
  SourceAuthority           advances

standing grant:
  GrantId                   unchanged
  GrantAuthority            unchanged solely due source edit
  policy authority          unchanged solely due source edit

Expert:
  candidate/binding         unchanged

old dependency:
  stale

next read:
  signed source_resources   [A,B]
  provider runtime          reads A + B
~~~

## 4. Planning-base code map

All anchors below refer to `b3f4bb06`.

### 4.1 Hosted Calendar source candidates still leak leaf IDs

`crates/modules/context/src/application/source_candidates.rs`:

- `calendar.timeline` branch around lines 154-196.
- EventKit already uses `connection_view_resource`.
- hosted Google/Microsoft path still loops `for calendar in connection.resources()` around line 187.

03 intentionally left this for 04.

### 4.2 Hosted Expert read still loops selected leaves

`crates/app/src/vault_host/conversation_turn/expert_host.rs`:

- hosted Calendar branch around lines 1028-1100.
- validates `calendar.google` / `calendar.microsoft`.
- loops every selected `SourceSelectionReference`.
- calls `floe_context::read_remote_calendar_view`.
- one leaf ref currently produces one remote read.

After 04 there is one selected logical ref and one generic `read_selected_remote_view` call.

### 4.3 Context has a duplicate Calendar remote read

`crates/modules/context/src/application/remote_sources.rs`:

| Anchor | Current |
|---|---|
| ~60 | `RemoteCalendarViewRead` |
| ~73 | `read_remote_calendar_view` |
| ~117 / ~173 | Calendar-specific `calendar_grant_binding` |
| ~430 | generic one-source View read already exists |
| ~529 | `read_remote_view` |
| ~591 | `read_selected_remote_view` |
| ~716 | dependency reauthorization |
| ~771 | special Calendar reauthorization branch |

The generic path already performs preview -> verify -> binding -> read -> response validation -> dependency. 04 extends it for Calendar and deletes the duplicate function.

### 4.4 Generic remote View registry does not include Calendar

`crates/modules/context/src/application/remote_views.rs`:

- ~21 `MAIL_VIEW`.
- ~26 `is_remote_view` excludes Calendar.
- ~34 `remote_view_data_category` returns one category.
- ~53 `validate_remote_view_query`.
- ~81 `validate_remote_view`.
- ~117 `merge_remote_views`.
- ~300 `remote_view_dependency`.

Calendar needs:
- canonical View membership;
- Calendar query validation;
- Calendar result validation;
- exact `[Metadata, Content]` categories.

### 4.5 Generic Access request assumes one category

`crates/modules/access/src/application/remote_grants.rs`:

- `RemoteViewGrantRequest` has one `data_category`.
- `prepare_remote_view_grant_activation` passes it to `remote_view_scope`.

`crates/modules/access/src/application/remote_view.rs`:

- `remote_view_scope` takes one category.
- despite receiving `recipient`, the current standing scope is `LocalOnly`.
- `RemoteViewSourceReference` has one logical `resource`, no exact source-resource set.
- `remote_dependency_source_admits` assumes exactly one `source_resources` element equal to `reference.resource`.

04 must generalize categories and provenance without changing model recipient authority.

### 4.6 Access port duplicates Calendar protocol

`crates/modules/access/src/ports/remote_grants.rs`:

- ~29 `SignedSourcePreview`.
- ~40 `SignedCalendarPreview`.
- ~48 `RemoteCalendarQuery`.
- ~56 `RemoteSourceQuery`.
- ~79 `RemoteGrantTransport` has both generic and Calendar preview.
- ~99 `RemoteGrantStore` has generic View methods plus Calendar-specific verification/activation/find/policy/grant/pause/binding.
- ~180 generic `view_grant_binding`.
- ~187 Calendar `calendar_grant_binding`.

### 4.7 Calendar-specific Access application

`crates/modules/access/src/application/remote_calendar.rs`:

- ~37 `RemoteCalendarSourceReference`.
- ~62 `RemoteCalendarGrantRequest`.
- ~161 `remote_calendar_scope`.
- ~176 `admit_remote_calendar_read`.
- ~222 `preview_remote_calendar_grant`.
- ~274 `review_and_activate_remote_calendar_grant`.

Delete the file after caller migration.

### 4.8 Vault has parallel Calendar grant storage code

`crates/adapters/vault/src/vault/remote_calendar_grants.rs`:

- line ~11 `RemoteCalendarGrantBinding`.
- ~17 `remote_calendar_grant_binding`.
- ~66 `find_remote_calendar_grant`.
- ~116 `review_and_activate_remote_calendar_grant`.
- large Calendar-only test suite.

`crates/adapters/vault/src/vault/remote_view_grants.rs` already owns:
- `find_remote_view_grant`;
- `remote_view_grant_binding`;
- `remote_view_grant_policy`;
- `review_and_activate_remote_view_grant`;
- temporary `ConsumerPolicyAuthority` mapping.

Calendar should use this file after 04.

### 4.9 Remote authority still verifies two descriptor formats

`crates/adapters/vault/src/vault/remote_authority.rs`:

- ~31 `CalendarSourcePreviewWire`.
- ~55 `ViewSourcePreviewWire`.
- ~141 `verify_remote_view_source_preview`.
- ~498 `verify_remote_calendar_source_preview`.
- `MAX_PRODUCER_PROOF_BYTES` currently 4 KiB.

The generic descriptor must become the one signed source proof and carry exact `source_resources`.

### 4.10 Provider control has special Calendar preview endpoint

`crates/adapters/providers/src/control/authorization.rs`:

- `CalendarSourcePreviewResponse` near line 48.
- `RemoteViewSourcePreviewResponse` near line 58.
- `calendar_source_preview` around 388 -> `/v1/authority/calendar/source`.
- `view_source_preview` around 428 already permits `calendar.timeline`.
- `RemoteGrantTransport` impl has both methods around 1777/1803.

`crates/adapters/providers/src/sources/server.rs` repeats both transport methods around 828/856.

### 4.11 Generic provider admitted read is already reusable

`crates/adapters/providers/src/sources/server.rs`:

- `ServerSourceClient::read_admitted_view` around line 133 already posts to `/v1/views/{view}/admit`.
- `AuthorizedSourceClient::read_admitted_view` around line 886 is generic.

The live challenge expectation is named `RemoteCalendarAuthorizationExpectation` even though it is already used by generic View reads. This is stale protocol naming.

### 4.12 App remote Observe is generic with a Calendar branch

`crates/app/src/vault_host/remote_observe.rs`:

- generic review/enable bundle starts around line 112.
- lines ~197-250 detect Calendar and branch.
- disable around ~294 uses `pause_remote_calendar_grant`.
- `review_member` ~382 branches to `review_calendar_member`.
- `review_calendar_member` ~467.
- `enable_calendar_member` ~570.
- `RemoteObserveContext.resource` is the outer legacy leaf resource.

After 04 Calendar should run through generic member preparation and atomic remote View activation.

### 4.13 Review/interaction code still verifies a Calendar leaf

`crates/app/src/vault_host/review_snapshot.rs`:

- remote member capture around ~377 has Calendar special resource logic.
- `verified_remote_calendar_resource` around ~541.
- remote Calendar policy authority reads Calendar-specific policy.

`crates/app/src/vault_host/interaction_owners.rs`:
- hosted Calendar branches around ~443 and ~1250.
- policy lookup can select Calendar-specific remote policy path.

These become generic remote View paths. Native Calendar remains separate.

### 4.14 Rust Connections already supports a remote resource set

`crates/app/src/connection_services.rs`:

- `RemoteCalendarSourceMutation::Bind` already accepts `Vec<ConnectionResource>`.
- `SourceConnection` supports canonical resource sets and SourceAuthority.
- protocol `RemoteCalendarSourceMutationDto` already accepts multiple resources.

No new Rust owner is needed.

### 4.15 Flutter binds only one server Calendar resource

`apps/client/lib/features/connections/presentation/connector_screen.dart`:

- `_bindServerCalendar` around lines 300+ reads `selected.scope['calendar_id']`.
- requires one Calendar ID.
- binds local `RemoteCalendarSource` with one resource.

This must consume canonical server `calendar_ids`.

### 4.16 Server connector source scope is one leaf

`server/internal/connections/connectors.go`:

- Calendar definitions lines ~25-26 declare `ScopeFields: ["calendar_id"]`.
- Calendar scope validation lines ~170-181 requires one string.

`server/internal/application/config.go`:
- Google around 84 creates one leaf client from `scope["calendar_id"]`.
- Microsoft around 97 same.

`updateConnectorScope` in `server/internal/application/connectors_operations.go` around 389-442 already:
- canonical-validates scope;
- preserves revision/epoch on exact semantic no-op;
- increments both revision and epoch on actual changed scope;
- rebuilds runtime and durably saves.

That source-authority behavior should be reused for `calendar_ids`.

### 4.17 Provider Calendar client is one-leaf

Google `server/internal/connectors/googlecalendar/client.go`:
- Client owns one `calendarID`.
- `Calendar` around line 88 reads one provider Calendar.
- source handle includes connection+calendar.
- event evidence handle currently includes connection+event, not Calendar ID.

Microsoft has the same shape.

This leaf client is legitimate provider mechanics. The connection-level Service should aggregate multiple leaf clients.

### 4.18 Go authorization has two admission stacks

`server/internal/authorization/admissions.go`:

~~~
calendar   map[string]calendarAdmissionState
remoteView map[string]remoteViewAdmissionState
~~~

`source_service.go`:
- `CalendarAdmission` / `calendarAdmissionState`.
- `AdmitCalendar` around ~317.
- `ReadCalendar` around ~396.
- generic `PreviewView` around ~482.
- generic `AdmitView` ~533.
- generic `ReadView` ~589.
- `PreviewCalendar` remains later in file as the old special preview route.

`server/internal/transport/http/source.go` branches Calendar to `AdmitCalendar` / `ReadCalendar`.

`server/internal/transport/http/console.go` exposes `/v1/authority/calendar/source`.

### 4.19 Generic View admission is already close to the target

Generic `ViewAdmission` already carries:
- connector/connection;
- connection revision;
- logical `resources`;
- temporary policy authority;
- grant authority;
- purpose/consumer;
- bounds/query.

`AdmitView` already requires:
`resources == [remoteViewResource(viewID, connectionID)]`.

Calendar can reuse it after:
- connector/query validation;
- provider identity preflight;
- Calendar runtime dispatch.

### 4.20 Flutter server scope update currently re-reviews Observe

`apps/client/lib/features/connections/presentation/server_connector_panel.dart`:

- `_updateScope` reads current Observe status.
- after updating source scope, if active, calls `_setConnectionObserve(true)`.
- `_observeResource` returns Calendar `calendar_id`.
- `_setConnectionObserve` passes that resource to review/enable.

This violates the final resource-change invariant and must be corrected in 04 for generic remote source semantics. Initial explicit connection completion may still enable Observe.

## 5. 04-A — baseline and exact residual inventory

Before production edits:

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
docs/development/plans/connection-observe-authority/04-remote-observe-unification.md
docs/architecture/invariants.md
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
server/README.md
```

Capture pre-edit residuals:

~~~
rg -n "RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference" crates
rg -n "remote_calendar_scope|preview_remote_calendar_grant|review_and_activate_remote_calendar_grant|admit_remote_calendar_read" crates
rg -n "calendar_source_preview|verify_calendar_source_preview" crates server
rg -n "activate_calendar_grant|find_calendar_grant|calendar_grant_binding|pause_calendar_grant" crates
rg -n "remote_calendar_grants" crates
rg -n "RemoteCalendarViewRead|read_remote_calendar_view" crates
rg -n "for calendar in connection\.resources\(\)" crates/modules/context crates/app
rg -n "calendar_id|calendar_ids" server/internal apps/client/lib/features/connections
rg -n "CalendarAdmission|calendarAdmissionState|AdmitCalendar|ReadCalendar" server
rg -n "/v1/authority/calendar/source" .
rg -n "decodeCalendarEnvelope|decodeCalendarProof" server
rg -n "RemoteCalendarAuthorizationExpectation|CalendarChallengeParts|parse_calendar_challenge" crates
```

Classify each result into:
- delete in 04;
- legitimate Calendar domain/query/provider leaf mechanic;
- temporary 05 policy state;
- temporary 07 product-wire field.

Do not start implementation until every production special-authority result has an owner.

## 6. 04-B — make Calendar a canonical generic remote View

Modify:

~~~
crates/modules/context/src/application/remote_views.rs
crates/modules/context/src/lib.rs
```

### View membership

Add:

~~~
CALENDAR_CONTEXT_VIEW_ID
```

to generic `is_remote_view`.

Prefer importing the canonical constant from context contract instead of defining another string.

### Data categories

Replace singular:

~~~
remote_view_data_category(view_id) -> GrantDataCategory
```

with an exact category-set helper, for example:

~~~
remote_view_data_categories(view_id) -> &'static [GrantDataCategory]
```

or a canonical owned vector where needed.

Required values:

~~~
mail.communication   -> [Content]
work.context         -> [Derived]
life.logistics       -> [Derived]
calendar.timeline    -> [Metadata, Content]
```

Use the same exact categories in:
- grant review;
- source classification;
- blockers;
- dependency validation.

Do not reduce Calendar to Content-only just to fit the old API.

### Calendar query validation

Extend `validate_remote_view_query`:

- deserialize `CalendarViewQuery`;
- call its canonical `validate()`;
- return Calendar's exact item/byte bounds:
  `MAX_CALENDAR_CONTEXT_ITEMS`, `MAX_CALENDAR_CONTEXT_BYTES`.

Do not duplicate range/cursor rules in Context remote code.

### Calendar answer validation

Extend the generic response validation path to:
- deserialize `CalendarContextView`;
- call `validate_calendar_context_view_for_query`;
- preserve exact query range/cursor semantics;
- return canonical JSON value and observed/expires times.

If the current `validate_remote_view` signature lacks the query, change the generic validation API rather than adding a second Calendar validator branch in `remote_sources.rs`.

### Merge behavior

The product currently owns at most one current remote Calendar source. 04 does not need speculative multi-account Calendar merge.

For `merge_remote_views`:
- one Calendar source returns the validated Calendar view;
- if generic machinery can produce multiple Calendar sources despite the product invariant, fail closed unless a fully deterministic bounded merge is actually required by a live caller.

Do not introduce a second cross-account pagination scheme merely for theoretical future accounts.

## 7. 04-C — generalize remote View grant scope without broadening processing

Modify:

~~~
crates/modules/access/src/application/remote_view.rs
crates/modules/access/src/application/remote_grants.rs
```

### Multi-category scope

Change:

~~~
RemoteViewGrantRequest.data_category
```

to an exact canonical category set.

Change:

~~~
remote_view_scope(resource, category, consumers, ...)
```

to use that exact set.

Validate:
- non-empty canonical categories;
- logical resource;
- canonical consumers;
- Read;
- Assistant purpose.

### Processing boundary

Preserve the current standing remote-source processing semantics.

The paired source server's producer `audience` is:
- signed source/transport identity;
- review drift evidence;
- admission/release cryptographic identity.

It is not model-recipient consent.

Do not turn the source grant into an external-model processing grant.

If `remote_view_scope` still receives `recipient` only to test non-empty while storing `LocalOnly`, remove that unused coupling and fix the misleading comment. Keep producer audience comparison in `RemoteViewApproval` / signed review expectation.

`SourceProcessingPolicy::PairedSourceRecipient` may remain as existing App review-fingerprint input until 05. Do not use 04 to redesign the final policy digest.

### Calendar request

App passes:

~~~
view_id = calendar.timeline
resource = calendar.timeline:<connection>
categories = [Metadata, Content]
consumers = calendar_policy().consumers
```

through the same `RemoteViewGrantRequest`.

## 8. 04-D — make generic signed View preview carry exact source resources

Modify:

~~~
crates/modules/access/src/application/remote_view.rs
crates/modules/access/src/ports/remote_grants.rs
crates/adapters/vault/src/vault/remote_authority.rs
crates/adapters/providers/src/control/authorization.rs
crates/adapters/providers/src/sources/server.rs
server/internal/authorization/source_service.go
```

### `RemoteViewSourceReference`

Add required canonical exact source resources, preferably:

~~~
source_resources: Vec<ResourceHandle>
```

Do not use an optional field.

Add an accessor only if actual callers need it.

### Generic signed descriptor

`ViewSourcePreviewWire` / Go `PreviewView` descriptor contains:

~~~
resource
source_resources
```

Both are signed.

For non-Calendar views on this checkpoint, use the exact existing physical source representation. Where the current path has no distinct provider leaf model, `[resource]` is acceptable and preserves current semantics.

For Calendar:

~~~
resource
  = calendar.timeline:<connection-id>

source_resources
  = current record.Scope.calendar_ids
```

### Canonical validation

Rust verifier rejects:
- empty source resources;
- malformed handles;
- unsorted order;
- duplicates;
- over-budget encoded proof.

Server scope storage is canonical, so the producer must sign canonical order directly.

### Proof budget

The current producer proof cap is 4 KiB and is too small for a real multi-resource Calendar connection.

Introduce/adjust an explicit encoded proof/resource-set byte budget derived from:
- `RemoteViewSourcePreviewResponse` transport max;
- base64 expansion;
- dependency 64 KiB max;
- per-resource 256-byte bound.

Do not reintroduce a small semantic Calendar count limit such as 4.

The accepted server Calendar source scope must be guaranteed to fit:
1. the signed descriptor;
2. the HTTP response;
3. `ContextDependency.source_resources`.

A named serialized-byte budget is preferred. If implementation measurements justify a bounded count in addition to bytes, document it as a protocol/storage budget rather than permission semantics and keep it far above the former 4-calendar defect. Eleven and >4 must remain explicitly tested.

### Remove special descriptor

Delete:
- `CalendarSourcePreviewWire`;
- `verify_remote_calendar_source_preview`;
- `CalendarSourcePreviewResponse`;
- `RemoteGrantTransport::calendar_source_preview`.

## 9. 04-E — delete Calendar-specific Access remote grant API

Modify:

~~~
crates/modules/access/src/ports/remote_grants.rs
crates/modules/access/src/application/mod.rs
crates/modules/access/src/lib.rs
```

Delete:
- `RemoteCalendarQuery`;
- `SignedCalendarPreview`;
- Calendar source reference import;
- `verify_calendar_source_preview`;
- `activate_calendar_grant`;
- `find_calendar_grant`;
- `calendar_grant_policy`;
- `calendar_grant`;
- `pause_calendar_grant`;
- `calendar_grant_binding`.

The remaining `RemoteGrantStore` API must describe generic remote Views only.

Update test doubles in:
- Access;
- Context;
- App;
- provider adapter tests.

Do not retain default trait methods returning `CapabilityUnavailable`; that is a compatibility surface.

## 10. 04-F — delete `remote_calendar.rs`

Migrate callers first, then delete:

~~~
crates/modules/access/src/application/remote_calendar.rs
```

Delete its exports:
- `REMOTE_CALENDAR_RECIPIENT`;
- `RemoteCalendarConnection`;
- `RemoteCalendarGrantPreview`;
- `RemoteCalendarGrantRequest`;
- `RemoteCalendarGrantReviewExpectation`;
- `RemoteCalendarSourceReference`;
- `hosted_calendar_connector` if no non-legacy caller remains;
- `remote_calendar_scope`;
- `remote_calendar_source`;
- `preview_remote_calendar_grant`;
- `review_and_activate_remote_calendar_grant`;
- `pause_remote_calendar_grant`;
- `remote_calendar_dependency_source_admits`;
- `admit_remote_calendar_read`.

Provider/connector mapping that still has real domain value should move to Connections/Context/App product ownership rather than leaving a hollow Access Calendar module.

No forwarding wrapper file.

## 11. 04-G — move Calendar onto generic Vault remote View mapping

Modify:

~~~
crates/adapters/vault/src/repositories/remote_grants.rs
crates/adapters/vault/src/vault.rs
crates/adapters/vault/src/vault/remote_view_grants.rs
```

Delete:

~~~
crates/adapters/vault/src/vault/remote_calendar_grants.rs
```

### Generic storage

Calendar uses:

~~~
find_remote_view_grant(
  "calendar.timeline",
  stable_source,
  consumer
)

remote_view_grant_binding(
  "calendar.timeline",
  connector,
  connection
)

remote_view_grant_policy(...)
review_and_activate_remote_view_grant(...)
pause_remote_view_grant(...)
```

### Mapping semantics

`remote_view_grant_mappings` retains temporary:
- `policy_incarnation`;
- `policy_epoch`.

No Calendar-specific mapping columns/table are added.

### Ambiguity

Exactly one grant mapping is allowed for:
- Person;
- View;
- stable connector/connection/execution owner.

A legacy leaf Calendar grant/mapping in a disposable local profile is not translated. Fresh profile is the supported execution baseline when schema meaning/path changes.

### Tests to port

Move useful semantics from the deleted Calendar Vault tests into generic remote View tests:
- exact stable source;
- logical Calendar View resource;
- pause/reactivate;
- stale GrantAuthority conflict;
- ConsumerPolicyAuthority behavior until 05;
- duplicate mapping conflict;
- reopen/corruption behavior.

Delete sibling-leaf-grant tests because sibling leaves are source resources, not separate grants.

## 12. 04-H — Context generic Calendar read and reauthorization

Modify:

~~~
crates/modules/context/src/application/remote_sources.rs
crates/modules/context/src/application/remote_views.rs
crates/modules/context/src/lib.rs
```

### Delete special read

Delete:
- `RemoteCalendarViewRead`;
- `read_remote_calendar_view`;
- its public exports.

### Generic read

Hosted Calendar uses `read_selected_remote_view`.

Input selection:
- one logical `SourceSelectionReference`;
- resource `calendar.timeline:<connection>`.

Query:
- serialize `CalendarViewQuery` through its canonical serde shape.

Result:
- generic remote View path returns JSON + bindings;
- Calendar caller decodes `CalendarContextView`.

### `read_one_remote_source`

Use signed reference:

~~~
reference.resource
reference.source_resources
reference.source_authority
```

Construct dependency:

~~~
resources
  = [logical calendar.timeline:<connection>]

source_authority
  = signed source authority

source_resources
  = exact signed Calendar IDs
```

Change `remote_view_dependency` to accept a canonical resource slice instead of one `source_resource: &str`.

Existing non-Calendar callers pass the generic signed resource vector.

### Reauthorization

Remove connector-based Calendar branch from `authorize_remote_dependency`.

Generic flow:
1. validate stored dependency;
2. load grant;
3. recover logical grant resource;
4. parse View ID from connection resource;
5. require `is_remote_view(view_id)`;
6. fetch generic signed preview;
7. verify pinned producer/source;
8. compare:
   - stable source identity;
   - connection revision;
   - `SourceAuthority`;
   - exact signed `source_resources`;
   - approved source transport recipient where current processing requires it;
9. get generic `view_grant_binding`;
10. compare grant/policy authority.

Delete `remote_calendar_dependency_source_admits`.

### Source resource drift

A changed server Calendar resource set must fail old dependency reauthorization even if the old dependency has not expired.

This is independent from the grant staying active.

## 13. 04-I — hosted Calendar candidate becomes connection/View-level

Modify:

~~~
crates/modules/context/src/application/source_candidates.rs
crates/app/src/vault_host/expert_binding_settings.rs
related source-candidate tests
```

For both:
- `calendar.google`;
- `calendar.microsoft`;

emit exactly one candidate:

~~~
resource = connection_view_resource("calendar.timeline", connection_id)
```

Remove the hosted `for calendar in connection.resources()` loop.

`calendar.fixture`:
- if used as a connection-level test/provider source, use the same logical View candidate;
- do not preserve leaf candidate semantics solely for fixture compatibility.

After 04, production Calendar source candidate code must not loop leaves.

### Binding regression

Add:
1. hosted connection resources `[A]` -> candidate X;
2. `[A,B]` -> same X;
3. binding revision unchanged;
4. candidate ID unchanged;
5. connector/connection replacement -> different candidate;
6. leaf resource cannot be saved as a current hosted Calendar candidate.

## 14. 04-J — hosted Expert read uses generic selected View

Modify:

~~~
crates/app/src/vault_host/conversation_turn/expert_host.rs
relevant Expert host / registered runner tests
```

### Selection

Hosted Calendar requires:
- exactly one selected ref for the requirement;
- connector Google or Microsoft;
- exact current connection;
- paired server execution owner;
- resource == `calendar.timeline:<connection>`;
- contract version 1.

Do not require selected resource to be a Connection leaf.

### Read

Replace per-selected-leaf loop + `read_remote_calendar_view` with one generic:

~~~
read_selected_remote_view(
  view_id = "calendar.timeline",
  selected = [logical ref],
  query = serde_json::to_value(CalendarViewQuery),
  ...
)
```

Decode the ready Value into `CalendarContextView` and preserve every returned dependency.

A blocked source still produces the existing typed SourceAccess requirement.

### Actions

Existing 03 Action path already treats selected resource as logical permission and destination Calendar as physical `source_resources`. Preserve it. Do not broaden action destination.

## 15. 04-K — make App remote Observe fully generic for hosted Calendar

Modify:

~~~
crates/app/src/vault_host/remote_observe.rs
crates/app/src/first_party_observe.rs
```

### Resource derivation

For every policy View, including Calendar:

~~~
resource = connection_view_resource(policy.view_id, connection_id)
```

No `ctx.resource` leaf.

### Review

Delete:
- Calendar detection branch based on resource presence;
- `review_calendar_member`;
- local Calendar leaf evidence lookup from grant review;
- special Calendar preview call.

Use the same `RemoteViewGrantRequest` and generic source preview for Calendar.

Calendar categories come from `policy.categories`.

### Enable

Delete:
- `enable_calendar_member`;
- special remote Calendar grant review call.

Calendar participates in the same prepared activation list / atomic generic activation.

### Disable/status

Delete:
- `pause_remote_calendar_grant`;
- special expected-view Calendar matching that accepts arbitrary leaf grant resource.

Generic logical resource matching/pause/revoke covers Calendar.

### `remote_policies_for_target`

Remove the unused `calendar_resource` target parameter and any target wrapper whose only remaining purpose was leaf Calendar policy composition.

Keep trusted shipped consumer behavior from 03.

### Outer resource field

`RemoteObserveContext.resource` may remain structurally only if needed by the 07 wire bridge, but:
- 04 authorization code does not read it;
- same-snapshot product callers pass `None`;
- optionally reject non-None in App owner validation to guarantee no leaf compatibility path.

Do not reformat a supplied leaf into a logical resource.

## 16. 04-L — remote inline review/interaction convergence

Modify:

~~~
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_owners.rs
relevant interaction resolution tests
```

### Capture

Delete hosted:
- `verified_remote_calendar_resource`;
- one-leaf source verification;
- Calendar-specific remote policy authority lookup.

One remote member always uses:
- policy View ID;
- `connection_view_resource`;
- generic signed preview;
- current generic remote View grant expectation/policy.

### Live refresh

Hosted Calendar follows the same generic remote member revalidation as other Views:
- pinned producer;
- signed current source;
- connection revision;
- provider identity;
- source authority;
- exact source resources;
- grant;
- temporary consumer policy.

No leaf resource is part of review target identity.

### Native distinction

Do not touch the native Calendar one-member review path except shared helper cleanup. Native uses Connections/native subject, not remote producer transport.

## 17. 04-M — provider control/client special Calendar preview deletion

Modify:

~~~
crates/adapters/providers/src/control/authorization.rs
crates/adapters/providers/src/control/mod.rs
crates/adapters/providers/src/sources/server.rs
```

Delete:
- `CalendarSourcePreviewResponse`;
- `RemoteAuthorizationClient::calendar_source_preview`;
- `RemoteGrantTransport::calendar_source_preview` implementations;
- special `/v1/authority/calendar/source` call.

Generic `view_source_preview` handles `calendar.timeline`.

### Generic proof naming

Audit generic remote View authorization helpers currently carrying Calendar names.

Where a type/function already serves all Views, rename directly, for example:

~~~
RemoteCalendarAuthorizationExpectation
  -> RemoteViewAuthorizationExpectation

CalendarChallengeResponse
  -> RemoteViewChallengeResponse

CalendarChallengeParts
  -> RemoteViewChallengeParts

parse_calendar_challenge
  -> parse_remote_view_challenge
```

Only rename when the symbol is actually transport-generic. Keep legitimate Calendar query/domain utilities named Calendar.

No deprecated type aliases.

`calendar_query_sha256` may remain only if it is a real Calendar query utility with callers; delete it if caller-zero.

### Source preview response

Generic response includes current `source_resources`. Validate before returning to Access.

## 18. 04-N — server connection scope becomes canonical `calendar_ids`

Modify:

~~~
server/internal/connections/connectors.go
server/internal/application/config.go
server/internal/application/connectors_operations.go tests as needed
connector catalog/fixtures
```

### Connector definitions

Replace:

~~~
calendar_id
```

with:

~~~
calendar_ids
```

for Google and Microsoft Calendar scope.

Old local server scope is disposable. No fallback accepting both names.

### Scope validation

Accept a non-empty list.

For each Calendar ID:
- string only;
- trimmed;
- non-empty;
- no control characters;
- provider-safe opaque length;
- no wildcard semantics.

Canonicalize:
- sort;
- reject duplicate IDs;
- persist canonical list.

An input order change alone must yield the same canonical scope and therefore be a semantic no-op.

### Budget

Use an explicit serialized-byte budget compatible with:
- signed source descriptor;
- HTTP source preview;
- Rust dependency provenance.

Do not make `len(calendar_ids) <= 4` or another tiny count a permission invariant.

Keep existing broad connection resource safety bounds only where they fit the transport byte budget.

### Source authority

Reuse existing `updateConnectorScope` behavior:
- exact canonical no-op -> revision/epoch unchanged;
- actual set change -> revision++ and epoch++ once.

Add tests for reorder/no-op and add/remove advancement.

## 19. 04-O — connection-level provider runtime owns many leaf Calendars

Modify:

~~~
server/internal/connections/ports.go
server/internal/application/config.go
server/internal/connectors/googlecalendar/client.go
server/internal/connectors/googlecalendar/service.go
server/internal/connectors/microsoftcalendar/client.go
server/internal/connectors/microsoftcalendar/service.go
related connector tests
```

### Keep leaf mechanics leaf-local

It is valid for one provider leaf client to own one Calendar ID internally.

Do not move provider Calendar IDs into generic Connections contracts beyond the connection resource list.

### Build one runtime per connection

For each canonical `calendar_ids` entry:
- build a leaf Calendar client using the same credential and connection ID;
- compose them into one Calendar service/runtime.

The runtime holds the canonical leaf order.

### Composite cursor

One connection-level opaque cursor must page across multiple leaf clients.

Recommended semantics:

~~~
version
resource_set_digest
resource_index
provider_cursor
```

Encode in a bounded URL-safe representation.

Validate:
- exact version;
- digest == current canonical resource-set digest;
- index in range;
- provider cursor remains within existing leaf bounds;
- encoded cursor <= Calendar query cursor max.

Do not expose provider secrets/tokens.

### Paging algorithm

For a request `(range, cursor, limit)`:

1. resolve starting resource index/provider cursor;
2. read current leaf with remaining item budget;
3. append validated items;
4. if leaf has provider next cursor:
   - return composite cursor for same resource;
   - `coverage_complete = false`;
5. if leaf completes and remaining budget exists:
   - advance to next resource;
   - continue;
6. if item limit is reached exactly at a leaf boundary while later resources remain:
   - return composite cursor for next resource with empty provider cursor;
7. only when every configured resource is exhausted:
   - no next cursor;
   - `coverage_complete = true`.

This avoids an unbounded cross-resource buffer.

### Determinism

Canonical resource order makes pagination deterministic.

`CalendarContextView` does not require global start-time sorting across provider leaves. Do not invent an unbounded merge/sort buffer.

If product semantics later require globally ordered cross-calendar pagination, design it explicitly; it is not necessary for this permission unification.

### Source handle

Aggregate view `source_handle` is connection/resource-set scoped, not one leaf.

Use an opaque digest/handle; do not leak the raw Calendar ID set unnecessarily.

### Evidence handles

Current event evidence handles can collide if two Calendar leaves expose the same provider event ID.

Include the Calendar ID in the opaque evidence-handle input:

~~~
connection_id + calendar_id + provider_event_id
```

Do not expose raw provider IDs in the output.

### Result bounds

Final aggregate Calendar view must still satisfy:
- max items;
- max serialized bytes;
- freshness;
- query range;
- unique evidence handles;
- cursor bounds.

## 20. 04-P — generic server View source preview

Modify:

~~~
server/internal/authorization/source_service.go
server/internal/transport/http/source.go
server/internal/transport/http/source_decode.go
server/internal/transport/http/console.go
```

### `PreviewView`

Require for Calendar too:

~~~
input.resource == remoteViewResource(viewID, connectionID)
```

Delete the current Calendar exception.

Validate connector compatibility:
- Calendar -> `calendar.google | calendar.microsoft`;
- Mail/Work/Logistics retain their current connector validation.

For Calendar descriptor:

~~~
source_resources = canonical record.Scope.calendar_ids
```

For other Views use their canonical current source representation.

Descriptor includes:
- view ID;
- logical resource;
- exact source resources;
- connection revision;
- source incarnation/epoch;
- provider identity;
- producer/pairing fields.

### Delete special preview

Delete:
- `PreviewCalendar`;
- `/v1/authority/calendar/source` handler from `console.go`;
- `calendar_source_preview` operation descriptor.

### HTTP decoder naming

`decodeCalendarEnvelope` / `decodeCalendarProof` are already generic `/v1/views` transport helpers.

Rename to source/view-generic names if they remain live, such as:
- `decodeSourceEnvelope`;
- `decodeSourceProof`.

Keep Calendar field/query validators only where they validate Calendar domain payloads.

## 21. 04-Q — generic server View admission/read handles Calendar

Modify:

~~~
server/internal/authorization/admissions.go
server/internal/authorization/source_service.go
server/internal/transport/http/source.go
```

### Admission state

Delete:
- `calendar map`;
- `CalendarAdmission` methods;
- `calendarAdmissionState`;
- `deleteCalendarAdmission`.

Use only `remoteViewAdmissionState`.

Ensure it stores enough canonical data:
- view/path;
- exact query bytes;
- principal;
- connector;
- connection;
- expected connection revision;
- expiry.

### `AdmitView`

Change signature to receive `context.Context` if needed for Calendar provider identity preflight.

For `calendar.timeline`:
- connector must be Google/Microsoft Calendar;
- parse/validate `CalendarViewQuery` with existing exact bounds;
- require logical resource;
- load current connection;
- require exact connection revision / source epoch / provider identity state;
- call current Calendar provider identity preflight before admission;
- issue generic admission challenge.

`Request.Resources` remains:

~~~
[calendar.timeline:<connection>]
```

It never contains Calendar IDs.

### `ReadView`

Before provider I/O:
- claim exact admission;
- match generic admission state;
- verify query digest;
- reload current record;
- require same connector/person/revision/source epoch/provider identity;
- fail if resource set changed because revision/epoch changed.

For Calendar:
- dispatch to the one `CalendarRuntime` for the connection;
- runtime internally reads the configured `calendar_ids`;
- bound result;
- stage generic release.

### Delete special admission/read

Delete:
- `AdmitCalendar`;
- `ReadCalendar`;
- `calendarRecord`;
- `calendarRecordForRequest`;
- special calendar admission-capacity loop/state.

### HTTP route

`server/internal/transport/http/source.go` becomes branch-free by authorization transport:

~~~
POST /v1/views/{view}/source-preview -> PreviewView
POST /v1/views/{view}/admit          -> AdmitView
POST /v1/views/{view}/read           -> ReadView
POST /v1/views/{view}/release        -> Release
```

Calendar domain dispatch stays behind `ReadView`, not in HTTP.

Retain an availability/not-found check only if it represents a real missing connector runtime; do not create a Calendar-specific authorization route.

## 22. 04-R — local Rust remote SourceConnection mirrors server resource set

Modify:

~~~
apps/client/lib/features/connections/presentation/connector_screen.dart
apps/client/lib/features/connections/domain/*
apps/client/lib/features/connections/application/*
related protocol/App tests as needed
```

Current `_bindServerCalendar` reads one `scope['calendar_id']`.

Change to the canonical server `calendar_ids` list.

Build one `ConnectionResource` per ID.

Because server catalog currently may not expose per-Calendar display names, use a bounded honest label derived from available metadata. Prefer the ID itself over inventing a provider title that suggests a different resource.

Bind/configure the existing Rust remote Calendar `SourceConnection` with the full set.

The App source service already owns:
- local expected-revision CAS;
- sorted resource set;
- local SourceAuthority advancement.

No new owner path.

### Synchronization acceptance

When server catalog changes `[A] -> [A,B]`:
- local `SourceConnection.resources` becomes A+B;
- local source revision/SourceAuthority advances once;
- existing logical Expert binding remains;
- existing logical remote grant remains;
- next signed source preview reports A+B.

## 23. 04-S — connection configuration UI supports multiple Calendar IDs

Modify minimally:

~~~
apps/client/lib/features/connections/presentation/server_connector_panel.dart
server connector UI tests
```

The server connector definition exposes `calendar_ids`.

Represent it as a resource-set input, not an Observe permission picker.

A simple bounded multi-line UI is acceptable:
- one Calendar ID per line;
- IDs themselves cannot contain control/newline characters;
- trim blank lines;
- preserve no duplicates;
- server remains authoritative for canonical sort/validation.

Do not use comma-separated parsing for opaque provider IDs if commas are valid provider characters.

Display existing list as one ID per line.

Update `_scopeComplete` and `_scopeValue` so `calendar_ids` sends a JSON list, not one string.

This is source connection configuration. It is not a second Floe grant resource selection list.

## 24. 04-T — stop resource edits from re-reviewing remote Observe

Modify:

~~~
apps/client/lib/features/connections/presentation/server_connector_panel.dart
related tests
```

Current `_updateScope`:
1. reads Observe status;
2. updates source scope;
3. if active, calls `_setConnectionObserve(true)`.

Delete step 1/3 behavior.

Target:

~~~
source scope update
  -> update server connection source
  -> refresh product state
  -> standing logical grant remains as-is
```

Old dependencies stale because source authority/revision changed.

### Initial connection enable

Do not remove the current explicit post-authorization first-enable behavior merely because scope updates no longer re-review.

Completing a new connection is a separate explicit user gesture under the current product policy.

### Outer resource

Delete `_observeResource` Calendar leaf derivation.

Call `RemoteAccessGateway.connectionObserve*` with no resource.

The gateway parameter may remain until 07, but 04 caller supplies `null`.

Add tests:
- active Calendar Observe + source scope update -> no review call;
- source update does not silently enable an off grant;
- explicit connection completion still performs current review/enable flow;
- disconnect still pauses/revokes before source deletion.

## 25. 04-U — remote View review bundle convergence

Modify:

~~~
crates/app/src/vault_host/review_snapshot.rs
crates/app/src/vault_host/interaction_owners.rs
crates/app/src/vault_host/tests/interaction_resolution.rs
```

### Reviewed member

Hosted Calendar member:

~~~
member_id = calendar.timeline
resource = calendar.timeline:<connection>
source_revision = signed remote SourceAuthority
connection_revision = signed server connection revision
expected_grant = generic remote View grant
policy_authority = generic remote View mapping authority
```

Exact Calendar IDs are source provenance and signed source evidence. They are not member permission resources.

### Drift

Pending review supersedes/fails on:
- producer change;
- source authority change;
- server connection revision change;
- provider identity change;
- exact signed source resource set change through source authority/revision;
- policy fingerprint change;
- grant/policy expectation change.

It does not fail merely because a caller-supplied leaf `resource` differs; there is no such authority input after 04.

## 26. 04-V — permanent acceptance tests

### 26.1 Hosted candidate/binding

Rust:
1. Google connection resources `[A]` -> one logical candidate X;
2. `[A,B]` -> same X;
3. candidate/binding revision stable;
4. Microsoft same behavior;
5. no hosted production candidate leaf loop.

### 26.2 Generic Calendar grant

Vault/Access:
1. review `calendar.timeline:<connection>`;
2. scope categories exactly Metadata+Content;
3. trusted Calendar consumers use one grant;
4. extension consumer denied;
5. source epoch/resource changes do not change grant ID/GrantAuthority;
6. pause/reactivate uses generic remote View API;
7. no remote Calendar mapping table/file.

### 26.3 Signed preview

Rust + Go:
1. Calendar preview logical resource correct;
2. `source_resources=[A,B]`;
3. canonical order required;
4. changed source set -> new source authority/revision;
5. old signature/dependency rejected;
6. malformed/duplicate/oversize source set rejected;
7. >4/eleven resources work within byte budget.

### 26.4 Generic Context Calendar read

1. one logical selected source;
2. generic View preview/binding;
3. Calendar query exact range/cursor validated;
4. Calendar response validated;
5. dependency permission resource logical;
6. dependency source resources exact;
7. old dependency fails after source edit;
8. no `RemoteCalendarViewRead`.

### 26.5 Server multi-resource runtime

Google and Microsoft tests:
1. connection configured with at least A+B;
2. first page walks canonical A then B subject to item limit;
3. provider page cursor on A returns composite cursor for A;
4. completion of A continues into B when budget remains;
5. exact boundary returns next-resource cursor;
6. final page marks coverage complete;
7. stale resource-set digest cursor rejected;
8. duplicate provider event ID on A/B yields distinct opaque evidence handles;
9. output does not leak Calendar IDs, provider event IDs, credential or token except where source configuration is explicitly returned by management;
10. result byte/item bounds enforced.

### 26.6 Server generic authorization

1. `/v1/views/calendar.timeline/source-preview` only;
2. `/v1/views/calendar.timeline/admit` uses `ViewAdmission`;
3. `/read` uses `ReadView`;
4. `/release` generic;
5. no `/v1/authority/calendar/source`;
6. no Calendar admission map;
7. source resource change between preview/admit/read fails closed;
8. wrong Person/client/device/connector/connection/producer/provider identity fails;
9. stale grant/policy/source authority fails;
10. Mail/Work/Logistics regressions remain green.

### 26.7 Resource edit invariant

Full Rust+server product integration:

1. server source `[A]`;
2. local remote SourceConnection mirrors A;
3. one logical Expert binding X;
4. one logical standing grant G;
5. record grant ID, GrantAuthority, policy authority, candidate ID, binding revision;
6. server scope update `[A,B]`;
7. server revision/epoch advance;
8. local SourceConnection updates A+B and local SourceAuthority advances;
9. no Observe re-review runs;
10. grant G ID/GrantAuthority/policy authority unchanged;
11. X/binding revision unchanged;
12. next read gets signed A+B source resources;
13. server runtime reads A+B;
14. new dependency records A+B;
15. old dependency stale.

This is the defining 04 regression.

## 27. Residual/deletion audit

Run after implementation:

~~~
rg -n "RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference" crates
rg -n "RemoteCalendarGrantRequest|RemoteCalendarGrantPreview|RemoteCalendarGrantReviewExpectation" crates
rg -n "remote_calendar_scope|remote_calendar_source|preview_remote_calendar_grant|review_and_activate_remote_calendar_grant|admit_remote_calendar_read|remote_calendar_dependency_source_admits" crates
rg -n "calendar_source_preview|verify_calendar_source_preview" crates server
rg -n "activate_calendar_grant|find_calendar_grant|calendar_grant_binding|pause_calendar_grant" crates
rg -n "remote_calendar_grants" crates
rg -n "RemoteCalendarViewRead|read_remote_calendar_view" crates
rg -n "for calendar in connection\.resources\(\)" crates/modules/context crates/app
rg -n "calendarAdmissionState|CalendarAdmission|AdmitCalendar|ReadCalendar" server
rg -n "/v1/authority/calendar/source" .
rg -n "calendar_id" server/internal apps/client/lib/features/connections
rg -n "_observeResource" apps/client
rg -n "RemoteCalendarAuthorizationExpectation|CalendarChallengeParts|parse_calendar_challenge" crates
rg -n "decodeCalendarEnvelope|decodeCalendarProof" server
```

### Required zero production matches

- all Calendar-specific remote Access grant/query/source types;
- `remote_calendar.rs`;
- `remote_calendar_grants.rs`;
- Calendar-specific RemoteGrantStore methods;
- Calendar-specific remote source preview;
- `RemoteCalendarViewRead`;
- hosted Calendar leaf candidate loop;
- hosted Calendar leaf Expert selection;
- `calendarAdmissionState`;
- `CalendarAdmission`;
- `AdmitCalendar`;
- `ReadCalendar`;
- special Calendar source-preview route;
- singular server `calendar_id` scope key;
- Flutter `_observeResource`;
- automatic Observe re-review after remote Calendar resource edit.

### Allowed classified matches

- `CalendarViewQuery`, `CalendarContextView`, Calendar provider/domain types;
- provider leaf client `calendarID` internal fields;
- native Calendar `calendar_ids` at real provider/OS boundary;
- canonical server source-scope `calendar_ids`;
- `ConsumerPolicyAuthority` / policy mapping, owner 05;
- outer optional RemoteAccess `resource` wire field, owner 07, provided it is caller-zero/non-authoritative;
- historical plan/docs text until final plan cleanup;
- dormant Android native code outside current Apple priority.

Every remaining Calendar ID occurrence must be classified as:
- connection resource configuration;
- provider leaf mechanics;
- exact provenance;
- domain query/data;
not standing permission or Expert config.

## 28. Architecture/documentation convergence

Update current-state docs in the implementation change:

~~~
docs/architecture/runtime.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
docs/product/integrations-and-privacy.md
server/README.md if source protocol/scope is documented there
```

After 04 they must say:

- native and hosted Calendar Expert selection is connection/View-level;
- hosted Calendar is a normal generic remote `calendar.timeline` View for grant/preview/admission/read/release;
- server connection owns canonical current Calendar resources;
- producer source/resource changes advance source revision/authority without changing standing grant;
- signed remote source preview records exact current source resources;
- Context dependency separates logical grant resource and exact provider resources;
- one generic remote View admission map/route exists;
- provider runtime internally resolves the connection's current Calendar resources;
- first-party consumers remain trusted shipped manifest policy;
- `ConsumerPolicyAuthority` remains a temporary pre-05 detail;
- product RemoteAccess resource field remains temporary until 07 and has no Calendar authority meaning;
- resource edits do not re-review standing Observe.

Fix stale current product docs that still say active trusted Expert exact selected resources determine first-party grant consumers or that adding resources forces grant expansion. Those statements are no longer true after 03/04.

Do not rewrite historical ADR prose as current progress. If an ADR has a durable statement directly contradicted by the implemented architecture, amend/supersede only when required by repo policy; final rationale convergence remains checkpoint 09.

## 29. Suggested implementation slices

Multiple commits are acceptable. Keep each slice architecture-coherent.

### 04-A — generic Calendar View contract

- Calendar in generic remote View registry;
- category-set semantics;
- generic Calendar query/result validation;
- signed exact `source_resources`;
- RemoteViewSourceReference update.

Suggested commit:

~~~
context: model remote calendar as a generic view
~~~

### 04-B — generic remote Calendar grant/storage

- multi-category RemoteViewGrantRequest/scope;
- Calendar through generic RemoteGrantStore;
- delete Calendar-specific Access port/API;
- delete `remote_calendar.rs`;
- delete `remote_calendar_grants.rs`.

Suggested commit:

~~~
access: remove remote calendar grant special case
~~~

### 04-C — hosted source candidate/read/App cutover

- one hosted logical candidate;
- Expert binding stability;
- generic selected remote View read;
- App remote Observe generic member;
- interaction/review convergence.

Suggested commit:

~~~
app: converge hosted calendar on remote view authority
~~~

### 04-D — provider control generic preview/proof

- delete special Calendar preview endpoint/client;
- generic source resource proof;
- transport-generic proof naming.

Suggested commit:

~~~
providers: use generic remote view source proof for calendar
~~~

### 04-E — server connection multi-resource Calendar

- `calendar_ids`;
- canonical scope;
- revision/epoch tests;
- multi-leaf provider runtime;
- composite pagination;
- evidence-handle fix.

Suggested commit:

~~~
server: own calendar resource sets per connection
~~~

### 04-F — server generic admission/read

- generic PreviewView source resources;
- generic AdmitView/ReadView Calendar support;
- delete Calendar admission/read/map/route;
- HTTP helper naming cleanup;
- Go integration tests.

Suggested commit:

~~~
server: route calendar through generic view admission
~~~

### 04-G — client connection scope/invariant

- bind full server `calendar_ids`;
- multi-resource source configuration UI;
- stop resource-edit Observe re-review;
- no outer Calendar Observe resource caller.

Suggested commit:

~~~
client: keep remote calendar observe stable across scope edits
~~~

### 04-H — deletion/docs/closure

- residual audit;
- architecture/product/server docs;
- execution evidence;
- README 04 Complete.

Suggested commit:

~~~
docs: complete connection observe checkpoint 04
~~~

Combine slices if doing so reduces transient complexity. Do not add forwarding wrappers just to preserve intermediate compilation.

## 30. Verification

Run targeted Rust checks during the cutover.

Minimum Rust close gate:

~~~
cargo test -p floe-access remote
cargo test -p floe-context remote
cargo test -p floe-vault remote
cargo test -p floe-provider-adapters remote
cargo test -p floe-app remote_observe
cargo test -p floe-app interaction_resolution
cargo test -p floe-app registered_runner
cargo test -p floe-conversation
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
```

If a filter matches zero tests, run the actual nearest test target/full crate and report that command instead.

### Go

From `server/`:

~~~
go test ./...
go test -race ./...
go vet ./...
```

Also run focused connector/authorization tests while iterating, especially:

~~~
go test ./internal/connectors/googlecalendar
go test ./internal/connectors/microsoftcalendar
go test ./internal/authorization
go test ./internal/application
go test ./internal/transport/http
```

Use `gofmt` on changed Go files.

### FFI / Flutter

Because connection catalog/source configuration and RemoteAccess caller behavior change:

~~~
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/connections/server_connector_panel_test.dart
flutter test test/features/connections/connector_screen_test.dart
flutter test test/features/connections/app_wire_calendar_source_gateway_test.dart
flutter test test/features/connections
flutter build macos
```

Run full `flutter test` when practical and distinguish unrelated pre-existing failures.

### Broad Rust

Before closure:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
```

If default parallel is also run and exposes the known unrelated shared-counter race, report separately; do not weaken tests.

### Network/provider integration

Provider connector tests use local HTTP fixtures. They must prove:
- two configured resources;
- deterministic pagination;
- no credential/provider private-field leakage;
- typed credential/permission/rate-limit/unavailable errors.

No real Google/Microsoft account is required for 04 completion unless the execution environment already has an explicitly authorized disposable account.

Do not alter external credentials/account state to manufacture a live smoke.

If live hosted Calendar smoke is unavailable, report SKIPPED with reason.

## 31. Close procedure

Before marking 04 Complete:

1. rerun every residual search;
2. verify all Calendar-specific remote authorization types/files/routes are gone;
3. verify every hosted Calendar Expert candidate is logical connection/View;
4. verify every hosted Calendar grant is generic remote View logical resource;
5. verify signed preview contains exact source resources;
6. verify Context dependencies record logical `resources` + exact `source_resources`;
7. verify server has one generic admission map/route;
8. verify `calendar_ids` is the only server Calendar connection resource scope;
9. verify resource edit does not auto re-review Observe;
10. verify no compatibility decoder/old scope key/fallback remains;
11. classify 05 policy and 07 product-wire residuals explicitly;
12. run Rust/Go/Flutter/FFI/architecture/broad gates;
13. update architecture/product/server docs;
14. append execution evidence here;
15. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Complete
    - 05 Not started
16. commit closure;
17. stop. Do not begin checkpoint 05.

Execution evidence must record:

- execution date;
- start local HEAD / fetched origin/main;
- implementation and closure commit SHAs;
- final hosted candidate/reference shape;
- final generic Calendar GrantScope;
- final `RemoteViewSourceReference` source-resource shape;
- final generic RemoteGrantStore surface;
- deleted Access/Vault files/types;
- final Context hosted Calendar read path;
- final App review/interaction path;
- final server `calendar_ids` scope validation/budget;
- final server source revision/epoch behavior;
- final provider multi-resource runtime and cursor shape;
- final generic server preview/admit/read/release route;
- deleted special HTTP route/admission state;
- Flutter source-scope representation;
- resource-edit grant/binding invariant result;
- remote multi-calendar acceptance result;
- residual 05/07 matches;
- architecture/product/server docs changed;
- commands and real results;
- live provider smoke result or SKIPPED reason;
- final clean worktree.

## 32. Required agent report

Report:

1. start HEAD / origin-main / final HEAD;
2. hosted Calendar candidate final shape and binding stability;
3. generic remote View category/query/result contract changes;
4. generic signed source preview and exact `source_resources`;
5. final remote Calendar logical GrantScope;
6. final `RemoteGrantTransport` / `RemoteGrantStore` surfaces;
7. deleted `remote_calendar.rs` and Vault Calendar grant path;
8. Context generic hosted Calendar read and dependency example;
9. App remote Observe/review/interaction convergence;
10. provider special preview/proof deletion and generic proof naming;
11. server `calendar_ids` source scope and source-authority advancement;
12. Google/Microsoft multi-resource runtime and composite cursor semantics;
13. generic Go PreviewView/AdmitView/ReadView/release cutover;
14. deleted Go Calendar admission/source-preview route/state;
15. Flutter source configuration and removal of scope-change auto re-review;
16. `[A] -> [A,B]` grant/candidate/binding/dependency regression result;
17. multi-calendar provider/server acceptance result;
18. tests moved/rewritten/deleted;
19. residual audit with explicit 05/07 matches;
20. architecture/product/server docs updated;
21. Rust commands with real outcomes;
22. Go commands with real outcomes;
23. Flutter/FFI commands with real outcomes;
24. live remote provider smoke or skip reason;
25. checkpoint commit SHA(s);
26. clean worktree confirmation;
27. confirmation that checkpoint 05 was not started.

## Execution evidence — 2026-09-29

- Baseline: local HEAD and fetched `origin/main` were both `b2b7602b53ef53a002e8f15875267cc6d584a45d`; worktree was clean. Implementation commits: `44ac4087`, `ba10e78f`, `2b159005`, `d1aab42b`, `c69b6bb7`, `8a45526e`, `868bece6`, `40906bba`. This document and the parent index are the closure commit; its SHA is recorded in the Git history and final report rather than self-referenced in the commit body.
- Hosted Google/Microsoft Calendar exposes exactly one `calendar.timeline:<connection-id>` candidate/selected reference per connection, independent of leaves. Its generic remote View contract admits Metadata + Content and validates Calendar query/result. One generic `RemoteGrantStore`/`RemoteGrantTransport` grant has that logical resource, `Read`, `EverydayAssistance`, and `LocalOnly`; the exact current canonical `source_resources` are separately signed by `RemoteViewSourceReference` and bounded to 16 KiB. The old `remote_calendar.rs`, `remote_calendar_grants.rs`, special Rust grant/query/source/read types, methods, provider preview and Calendar proof parser were deleted.
- Context's hosted read uses generic preview, admission, read and release. Dependency `resources` contains `calendar.timeline:<connection-id>`, `source_resources` contains the exact current `[A,B]`, and `source_authority` records the current epoch. Reauthorization verifies a fresh signed source preview and stales old epochs or changed leaves. App derives the logical resource for Observe, uses generic policy/inline review/interaction resolution, rejects a non-null Calendar outer resource, and sends `everyday_assistance` for admitted remote View reads. The product wire field remains structurally optional and non-authoritative until 07.
- Server connection scope accepts only canonical nonempty `calendar_ids` (sorted, unique, valid opaque identifiers; at most 12 KiB serialized), not singular `calendar_id`. Membership changes advance connection revision and producer source epoch; reorder is a no-op. Flutter represents IDs as bounded one-per-line text, preserving commas, mirrors the full set into local Connections, and does not auto-review/enable Observe after an edit. Local membership change advances Connections revision and `SourceAuthority` exactly once. Server Google/Microsoft connection runtimes own all configured leaves; a bounded connection-level pager uses `version + resource_set_digest + resource_index + provider_cursor`, reads up to 16 provider pages per request, and gives distinct event evidence handles even for equal provider event IDs across calendars.
- Calendar uses the generic Go `PreviewView -> AdmitView -> ReadView -> Release` route and single generic admission map. The old `CalendarAdmission`/`calendarAdmissionState`, `AdmitCalendar`, `ReadCalendar`, and `/v1/authority/calendar/source` production route are gone; a negative HTTP test asserts the old route is 404. The generic signed preview and response expose exact source resources while keeping provider-private data out of Agent context.
- Permanent `[A] -> [A,B]` regressions pass: candidate/reference and Expert binding revision stay stable, grant ID/authority and pre-05 `ConsumerPolicyAuthority` stay stable, Observe status remains active without automatic review, server and local source authorities advance, signed next preview has `[A,B]`, provider reads both leaves, new dependency records `[A,B]`, and old epoch reauthorization fails. Go tests also cover 11-resource scope, paging/stale cursor, duplicate event IDs, budgets and private-field exclusion. Existing Mail/Work/Logistics generic remote View and native Calendar tests pass. Obsolete special-path tests were replaced with generic-path/negative-route tests; native legacy mapping-table rejection remains fail-closed.
- All 14 searches in §27 were rerun. Remote Calendar authorization types/files/methods, special preview/proof, hosted leaf candidate loop, special admission state/route, Flutter `_observeResource`, singular production scope and compatibility fallback have zero production matches. Remaining `remote_calendar_source` names are local Connections configuration commands, `ReadCalendarView` is the provider-domain runtime, the old route occurs only in its 404 test and this historical plan, and Calendar IDs occur only in connection configuration, provider leaf mechanics, exact provenance or domain data. `ConsumerPolicyAuthority` and generic policy mapping remain for 05; optional outer RemoteAccess `resource` remains for 07 and is caller-null/non-authoritative for Calendar.
- Current-state docs updated: `docs/architecture/runtime.md`, `docs/architecture/modules.md`, `docs/architecture/authority-recovery.md`, `docs/product/integrations-and-privacy.md`, and `server/README.md`.
- Rust: targeted Access/Context/Vault/App/Conversation tests passed; provider-adapters full crate passed because its `remote` filter matched no tests. `cargo check --workspace --lib`, `cargo build -p floe-ffi`, both architecture gates, and `git diff --check` passed. The exact broad `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1` passed on the final code. An earlier broad run exposed one native legacy-table rejection regression; restoring that fail-closed check and rerunning its targeted test and the broad gate resolved it.
- Go: `go test ./...`, `go test -race ./...`, `go vet ./...`, and focused Google/Microsoft Calendar, authorization, application and HTTP suites passed. Flutter: `flutter analyze`, the three named connection panel/screen/wire tests, `flutter test test/features/connections` (66), full `flutter test` (367), and `flutter build macos` passed. Go race emitted only macOS linker warnings. Google/Microsoft live-account smoke: **SKIPPED** — no explicitly authorized disposable accounts or credentials were available; no external account state was changed.
- Closure requires the committed documentation update and a clean worktree. Checkpoint 05 was not started.
