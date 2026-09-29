# 07: Product wire, FFI and UI convergence

Prerequisite: 06 complete.

Status: Ready for execution. Not started.

Planning base: main at 988cec65595a2f7a2b64358b6060e6791e22b137 on 2026-09-29.

Checkpoint 06 completed the authority/ownership cutover. Calendar, Contacts, Attention and Wellbeing standing source truth now belongs to Connections; standing grants name logical connection/View resources; Context records exact physical provenance. Checkpoint 07 must not redesign those semantics. It makes the product boundary tell the same story.

The final product invariant is:

~~~
source configuration
  = Connections intent

Use with Floe
  = one connection-level standing Observe intent

review expectation
  = backend-produced compare-only snapshot

provider leaves / Contacts handles
  != standing permission input
~~~

This is a direct wire/UI cutover. Do not preserve old Calendar Access DTOs, old standing Personal Access DTOs, remote outer resource arguments, dual decoders, compatibility aliases, forwarding gateways, or selected/granted leaf projections.

Line numbers below are planning-base anchors on 988cec65. Re-resolve symbols on the actual execution HEAD before editing.

## 1. Scope and checkpoint boundary

### Converge in 07

- one App-owned product ConnectionObserve contract for native Calendar, hosted/remote Views, Contacts, Attention and Wellbeing;
- one connection-level inspect/review/set-enabled product flow;
- one backend-produced review expectation echoed by enable;
- Connections-owned source setup/configuration separated from Observe permission;
- native Calendar source/subject setup removed from Access permission wire;
- Contacts handle selection removed from standing Access review and moved to source configuration;
- standing Attention/Wellbeing source establishment/subject refresh removed from standing Personal Access wire;
- remote ConnectionObserve removed from the remote-specific product operation and exposed through the same product contract;
- protocol + FFI + Flutter cut over in one snapshot;
- old standing Calendar/Personal product DTOs/gateways/tests deleted in this checkpoint when caller-zero;
- product copy and settings surfaces converged on Connections as the source editor and one Use with Floe toggle.

### Keep separate

- Feasibility / schedule.feasibility contextual query review;
- provider OAuth / pairing / credential setup;
- OS permission prompts;
- Calendar provider read calendar_ids after Context resolves current Connections resources;
- remote signed View member descriptors and provider routing internals;
- exact-recipient model processing consent;
- Actions approval/execution/recovery;
- Expert binding configuration;
- Checkpoint 08 repo-wide legacy/dependency/public-surface conformance sweep;
- Checkpoint 09 full verification, final ADR/docs convergence and plan lifecycle.

### Android

Keep shared Rust contracts coherent where existing code requires them. Do not add Android Contacts/Calendar product flows or parity work.

## 2. Frozen final product contract

The product surface must represent three distinct operations.

### Inspect

Input:

~~~
connector_id
connection_id
~~~

Output is a ConnectionObserveOverview derived from current Connections source + current canonical first-party View grants.

The overview is display state, not mutation authority. It contains:

~~~
connector_id
connection_id
status
enabled
source_resources        # current connection resources for display only
members[]               # logical View/member state for audit/display
~~~

A member may expose:

~~~
view_id
state
review_required
~~~

Do not require the client to carry GrantId, GrantAuthority, SourceAuthority or leaf permission resources merely to toggle the UI. If an audit requirement needs an authority identifier, keep it inside a backend-produced review expectation rather than making the overview the mutation token.

Delete the independent granted_resources projection. Rename selected_resources to source_resources or an equally explicit source-configuration name. It is Connections state and must never be treated as permission scope.

### Review

Input:

~~~
connector_id
connection_id
~~~

Backend reloads current source and first-party policy and returns an opaque/exact reviewed expectation.

Canonical expectation meaning:

~~~
source_authority
connection_revision?            # when the producer/source exposes one
reviewed_native_subject?        # native sources only
reviewed_producer_fingerprint?  # hosted/remote only
members[]:
  view_id
  policy_digest
  logical resource
  expected grant state          # reviewed absence OR exact GrantId+GrantAuthority
~~~

The product client may display safe member/View information, but it must not construct or edit this authority snapshot.

Do not put remote provider_identity / recipient into the reviewed product authority if they remain live routing facts. Current interaction code already treats them as live routing fields rather than reviewed authority. Keep them inside the remote adapter/helper and revalidate them live.

### Set enabled

Input:

~~~
connector_id
connection_id
enabled
disconnecting/revoking intent only where the owning disconnect flow requires it
expected reviewed snapshot when enabling
~~~

Rules:

- enabled=true requires the exact backend-produced review expectation;
- enabled=false requires no review expectation and pauses standing grants;
- disconnecting/revoking narrows/removes standing grants but does not itself mutate source resources;
- backend reloads current Connections source, current first-party product policy and current grants before mutation;
- no Calendar IDs, Contacts handles, consumers, purpose, processing, caller-supplied SourceAuthority, policy epoch or arbitrary logical resource selector appear as top-level mutation input.

## 3. Current residual topology to replace

At the planning base the product still has four representations of the same standing Observe intent.

### Native Calendar

crates/app/src/local_access_services.rs:21-118:
- LocalAccessCommand::Calendar;
- CalendarAccessChange;
- CalendarAccessConfiguration;
- CalendarAccessOverview;
- LocalAccessInspection::CalendarAccess;
- LocalAccessResult.calendar_access plus LocalAccessResult.connection_observe.

crates/app/src/vault_host/calendar_access.rs:311-470:
- apply_calendar_access handles Inspect/Review/Pause/Remove;
- Review accepts calendar_ids and a caller-carried source authority;
- the backend then reloads the same Connections resource set.

crates/app/src/vault_host/calendar_access.rs:519-559:
- overview creates selected_resources;
- active state duplicates them into granted_resources.

### Standing personal

crates/modules/access/src/application/personal_grants.rs:27-90:
- PersonalAccessChange and ContactsAccessChange are still public product-shaped commands;
- PersonalAccessOverview is shared by standing personal sources and contextual Feasibility.

crates/app/src/vault_host/personal_access.rs:18-455:
- StandingChange::Inspect/Review/SetEnabled;
- Contacts Review still carries selected handles;
- Review may both configure SourceConnection and activate the standing grant.

This is now a product orchestration debt, not an ownership debt.

### Remote

crates/app/src/remote_services.rs:162-217:
- RemoteAccessCommand::ConnectionObserve and ConnectionObserveReview live beside pairing/enrollment;
- both carry resource: Option<String>.

crates/app/src/vault_host/remote_observe.rs:20-45:
- RemoteObserveContext also carries resource;
- validate_observe_identity rejects every Some(resource).

Thus the resource parameter has no valid product meaning and is a deletion target, not optionality to preserve.

### Flutter

Native Calendar:
- apps/client/lib/features/connections/domain/native_calendar_access.dart:1-175;
- apps/client/lib/features/connections/application/native_calendar_access_gateway.dart:1-182;
- apps/client/lib/features/connections/presentation/connector_screen.dart:734-895.

Standing personal:
- apps/client/lib/features/settings/domain/agent_personal_access.dart:1-181;
- apps/client/lib/app/runtime/local_owner_gateways.dart:332 onward;
- apps/client/lib/features/connections/presentation/personal_access_cards.dart;
- connector_screen.dart:1050-1185 and 1313 onward.

Remote:
- apps/client/lib/features/connections/application/remote_access_gateway.dart:4-108;
- server_connector_panel.dart:310-336 and 526-649.

The production symbol _observeResource is already absent at the planning base. Keep it absent; the live residual is the resource nullable argument and callers passing resource: null.

## 4. 07-A — Make connection_observe.rs the canonical App product contract

### Edit

crates/app/src/connection_observe.rs:15-129

Replace the temporary projection-only types with the canonical App product contract.

1. Keep ConnectionObserveStatus, but make every status have one defined product meaning:
   - Active;
   - Paused;
   - NeedsReview;
   - NeedsSystemAccess;
   - ReconnectRequired;
   - Unavailable.

2. Narrow ConnectionObserveMember:
   - keep View identity and safe display state;
   - remove per-member source authority duplication;
   - do not expose a copied resource list as "granted resources";
   - do not make the overview carry the grant CAS token used for mutation.

3. Rewrite ConnectionObserveOverview:
   - connector_id;
   - connection_id;
   - status;
   - enabled;
   - source_resources as a display projection of current SourceConnection.resources;
   - canonical logical members.

4. Delete from_calendar. There must be no Calendar-specific compatibility converter after the native caller cutover.

5. Add an App-owned ConnectionObserveExpectation / reviewed-member value with:
   - exact SourceAuthority;
   - optional connection revision;
   - optional native subject;
   - optional producer fingerprint;
   - canonical members with policy digest, logical View resource and reviewed grant expectation.

6. Add validation/canonicalization:
   - 1-8 members;
   - canonical View order;
   - no duplicate View;
   - exact grant expectation pairing;
   - valid authorities;
   - canonical policy digest;
   - one connection/source identity per bundle.

7. Define App product request/operation values for inspect, review and set-enabled. Do not put provider leaf selectors in them.

### Reuse current review semantics

crates/app/src/vault_host/review_snapshot.rs:31-49 and 167-453 already express almost the exact reviewed authority shape for durable inline interactions.

Do not create a second policy algorithm. Extract/reuse common App helpers so:
- direct product review;
- inline interaction snapshot capture;
- revalidation before enable

derive the same policy digest/member/grant/source expectations.

Conversation's durable InlineObserveTarget remains a Conversation contract. Map the App-reviewed expectation into that durable type at the interaction boundary; do not make product wire serialize Conversation internals directly.

### Tests

Rewrite connection_observe.rs unit tests so they prove:
- exact member set derives Active/Paused/NeedsReview;
- source resources do not affect grant/member identity;
- review expectation canonicalization rejects duplicate/missing grant pairs;
- overview is not accepted as a mutation expectation.

Suggested commit:

~~~
app: define canonical connection observe product contract
~~~

## 5. 07-B — Extract one App standing Observe orchestration path

The canonical App service must own product composition while delegating authority to Connections/Access.

### Native Calendar

Current anchors:
- local_access_services.rs:42-92;
- vault_host/calendar_access.rs:311-470;
- vault_host/calendar_access.rs:519-559;
- interaction_owners.rs:1052-1135;
- review_snapshot.rs:167-276.

Refactor Calendar standing Observe into connector-neutral helpers called by the canonical ConnectionObserve product service.

Inspect:
1. load the serving EventKit SourceConnection;
2. derive canonical calendar.timeline:<connection>;
3. load the exact current non-revoked grant for that stable source/resource;
4. return ConnectionObserveOverview.

Review:
1. reload current SourceConnection;
2. probe the current native subject using the connection's current resources;
3. if the subject changed, update Connections with expected revision before returning the review snapshot;
4. reload source after probe/update;
5. derive current calendar policy from first_party_observe;
6. record reviewed source authority, connection revision, native subject, policy digest and exact current grant expectation;
7. return common ConnectionObserveExpectation;
8. do not mutate the Access grant.

Enable:
1. reload source;
2. re-probe subject and compare exact reviewed source/resource/subject identity;
3. reload product policy and grant expectation;
4. reject drift before mutation;
5. activate/re-activate the one logical calendar.timeline grant by exact expected GrantAuthority or reviewed absence;
6. no Calendar ID enters GrantScope or the product request.

Disable:
- pause the current standing logical grant;
- do not edit/disconnect source resources.

Disconnecting:
- revoke/narrow the standing grant if the connection-disconnect workflow requests it;
- the Connections disconnect command remains the source lifecycle mutation.

When all product and interaction callers use these helpers, delete CalendarAccessChange, CalendarAccessOverview, CalendarAccessState and CalendarAccessConfiguration.

### Standing Contacts / Attention / Wellbeing

Current anchors:
- personal_source_spec.rs:6-114;
- vault_host/personal_access.rs:18-455;
- modules/access/personal_grants.rs:27-90;
- interaction_owners.rs:1137 onward;
- review_snapshot.rs:279-377.

Split the current apply_standing responsibilities in two.

A. Source setup/configuration:
- Connections-owned;
- native subject probe + SourceConnection establish/configure reviewed-native;
- Contacts selected handles belong here;
- Attention/Wellbeing canonical singleton resource belongs here;
- no grant mutation.

B. Standing Observe:
- common ConnectionObserve inspect/review/set-enabled;
- logical connection/View grant only;
- no selected handles in the permission request.

After the split, remove standing Contacts/Attention/Wellbeing branches from PersonalAccessChange / ContactsAccessChange. Keep Feasibility as its own contextual path.

### Feasibility

crates/modules/access/src/application/personal_grants.rs:251-350 is intentionally contextual.

Rename/narrow the product-facing PersonalAccess types to Feasibility-specific names where they have become Feasibility-only:
- FeasibilityAccessChange;
- FeasibilityAccessConfiguration;
- FeasibilityAccessOverview or the narrowest equivalent.

Do not add aliases under the old PersonalAccess names merely to stage the cutover.

Different event/destination/query still requires explicit review. Query changes continue to advance GrantAuthority; subject changes continue to advance the contextual SourceAuthority.

Suggested commit:

~~~
app: converge standing observe on connection intent
~~~

## 6. 07-C — Move native source setup out of Access product wire

A source configuration edit and Use with Floe are separate product operations.

### Calendar source setup

Current Connections path:
- crates/app/src/connection_services.rs:204-356;
- protocol dto/connections.rs:21-165;
- AppWire app_wire.rs around native_source_mutation at 1086-1140;
- Flutter app_wire_calendar_source_gateway.dart:14-80 and 173-205.

Keep Calendar resource setup under Connections.

Move the native subject/setup operation out of Local Access:
- CalendarSubjectIntent in local_access_services.rs:11-18;
- LocalAccessInspection::CalendarSubject at lines 95-105;
- AppWire AccessCalendarPreview;
- CalendarSubjectIntentDto / access.calendar.preview.

The replacement Connections-owned setup operation must:
1. accept source-configuration intent, not permission intent;
2. probe native subject outside a Vault transaction;
3. use SourceConnectionService::establish_reviewed_native / configure_reviewed_native / update_native_subject as appropriate;
4. compare expected local source revision for CAS;
5. return the current SourceConnection;
6. never activate/pause/revoke a grant.

Do not move source setup into Context or Access.

### Personal native source setup

Add the minimal Connections product operation needed to establish/configure reviewed native personal sources:
- connector identity;
- current expected source revision when updating;
- selected Contacts handles where applicable.

App may use PersonalSourceSpec to derive:
- stable connection ID;
- execution owner;
- resource mode;
- singleton Attention/Wellbeing resources.

The client does not send the canonical singleton resource or standing grant scope.

The operation:
1. validates supported standing native connector;
2. probes the relevant native subject;
3. builds canonical ConnectionResource values;
4. establish_reviewed_native on first setup;
5. configure_reviewed_native on resource/subject change;
6. returns SourceConnection.

Contacts resource change [A] -> [A,B] while Observe is active must:
- update only Connections;
- advance SourceAuthority exactly once;
- preserve GrantId;
- preserve GrantAuthority when permission policy is unchanged;
- perform no ConnectionObserve review/enable command;
- leave the UI standing Observe toggle Active while making old dependencies stale.

Attention/Wellbeing system access refresh follows the same separation: source subject refresh is Connections work; standing permission toggle is ConnectionObserve work.

### Product model naming

SourceConnectionDto is already generic at protocol dto/connections.rs:28-38. Reuse it.

Do not create PersonalSourceConnectionDto if SourceConnectionDto already expresses the data.

CalendarSourceConnection Dart is Calendar-named despite decoding generic source fields. During this checkpoint either:
- rename/generalize it to SourceConnection and use it for local Calendar/personal source configuration; or
- add the smallest generic source model and delete the Calendar-only duplicate when caller-zero.

Do not retain two decoders for the same SourceConnection JSON.

Suggested commit:

~~~
connections: separate native source setup from observe permission
~~~

## 7. 07-D — Converge direct interaction enable paths on the same helpers

Current anchors:
- vault_host/interaction_owners.rs:1052-1135 native Calendar;
- 1137-1214 personal;
- 1216-1275 remote;
- 1277 onward remote reviewed enable;
- review_snapshot.rs:167-453.

Today inline interaction decisions manually reconstruct native CalendarAccessChange and personal access review calls.

After 07-B:
- native inline enable calls the same App standing Observe enable helper as direct product UI;
- Attention/Wellbeing inline enable calls the same helper;
- remote inline enable calls the same common expectation comparator + remote internal activation helper.

The interaction target remains durable reviewed intent. It must never route through product wire/FFI.

Do not change:
- linked-interaction resume;
- exact decision digest;
- reviewed target drift semantics;
- durable conversation lifecycle.

Contacts remains non-inline when a picker/resource edit is required. A generic connection-level Observe enable may only be inline if the current Contacts SourceConnection already exists and the durable reviewed target binds its current source authority/resources; do not silently choose new handles from an interaction.

## 8. 07-E — Remove the outer remote resource parameter and remote-specific Observe product command

### Rust App

crates/app/src/remote_services.rs:162-217:
- remove ConnectionObserve and ConnectionObserveReview from RemoteAccessCommand after common product service is wired;
- RemoteAccessCommand remains pairing/enrollment/remote-authority concerns only.

crates/app/src/vault_host/remote_observe.rs:20-45:
- remove RemoteObserveContext.resource;
- change validate_observe_identity to connector+connection validation only;
- all review/enable/disable/status helpers receive the exact connection identity only.

crates/app/src/vault_host.rs:
- move ConnectionObserve dispatch out of remote-access operation handling into canonical ConnectionObserve service;
- preserve saved remote pairing/producer reload before provider I/O.

### Protocol/FFI

crates/bindings/protocol/src/dto/access.rs:17-105:
- remove RemoteAccessOperationDto::ConnectionObserve;
- remove RemoteAccessOperationDto::ConnectionObserveReview;
- remove the dead resource field with them.

crates/bindings/ffi/src/remote_wire.rs:
- delete the conversion branches for those operations.

No compatibility decoder may continue accepting remote resource or old remote ConnectionObserve kinds.

### Flutter

apps/client/lib/features/connections/application/remote_access_gateway.dart:4-108:
- remove connectionObserve / connectionObserveReview from RemoteAccessGateway;
- keep producer pairing/enrollment methods only.

server_connector_panel.dart:310-336 and 526-649:
- replace remote-specific Observe calls with the common ConnectionObserveGateway;
- remove every resource: null argument.

The current production _observeResource symbol is already zero-match. Keep it zero.

Suggested commit:

~~~
bindings: remove remote observe resource plumbing
~~~

## 9. 07-F — Protocol and FFI direct cutover

Define the product wire in crates/bindings/protocol around the common App contract.

### New canonical DTOs

Use one set of DTOs for local native and hosted/remote standing Observe:

~~~
ConnectionObserveOverviewDto
ConnectionObserveMemberDto
ConnectionObserveExpectationDto
ConnectionObserveReviewedMemberDto
ConnectionObserveMutationDto
~~~

Exact naming may be adjusted to repository conventions, but there must be only one product meaning.

Product query/command kinds should be connection-level, for example:

~~~
connection_observe.inspect
connection_observe.review
connection_observe.set_enabled
~~~

Use the existing App command/query envelope and owner-operation correlation conventions. Caller Person/device identity remains host-derived.

Strict decoder:
- deny unknown fields;
- reject nil/invalid operation IDs under existing envelope rules;
- reject empty connector/connection;
- reject enable=true without expectation;
- reject expectation on disable unless a real narrowing reason is designed;
- reject disconnecting=true with enable=true;
- canonical member ordering and exact grant expectation pairs.

### Delete native Calendar permission wire

Remove:
- CalendarAccessChangeDto in dto/local_access.rs:22-42;
- CalendarAccessOverviewDto at 45-59;
- LocalAccessResultDto.calendar_access;
- access.calendar.inspect query;
- access.calendar.configure command;
- access.calendar.preview once source subject setup moved to Connections;
- FFI calendar_access_change / calendar_access_dto conversions;
- AppWire command/query branches for those kinds.

Calendar IDs may remain in Connections source configuration/provider acquisition DTOs. Their existence is not a reason to keep them in Observe permission wire.

### Delete standing Personal permission wire

Remove standing uses of:
- PersonalAccessChangeDto;
- ContactsAccessChangeDto;
- PersonalAccessOverviewDto;
- access.personal.configure/inspect;
- access.contacts.configure/inspect.

Replace the surviving Feasibility contextual path with Feasibility-specific DTOs/kinds. Do not serialize Feasibility through ConnectionObserve.

### Remove temporary local result duplication

local_access_services.rs:108-118 and AppWire local_access_result conversion currently carry:
- calendar_access;
- connection_observe;
- personal_access.

After cutover:
- standing Observe uses the new ConnectionObserve result type;
- LocalAccess result keeps only genuinely contextual/local Access operations still owned there, such as Feasibility if it remains on that owner operation channel;
- no duplicated Calendar projection.

### FFI

Update:
- crates/bindings/ffi/src/app_wire.rs;
- crates/bindings/ffi/src/conversion/owners.rs;
- crates/bindings/ffi/src/remote_wire.rs;
- public exports in floe-protocol/floe-ffi.

Build Rust bindings and Flutter from the same commit. Do not leave old wire aliases.

Suggested commit:

~~~
protocol: expose connection-level observe intent
~~~

## 10. 07-G — Flutter common gateway and model

Create one Dart ConnectionObserveGateway used by all standing source detail screens.

Interface:

~~~
inspect(connectorId, connectionId)
review(connectorId, connectionId)
setEnabled(connectorId, connectionId, enabled, expected?, disconnecting?)
~~~

Models:
- ConnectionObserveOverview;
- ConnectionObserveExpectation;
- reviewed member.

Do not expose arbitrary resource/calendarIds/selectedHandles in this gateway.

The gateway talks to the new common AppWire product kinds. Remote and native screens do not get separate implementations of the standing permission algorithm.

Wire parsing must be strict and reject old:
- selected_resources + granted_resources pair;
- remote outer resource;
- consumer_policy/policy authority;
- Calendar-specific permission DTO fields.

## 11. 07-H — Native Calendar Flutter deletion

Current files:
- domain/native_calendar_access.dart:1-175;
- application/native_calendar_access_gateway.dart:1-182;
- connector_screen.dart:734-895;
- runtime wiring app_runtime.dart:67;
- local_owner_gateways_scope.dart:24.

After source setup has moved to Connections and generic Observe is usable:

1. replace _deviceCalendarObserve with the common ConnectionObserve control;
2. inspect/review/set-enabled names connector+connection only;
3. keep calendar resource editor on CalendarSourceGateway / Connections;
4. subject probing/setup is Connections source setup, not Observe gateway;
5. delete NativeCalendarAccessGateway and NativeCalendarAccessOverview;
6. delete native_calendar_access.dart and native_calendar_access_gateway.dart when caller-zero;
7. remove runtime/scope fields and constructors.

UI must continue to show:
- System access / source configuration;
- current selected Calendars from Connections;
- one Use with Floe switch.

It must not show:
- a second grant Calendar picker;
- granted N of M Calendars;
- consumer checkboxes.

## 12. 07-I — Personal Flutter source/permission split

### Contacts

Current personal_access_cards.dart:499-652 combines:
- contact picker;
- subject inspection;
- permission review;
- enable.

Rewrite into:
1. Contacts source editor:
   - read available contacts;
   - choose 1-64 handles;
   - save source configuration through Connections native-personal source setup;
   - show current configured source handles from SourceConnection;
2. common Use with Floe control:
   - inspect generic connection Observe;
   - review generic connection;
   - enable/disable generic connection;
   - no handles in the Observe call.

When handles are edited while active, save only source configuration. Do not invoke review/setEnabled solely because the source widened or narrowed.

### Attention / Wellbeing

Current cards have custom permission review buttons. Replace standing permission mutation with the common ConnectionObserve control.

Their system/native setup action:
- requests/refreshes OS access;
- probes native subject;
- establishes/refreshes SourceConnection via Connections.

The standing grant is a separate toggle.

### Feasibility

Keep the contextual Feasibility card, but bind it to the new Feasibility-only gateway/model after the PersonalAccess split.

Do not add Feasibility to ConnectionObserve.

### Remove old personal gateway

settings/domain/agent_personal_access.dart and NativePersonalAccessGateway currently combine four domains.

After cutover:
- delete standing Contacts/Attention/Wellbeing methods;
- rename/narrow the surviving interface to FeasibilityAccessGateway;
- update runtime wiring and _ScopedPersonalAccessGateway accordingly;
- no compatibility wrapper under AgentPersonalAccessGateway.

## 13. 07-J — Remote Flutter convergence

server_connector_panel.dart already presents the desired single Use with Floe toggle.

Keep its behavior, change only the gateway/model boundary:
- common ConnectionObserveGateway;
- no resource:null;
- enabling performs review then presents/echoes the exact reviewed expectation;
- dismissing review performs no mutation;
- disabling pauses grants;
- disconnect flow may request revoke then separately disconnect the connector/source.

Remote Calendar scope editor remains connector configuration. Its Calendar IDs are allowed there.

A remote Calendar scope edit while Observe is active:
- updates server/Connections source state;
- does not auto-enable or re-review grant merely to include new Calendar IDs;
- later reads carry new SourceAuthority/provenance;
- an explicit future enable/review uses the current source.

Gmail, Google/Microsoft Calendar and other supported hosted Views use the same ConnectionObserve product gateway.

## 14. UI semantics and copy

Connections is the editor.

For every standing source screen, present:

1. connection/system/source setup;
2. current source resources where meaningful;
3. one Use with Floe control.

Use with Floe Off:
- pauses standing Observe;
- preserves source connection/resources/credentials.

Use with Floe On:
- requests a backend review snapshot;
- may show safe logical View/member information;
- echoes that exact snapshot to enable;
- never asks the user to choose grant leaf resources.

Resource/source edit:
- changes Connections;
- does not run an Access grant expansion ceremony;
- may make prior Context evidence stale;
- does not change the standing grant identity/authority when policy itself is unchanged.

Data & privacy is already navigation-only at apps/client/lib/features/settings/presentation/data_privacy.dart:84-100. Keep it navigation/summary-only. Do not reintroduce editable Calendar/personal access there.

Third-party Expert installation/binding must not produce a product permission selector.

## 15. Preserve recovery and failure semantics

Do not collapse source availability into grant state.

Map failures distinctly:

- OS permission/source unavailable -> NeedsSystemAccess or Unavailable;
- remote pairing/source replacement -> ReconnectRequired where appropriate;
- no/currently unusable grant -> NeedsReview or Paused;
- stale reviewed expectation -> typed conflict/review-required;
- source drift during review/enable -> fail closed before grant mutation.

Never silently:
- recreate a disconnected source from an Observe toggle;
- change Contacts handles from a permission mutation;
- substitute the latest grant/source into an old reviewed expectation;
- reroute to a different connection.

## 16. Tests to rewrite/delete — Rust App

### Native Calendar

Rewrite crates/app/src/vault_host/tests/native_calendar_access.rs around the common product service.

Retain semantic regressions:
- inspect without grant -> NeedsReview;
- fresh review then enable -> one logical grant;
- 11 current Calendar resources -> one logical calendar.timeline grant;
- pause keeps source/resources;
- source resource change preserves GrantId/GrantAuthority and stales old evidence;
- stale source/subject/grant review fails before mutation;
- foreign Person/device/connection rejected.

Delete assertions whose only meaning is:
- selected_resources vs granted_resources;
- CalendarAccessOverview type shape;
- calendar_ids in the permission Review command.

Provider subject acquisition may still see all 11 current IDs internally.

### Remote

Rewrite vault_host/tests/remote_product.rs:
- generic product ConnectionObserve uses exact connector+connection;
- enable requires reviewed expectation before I/O;
- saved remote identity/producer still reloads;
- outer resource field no longer exists.

### Personal

Add App tests:
- Contacts source [A] -> [A,B] while grant active: source mutation only, one SourceAuthority advance, same grant ID/authority;
- generic Contacts review/enable contains people.identity logical View only;
- Attention/Wellbeing subject refresh is source mutation, not grant review;
- generic enable after pause revalidates current source;
- Feasibility remains on contextual path and different query requires explicit review.

## 17. Protocol/FFI regressions

Rewrite crates/bindings/protocol/tests/local_owner_wire.rs and relevant access/protocol tests.

Positive:
- inspect/review/set-enabled common DTO roundtrip;
- review expectation exact grant absence/pair;
- native and remote use the same DTO;
- Feasibility contextual DTO remains separate.

Negative strict decoding:
- calendar_ids on ConnectionObserve rejected;
- selected_resources/granted_resources old overview rejected;
- outer resource rejected;
- consumers/purpose/processing rejected;
- consumer_policy/policy_authority rejected;
- caller SourceAuthority as top-level mutation input rejected;
- old access.calendar.configure/access.calendar.inspect/access.personal/access.contacts kinds rejected;
- enable without expected review rejected.

Update FFI C ABI/AppWire tests from the same snapshot.

## 18. Flutter regressions

### Generic Observe gateway

Add focused gateway/model tests:
- inspect names exact connector+connection;
- review names exact connector+connection;
- enable echoes exact backend review;
- no leaf resource field serialized;
- old parser fields rejected.

### Native Calendar

Rewrite/delete apps/client/test/features/connections/native_calendar_access_test.dart.

The replacement tests must prove:
- 11 configured Calendars still display one Use with Floe toggle;
- enable wire contains no Calendar IDs;
- Calendar source editor still sends all selected IDs to Connections setup;
- source edit while active sends no Observe enable/review call;
- Off keeps source resources;
- On after Off reviews current connection then enables.

### Contacts

Rewrite personal_contact_selection_test.dart:
- invalid handles rejected before source configuration I/O;
- valid selection saves through Connections source setup;
- source edit does not call ConnectionObserve;
- permission toggle carries no selected handles.

### Remote

Update server_connector_panel_test.dart:
- remove resource:null expectations;
- keep displayed-connection binding;
- keep review-before-enable;
- keep dismiss=no mutation;
- keep Calendar scope edit not auto-reviewing Observe;
- assert Gmail and Calendar use the same common gateway shape.

### Settings

Keep Data & privacy navigation test. Ensure no second editable standing permission control appears there.

## 19. Residual/deletion audit

Run on the execution HEAD:

~~~
rg -n "CalendarAccessChange|CalendarAccessOverview|CalendarAccessState|CalendarAccessConfiguration" crates apps
rg -n "CalendarAccessChangeDto|CalendarAccessOverviewDto" crates apps
rg -n "access\.calendar\.(inspect|configure|preview)" crates apps
rg -n "NativeCalendarAccessGateway|NativeCalendarAccessOverview" apps/client
rg -n "PersonalAccessChange|ContactsAccessChange|PersonalAccessOverview" crates apps
rg -n "access\.(personal|contacts)\.(configure|inspect)" crates apps
rg -n "AgentPersonalAccessGateway|NativePersonalAccessGateway" apps/client
rg -n "granted_resources|grantedResources" crates apps
rg -n "selected_resources|selectedResources" crates apps
rg -n "resource: Option<String>|resource: Option<&|resource: null" crates apps
rg -n "ConnectionObserve.*resource|ConnectionObserveReview.*resource" crates apps
rg -n "_observeResource" crates apps
rg -n "consumer_policy|ConsumerPolicyAuthority|policy_authority" crates apps
rg -n "calendar_ids" crates/app crates/bindings apps/client/lib/features/connections
rg -n "selected_handles" crates/app crates/bindings apps/client/lib
~~~

### Required zero production matches after 07

- CalendarAccessChange / CalendarAccessOverview standing product types;
- CalendarAccessChangeDto / CalendarAccessOverviewDto;
- native Calendar Access permission gateway/model;
- granted_resources / grantedResources;
- remote outer ConnectionObserve resource parameter;
- resource:null product calls;
- standing Contacts/Attention/Wellbeing PersonalAccess product commands;
- selected_handles inside standing Observe permission DTOs;
- calendar_ids inside standing Observe permission DTOs;
- consumer_policy / policy authority product fields;
- _observeResource.

### Allowed classified matches

calendar_ids:
- Connections Calendar source configuration;
- provider/native Calendar acquisition;
- signed hosted source provenance;
- server connector scope.

selected_handles:
- Contacts source configuration;
- Contacts provider/native acquisition;
- query/provenance test fixtures.

PersonalAccess naming:
- only if a non-standing contextual Feasibility internal symbol remains and the name is genuinely not a product compatibility alias. Prefer Feasibility-specific naming.

selected_resources:
- unrelated domains with a different semantic meaning may survive only after classification; no standing Observe product projection may use it.

Every surviving match must be classified in execution evidence.

## 20. 07/08 deletion boundary

Checkpoint 07 deletes obsolete product surfaces made caller-zero by this cutover, including whole Flutter files when appropriate.

Checkpoint 08 remains responsible for:
- repo-wide unrelated compatibility/dependency/public-export purge;
- machine conformance checker additions;
- final Cargo dependency narrowing beyond the direct 07 caller cutover;
- broad historical residual classification across all languages;
- cross-owner conformance scenario.

Do not deliberately leave a known old product gateway/DTO for 08 if 07 makes it caller-zero.

## 21. Architecture/product documentation

After implementation, inspect and update only current truth:

~~~
docs/architecture/modules.md
docs/architecture/runtime.md
docs/architecture/authority-recovery.md
docs/product/integrations-and-privacy.md
~~~

Required final statements:
- source setup/resources are Connections product intent;
- Use with Floe is one connection-level standing Observe product intent;
- the client never sends leaf resources as standing permission scope;
- review expectations are backend-produced compare-only snapshots;
- native/remote/personal standing Observe share one product wire;
- Feasibility remains contextual and separate;
- Data & privacy links to Connections rather than editing source permission.

Do not add a new ADR solely for wire convergence. Checkpoint 09 owns final durable rationale/ADR convergence unless implementation discovers a genuinely new architecture decision.

## 22. Suggested implementation slices

### 07-A — canonical App contract

- rewrite connection_observe.rs;
- extract reviewed expectation shape;
- common status/member tests.

Commit:

~~~
app: define canonical connection observe product contract
~~~

### 07-B — App standing orchestration

- native Calendar common inspect/review/enable/disable;
- personal standing common path;
- interaction helper reuse;
- Feasibility split.

Commit:

~~~
app: converge standing observe on connection intent
~~~

### 07-C — Connections source setup

- Calendar native subject/setup ownership;
- personal reviewed-native setup/config;
- Contacts source edit path;
- source setup tests.

Commit:

~~~
connections: separate native source setup from observe permission
~~~

### 07-D — protocol/FFI

- generic ConnectionObserve DTO/commands/queries;
- remove Calendar standing Access DTO;
- remove standing Personal DTO;
- Feasibility-specific DTO;
- remove remote outer resource/remote Observe operation;
- strict rejection tests.

Commit:

~~~
protocol: expose connection-level observe intent
~~~

### 07-E — Flutter

- common gateway/model/control;
- native Calendar cutover and old files deletion;
- Contacts source editor + common toggle;
- Attention/Wellbeing common toggle;
- Feasibility-only gateway;
- remote panel common gateway;
- tests.

Commit:

~~~
flutter: converge use with floe on connection observe
~~~

### 07-F — closure

- residual searches;
- architecture/product docs;
- execution evidence;
- README status update to 07 Complete;
- verification.

Commit:

~~~
docs: complete connection observe checkpoint 07
~~~

Combine slices when direct cutover makes the final system smaller. Do not preserve intermediate compatibility types merely to keep each slice compiling independently.

## 23. Verification during iteration

Run targeted owner tests first after each slice.

### Rust targeted

At minimum:

~~~
cargo test -p floe-app connection_observe
cargo test -p floe-app native_calendar
cargo test -p floe-app personal
cargo test -p floe-app remote_product
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo test -p floe-connections
cargo test -p floe-access feasibility
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
~~~

If a named filter matches zero tests, run the nearest full crate target and record that fact.

### FFI / Flutter

~~~
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/connections
flutter test test/features/settings
flutter test
flutter build macos
~~~

Do not rewrite the known unrelated Expert registry golden merely to make this checkpoint green. If the exact pre-existing 29-pixel mismatch recurs unchanged, classify it separately with evidence.

### Apple/native deterministic checks

Run deterministic packages affected by source setup changes:
- FloeAppleContacts;
- native Calendar/EventKit fixture/package used by current repo;
- FloeAppleHealth;
- ScreenTime/Attention checks if their bridge/setup path changed.

Do not request permissions, change signing/account state or touch external credentials solely for verification.

### Broad Rust

Final:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
~~~

Record default-parallel global-runner races separately if the known race recurs; do not weaken tests.

### Live smoke

Only if an explicitly approved disposable source/device exists:
- native Calendar source edit + Observe continuity;
- Contacts source edit + Observe continuity;
- one remote connection Observe toggle.

Otherwise record SKIPPED and the reason.

## 24. Required acceptance scenarios

### 24.1 Native Calendar 11-resource flow

1. EventKit SourceConnection has 11 resources.
2. UI shows those resources as connection configuration.
3. UI shows one Use with Floe toggle.
4. review request names connector+connection only.
5. backend probes all current Calendar IDs internally.
6. enable request echoes backend review and carries no Calendar IDs.
7. one logical calendar.timeline:<connection> grant is Active.
8. edit resources to 12 via Connections.
9. SourceAuthority advances.
10. GrantId/GrantAuthority unchanged.
11. no automatic Observe review/enable call.
12. next Context read uses all 12 current resources and old evidence is stale.

### 24.2 Contacts edit while active

1. Contacts SourceConnection resources [A].
2. people.identity:<connection> grant Active.
3. UI source editor changes to [A,B].
4. only Connections source setup/config command executes.
5. SourceAuthority advances once.
6. GrantId/GrantAuthority unchanged.
7. common ConnectionObserve remains Active.
8. no selected_handles appear in Observe wire.
9. next read source_resources=[A,B].
10. old dependency stale.

### 24.3 Attention / Wellbeing

1. OS/system setup produces serving SourceConnection.
2. generic Observe review/enable activates logical View grant.
3. subject refresh changes SourceAuthority only.
4. source refresh performs no grant mutation.
5. pause preserves source state.
6. re-enable reviews the current subject/source and preserves stable grant identity when policy is unchanged.

### 24.4 Remote

1. Gmail or Calendar connection exists.
2. generic inspect names connector+connection.
3. review returns canonical backend snapshot.
4. product wire has no outer resource.
5. enable echoes the exact snapshot.
6. remote adapter reloads saved pairing/producer/provider identity internally.
7. scope/resource edit does not auto-expand/re-review grant.
8. disconnect revoke and source disconnect remain separate owner actions.

### 24.5 Feasibility

1. no ConnectionObserve candidate/control absorbs the query;
2. different destination/query still requires explicit contextual review;
3. query change advances GrantAuthority;
4. subject change advances contextual SourceAuthority;
5. no standing source-resource editor is invented for Feasibility.

## 25. Close procedure

Before marking 07 Complete:

1. re-fetch origin/main and record start/final revisions;
2. prove one App product ConnectionObserve contract exists;
3. prove native Calendar, hosted remote and standing personal callers use it;
4. prove Contacts source configuration is independent from Observe permission;
5. prove native subject/source setup is Connections-owned product intent;
6. prove Feasibility remains contextual;
7. prove no Calendar IDs/Contacts handles/outer resource enter standing Observe wire;
8. prove old native Calendar Access product types/gateway are caller-zero and deleted;
9. prove old standing Personal Access product types/gateway are deleted or narrowed to Feasibility-specific names;
10. prove remote-specific ConnectionObserve product operation/resource is deleted;
11. prove selected/granted dual projection is gone;
12. run and classify all residual searches;
13. run targeted Rust, protocol, FFI and Flutter tests;
14. run Apple/native deterministic checks affected by source setup;
15. run broad serialized Rust gate;
16. run full Flutter suite and classify only pre-existing unrelated failures;
17. run macOS build;
18. update current architecture/product docs;
19. append exact execution evidence to this file;
20. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Complete
    - 05 Complete
    - 06 Complete
    - 07 Complete
    - 08 Not started
    - 09 Not started
21. commit closure;
22. stop. Do not begin Checkpoint 08.

## 26. Required execution evidence

Record:

1. date;
2. start local HEAD and fetched origin/main;
3. implementation/closure SHAs;
4. final ConnectionObserve App contract;
5. final overview fields;
6. final reviewed expectation fields;
7. final inspect/review/set-enabled product intent;
8. native Calendar source setup final owner/wire;
9. Contacts source configuration final owner/wire;
10. Attention/Wellbeing source setup final owner/wire;
11. final Feasibility product path;
12. native Calendar Observe final runtime path;
13. personal standing Observe final runtime path;
14. remote Observe final runtime path;
15. interaction enable helper convergence;
16. deleted Calendar Access App/protocol/Flutter symbols/files;
17. deleted standing Personal Access App/protocol/Flutter symbols/files;
18. deleted remote outer resource/remote-specific Observe product symbols;
19. selected/granted projection deletion proof;
20. native 11-Calendar acceptance proof;
21. Contacts source-edit/grant-continuity proof;
22. pause/re-enable/disconnect behavior;
23. strict wire rejection matrix;
24. Flutter common gateway/control;
25. Data & privacy non-editor confirmation;
26. residual audit and every allowed match classification;
27. architecture/product docs changed;
28. targeted Rust verification;
29. protocol/FFI verification;
30. Flutter verification;
31. native deterministic verification;
32. broad Rust verification;
33. live smoke or SKIPPED reason;
34. final clean worktree;
35. confirmation Checkpoint 08 was not started.

## 27. Agent report format

Report only:

1. start HEAD / fetched origin-main / final HEAD;
2. implementation and closure SHAs by 07 slice;
3. final common ConnectionObserve App contract;
4. final overview/review/set-enabled wire shape;
5. native Calendar source setup owner and deleted permission wire;
6. Contacts source setup owner and source-edit behavior;
7. Attention/Wellbeing source setup behavior;
8. Feasibility contextual path retained;
9. final native Calendar standing Observe path;
10. final personal standing Observe path;
11. final remote standing Observe path;
12. interaction helper convergence;
13. deleted App/protocol/FFI/Flutter types/files;
14. proof no Calendar IDs/Contacts handles/outer resource enter Observe wire;
15. proof selected/granted projection is gone;
16. native 11-resource regression;
17. Contacts [A] -> [A,B] grant/binding continuity regression;
18. pause/re-enable/disconnect semantics;
19. strict protocol rejection results;
20. Flutter generic gateway/UI tests;
21. residual audit/classification;
22. docs updated;
23. targeted Rust results;
24. FFI build/results;
25. Flutter analyze/tests/macos build;
26. Apple/native deterministic results;
27. broad serialized Rust result;
28. live smoke or SKIPPED reason;
29. clean worktree;
30. confirmation Checkpoint 08 was not started.
