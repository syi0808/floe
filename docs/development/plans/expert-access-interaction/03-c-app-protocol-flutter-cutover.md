# Checkpoint 03-C — App, protocol and Flutter ownership cutover

- **Execution baseline:** Checkpoint 03-B completion on top of main bbce2fa2ae2da4bffb05185f39d8d50aa3e6b82a
- **Depends on:** 03-A source-independent Registry; 03-B source-owned Calendar Access persistence
- **Goal:** remove the Calendar-Expert management surface from App/protocol/FFI/Flutter and expose Calendar Observe management only through Access/Connections ownership.

This checkpoint preserves the current explicit Calendar review/enable/scope/remove product behavior only long enough to remove the wrong owner. Checkpoint 04 later simplifies the UX to the final connection-level Use with Floe model.

---

## 1. Baseline anchors

| Surface | Current symbol | Required disposition |
|---|---|---|
| crates/app/src/expert_services.rs | CalendarExpertInstall | delete |
| same | ExpertCommand::InstallCalendar | delete |
| same | ExpertInspection::Calendar | delete |
| same | ExpertOperationResult.calendar_experts | delete |
| crates/app/src/local_access_services.rs | CalendarGrantConfiguration with Registry ids | replace |
| same | LocalAccessResult.calendar_experts | replace with Access-owned projection |
| crates/app/src/worker.rs | WorkerAction::CalendarExperts | delete |
| same | WorkerAction::CalendarAccess carrying floe_experts type | replace with Access/App type |
| same | WorkerResult.calendar_experts | delete/replace |
| crates/app/src/vault_host.rs | CalendarExperts / CalendarAccess branches | delete/refactor |
| crates/app/src/vault_host/calendar_access.rs | VaultCalendarSetups / CalendarSetupStore | delete/refactor |
| crates/bindings/protocol/src/dto/commands.rs | experts.calendar.install | delete |
| same | access.calendar.configure | retain name only if payload becomes Access-owned |
| crates/bindings/protocol/src/dto/queries.rs | experts.calendar.inspect | delete |
| same | access.calendar.preview | keep as native subject/source preview if still needed |
| crates/bindings/protocol/src/dto/experts.rs | CalendarExpertInstallDto / calendar_experts | delete |
| crates/bindings/protocol/src/dto/local_access.rs | setup_id / Registry revision Calendar config | replace |
| crates/bindings/protocol/src/dto/agent.rs | RegistryConfigurationTargetDto::CalendarView | delete |
| crates/bindings/ffi/src/app_wire.rs | Calendar Expert command/query conversion | delete/replace |
| Flutter AgentCalendarExpertController | entire file | delete |
| Flutter AgentCalendarExperts domain | entire file | delete |
| Flutter AgentCalendarSettings | entire file | delete |
| AgentController | calendarExpertController integration | delete |
| ConnectorScreen | AgentCalendarSettings | replace with Access/connection-owned surface |

---

## 2. App ownership after cutover

### 2.1 Experts service is Registry-only

Final Expert API surface manages only:

~~~text
inspect Registry
enable/disable installation
enable/disable assignment
read operation result
~~~

Delete:

~~~text
CalendarExpertInstall
ExpertCommand::InstallCalendar
ExpertInspection::Calendar
CalendarExpertOverview re-export
ExpertOperationResult.calendar_experts
~~~

Expert service must not know Calendar provider, CalendarScope, source authority or native subject fingerprint.

### 2.2 Registry configuration

Delete RegistryConfigurationTarget::CalendarView.

Final target variants are installation/assignment only unless another real current generic Registry target exists.

Update:

- floe-experts RegistryConfigurationTarget;
- protocol RegistryConfigurationTargetDto;
- FFI conversion;
- Flutter AgentRegistry target enum;
- protocol and registry tests.

Do not replace CalendarView with a generic source-view Registry target. Source scope belongs to Access/Connections.

---

## 3. Access-owned Calendar management contract

### 3.1 Owner input

Define App/Access-owned values, not floe_experts values.

Suggested semantic types:

~~~text
CalendarAccessReview
  connection_id
  expected_source_authority
  selected_calendar_ids
  expected_native_subject_fingerprint?  # only when a preview was explicitly reviewed

CalendarAccessChange
  Pause { grant_id, expected_grant_authority }
  ResumeOrReview { connection_id, expected_source_authority, selected_calendar_ids, expected_native_subject_fingerprint? }
  SetResources { connection_id, expected_source_authority, selected_calendar_ids, expected_native_subject_fingerprint? }
  Remove { grant_id, expected_grant_authority }

CalendarAccessOverview
  connection_id
  grant_id?
  grant_authority?
  state
  selected_resources
  consumers
  source_authority
  needs_review
~~~

Names may differ. The semantic requirements do not.

### 3.2 No Registry identity in Access commands

No Calendar Access command/query may carry:

- Registry instance_id;
- Registry expected_revision;
- setup_id;
- Registry view handle;
- Expert assignment/install id.

### 3.3 Caller authority

Person/device come from verified CallerContext/AppHost.

Flutter may supply:

- the connection it is operating on;
- selected resource ids the user chose;
- expected opaque current authority values needed for CAS/review;
- an expected native subject fingerprint only when that exact preview was shown and reviewed.

Backend must reload:

- current connection;
- current source authority;
- current DataAccessGrant;
- current CalendarGrantPolicy;
- current native subject when required.

Do not accept a caller-composed GrantScope or consumer list. Product composition supplies canonical consumers.

---

## 4. Preserve current explicit review semantics

Checkpoint 03 must not implement the Checkpoint 04 product decision that connection completion automatically creates Observe permission.

For now:

- explicit Calendar review remains explicit;
- pause/resume/review remain explicit;
- resource scope changes remain explicit;
- OS permission remains OS-owned;
- ActionAuthority remains separate.

The only change is **who owns the operation**.

Do not introduce the final Use with Floe toggle or automatic connection-time grant in this checkpoint.

---

## 5. Worker/App cutover

### 5.1 Remove CalendarExperts

Delete WorkerAction::CalendarExperts.

Delete stage name calendar_experts.

Delete calendar_experts from WorkerResult.

Delete execute_action branches that install/inspect Calendar Expert setup.

### 5.2 Refactor CalendarAccess

WorkerAction::CalendarAccess may remain as an Access owner operation, but its payload must be an Access/App CalendarAccessChange rather than floe_experts::CalendarAccessConfiguration.

WorkerResult should return an Access-owned CalendarAccessOverview or equivalent.

Calendar subject preview remains a separate read if it is still needed for native subject review.

### 5.3 vault_host/calendar_access.rs

Keep owner-correct composition pieces:

- calendar_first_party_consumers();
- current CalendarConnection reader;
- native subject source/probe;
- current grant/policy repository adapter.

Delete:

- VaultCalendarSetups;
- CalendarSetupStore implementation;
- DeviceCalendarAdmission methods expressed in CalendarExpertSetup terms;
- ExpertPackaging dependencies.

Replace with a small Calendar Access service/composition adapter that invokes the 03-B grant/policy operations.

Do not move policy decisions into Worker.

---

## 6. Protocol cutover

### 6.1 Delete Experts Calendar wire

Delete:

~~~text
experts.calendar.install
experts.calendar.inspect
CalendarExpertInstallDto
CalendarExpertOverviewDto
calendar_experts field in ExpertOperationResultDto
~~~

Update protocol fixtures so these old commands are rejected as unknown rather than accepted deprecated aliases.

### 6.2 Calendar Access wire

Keep owner-aligned names. A reasonable final Checkpoint 03 surface is:

~~~text
access.calendar.preview       # native source/subject preview, if needed
access.calendar.inspect       # current grant/effective access projection
access.calendar.review        # explicit review/activation
access.calendar.configure     # pause/resource/remove, if one typed enum remains
access.local.read_result
~~~

Exact command names may be adjusted to existing protocol conventions.

Requirements:

- no setup_id;
- no Registry instance/revision;
- grant mutation uses GrantId + expected GrantAuthority where applicable;
- source review uses connection + expected source authority;
- selected resources bounded and validated;
- no consumer list from client;
- no provider credential/token/endpoint;
- no native subject result is trusted without backend fresh validation.

### 6.3 Result DTO

Replace LocalAccessResultDto.calendar_experts with calendar_access: Option<CalendarAccessOverviewDto> or an equivalent owner-named field.

Do not serialize Expert Registry internals in the Calendar Access projection.

### 6.4 FFI

Update app_wire conversions atomically with protocol.

Remove any FFI public variant or conversion carrying CalendarExpert setup/overview.

No compatibility route.

---

## 7. Remote Calendar control surface

The existing RemoteAccess CalendarGrantPreview/Review/Status/Pause commands are already Access-owned.

After 03-B:

- keep their source/pairing verification semantics;
- update them to the new CalendarGrantPolicy persistence;
- remove any fields whose only purpose was the old mapping table;
- keep canonical consumer list server-side/product-derived.

Do not move remote Calendar grant controls into Experts.

Checkpoint 04 later relocates/simplifies the UX from server settings to concrete connection detail.

---

## 8. Flutter deletion and replacement

### 8.1 Delete Expert-owned Calendar files

Delete production files:

~~~text
apps/client/lib/features/experts/application/agent_calendar_expert_controller.dart
apps/client/lib/features/experts/domain/agent_calendar_experts.dart
apps/client/lib/features/experts/presentation/agent_calendar_expert_dialog.dart
~~~

Delete matching tests/support:

~~~text
apps/client/test/features/experts/agent_calendar_expert_controller_test.dart
apps/client/test/features/experts/agent_calendar_expert_dialog_test.dart
apps/client/test/features/experts/agent_calendar_experts_test.dart
apps/client/test/support/agent_calendar_experts.dart
~~~

### 8.2 AgentController

Remove:

- AgentCalendarExpertController construction;
- calendarExperts getters/failures;
- pending Calendar setup methods;
- setCalendarAccessEnabled/changeCalendarAccessScope/removeCalendarAccess forwarding;
- busy-state coupling to calendarExpertController;
- owner gateway inference for AgentCalendarExpertGateway.

AgentController should remain Conversation/Registry/Memory/Connections/Proposal composition, not Calendar access owner.

### 8.3 LocalOwnerGateways

Remove calendarExperts gateway.

Add an Access-owned Calendar gateway only if ConnectorScreen/application composition needs it. Prefer putting it under features/connections/application or a generic access package, not features/experts.

### 8.4 Connection detail replacement

ConnectorScreen currently embeds AgentCalendarSettings.

Replace it with a minimal connection-owned Calendar access panel/controller that preserves current Checkpoint 03 behavior:

- inspect OS/system access;
- inspect current Calendar Access grant status;
- review access;
- show/change selected Calendar resources;
- pause/remove access.

Do not add final Checkpoint 04 Use with Floe semantics yet.

Do not expose DataAccessGrant jargon in primary copy unless the existing UI already uses a developer/security detail section.

### 8.5 Registry UI

Remove CalendarView from Flutter Registry target/domain.

Expert Registry screens may still enable/disable Expert installation/assignment. They no longer edit source permissions.

---

## 9. Tests

### App ownership

- Experts command/query surface is Registry-only;
- Calendar review/configure routes through Access owner;
- Calendar operations never mutate Registry revision;
- disabling/removing Calendar access does not disable Schedule Expert;
- enabling/disabling Schedule Expert does not mutate Calendar grant;
- CallerContext Person/device cannot be overridden by payload.

### Protocol

- old experts.calendar.install/inspect fail decoding;
- Calendar Access command accepts no setup_id/Registry revision;
- forged consumer/source credential fields rejected;
- Registry CalendarView target rejected;
- access result contains owner projection only;
- FFI round-trip matches new command/query/result.

### Flutter

- no AgentCalendarExpertController;
- no AgentCalendarSettings;
- ConnectorScreen Calendar detail loads Access-owned status;
- explicit review/pause/resource/remove flows work through access gateway;
- Registry UI does not show/edit Calendar source permission;
- errors map to connection/access presentation, not Calendar Expert management.

### Separation regression

- Schedule card remains enabled after Calendar grant pause;
- Calendar grant can be reviewed while Schedule installation is disabled; that does not enable the Expert;
- source scope changes do not alter Registry revision;
- ActionAuthority unchanged by Observe changes.

---

## 10. Residual gate

At 03-C exit, production search must be zero for:

~~~text
CalendarExpertSetup
CalendarExpertSetupResult
CalendarExpertOverview
CalendarAccessConfiguration  # floe_experts version
CalendarAccessChange         # floe_experts version
CalendarViewBinding
CalendarSetupStore
install_calendar_expert
apply_calendar_access
WorkerAction::CalendarExperts
calendar_experts
ExpertCommand::InstallCalendar
ExpertInspection::Calendar
experts.calendar.install
experts.calendar.inspect
CalendarExpertInstallDto
CalendarExpertOverviewDto
RegistryConfigurationTarget::CalendarView
RegistryConfigurationTargetDto::CalendarView
AgentCalendarExpertController
AgentCalendarExperts
AgentCalendarSettings
agent_calendar_expert_
~~~

Calendar Access-owned types with similar generic words are allowed only if they contain no Registry/setup identity.

Search setup_id across Access Calendar wire/App. Expected zero.

Search expected_revision across Calendar Access wire/App. Registry revision must be zero matches; GrantAuthority CAS is allowed under its own type.

---

## 11. Verification

Targeted:

- App owner routing/local operation tests;
- protocol DTO/wire tests;
- FFI app_wire tests;
- Flutter connector/registry/controller tests.

Then:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
cd apps/client && flutter analyze
cd apps/client && flutter test
cd apps/client && flutter build macos
~~~

Do not proceed to 03-D while the UI or wire still represents Calendar access as an Expert installation/setup.