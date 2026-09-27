# 07: Product wire, FFI and UI convergence

Prerequisite: 06 complete.

This checkpoint removes protocol-shaped permission details from product surfaces and makes Use with Floe a single connection-level control over the current connection resource scope.

## Exit state

1. user cannot select a second Floe-specific Calendar resource list;
2. Use with Floe enable/disable names a connection, not Calendar leaf resources;
3. native and remote standing Observe project through one product-level ConnectionObserve shape;
4. selected_resources vs granted_resources duplication is gone;
5. consumer_policy fields are gone;
6. remote Flutter _observeResource Calendar special case is gone;
7. native Calendar-specific grant DTO/gateway surface is deleted where generic connection Observe replaces it.

## 07-A: canonical product intent

Define one product intent for standing Observe:

- connector_id;
- connection_id;
- enabled;
- disconnecting/revoke intent where required;
- exact reviewed expectation returned by backend preview/review capture, not caller-constructed grant policy.

The caller does not send:
- Calendar IDs;
- consumers;
- purpose;
- processing;
- SourceAuthority as an arbitrary mutable input;
- policy epoch.

The backend reloads current Connections state and product policy before mutation.

Native/remote transport differences stay behind App composition.

## 07-B: App service convergence

Current files:

- crates/app/src/connection_observe.rs
- local_access_services.rs
- remote_services.rs
- vault_host/calendar_access.rs
- vault_host/remote_observe.rs
- review_snapshot.rs
- interaction_owners.rs.

Move standing Calendar/Contacts/Attention/Wellbeing/remote View toggle projection toward ConnectionObserveOverview.

Target overview:

- connector_id;
- connection_id;
- status/enabled;
- current connection resources for display;
- logical View members/grant status as needed for audit;
- recovery reason where relevant.

Delete granted_resources as an independent resource set. If an audit screen needs to show the logical granted Views, expose View/member identity, not a copied leaf-resource list.

CalendarAccessOverview/CalendarAccessChange should disappear if no non-standing Calendar-specific operation remains. OS system permission/source setup stays in Connections/native setup APIs, not Access grant DTOs.

## 07-C: protocol/FFI cutover

Update:

- crates/bindings/protocol/src/dto/access.rs
- dto/local_access.rs
- dto/agent.rs where Calendar subject/grant fields overlap
- crates/bindings/ffi/src/remote_wire.rs
- app_wire.rs
- conversion/owners.rs.

Delete:
- calendar_ids from grant review/change operations;
- consumer_policy;
- policy_authority;
- granted_resources leaf list;
- remote resource parameter used only for Calendar Observe.

Keep system/source setup fields only on the connection setup operation that actually owns them.

Build Rust bindings and Flutter from the same snapshot. No old wire decoder.

## 07-D: Flutter deletion

Primary files:

- apps/client/lib/features/connections/presentation/server_connector_panel.dart
- connector_screen.dart
- native_calendar_access.dart
- native_calendar_access_gateway.dart
- remote_access_gateway.dart
- remote_owner_models.dart
- connection tests.

Delete _observeResource and every Calendar-specific Observe call argument.

Connection UI behavior:

- Connector configuration/resource list is shown as the connector's current scope;
- Use with Floe is one toggle;
- turning Off pauses standing Observe and keeps connection/resources;
- turning On reviews current connection state and activates the product View bundle;
- editing connector resources while On updates Connections/SourceAuthority; it does not perform a second grant expansion ceremony;
- provider-side/source drift that makes the connection unverifiable reports recovery/unavailable and blocks reads, but does not silently rewrite grant scope.

Do not show Expert consumer checkboxes. If an audit view names first-party features, it is read-only policy information.

## 07-E: copy and product semantics

Remove copy implying a separate "Calendars available to Floe" grant selection if the connector itself already owns the selected/allowed calendars.

For native Calendar distinguish only:

1. System access / connector resource configuration.
2. Use with Floe standing Observe.

Do not reintroduce Data & privacy as a second editor.

## Tests

Flutter/product tests must prove:

1. 11 connected Calendars show one Use with Floe toggle.
2. toggle enable sends no Calendar IDs.
3. resource configuration change while enabled does not call grant re-review/enable again solely to widen scope.
4. Off keeps connection/resources intact.
5. On after Off revalidates current source and resumes standing grant.
6. remote Calendar and Gmail use the same ConnectionObserve gateway shape.
7. consumer_policy/granted_resources are absent from parsed wire.
8. third-party Expert install does not add a UI permission selector.
9. system permission recovery is distinct from Use with Floe state.

Delete stale golden/parser tests for the old DTOs rather than keeping alternate decoding.

## Residual audit

Zero production matches:

~~~
_observeResource
granted_resources
consumer_policy
CalendarAccessChange::Review
calendar_ids: ... Observe
connectionObserve(... resource:
native_calendar_access_gateway
~~~

Native Calendar provider read requests may still carry calendar_ids internally after Connections resolves them; that is not product wire.

## Verification

~~~
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo test -p floe-app
cargo build -p floe-ffi
(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)
python3 tools/architecture/check_boundaries.py
git diff --check
~~~
