> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: Connections, Experts, Knowledge and Settings

Baseline `3f4b407f8079d611224cd7adbef121f9e7e75e8e`; branch `refactor/architecture-20261002`.

This is the pre-removal natural-language behavior record for the 19 enumerated files, not the earlier inventory. No owned source was changed or removed. The assignment estimated 21 files; actual frozen directories contain 19.

Coverage: 5389 source lines; 94 direct test/testWidgets registration sites; 100 statically expanded registrations; 131 separately described scenarios. These are source-derived counts, not executed tests.

## Classification and interpretation

- **D**: Durable safety/property: re-prove at canonical owner after structural closure; old transport representation need not survive.
- **P**: Product/presentation hypothesis: reassess against accepted product intent; old strings, geometry, keys and counts do not become requirements.
- **O**: Obsolete representation/compatibility assertion: retire representation, retain separately identified safety meaning.
- **H**: Harness/support: no independent product requirement or production ownership.

Each behavior below inherits the exact file path, full-file SHA-256, current/target owner and direct dependencies from its file section. Each registration also has its exact symbol, inclusive line span and source SHA-256. Parameter/inner-table branches are individually expanded. A behavior record does not bless obsolete semantics; D retains the safety meaning, not the old protocol or class.

Read authorities: `AGENTS.md`; both `.agents/skills/{architecture-change,code-change-verification}/SKILL.md`; `docs/README.md`; `docs/architecture/{README,invariants}.md`; `docs/plans/README.md`; architecture-refactor §7; full client-tests-plan; owned-file entries in `file-read-ledger/client-symbol-map.json` and `client-test-symbol-map.json`. Source outranks labels and preparation line indexes.

## Consumer and removal boundaries

- consumer: apps/client/test/features/settings/settings_screen_test.dart:18,152–155,337–342,421–429; dependency: apps/client/test/features/actions/calendar_action_execution_test.dart:15–98 Executor; further_dependency: apps/client/test/features/actions/calendar_action_ui_test.dart:16–113 action/connection/Gateway; other_known_consumer: apps/client/test/features/conversation/agent_panel_test.dart:20; decision: Retain until Actions and Conversation ledgers/removals close the coupled test import chain. This partition cannot delete Executor or its base file.
- consumer: apps/client/test/features/settings/settings_screen_test.dart:265–271; dependency: server/internal/connectors/gmail/testdata/ready_snapshot.json; decision: Shared cross-language fixture outside scope; record the Dart file-read edge. No Go source research or fixture deletion was performed. Aggregate coordinator must reconcile Go/Rust/tool consumers before fixture removal.
- consumer: apps/client/test/features/connections/connector_screen_test.dart:6,954–968; dependency: apps/client/lib/features/day/application/fake_day_gateway.dart; decision: Production preview fake: KEEP, not shared test support.
- consumer: apps/client/test/features/experts/agent_registry_dialog_test.dart:30–42; dependency: Pretendard OTF, Flutter MaterialIcons font, Lucide package font; localization/theme/widget imports across all widget files; decision: Production assets and libraries: KEEP. These tests do not own deletion of dev dependencies, fonts, native adapters, localization or preview code.

## Supporting source reads

- `apps/client/test/support/agent_registry.dart` lines 1–170 (full file), H; file SHA-256 `187475bc2dfc191f30d30fdc42f53dddba284a4c1861f5e92bd66ff8de0b3749`; span SHA-256 `187475bc2dfc191f30d30fdc42f53dddba284a4c1861f5e92bd66ff8de0b3749`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/support/agent_vault_gateway.dart` lines 1–71 (full file), H; file SHA-256 `78d0883eb5ec6fbf55ac0b310dc73aae9519c4b386ff9f825e05eee73b998d4b`; span SHA-256 `78d0883eb5ec6fbf55ac0b310dc73aae9519c4b386ff9f825e05eee73b998d4b`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/support/agent_gateway.dart` lines 1–199 (full file), H; file SHA-256 `9956bc2c06f3a47d77a52ac79afeb2992c834895dbe9f517466602a09c47acb5`; span SHA-256 `9956bc2c06f3a47d77a52ac79afeb2992c834895dbe9f517466602a09c47acb5`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/support/app_wire_transport.dart` lines 1–59 (full file), H; file SHA-256 `ace9f2982ef66db290c102679ec48286cdd5cbdfea230f87a7de0c969a4b6b17`; span SHA-256 `ace9f2982ef66db290c102679ec48286cdd5cbdfea230f87a7de0c969a4b6b17`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/support/server_credentials.dart` lines 1–16 (full file), H; file SHA-256 `deba571bebd5428a9a98e805dfe299b7dff9c04e45d19ae84087c51647caae71`; span SHA-256 `deba571bebd5428a9a98e805dfe299b7dff9c04e45d19ae84087c51647caae71`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/features/actions/calendar_action_execution_test.dart` lines 1–98 (helper-only partial file), H; file SHA-256 `196933ea6dde056c7951551abb56f8fcf298159eddf9372255109905889d133c`; span SHA-256 `012b71cc452b730f0d5866b57d658ff589a14173aa06b94fe4b249315e9b64b1`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.
- `apps/client/test/features/actions/calendar_action_ui_test.dart` lines 1–113 (helper-only partial file), H; file SHA-256 `c747b226240aa8150140d4304c46279e85cce3826c1a572f8141b5e72d80d5c1`; span SHA-256 `eb38af05a760f4886c73df61ac2f7a804be1d1fa67e624cd435c37b56adca269`. Outside this partition deletion ownership; coordinate every consumer before removing shared test support. Product imports remain KEEP.

## Source-by-source behavior record

## `apps/client/test/features/connections/agent_connections_test.dart`

Full read: lines 1–183; 3 registration sites / 3 expanded registrations / 4 scenarios.
File SHA-256: `bbfec8fc6b53cc6617124cdc731ead14227ad886ebad262eeda5d50533c8d4d6`

Current owner: NativeConnectionsGateway; AgentConnection decoder; AgentConnectionSettings
Target owner/disposition: Connections owns source state/authority; client connections gateway validates owner projections and settings renders them.
Harness (H): _connection (124–183) is an inline synthetic provider descriptor/degraded snapshot; CallbackAppWireTransport is shared H support.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/runtime/local_owner_gateways.dart` → `apps/client/lib/app/runtime/local_owner_gateways.dart`
- L3: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`
- L5: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L6: `package:floe_client/features/connections/presentation/agent_connection_settings.dart` → `apps/client/lib/features/connections/presentation/agent_connection_settings.dart`
- L7: `package:floe_client/features/connections/domain/agent_connections.dart` → `apps/client/lib/features/connections/domain/agent_connections.dart`
- L8: `package:flutter/material.dart` → `SDK/package dependency`
- L9: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### agent_connections_test#1: 'native gateway reads strict provider-neutral connection snapshots'
Source: `apps/client/test/features/connections/agent_connections_test.dart:12–46`; `test`; 1 expanded registration(s); SHA-256 `074de6a344cc46fdadaaf1731309b7068a24ed5bc7d6d22a4f1443fa790dbd98`.

- **agent_connections_test#1.1 — degraded yet usable snapshot [D]**
  - Preconditions: Callback AppWire returns a ready connections operation containing an Apple EventKit source with an observe capability, separate create capability, partial_fetch failure and bounded timeline view.
  - Input/action: Read connections for person-1.
  - Expected outcome: Send only connections.overview; decode provider apple_event_kit, degraded state, item count 2, partial_fetch and usable=true.
  - Failure/race and evidence limits: A degraded source remains usable when its existing bounded projection is present; no refresh, native acquisition or Action execution occurs.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_connections_test#2: 'connection parser rejects authority and projection escalation'
Source: `apps/client/test/features/connections/agent_connections_test.dart:48–87`; `test`; 1 expanded registration(s); SHA-256 `bf82e26228694f835ed73a6dca77583d224d976cf9b01ab4930450460da55824`.

- **agent_connections_test#2.1 — act capability cannot claim an observe projection [D]**
  - Branch evidence: L55–74 within this file.
  - Preconditions: Clone the valid descriptor.
  - Input/action: Append calendar.events.delete with authority act, read scope and output_view_id calendar.timeline.
  - Expected outcome: Reject the snapshot with FormatException.
  - Failure/race and evidence limits: Malformed capability authority is rejected before a client accepts the projection.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_connections_test#2.2 — projection bound cannot be exceeded [D]**
  - Branch evidence: L75–86 within this file.
  - Preconditions: Clone the valid timeline view whose descriptor max_items is 128.
  - Input/action: Set item_count to 129 and parse.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Tests this one over-bound count, not every size/provenance limit; numeric limit is legacy evidence.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_connections_test#3: 'settings explains degraded source without implying action access'
Source: `apps/client/test/features/connections/agent_connections_test.dart:89–121`; `testWidgets`; 1 expanded registration(s); SHA-256 `c99d30b47a52bc1d46e4d8069515ba3e656cb5ccce7464127afb79ec0787ad54`.

- **agent_connections_test#3.1 — degraded status and separate Action approval [P]**
  - Preconditions: Render the valid degraded source, not loading and not failed.
  - Input/action: Mount AgentConnectionSettings.
  - Expected outcome: Show Apple Calendar, Partial, partial-refresh explanation and separate approval for Actions.
  - Failure/race and evidence limits: No click or owner mutation is exercised.
  - Target classification/disposition: Preserve truthful degraded-state and source-versus-Action authority distinction; reassess exact copy/layout.
## `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart`

Full read: lines 1–173; 6 registration sites / 6 expanded registrations / 8 scenarios.
File SHA-256: `4cdd9327fdf08f86d15026db6f6dd5ec145e54d049b8814b64a8b023c4160fea`

Current owner: AppWireCalendarSourceGateway and SourceConnection decoder
Target owner/disposition: Connections canonical source command/query gateway; App-derived local execution identity and source-owned CAS.
Harness (H): _source (10–25), _remoteSource (27–32) and person/device constants are inline H fixtures; callback transport does not execute Rust.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/app_wire_calendar_source_gateway.dart` → `apps/client/lib/features/connections/application/app_wire_calendar_source_gateway.dart`
- L2: `package:floe_client/features/connections/domain/source_connection.dart` → `apps/client/lib/features/connections/domain/source_connection.dart`
- L3: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L5: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`

### app_wire_calendar_source_gateway_test#1: 'native source query decodes Connections authority'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:35–45`; `test`; 1 expanded registration(s); SHA-256 `f70b9a13feffc80f771595c659bb380d75d2db99db0da2c3645d1cfe0a47febc`.

- **app_wire_calendar_source_gateway_test#1.1 — native inspection [D]**
  - Preconditions: A native source has local mac-local owner, home resource, revision 1 and source authority epoch 1.
  - Input/action: inspectNative for the fixture person.
  - Expected outcome: Issue only connections.native_calendar.source and decode connection calendar-source, selected home and authority epoch 1.
  - Failure/race and evidence limits: No injected failure or race; only the described success path is asserted.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### app_wire_calendar_source_gateway_test#2: 'configure submits expected revision, not a caller-computed next'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:47–83`; `test`; 1 expanded registration(s); SHA-256 `7b70ba4681ad8caaa6927e001c102df2b69d7c8e1590f697ea40537716ae6536`.

- **app_wire_calendar_source_gateway_test#2.1 — native configure CAS [D]**
  - Preconditions: Current native source is revision 7 with local owner.
  - Input/action: Configure selected Work resource.
  - Expected outcome: Mutation type configure binds calendar-source and expected_revision=7; resources carry work/Work; no caller connection_revision or source_authority is sent; owner response revision 8 is accepted.
  - Failure/race and evidence limits: The test inspects CAS input rather than injecting a rejected stale revision; server-side conflict is not exercised.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### app_wire_calendar_source_gateway_test#3: 'foreign execution owner cannot be configured'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:85–98`; `test`; 1 expanded registration(s); SHA-256 `0f3452dd0a22afea1200e7d47a6cca128087ace6319c61985ec34768dbf89223`.

- **app_wire_calendar_source_gateway_test#3.1 — foreign owner mutation blocked [D]**
  - Preconditions: Current native source execution_owner_id is foreign-device; callback would throw on any I/O.
  - Input/action: Attempt disconnectNative despite the label referring to configure.
  - Expected outcome: Reject with FormatException without reaching transport.
  - Failure/race and evidence limits: Only disconnect is actually called; do not claim separate configure coverage.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### app_wire_calendar_source_gateway_test#4: 'remote binding sends local expected revision without producer revision'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:100–137`; `test`; 1 expanded registration(s); SHA-256 `1d6991873bc279f3fc177613ecf9d986a1768118561514f8d34ce25ddafb5551`.

- **app_wire_calendar_source_gateway_test#4.1 — remote bind CAS [D]**
  - Preconditions: Current remote Google source local revision 4, local execution owner and null native fingerprint.
  - Input/action: Bind server-connection to Home.
  - Expected outcome: Send connections.remote_calendar.mutate / bind with connector and connection IDs and expected_revision=4; omit connection_revision and source_authority; accept owner revision 5.
  - Failure/race and evidence limits: Does not send producer revision as local source revision; no real remote provider or Rust state is touched.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### app_wire_calendar_source_gateway_test#5: 'remote source query rejects a foreign execution owner'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:139–150`; `test`; 1 expanded registration(s); SHA-256 `c5e065a353b79757f69d97e396a94c8daff66b6ea140113f9a49ffaf47a22876`.

- **app_wire_calendar_source_gateway_test#5.1 — foreign remote source read blocked [D]**
  - Preconditions: Inspect callback returns a remote source owned by foreign-device.
  - Input/action: inspectRemote for fixture person.
  - Expected outcome: Reject the returned list with FormatException.
  - Failure/race and evidence limits: Read rejection prevents foreign ownership becoming accepted client state.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### app_wire_calendar_source_gateway_test#6: 'source decoder rejects malformed resource and native subject'
Source: `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart:152–172`; `test`; 1 expanded registration(s); SHA-256 `37bc8655dd795925323bd1688af51bc55fda2d72ebbccfb258ccc0875064d657`.

- **app_wire_calendar_source_gateway_test#6.1 — non-map resource [D]**
  - Branch evidence: L154–157 within this file.
  - Preconditions: Start from otherwise valid native source fixture.
  - Input/action: Replace resources with numeric entry 42. Parse SourceConnection.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Strict source decoding is tested locally, without transport.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **app_wire_calendar_source_gateway_test#6.2 — unsorted resource identity [D]**
  - Branch evidence: L158–164 within this file.
  - Preconditions: Start from otherwise valid native source fixture.
  - Input/action: Provide Work then Home resources, contrary to canonical ordering. Parse SourceConnection.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Strict source decoding is tested locally, without transport.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **app_wire_calendar_source_gateway_test#6.3 — missing native subject [D]**
  - Branch evidence: L165–165 within this file.
  - Preconditions: Start from otherwise valid native source fixture.
  - Input/action: Set native_subject_fingerprint to null on native EventKit source. Parse SourceConnection.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Strict source decoding is tested locally, without transport.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/connection_observe_gateway_test.dart`

Full read: lines 1–114; 2 registration sites / 2 expanded registrations / 2 scenarios.
File SHA-256: `143d755ec6480540c304efda37a5c81e94b035bcafe440875998b5b4fc539312`

Current owner: AppWireConnectionObserveGateway; ConnectionObserveReview decoder
Target owner/disposition: Connections/Access owner review and enablement commands; product client echoes the reviewed bundle, not arbitrary resource authority.
Harness (H): overview (10–23) and expectation (25–44) synthesize active/paused members plus source authority, revision, producer fingerprint and policy digest.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/connection_observe_gateway.dart` → `apps/client/lib/features/connections/application/connection_observe_gateway.dart`
- L2: `package:floe_client/features/connections/domain/connection_observe.dart` → `apps/client/lib/features/connections/domain/connection_observe.dart`
- L3: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L5: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`

### connection_observe_gateway_test#1: 'inspect, review and enable use one connection-level owner wire'
Source: `apps/client/test/features/connections/connection_observe_gateway_test.dart:47–108`; `test`; 1 expanded registration(s); SHA-256 `f1ad50caccda49f5b29bcbf4e53d113525b4869a2cae0d517fffd0098d735148`.

- **connection_observe_gateway_test#1.1 — inspect then review then enable [D]**
  - Preconditions: Google connection is paused; owner callback retains completed results by operation ID.
  - Input/action: Inspect connector/connection; review it; set enabled=true using that review.
  - Expected outcome: Inspection yields provider-calendar-a; start order is inspect, review, set_enabled; final state enabled; mutation expected exactly equals the owner review including source revision, authority, producer and member binding; mutation has no source_resources or selected_handles.
  - Failure/race and evidence limits: No concurrent stale review is injected; binding integrity is asserted by exact echo, not independent authority manufacture.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### connection_observe_gateway_test#2: 'review rejects remote routing authority in the product snapshot'
Source: `apps/client/test/features/connections/connection_observe_gateway_test.dart:110–113`; `test`; 1 expanded registration(s); SHA-256 `d5bce9dc3c3d40982f8f770aa7c2407437fafc569351ee338378b9b5bdf080eb`.

- **connection_observe_gateway_test#2.1 — no remote routing authority in review [D]**
  - Preconditions: Start from valid connection-level expectation.
  - Input/action: Add provider_identity=provider and parse review.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Unknown routing identity must not become product-side authority.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/connector_screen_test.dart`

Full read: lines 1–1206; 13 registration sites / 15 expanded registrations / 15 scenarios.
File SHA-256: `08fb7fc20365e340dbe48dfa8f33e3d2d71f84ad006490f13b9423eb8c4a6b14`

Current owner: ConnectorScreen composition; CalendarSourceGateway; ConnectionObserveGateway; AppleContextApi; CalendarConnectionView
Target owner/disposition: Connections owns source identity/selection and presentation; Access/Connections owns current-source Observe; Day mirrors remain projections.
Harness (H): _RecordingCalendarGateway (780–871), _StubCalendarObserveGateway (873–952), _DeviceCalendarGateway (954–968), _CatalogClient (970–1024), _CalendarCatalogClient (1026–1049), connector constants (1051–1112), _AppleConnections/_appleConnection (1114–1206) are H. FakeDayGateway is imported product preview code (KEEP). No Rust call is executed despite one registration label.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/design_tokens.dart` → `apps/client/lib/app/design_tokens.dart`
- L2: `package:floe_client/app/floe_squircle.dart` → `apps/client/lib/app/floe_squircle.dart`
- L3: `package:floe_client/app/floe_primitives.dart` → `apps/client/lib/app/floe_primitives.dart`
- L4: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L5: `package:floe_client/features/day/application/calendar_gateway.dart` → `apps/client/lib/features/day/application/calendar_gateway.dart`
- L6: `package:floe_client/features/day/application/fake_day_gateway.dart` → `apps/client/lib/features/day/application/fake_day_gateway.dart`
- L7: `package:floe_client/features/day/domain/day_models.dart` → `apps/client/lib/features/day/domain/day_models.dart`
- L8: `package:floe_client/features/connections/application/connection_observe_gateway.dart` → `apps/client/lib/features/connections/application/connection_observe_gateway.dart`
- L9: `package:floe_client/features/connections/domain/connection_observe.dart` → `apps/client/lib/features/connections/domain/connection_observe.dart`
- L10: `package:floe_client/features/connections/domain/source_connection.dart` → `apps/client/lib/features/connections/domain/source_connection.dart`
- L11: `package:floe_client/features/connections/application/calendar_connection_view.dart` → `apps/client/lib/features/connections/application/calendar_connection_view.dart`
- L12: `package:floe_client/features/connections/application/calendar_source_gateway.dart` → `apps/client/lib/features/connections/application/calendar_source_gateway.dart`
- L13: `package:floe_client/features/connections/presentation/connector_screen.dart` → `apps/client/lib/features/connections/presentation/connector_screen.dart`
- L14: `package:floe_client/features/conversation/application/agent_controller.dart` → `apps/client/lib/features/conversation/application/agent_controller.dart`
- L15: `package:floe_client/infrastructure/native/apple_context_gateway.dart` → `apps/client/lib/infrastructure/native/apple_context_gateway.dart`
- L16: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L17: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L18: `package:figma_squircle/figma_squircle.dart` → `SDK/package dependency`
- L19: `package:flutter/material.dart` → `SDK/package dependency`
- L20: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L22: `../../support/server_credentials.dart` → `apps/client/test/support/server_credentials.dart`
- L23: `../../support/agent_registry.dart` → `apps/client/test/support/agent_registry.dart`

### connector_screen_test#1: 'Apple connections expose status and exact connection detail'
Source: `apps/client/test/features/connections/connector_screen_test.dart:26–75`; `testWidgets`; 1 expanded registration(s); SHA-256 `f7d0f29e79b1887028bc4bedfcd5815bbf86fc32a7bd5b4dd94ffcde1d42a4a6`.

- **connector_screen_test#1.1 — Apple inventory and revoked Contacts detail [P]**
  - Preconditions: iOS platform; fake Contacts revoked, Attention unsupported and Health pending entries, all bound to apple-test.
  - Input/action: Render catalog, open Contacts.
  - Expected outcome: All three cards present; detail identifies contacts.apple and apple-test, System access and Allow access; no Review selection.
  - Failure/race and evidence limits: Permission request is not tapped and Health projection is not acquired; only surface/state presentation is asserted.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### connector_screen_test#2: 'service has only an icon surface and still opens details'
Source: `apps/client/test/features/connections/connector_screen_test.dart:77–137`; `testWidgets`; 1 expanded registration(s); SHA-256 `6b4e81efb5f9508c4cf0ff441853db25f8b3fcce692281e57579908efac53bae`.

- **connector_screen_test#2.1 — macOS service visual/navigation [P]**
  - Preconditions: macOS with no gateway/connection and known local device.
  - Input/action: Render service, open macOS Calendar and return.
  - Expected outcome: One FloePressable, icon FloeSquircle without its border; enclosing anti-aliased neutral0 Material has neutral200 width-1 squircle border; macOS Calendar wording (not Apple Calendar), no device-binding detail on card; Back returns Available services; no remote-server surface or exception.
  - Failure/race and evidence limits: Exact shape/style is legacy presentation evidence; no backend mutation.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### connector_screen_test#3: 'macOS detail composes system, selected, and consumer access'
Source: `apps/client/test/features/connections/connector_screen_test.dart:139–188`; `testWidgets`; 1 expanded registration(s); SHA-256 `1e0f50a180fbb0e024bee4c8beeb679ec5567fe7d0b869118c774bf30a804b15`.

- **connector_screen_test#3.1 — system access/source selection composition [P]**
  - Preconditions: Loaded AgentController registry; local EventKit connection Home revision 1; fake system access allowed; open device detail initially.
  - Input/action: Mount and settle.
  - Expected outcome: System access Allowed and Calendars available to Floe appear; no Data Floe can use or old calendar-access-setup control.
  - Failure/race and evidence limits: No grant edit or system permission prompt; absence is UI ownership evidence.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### connector_screen_test#4: 'device detail manages one Use with Floe control'
Source: `apps/client/test/features/connections/connector_screen_test.dart:190–265`; `testWidgets`; 1 expanded registration(s); SHA-256 `3684b15b1a65a05064a57429c6bd3dd1632440bc2d3b64861a0a6f5daac337e1`.

- **connector_screen_test#4.1 — one connection-level Observe toggle [D]**
  - Preconditions: Local Calendar source has eleven calendar IDs, source authority and needs_review Observe.
  - Input/action: Mount detail, toggle Use with Floe on, then off.
  - Expected outcome: One Observe section/control; initial Needs review; calls inspect→review→enable, then disable; Active then Paused; reviewed bundle string excludes calendar-0.
  - Failure/race and evidence limits: Provider resource IDs are not copied into per-resource grants. This test auto-enables from this device control after review and does not press a separate Allow dialog, unlike server/personal card tests; preserve explicit owner review binding, reassess UX difference.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### connector_screen_test#5: 'calendar resource refresh preserves active Observe without review'
Source: `apps/client/test/features/connections/connector_screen_test.dart:267–323`; `testWidgets`; 1 expanded registration(s); SHA-256 `32453e259b8e7b4eff9154df3f2197e04f690264f558b221cc4363f3abe32f96`.

- **connector_screen_test#5.1 — active source refresh [D]**
  - Preconditions: Observe active with Home; same connection ID is initially source revision/epoch 1.
  - Input/action: Rebuild with Home+Work and revision/epoch 2; fake sourceResources updated.
  - Expected outcome: Inspect twice only; Active remains; copy says current calendars are used; no Using 1 of 2 summary.
  - Failure/race and evidence limits: Refreshing source resources does not call review/enable; no stale asynchronous response is injected. UI statement is not proof that arbitrary expansion may bypass source-processing policy.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### connector_screen_test#6: 'device-native and disconnected server catalog are composed'
Source: `apps/client/test/features/connections/connector_screen_test.dart:325–369`; `testWidgets`; 1 expanded registration(s); SHA-256 `ef317ca5ee2edcb4dccd67d507cc3d2239a897fc1ec35459aec0a14275c311d2`.

- **connector_screen_test#6.1 — native and server catalog [P]**
  - Preconditions: macOS no device connection; server fake returns available GitHub and unavailable Gmail.
  - Input/action: Render catalog.
  - Expected outcome: Native Calendar, GitHub and Gmail cards/names all appear; two Unavailable labels; macOS Calendar, Unavailable services and GitHub-before-Gmail ordering; no binding/server-address technical copy.
  - Failure/race and evidence limits: No connect mutation or unavailable-service retry exercised.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### connector_screen_test#7: '${testCase.platform.name} exposes its connected device calendar'
Source: `apps/client/test/features/connections/connector_screen_test.dart:404–459`; `testWidgets`; 3 expanded registration(s); SHA-256 `22609cc7a35b0b3df2b18d8e31c361310220d13ccd2e7b506bb7490dc6a80ada`.

- **connector_screen_test#7.1 — iOS [P]**
  - Branch evidence: L381–387 within this file.
  - Preconditions: Platform iOS; connected event_kit Calendar with one calendar at revision 1; local fake gateway.
  - Input/action: Render then tap Apple Calendar.
  - Expected outcome: Show connector-calendar-apple, localized Apple Calendar and calendars already on this iPhone or iPad; connected-services count 1 and Connected status; detail has two name instances, one description and Back; no widget exception.
  - Failure/race and evidence limits: No platform API used. Apple presentation fixture only.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

- **connector_screen_test#7.2 — macOS [P]**
  - Branch evidence: L388–394 within this file.
  - Preconditions: Platform macOS; connected event_kit Calendar with one calendar at revision 1; local fake gateway.
  - Input/action: Render then tap macOS Calendar.
  - Expected outcome: Show connector-calendar-apple, localized macOS Calendar and calendars already on this Mac; connected-services count 1 and Connected status; detail has two name instances, one description and Back; no widget exception.
  - Failure/race and evidence limits: No platform API used. Apple presentation fixture only.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

- **connector_screen_test#7.3 — Android [P]**
  - Branch evidence: L395–402 within this file.
  - Preconditions: Platform Android; connected android Calendar with one calendar at revision 1; local fake gateway.
  - Input/action: Render then tap Android Calendar.
  - Expected outcome: Show connector-calendar-android, localized Android Calendar and selected calendars on this Android device; connected-services count 1 and Connected status; detail has two name instances, one description and Back; no widget exception.
  - Failure/race and evidence limits: No platform API used. Dormant Android evidence only; no parity/build authorization.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### connector_screen_test#8: 'does not count a connection from another device provider'
Source: `apps/client/test/features/connections/connector_screen_test.dart:462–503`; `testWidgets`; 1 expanded registration(s); SHA-256 `2e2e483f22feaf9f6f55da8505f5902c75e205c2106051cb78ed38a3abaa3006`.

- **connector_screen_test#8.1 — mismatched device provider [D]**
  - Preconditions: Android presentation receives EventKit connection with empty calendars.
  - Input/action: Render.
  - Expected outcome: Show Android Calendar card as Available with Available services; no connected-service count.
  - Failure/race and evidence limits: Foreign provider must not be mistaken for current device source; Android UI remains dormant evidence.
  - Target classification/disposition: Preserve provider/device identity distinction in canonical source projection; retire dormant platform-specific presentation assertion as appropriate.

### connector_screen_test#9: 'binds the exact server Calendar selector into Rust state'
Source: `apps/client/test/features/connections/connector_screen_test.dart:505–546`; `testWidgets`; 1 expanded registration(s); SHA-256 `65372cfcb0202d5e4677213d91f57358c7d7036f9488c03a6b877e7159f55f80`.

- **connector_screen_test#9.1 — exact remote selector binding [D]**
  - Preconditions: No active source; single connected Google catalog entry with two IDs opaque,id and primary@example.test; recording source gateway.
  - Input/action: Mount screen and allow catalog reconciliation.
  - Expected outcome: Bind exact Google connection ...da61 to local-test-device/google_calendar with null expected local revision; pass both handles verbatim; onChanged once.
  - Failure/race and evidence limits: Despite test name, binding goes only to fake gateway, not Rust. Automatic single-provider choice is P; exact identity/opaque handle preservation is D.
  - Target classification/disposition: Preserve exact owner-bound selector intent; reassess automatic single-provider source selection with accepted Connections design.

### connector_screen_test#10: 'requires an explicit choice when two server calendars connect'
Source: `apps/client/test/features/connections/connector_screen_test.dart:548–609`; `testWidgets`; 1 expanded registration(s); SHA-256 `b97d5262350c07139c1ac5cc3669a7ec553ddce58ba26275fa7387b27fd7a801`.

- **connector_screen_test#10.1 — two remote providers require choice [D]**
  - Preconditions: No active source; Google and Microsoft catalog connections both connected.
  - Input/action: Render; then explicitly choose Microsoft Use for Floe & Schedule.
  - Expected outcome: Before click no bind/callback and two choice controls; after click bind exact Microsoft ID with null expected local revision, microsoft_calendar and calendar@microsoft.test; callback once.
  - Failure/race and evidence limits: Catalog availability alone cannot arbitrarily choose among multiple providers.
  - Target classification/disposition: Preserve explicit ambiguous-source choice and exact binding identity; reassess copy and selection presentation.

### connector_screen_test#11: 'catalog refresh cannot disconnect a remote source'
Source: `apps/client/test/features/connections/connector_screen_test.dart:611–648`; `testWidgets`; 1 expanded registration(s); SHA-256 `29cf646dbb2dc421492c911541ce76c813c716689f09e7139b792a200ec6b73b`.

- **connector_screen_test#11.1 — catalog disappearance is not disconnect [D]**
  - Preconditions: Existing remote Google SourceConnection is supplied as active/remote source; refreshed server catalog empty.
  - Input/action: Render and settle.
  - Expected outcome: disconnectCount stays zero and no bind connection ID is recorded.
  - Failure/race and evidence limits: Missing catalog data is not authorization to disconnect a durable source.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### connector_screen_test#12: 'keeps device calendar active until server is explicitly chosen'
Source: `apps/client/test/features/connections/connector_screen_test.dart:650–718`; `testWidgets`; 1 expanded registration(s); SHA-256 `e1c7624d92fda1e73f8abf9a09c66065a3a25caeea7e7f7913d3f60eebb08cb2`.

- **connector_screen_test#12.1 — device source stays active until choice [D]**
  - Preconditions: Active EventKit device source revision 4 plus connected Google catalog.
  - Input/action: Render, then explicitly select Google.
  - Expected outcome: Before choice no bind/callback; macOS Calendar is shown Active and supplying context; after choice fake provider google_calendar and callback once.
  - Failure/race and evidence limits: Availability of server source cannot silently replace existing device source.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### connector_screen_test#13: 'offers device calendar selection while a server calendar is active'
Source: `apps/client/test/features/connections/connector_screen_test.dart:720–777`; `testWidgets`; 1 expanded registration(s); SHA-256 `234dcb29373fb3628fae89fbaac723859689665ed20082edc112d938964d1428`.

- **connector_screen_test#13.1 — navigate back to device choice [P]**
  - Preconditions: Active Google source revision 7 plus local Calendar gateway.
  - Input/action: Click Choose calendars on device source.
  - Expected outcome: Google is initially named current context supplier; device detail opens with Back to connections and Connect controls.
  - Failure/race and evidence limits: Navigation alone does not assert source replacement or native writes.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.
## `apps/client/test/features/connections/local_server_http_test.dart`

Full read: lines 1–337; 4 registration sites / 4 expanded registrations / 4 scenarios.
File SHA-256: `0777f69bd521c0d66f60faaf98ff833353564f0c41811109715a75d255c28a1d`

Current owner: LocalServerClient HTTP request/response boundary
Target owner/disposition: Connections verified loopback adapter and canonical inference/connector API; old direct client-generation route is subject to canonical cutover.
Harness (H): Each test creates a real ephemeral IPv4 loopback HttpServer and closes it in teardown. Responses are fixture JSON; MemoryServerCredentials is H. No real Go server/provider is started and no stored user credential is used.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `dart:convert` → `SDK/package dependency`
- L2: `dart:io` → `SDK/package dependency`
- L4: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L5: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L7: `../../support/server_credentials.dart` → `apps/client/test/support/server_credentials.dart`

### local_server_http_test#1: 'purpose, connection, trace and generation contracts use only v1'
Source: `apps/client/test/features/connections/local_server_http_test.dart:10–76`; `test`; 1 expanded registration(s); SHA-256 `acf6f91369d5ded73809a6f9f13d739e17e6d92fb8723784722b9fc9c360168a`.

- **local_server_http_test#1.1 — four HTTP endpoints and purpose request [O]**
  - Preconditions: Ephemeral loopback server serves v1 purpose map, empty connections/traces, and synthetic generate result.
  - Input/action: checkConnection, connections, privacyActivity, generate everydayAssistance with synthetic input/schema.
  - Expected outcome: Requests arrive in order at /v1/inference-purposes, /v1/connections, /v1/traces, /v1/generate; generation sends schema 1 and purpose everyday_assistance without inference_class; returns output {"ok":true}; both lists empty.
  - Failure/race and evidence limits: This proves old Dart-to-loopback transport shape only, not model routing, remote policy or actual inference.
  - Target classification/disposition: Retire direct product model-generation/inference_class-era routing assertions where replaced; preserve purpose-only canonical inference intent and strict paired boundaries, not literal internal versions.

### local_server_http_test#2: 'HTTP client does not follow redirects or forward tokens'
Source: `apps/client/test/features/connections/local_server_http_test.dart:78–100`; `test`; 1 expanded registration(s); SHA-256 `457952d04aeb7f25d6fdbdb0714e6ebf36b0ae4aab0658854aebda0bc113726a`.

- **local_server_http_test#2.1 — redirect token containment [D]**
  - Preconditions: Loopback server responds 302 to token-bearing request with Location pointing to /stolen.
  - Input/action: Request inference purposes with fixture bearer token.
  - Expected outcome: Throw ServerConnectionException; server sees exactly one request.
  - Failure/race and evidence limits: Client must not follow redirect or forward token to redirected destination; even same-loopback redirect is refused.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_http_test#3: 'pairing identity and connector lifecycle follow the paired API'
Source: `apps/client/test/features/connections/local_server_http_test.dart:102–282`; `test`; 1 expanded registration(s); SHA-256 `88e86b70089045bf76c1bccdc7c6c21f46a588f03376f871abd3305128e7f9e9`.

- **local_server_http_test#3.1 — paired lifecycle and catalog refresh identity [D]**
  - Preconditions: Ephemeral server verifies paired default person and local-persistent-device; first catalog has available GitHub and unavailable Gmail; later catalog injects connected ID not-a-uuid.
  - Input/action: Start pairing; read catalog; connect GitHub with one-shot-secret and owner/repository scope; PATCH scope at connection revision 1; DELETE exact connection at revision 2; refresh catalog.
  - Expected outcome: Pair/start carries active identity; catalog preserves device and connector names/status; connect returns connected; scope update returns floe/server; mutation requests bind exact UUID and revisions; calls follow POST start, GET catalog, POST connect, PATCH scope, DELETE connector, GET catalog.
  - Failure/race and evidence limits: Refreshed malformed connected catalog raises invalid_response; initial connect attempt fixture uses non-UUID connection-1 and is accepted in attempt model, so do not claim universal UUID validation. Tests CAS serialization, not server conflict handling.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_http_test#4: 'strict pairing start binds issuer and preserves challenge identity'
Source: `apps/client/test/features/connections/local_server_http_test.dart:284–336`; `test`; 1 expanded registration(s); SHA-256 `8f4256807b3abfdfbfdaa6d976ab36d2dbebca7c2cf395569e3aff05b8a48acb`.

- **local_server_http_test#4.1 — issuer-bound strict pairing challenge [D]**
  - Preconditions: Loopback fixture provides separate pairing/challenge UUIDs, expected person/device, producer/issuer fingerprints and challenge/signature.
  - Input/action: startPairingStrict with issuer key ID and public key.
  - Expected outcome: POST /pair/start sends schema 1 and exact issuer identity; response retains distinct pairing versus challenge IDs and owner fingerprint.
  - Failure/race and evidence limits: No cryptographic signature verification or altered challenge failure is tested; only wire identity preservation.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/local_server_test.dart`

Full read: lines 1–348; 9 registration sites / 9 expanded registrations / 16 scenarios.
File SHA-256: `fe3e184a40a5ddabc9a1e389fb82395c27b73a8beb31dca64f653c67bbba95a9`

Current owner: LocalServerClient, KeychainServerCredentialStore and LocalServerPanel
Target owner/disposition: Connections pairing persistence and loopback transport; platform secure-store boundary.
Harness (H): _PairingClient (237–275) fakes connection checks, pairing challenge and cancel request; _PairingGateway (277–312), _pairingStatus (314–323), issuer/producer fixtures (325–339), _FailingCredentials (341–348) and MemoryServerCredentials are H. Keychain MethodChannel is mocked; no actual key slot is accessed.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/domain/remote_owner_models.dart` → `apps/client/lib/features/connections/domain/remote_owner_models.dart`
- L3: `dart:async` → `SDK/package dependency`
- L4: `dart:convert` → `SDK/package dependency`
- L6: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L7: `package:floe_client/features/connections/presentation/local_server_panel.dart` → `apps/client/lib/features/connections/presentation/local_server_panel.dart`
- L8: `package:floe_client/features/connections/application/remote_pairing_gateway.dart` → `apps/client/lib/features/connections/application/remote_pairing_gateway.dart`
- L9: `package:floe_client/app/local_identity.dart` → `apps/client/lib/app/local_identity.dart`
- L10: `package:flutter/material.dart` → `SDK/package dependency`
- L11: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L13: `../../support/server_credentials.dart` → `apps/client/test/support/server_credentials.dart`

### local_server_test#1: 'approved token is released only after verified secure persistence'
Source: `apps/client/test/features/connections/local_server_test.dart:16–46`; `testWidgets`; 1 expanded registration(s); SHA-256 `d785b18db6f1f85d7521845bc762e688511d7baa32eaf0ef58eee163e497e02e`.

- **local_server_test#1.1 — persistence failure then retry [D]**
  - Preconditions: Panel uses approved pairing gateway and credential store whose writes initially throw.
  - Input/action: Pair device; observe failed persistence; allow writes; press pairing retry.
  - Expected outcome: Before retry store stays null, release count zero, no Connected label; after retry restored token has expected person/device, release count exactly one and Connected label appears.
  - Failure/race and evidence limits: Never release approved credential result before secure persistence succeeds. The fixture injects write failure; readback integrity is inferred by reading stored connection, not a separate corrupt-readback branch.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#2: 'stalled Keychain reads stop waiting without forgetting credentials'
Source: `apps/client/test/features/connections/local_server_test.dart:48–75`; `testWidgets`; 1 expanded registration(s); SHA-256 `18d36e8a7c46f3fb3c70848cb3bd6d8c97dd7fac9ff2d08a09e5d68323bf6d46`.

- **local_server_test#2.1 — stalled Keychain read [D]**
  - Preconditions: Mock secure-store channel records methods and returns an unresolved completer.
  - Input/action: Call credential read and advance fake widget time five seconds.
  - Expected outcome: Read raises TimeoutException and only read was invoked.
  - Failure/race and evidence limits: Timeout stops waiting without invoking delete or forgetting credentials; teardown later completes mocked read, not a real keychain operation.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#3: 'addresses stay literal loopback with no path or credentials'
Source: `apps/client/test/features/connections/local_server_test.dart:77–96`; `test`; 1 expanded registration(s); SHA-256 `ab023c88d99838b34b73437f473ac14d568cd91986238dbc9119e7aa329c7fdf`.

- **local_server_test#3.1 — localhost normalization [D]**
  - Branch evidence: L78–81 within this file.
  - Preconditions: No network call; address normalizer is pure.
  - Input/action: Normalize http://localhost:9431/.
  - Expected outcome: Return http://127.0.0.1:9431.
  - Failure/race and evidence limits: Preserves explicit loopback port and strips only root slash.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.2 — remote host [D]**
  - Branch evidence: L83–83 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize https://example.com.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.3 — LAN host [D]**
  - Branch evidence: L84–84 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://192.168.1.1:8431.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.4 — path [D]**
  - Branch evidence: L85–85 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://127.0.0.1:8431/path.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.5 — userinfo [D]**
  - Branch evidence: L86–86 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://user@127.0.0.1.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.6 — query [D]**
  - Branch evidence: L87–87 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://127.0.0.1?key=1.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.7 — fragment [D]**
  - Branch evidence: L88–88 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://127.0.0.1#secret.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **local_server_test#3.8 — zero port [D]**
  - Branch evidence: L89–89 within this file.
  - Preconditions: Pure normalizer; no credentials or network.
  - Input/action: Normalize http://127.0.0.1:0.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Reject unsafe/noncanonical loopback destination before any transmission.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#4: 'connection credentials persist independently of provider keys'
Source: `apps/client/test/features/connections/local_server_test.dart:98–124`; `test`; 1 expanded registration(s); SHA-256 `a0d654e410b3c41139d29dda3b69510c37c7972ba7b9063e740b3ee0248e4944`.

- **local_server_test#4.1 — minimal credential round trip [D]**
  - Preconditions: Empty MemoryServerCredentials; connection has loopback URL, long token, client ID and matching default person/device.
  - Input/action: Save; reconstruct client over same store; restore; explicitly delete in-memory store; read again.
  - Expected outcome: Serialized keys exactly base_url/token/client_id/person_id/device_id, with no model or inference_class; after store deletion connection is null.
  - Failure/race and evidence limits: Persistence is independent of model/provider key configuration; deletion affects only the synthetic store.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#5: 'invalid saved credential fails closed'
Source: `apps/client/test/features/connections/local_server_test.dart:126–137`; `test`; 1 expanded registration(s); SHA-256 `37e93d307f1f85c51006e0fc02b07fec1732b8ced0642461fc3c3be44b73128e`.

- **local_server_test#5.1 — identity-missing saved data [D]**
  - Preconditions: Memory store contains URL/token/client_id but no person/device.
  - Input/action: Load connection.
  - Expected outcome: Throw ServerConnectionException.
  - Failure/race and evidence limits: Malformed stored identity fails closed; test does not authorize/reset real storage or assert automatic deletion.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#6: 'obsolete saved recipients never become connection consent'
Source: `apps/client/test/features/connections/local_server_test.dart:139–154`; `test`; 1 expanded registration(s); SHA-256 `ed85e6e2b7beb1ce2881f3d232afba2375cb77943d1fcc78b46840e596505a19`.

- **local_server_test#6.1 — obsolete recipient fields inert [O]**
  - Preconditions: Stored matching connection also has allow_external=true and external_recipients.
  - Input/action: Load and reserialize connection.
  - Expected outcome: Neither obsolete authority field is present in model JSON; original stored bytes still contain fixture recipient.
  - Failure/race and evidence limits: Legacy fields are ignored, never upgraded into connection consent; no migration write is asserted.
  - Target classification/disposition: Retire old saved-recipient compatibility behavior for clean-profile cutover; preserve pairing-is-not-source-processing-consent as D.

### local_server_test#7: 'connection identity must match the active Person and device'
Source: `apps/client/test/features/connections/local_server_test.dart:156–178`; `test`; 1 expanded registration(s); SHA-256 `3e0a52e5d84f75d9e460f3aba78b1f64b6f3ce3e379855a8029e30892dd1c065`.

- **local_server_test#7.1 — foreign device save [D]**
  - Preconditions: Client current-device; candidate connection belongs to other-device for same person.
  - Input/action: Attempt save.
  - Expected outcome: Throw ServerConnectionException code connection_identity_mismatch.
  - Failure/race and evidence limits: Failure blocks persistence of a foreign-device pairing identity.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### local_server_test#8: 'server panel exposes address entry and recovers from corrupt storage'
Source: `apps/client/test/features/connections/local_server_test.dart:180–206`; `testWidgets`; 1 expanded registration(s); SHA-256 `0805a5bda62390bf67bcda7ac820f6657b96d430ab1dc94b79c2b7035141f6c1`.

- **local_server_test#8.1 — explicit recovery from corrupt saved text [D]**
  - Preconditions: Panel reads memory store value invalid.
  - Input/action: Render, then choose Forget connection, then unmount.
  - Expected outcome: Address entry and invalid-saved-connection guidance appear; explicit forget clears store and exposes Pair this device; disposal produces no widget exception.
  - Failure/race and evidence limits: Corruption does not silently delete saved data; recovery requires the explicit UI action.
  - Target classification/disposition: Preserve explicit user-directed recovery and non-destructive read failure; reassess wording/layout.

### local_server_test#9: 'pairing cancellation sends the strict pairing identity'
Source: `apps/client/test/features/connections/local_server_test.dart:208–234`; `testWidgets`; 1 expanded registration(s); SHA-256 `1d68946fd401eb7663716b5f443232764e19108626b0e5c673a90fe129b70403`.

- **local_server_test#9.1 — cancel exact pending pairing [D]**
  - Preconditions: Fake pairing begins pending with fixed pairing ID and polling proof.
  - Input/action: Pair device then press Cancel pairing; pump two seconds.
  - Expected outcome: Client cancellation body is schema_version 1, exact pairing_id and proof polling-proof.
  - Failure/race and evidence limits: Cancellation is a distinct pairing action, not source disconnect; no cancel-after-completion race is injected.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/native_personal_source_gateway_test.dart`

Full read: lines 1–69; 2 registration sites / 2 expanded registrations / 2 scenarios.
File SHA-256: `1b6d246fc49508d9de32ce4f11da0233e1095c975fc85a9de71900596ef37945`

Current owner: AppWireNativePersonalSourceGateway
Target owner/disposition: Connections source setup and client boundary validation; selected-resource CAS separate from Access grants.
Harness (H): source (8–23) synthesizes Contacts source, apple:device owner, revision and native fingerprint.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/native_personal_source_gateway.dart` → `apps/client/lib/features/connections/application/native_personal_source_gateway.dart`
- L2: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L4: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`

### native_personal_source_gateway_test#1: 'Contacts edit sends only Connections setup with source CAS'
Source: `apps/client/test/features/connections/native_personal_source_gateway_test.dart:26–52`; `test`; 1 expanded registration(s); SHA-256 `f7ec97d778339a431ea12b1af4e1cc28731f097d7778f39e75f526055e8c9d2b`.

- **native_personal_source_gateway_test#1.1 — sorted Contacts CAS edit [D]**
  - Preconditions: Contacts source is at expected revision 1 for mac-local.
  - Input/action: setup contacts.apple with selected B then A.
  - Expected outcome: Send connections.native_personal.setup with expected_revision=1 and sorted A,B; decode source revision 2 resources A,B; do not send grant_id or expected_grant_authority.
  - Failure/race and evidence limits: No grant review, grant mutation or native Contacts read is dispatched by this callback test.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### native_personal_source_gateway_test#2: 'source decoder rejects foreign owner and absent subject'
Source: `apps/client/test/features/connections/native_personal_source_gateway_test.dart:54–68`; `test`; 1 expanded registration(s); SHA-256 `9a28691305099a9de9cf7457f1aa3a3c843af38054c5e8c59ac62c1e7f310887`.

- **native_personal_source_gateway_test#2.1 — foreign Contacts owner reply [D]**
  - Preconditions: Response contains an otherwise valid A-selected source with execution owner other-device.
  - Input/action: Inspect contacts.apple.
  - Expected outcome: Reject with FormatException.
  - Failure/race and evidence limits: Despite label mentioning absent subject, native_subject_fingerprint remains present; this case proves only foreign-owner rejection.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/personal_connection_cards_test.dart`

Full read: lines 1–207; 2 registration sites / 2 expanded registrations / 2 scenarios.
File SHA-256: `25dc6ca446c28c593f40e648cd2e9eec5a28d87838d6fe557682458fe4762300`

Current owner: PersonalContactsSourceCard and PersonalSingletonSourceCard; fake NativePersonalSourceGateway and ConnectionObserveGateway
Target owner/disposition: Connections UI delegates source configuration and separate source-processing review to their canonical owners.
Harness (H): _host (80–85), _SourceGateway (87–132), _ObserveGateway (134–207) are inline H fakes. Observe state is synthetic; review fixture uses attention.coarse even in the Contacts fake and is not native acquisition proof.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:floe_client/features/connections/application/connection_observe_gateway.dart` → `apps/client/lib/features/connections/application/connection_observe_gateway.dart`
- L3: `package:floe_client/features/connections/application/native_personal_source_gateway.dart` → `apps/client/lib/features/connections/application/native_personal_source_gateway.dart`
- L4: `package:floe_client/features/connections/domain/connection_observe.dart` → `apps/client/lib/features/connections/domain/connection_observe.dart`
- L5: `package:floe_client/features/connections/domain/source_connection.dart` → `apps/client/lib/features/connections/domain/source_connection.dart`
- L6: `package:floe_client/features/connections/presentation/personal_connection_cards.dart` → `apps/client/lib/features/connections/presentation/personal_connection_cards.dart`
- L7: `package:flutter/material.dart` → `SDK/package dependency`
- L8: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L9: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`

### personal_connection_cards_test#1: 'Contacts source edit never reviews or re-enables Observe'
Source: `apps/client/test/features/connections/personal_connection_cards_test.dart:12–47`; `testWidgets`; 1 expanded registration(s); SHA-256 `66caf928ef598a3e644b36bf79f5c158abbe72c4b16be37ade597a5d47d8ef57`.

- **personal_connection_cards_test#1.1 — edit Contacts without new grant [D]**
  - Preconditions: Existing Contacts source selects A; Observe active; native read stub lists Alice=A and Bob=B.
  - Input/action: Mount, select Bob, Save selection.
  - Expected outcome: Inspect Observe once; setup once with A,B; no review or enable calls; Use with Floe remains shown and UI says this connection can be used.
  - Failure/race and evidence limits: Expansion of source resources does not itself issue an Observe re-enablement command; backend safety/admission is not simulated.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### personal_connection_cards_test#2: 'Attention source setup and Observe review are separate'
Source: `apps/client/test/features/connections/personal_connection_cards_test.dart:49–77`; `testWidgets`; 1 expanded registration(s); SHA-256 `4606a395b2ff939b245741db17653532fb93bab40a93731dc90a2ae482d85b00`.

- **personal_connection_cards_test#2.1 — Attention setup before explicit Allow [D]**
  - Preconditions: Attention source does not exist; Observe reports needs_review.
  - Input/action: Set up source; then activate Use with Floe; then press Allow.
  - Expected outcome: Before setup no Use control; setup exactly once and only inspect; activating control adds review, not enable; Allow finally adds enable.
  - Failure/race and evidence limits: Explicit source setup and permission decision remain distinct; cancellation is not covered in this registration.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/personal_contact_selection_test.dart`

Full read: lines 1–37; 1 registration sites / 1 expanded registrations / 8 scenarios.
File SHA-256: `59f392e5b0137c2b5c80b7b934cf57c8dcff2cfc2b3aa5b1187fdc0aae7cc57e`

Current owner: AppWireNativePersonalSourceGateway input validation
Target owner/disposition: Connections source selection boundary; limits are evaluated against final canonical contract.
Harness (H): Callback rejects any unexpected I/O; this is pure validation, not native Contacts acquisition.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/native_personal_source_gateway.dart` → `apps/client/lib/features/connections/application/native_personal_source_gateway.dart`
- L2: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L4: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`

### personal_contact_selection_test#1: 'Contacts source selection rejects invalid resource handles before I/O'
Source: `apps/client/test/features/connections/personal_contact_selection_test.dart:7–36`; `test`; 1 expanded registration(s); SHA-256 `5bc6b41e462cea75036eb63877aa026c1b291eede074236180f13118b59dd5a2`.

- **personal_contact_selection_test#1.1 — empty selection [D]**
  - Branch evidence: L17–17 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and an empty handle list.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.2 — duplicate [D]**
  - Branch evidence: L18–18 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and two identical same handles.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.3 — wildcard [D]**
  - Branch evidence: L19–19 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and the wildcard *.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.4 — leading whitespace [D]**
  - Branch evidence: L20–20 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and a handle starting with a space.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.5 — control character [D]**
  - Branch evidence: L21–21 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and a handle containing U+0001.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.6 — UTF-8 oversize [D]**
  - Branch evidence: L22–22 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and 129 copies of é in one handle (258 UTF-8 bytes).
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.7 — nil identity [D]**
  - Branch evidence: L23–23 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and the all-zero UUID handle.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **personal_contact_selection_test#1.8 — too many handles [D]**
  - Branch evidence: L24–24 within this file.
  - Preconditions: Gateway has device identity and a transport that fails if reached.
  - Input/action: Call Contacts setup with expectedRevision=null and 65 distinct contact:index handles.
  - Expected outcome: Throw FormatException before transport I/O.
  - Failure/race and evidence limits: Input is rejected independently for this table row; 64-count/byte thresholds are historic contract evidence, not automatically a future constant.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/remote_authority_test.dart`

Full read: lines 1–165; 3 registration sites / 3 expanded registrations / 3 scenarios.
File SHA-256: `bf7aeb78e3526a01a73753324baecc806712664688ead7f0a332581be4ab090c`

Current owner: NativeRemoteAccessGateway, LocalServerClient and SettingsScreen
Target owner/disposition: Verified Gateway pairing identity remains Connections-owned; source-processing and Actions permissions remain separate. Obsolete authority-enrollment UI/wire may retire.
Harness (H): producer/owner/pending/approved (9–41) and _MemoryCredentialStore (154–165) are H. All public keys/fingerprints here are inert fixtures, not cryptographic validation.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/remote_access_gateway.dart` → `apps/client/lib/features/connections/application/remote_access_gateway.dart`
- L2: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L3: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L4: `package:floe_client/features/settings/presentation/settings_screen.dart` → `apps/client/lib/features/settings/presentation/settings_screen.dart`
- L5: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L6: `package:flutter/material.dart` → `SDK/package dependency`

### remote_authority_test#1: 'inspection is read-only and review/status preserve explicit approval'
Source: `apps/client/test/features/connections/remote_authority_test.dart:43–103`; `test`; 1 expanded registration(s); SHA-256 `d4a4052302b3bdeab7de9e91160b9013616a09006ae04f0349fd994ec7a3bdcd`.

- **remote_authority_test#1.1 — inspection versus enrollment approval [D]**
  - Preconditions: Fake producer and owner identity; enrollment transitions local_confirmed=true/admin_approved=false to approved active on later status.
  - Input/action: Inspect producer, explicitly review/enroll, then query enrollment status.
  - Expected outcome: Inspection yields both fingerprints and exactly one call; review remains inactive without admin approval; status later active; call order inspect_producer, review_and_enroll, enrollment_status.
  - Failure/race and evidence limits: Read-only inspection is not enrollment or authority. Exact enrollment protocol is legacy shape.
  - Target classification/disposition: Preserve verified identity and explicit approval separation at canonical pairing/source authority; retire obsolete duplicated enrollment representation if cutover removes it.

### remote_authority_test#2: 'saved pairing remains storage-only and mismatched identity fails'
Source: `apps/client/test/features/connections/remote_authority_test.dart:105–134`; `test`; 1 expanded registration(s); SHA-256 `70cca665d3ffe7f2817c11edafa425538518e39aa259749bfb9f2aa6540912c5`.

- **remote_authority_test#2.1 — pairing storage bound to person [D]**
  - Preconditions: Client person-1/device-1 with in-memory store.
  - Input/action: Save matching connection then attempt save with other-person.
  - Expected outcome: Matching client ID restores; mismatched person throws ServerConnectionException.
  - Failure/race and evidence limits: Storage-only pairing cannot substitute for access consent; this asserts identity rejection, not grant authorization.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_authority_test#3: 'settings does not render the removed authority enrollment'
Source: `apps/client/test/features/connections/remote_authority_test.dart:136–151`; `testWidgets`; 1 expanded registration(s); SHA-256 `bf6b32d4230c69e5df09a6d68b7dca0bb4842338508114897f5ae2a294b490a9`.

- **remote_authority_test#3.1 — removed enrollment controls absent [O]**
  - Preconditions: SettingsScreen mounted with no client.
  - Input/action: Render and settle.
  - Expected outcome: No Server authority enrollment label or remote-authority-inspect control.
  - Failure/race and evidence limits: Static UI absence only; no enrollment operation exercised.
  - Target classification/disposition: Retire removed-control regression with the obsolete enrollment surface; keep canonical verified pairing and source processing review.
## `apps/client/test/features/connections/remote_owner_operation_test.dart`

Full read: lines 1–183; 5 registration sites / 5 expanded registrations / 5 scenarios.
File SHA-256: `c534f9bc51ae9ee62a2dcf6ffee0bfbc15009bd47dc846aa23a97e990bec27c0`

Current owner: RemoteOwnerOperation and AgentVaultException decoder
Target owner/disposition: Canonical retained owner-operation observation/recovery transport and source-owned failure reporting.
Harness (H): Callbacks synthesize request/reply loss; no live remote owner, credentials or external writes.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L2: `package:floe_client/app/runtime/agent_vault_gateway.dart` → `apps/client/lib/app/runtime/agent_vault_gateway.dart`
- L3: `package:floe_client/app/runtime/native_transport.dart` → `apps/client/lib/app/runtime/native_transport.dart`
- L4: `package:floe_client/features/connections/application/remote_owner_operation.dart` → `apps/client/lib/features/connections/application/remote_owner_operation.dart`

### remote_owner_operation_test#1: 'foreign operation result is never accepted or released'
Source: `apps/client/test/features/connections/remote_owner_operation_test.dart:7–18`; `test`; 1 expanded registration(s); SHA-256 `f9fabcd9dccb2865881c7725612478f2fd623ccf19c6c9dc20f9fff1ecba2c59`.

- **remote_owner_operation_test#1.1 — foreign result [D]**
  - Preconditions: Transport returns done=true for operation_id foreign.
  - Input/action: Perform inspect_producer.
  - Expected outcome: Reject with FormatException after one transport call.
  - Failure/race and evidence limits: No accept, result release, cancel or resubmit follows the mismatched identity.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_owner_operation_test#2: 'observer deadline retains ID and does not cancel or resubmit'
Source: `apps/client/test/features/connections/remote_owner_operation_test.dart:20–52`; `test`; 1 expanded registration(s); SHA-256 `45599d640c445d019f6c339fb2f87fa0d569fe3a2f3a649f9ca0776a586bd4f4`.

- **remote_owner_operation_test#2.1 — zero observer deadline [D]**
  - Preconditions: Transport would report unfinished then finished after a mutable flag; deadline is zero.
  - Input/action: Perform connection_observe; after pending exception set done=true and perform again.
  - Expected outcome: Both calls surface RemoteOperationPending; only one initial dispatch was recorded.
  - Failure/race and evidence limits: Timeout is observer-only; operation identity is retained and retry does not cancel, resubmit or automatically finish merely because fake backend flag changed.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_owner_operation_test#3: 'lost release acknowledgement reconciles not_found for the same accepted result'
Source: `apps/client/test/features/connections/remote_owner_operation_test.dart:54–79`; `test`; 1 expanded registration(s); SHA-256 `a37b8b3e3c3d9ce6495edef0c5f9f89d17239a38eb9887d5caad5641eb0f3685`.

- **remote_owner_operation_test#3.1 — lost release then not_found [D]**
  - Preconditions: Initial review_and_enroll completes; first release/read_result times out after release; next read_result reports not_found.
  - Input/action: Perform twice.
  - Expected outcome: First attempt is pending; second returns cached decoded true; exactly one submitted operation ID remains and release/reconciliation refers to it.
  - Failure/race and evidence limits: Loss of release acknowledgement must not replay effectful enrollment; not_found reconciles only the already accepted retained result.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_owner_operation_test#4: 'lost read acknowledgement retries the same operation without resubmitting'
Source: `apps/client/test/features/connections/remote_owner_operation_test.dart:81–127`; `test`; 1 expanded registration(s); SHA-256 `c5c3c48af47e633425575128618a8b1a69cee60b78cf121f479131de839cd88b`.

- **remote_owner_operation_test#4.1 — lost read acknowledgement [D]**
  - Preconditions: Initial connection_observe dispatch returns unfinished; first read_result times out; later reads complete.
  - Input/action: Perform, catch pending phase read_result, then perform again.
  - Expected outcome: Return true on second call; only one non-read dispatch; every read uses original operation ID and a different request ID; last read releases result.
  - Failure/race and evidence limits: Transient read loss preserves original effectful operation and correlation instead of redispatching.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_owner_operation_test#5: 'owner failure retains exact recovery and correlation metadata'
Source: `apps/client/test/features/connections/remote_owner_operation_test.dart:129–182`; `test`; 1 expanded registration(s); SHA-256 `ac02c2b31a483129954cd3a32eb327e7ac3b2e9e049fecfeb93d7c5d6998fff3`.

- **remote_owner_operation_test#5.1 — owner security failure metadata [D]**
  - Preconditions: Completed reply carries policy_denied/source_changed source security failure with reconcile, reload=true, seal=false, no retry and affected source reference.
  - Input/action: Perform connection_observe.
  - Expected outcome: Raise AgentVaultException preserving non-null correlation, policy_denied kind, reconcile recovery, reload=true, seal=false, source:exact and retryable=false.
  - Failure/race and evidence limits: Failure is not converted to generic retry or permission fallback; fixture also supplies safe_actions, stage and incident fields, but explicit assertions cover the listed subset.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/remote_pairing_gateway_test.dart`

Full read: lines 1–209; 5 registration sites / 7 expanded registrations / 13 scenarios.
File SHA-256: `6505c98b3e67d2543dd30e47452bca0ea10edecb91b1f402164ac0ea63061c10`

Current owner: NativeRemotePairingGateway; RemoteOwnerOperation; RemotePairingStatus decoder
Target owner/disposition: Canonical Connections pairing owner and retained-operation bridge; credential release only after verified persistence.
Harness (H): report (10–21) produces nested outcome; pairingId/target constants are synthetic. No real credential store or cryptographic challenge verification occurs.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L2: `package:floe_client/app/runtime/native_transport.dart` → `apps/client/lib/app/runtime/native_transport.dart`
- L3: `package:floe_client/features/connections/application/remote_pairing_gateway.dart` → `apps/client/lib/features/connections/application/remote_pairing_gateway.dart`
- L4: `package:floe_client/features/connections/application/remote_owner_operation.dart` → `apps/client/lib/features/connections/application/remote_owner_operation.dart`
- L5: `package:floe_client/features/connections/domain/remote_owner_models.dart` → `apps/client/lib/features/connections/domain/remote_owner_models.dart`

### remote_pairing_gateway_test#1: 'approved report stays retained until explicit persistence acknowledgement'
Source: `apps/client/test/features/connections/remote_pairing_gateway_test.dart:24–71`; `test`; 1 expanded registration(s); SHA-256 `59a44d94c79fed6991b5c32836dccb268055e4131f1d2312c23dc0ecec756176`.

- **remote_pairing_gateway_test#1.1 — approved result retention [D]**
  - Preconditions: Expected person/device match approved report containing token and client identity.
  - Input/action: Read status twice, then explicitly releaseApprovedPairing.
  - Expected outcome: First request uses schema 2 status target/pairing/proof; token is accessible but absent from toString; second status reuses original operation ID with no I/O; explicit release uses original operation ID and fresh request ID.
  - Failure/race and evidence limits: Credential-bearing result remains retained until explicit persistence acknowledgement; this test does not itself persist a credential.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_pairing_gateway_test#2: 'lost submit acknowledgement observes original operation without resubmission'
Source: `apps/client/test/features/connections/remote_pairing_gateway_test.dart:73–104`; `test`; 1 expanded registration(s); SHA-256 `220b4f4816167177e590aafb2733882f7c9df992e1f173986ab345328a133a9b`.

- **remote_pairing_gateway_test#2.1 — lost submission acknowledgement [D]**
  - Preconditions: First status submit times out after recording original request ID; read_result returns matching pending pairing.
  - Input/action: Request pairing status with polling proof.
  - Expected outcome: Observe same operation and return pending; submits remains one.
  - Failure/race and evidence limits: Acknowledgement loss cannot mint or dispatch a second credential/status operation.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_pairing_gateway_test#3: 'lost release retains operation identity and never creates another credential'
Source: `apps/client/test/features/connections/remote_pairing_gateway_test.dart:106–136`; `test`; 1 expanded registration(s); SHA-256 `e6479f47f4d6d19ff99b2219903f1d81f6e7989fd435af5724e323b7f8422385`.

- **remote_pairing_gateway_test#3.1 — lost release acknowledgement [D]**
  - Preconditions: Approved status retained; first release transport times out; next release succeeds.
  - Input/action: Request status, release once expecting pending, retry release.
  - Expected outcome: Exactly one non-read submission total.
  - Failure/race and evidence limits: Retry release retains operation identity and does not create another credential.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_pairing_gateway_test#4: 'strict reports reject flat wire, unknown fields and misplaced credentials'
Source: `apps/client/test/features/connections/remote_pairing_gateway_test.dart:138–181`; `test`; 1 expanded registration(s); SHA-256 `3cf0fd883e6091b10a8c1f7ad8742fafd8d63f94b5a725448d768ee4b4944139`.

- **remote_pairing_gateway_test#4.1 — flat old wire [O]**
  - Branch evidence: L142–148 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Use status/token at top level without nested outcome. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Retire the old flat/internal schema; retain strict identity and credential-bearing-state validation at the final boundary.

- **remote_pairing_gateway_test#4.2 — unknown top field [D]**
  - Branch evidence: L149–149 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Add unknown=true. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#4.3 — pending credential [D]**
  - Branch evidence: L150–153 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Put token in pending outcome. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#4.4 — approved lacks client [D]**
  - Branch evidence: L154–157 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Approved outcome has token but no client_id. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#4.5 — empty approved token [D]**
  - Branch evidence: L158–165 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Approved outcome has client_id and empty token. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#4.6 — unexpected producer [D]**
  - Branch evidence: L166–169 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Add producer object to status report. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#4.7 — unexpected issuer [D]**
  - Branch evidence: L170–173 within this file.
  - Preconditions: Start from otherwise valid pairing identity/report.
  - Input/action: Add issuer object to status report. Parse RemotePairingStatus.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Credential placement and strict status schema cannot be widened by unknown or legacy fields.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### remote_pairing_gateway_test#5: 'foreign $field is rejected before release'
Source: `apps/client/test/features/connections/remote_pairing_gateway_test.dart:184–207`; `test`; 3 expanded registration(s); SHA-256 `0634d928dff899f8444390f4bab55fc875ab68231c27615f21a75102e3f71021`.

- **remote_pairing_gateway_test#5.1 — pairing_id mismatch [D]**
  - Branch evidence: L183–207 within this file.
  - Preconditions: Gateway expects fixed pairing/person/device identities; pending result is otherwise valid.
  - Input/action: Change only pairing_id to foreign and request status.
  - Expected outcome: Throw FormatException after exactly one transport call.
  - Failure/race and evidence limits: No release of foreign result is permitted.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#5.2 — person_id mismatch [D]**
  - Branch evidence: L183–207 within this file.
  - Preconditions: Gateway expects fixed pairing/person/device identities; pending result is otherwise valid.
  - Input/action: Change only person_id to foreign and request status.
  - Expected outcome: Throw FormatException after exactly one transport call.
  - Failure/race and evidence limits: No release of foreign result is permitted.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **remote_pairing_gateway_test#5.3 — device_id mismatch [D]**
  - Branch evidence: L183–207 within this file.
  - Preconditions: Gateway expects fixed pairing/person/device identities; pending result is otherwise valid.
  - Input/action: Change only device_id to foreign and request status.
  - Expected outcome: Throw FormatException after exactly one transport call.
  - Failure/race and evidence limits: No release of foreign result is permitted.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/connections/server_connector_panel_test.dart`

Full read: lines 1–608; 9 registration sites / 9 expanded registrations / 9 scenarios.
File SHA-256: `6ebd34e0e117d90206b0a6e99212aec058d52749af1e88b74dcecf52d061125b`

Current owner: ServerConnectorPanel; ConnectorAuthorizationGateway UI directives; LocalServerClient; ConnectionObserveGateway
Target owner/disposition: Connections presentation; server/OS adapters own actual authorization; Access/Connections owns reviewed Observe, separately from source scope.
Harness (H): _capabilities/_connection (13–26), _calendarConnector (381–395), _ObserveGateway (397–485), _host (487–492), _oauthConnector/_attempt (494–518), _ConnectorClient (520–579), _Authorization (581–608) are H. Browser launcher is mocked, calls only record data, and one-shot label is not an explicit counted-submit assertion.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/features/connections/application/connector_authorization_gateway.dart` → `apps/client/lib/features/connections/application/connector_authorization_gateway.dart`
- L2: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L3: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L4: `package:floe_client/features/connections/presentation/server_connector_panel.dart` → `apps/client/lib/features/connections/presentation/server_connector_panel.dart`
- L5: `package:floe_client/features/connections/application/connection_observe_gateway.dart` → `apps/client/lib/features/connections/application/connection_observe_gateway.dart`
- L6: `package:floe_client/features/connections/domain/connection_observe.dart` → `apps/client/lib/features/connections/domain/connection_observe.dart`
- L7: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L8: `package:flutter/material.dart` → `SDK/package dependency`
- L9: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L11: `../../support/server_credentials.dart` → `apps/client/test/support/server_credentials.dart`

### server_connector_panel_test#1: 'secret connector sends credential once with selected scope'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:29–80`; `testWidgets`; 1 expanded registration(s); SHA-256 `c44d01b34518221041f25c5f1c644e59fc9ebe3915c25f47e379b96349c1da7c`.

- **server_connector_panel_test#1.1 — secret and typed scope submission [D]**
  - Preconditions: Available Home Assistant secret connector allows base_url and entities.
  - Input/action: Enter fixture URL, comma-separated sensor.office/light.desk, one-shot-secret; Connect securely.
  - Expected outcome: Fake receives secret and scope with URL plus two trimmed entity strings; changed callback once; secret input and plaintext secret no longer render.
  - Failure/race and evidence limits: No external submission occurs; fake stores last received secret but no submission counter, so repeated-submit count is not independently asserted.
  - Target classification/disposition: Preserve ephemeral secret handling and exact scoped intent at boundary; reassess form text/layout.

### server_connector_panel_test#2: 'OAuth connector opens authorization URL and polls to connected'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:82–118`; `testWidgets`; 1 expanded registration(s); SHA-256 `a2b2403db4e5916f993ef57c8a214a75ff6c52691d304b456b84d1145e24dbfe`.

- **server_connector_panel_test#2.1 — OAuth launch then polling [P]**
  - Preconditions: Start directive opens fixture authorization URL; next attempt is connected; launcher returns true.
  - Input/action: Continue to authorize and advance widget time.
  - Expected outcome: Launcher sees login.example.test; poll targets attempt; Connected appears; onChanged once.
  - Failure/race and evidence limits: No real browser, OAuth redirect, PKCE validation or launch-failure branch is exercised.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### server_connector_panel_test#3: 'device OAuth displays the user code while polling'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:120–149`; `testWidgets`; 1 expanded registration(s); SHA-256 `1579c122af8114fd45e270c34093bd733ae88694309dbc2996fd95660ac4af52`.

- **server_connector_panel_test#3.1 — device OAuth user code [P]**
  - Preconditions: Start and polling remain connecting with GitHub device authorization URL and ABCD-EFGH.
  - Input/action: Continue to authorize.
  - Expected outcome: Display instruction containing exact user code while pending.
  - Failure/race and evidence limits: No completion or expiry is asserted; authorization launcher is a stub.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### server_connector_panel_test#4: 'pending OAuth attempt can be cancelled'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:151–182`; `testWidgets`; 1 expanded registration(s); SHA-256 `217938695b9c9ae33c85832a7c3237554f3e97b918f541f644b495c96d411211`.

- **server_connector_panel_test#4.1 — explicit OAuth cancellation [D]**
  - Preconditions: Pending authorization start/poll; cancel result changes status to available.
  - Input/action: Start, then Cancel connection.
  - Expected outcome: Cancellation is sent for attempt; status becomes Available.
  - Failure/race and evidence limits: Explicit cancellation is distinct from passive observation; disposal and cancellation races not covered.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### server_connector_panel_test#5: 'Use with Floe stays bound to the displayed connection'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:184–239`; `testWidgets`; 1 expanded registration(s); SHA-256 `81dbce769f4871ea9e6f5286c85e6ab076c0d5a7d524aff035813463a4d1f6aa`.

- **server_connector_panel_test#5.1 — displayed connection rebinding [D]**
  - Preconditions: First connected Calendar has ID ...11 and Observe active.
  - Input/action: Mount, disable Use with Floe, then rebuild same panel with second ID ...12.
  - Expected outcome: Initial inspect and disable target first ID; disabled=false request is explicit; new panel performs inspect for second ID.
  - Failure/race and evidence limits: A reused widget must not act on prior displayed identity; no delayed response race is injected.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### server_connector_panel_test#6: 'Calendar scope edit never reviews active Observe'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:241–276`; `testWidgets`; 1 expanded registration(s); SHA-256 `d11cebdb5e6eac489b4171c4b7e4e5acf940cfc138d8c946a28202681ac65202`.

- **server_connector_panel_test#6.1 — scope edits do not review Observe [D]**
  - Preconditions: Connected Calendar Observe active; only initial inspect recorded.
  - Input/action: Enter calendar IDs as opaque,id newline primary newline duplicate opaque,id; Update scope.
  - Expected outcome: Update scope has distinct handles [opaque,id, primary], preserving embedded comma; Observe operations remain exactly the initial inspect.
  - Failure/race and evidence limits: Source resource changes cannot silently trigger review/enable; no actual grant owner is called.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### server_connector_panel_test#7: 'enabling reviews the bundle and echoes it back'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:278–313`; `testWidgets`; 1 expanded registration(s); SHA-256 `317cdd20677d60b564abf9aa44374472a74f25b532ae2bdf19fee196a158f46e`.

- **server_connector_panel_test#7.1 — reviewed enablement [D]**
  - Preconditions: Connected Calendar Observe paused.
  - Input/action: Use with Floe opens review; press Allow.
  - Expected outcome: Show Google Calendar review and calendar.timeline member; calls inspect, review, set_enabled(true); expected object echoes reviewed calendar.timeline bundle.
  - Failure/race and evidence limits: Grant enablement happens only after explicit Allow; backend stale-review conflict is not simulated.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### server_connector_panel_test#8: 'dismissing the review enables nothing'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:315–341`; `testWidgets`; 1 expanded registration(s); SHA-256 `5e4da202df6530fd233f9647ad7a20bc1876e918e4fc0a5465106b94f29c63d2`.

- **server_connector_panel_test#8.1 — review dismissal [D]**
  - Preconditions: Connected Calendar Observe paused.
  - Input/action: Open review then Cancel.
  - Expected outcome: No recorded action has enabled=true.
  - Failure/race and evidence limits: Closing review cannot implicitly grant use; source connection remains outside this mutation scope.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### server_connector_panel_test#9: 'non-calendar connector does not expose calendar grants'
Source: `apps/client/test/features/connections/server_connector_panel_test.dart:343–378`; `testWidgets`; 1 expanded registration(s); SHA-256 `38f64406af4168437f858f20b56b35d58fef0b4e08810c150d84fd6a16f8cfa7`.

- **server_connector_panel_test#9.1 — non-calendar surface [P]**
  - Preconditions: Connected GitHub repository with repo.read.
  - Input/action: Mount panel without Calendar Observe gateway.
  - Expected outcome: Neither connection-calendar-preview nor connection-view-preview renders.
  - Failure/race and evidence limits: No grant mutation performed; technical label mentions grants but actual assertions only inspect absence of preview controls.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.
## `apps/client/test/features/experts/agent_registry_dialog_test.dart`

Full read: lines 1–199; 4 registration sites / 5 expanded registrations / 5 scenarios.
File SHA-256: `bfe6244ac9f96ec03457e97461f4a3f08103d0c3b0c01f7abaa1144c9a00b918`

Current owner: AgentRegistrySettings; AgentController; TestRegistryGateway
Target owner/disposition: Experts registry/application controller owns installation, assignment and candidate-selection intent; Connections/Access alone grants source use.
Harness (H): app host (11–27), font-loading setUpAll (30–42), width table (44–78) and TestRegistryGateway are H. Real packaged Pretendard/Material/Lucide fonts are production assets, not deletable with this test. Mutable fixtures are copied per test; no actual package installation or capability discovery runs.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:floe_client/features/conversation/application/agent_controller.dart` → `apps/client/lib/features/conversation/application/agent_controller.dart`
- L3: `package:floe_client/features/experts/presentation/agent_registry_dialog.dart` → `apps/client/lib/features/experts/presentation/agent_registry_dialog.dart`
- L4: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L5: `package:flutter/material.dart` → `SDK/package dependency`
- L6: `package:flutter/services.dart` → `SDK/package dependency`
- L7: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L9: `../../support/agent_registry.dart` → `apps/client/test/support/agent_registry.dart`

### agent_registry_dialog_test#1: 'registry management remains readable and operates real controller at width $width'
Source: `apps/client/test/features/experts/agent_registry_dialog_test.dart:45–77`; `testWidgets`; 2 expanded registration(s); SHA-256 `1bd00af8167394cad619eed5a696574a717d14af588a9d164e9c635478f91831`.

- **agent_registry_dialog_test#1.1 — width 320 [P]**
  - Branch evidence: L44–77 within this file.
  - Preconditions: View 320×1000 at pixel ratio 1, text scale 2 and disabled animations; loaded ready registry with enabled installation and assignment.
  - Input/action: Scroll to combined capability control; tap it.
  - Expected outcome: Both installation and assignment become disabled; fake receives two changes; user-facing Schedule planning appears, package ID floe.schedule and assignment UUID do not; no widget exception.
  - Failure/race and evidence limits: Two mutations reflect existing combined UI semantics, not proof of atomic backend mutation; viewport restored in teardown.
  - Target classification/disposition: Reassess combined enablement/readability UI; retain explicit owner mutations and absence of implicit source grants.

- **agent_registry_dialog_test#1.2 — width 520 [P]**
  - Branch evidence: L44–77 within this file.
  - Preconditions: View 520×1000 at pixel ratio 1, text scale 1 and disabled animations; loaded ready registry with enabled installation and assignment.
  - Input/action: Scroll to combined capability control; tap it.
  - Expected outcome: Both installation and assignment become disabled; fake receives two changes; user-facing Schedule planning appears, package ID floe.schedule and assignment UUID do not; no widget exception.
  - Failure/race and evidence limits: Two mutations reflect existing combined UI semantics, not proof of atomic backend mutation; viewport restored in teardown.
  - Target classification/disposition: Reassess combined enablement/readability UI; retain explicit owner mutations and absence of implicit source grants.

### agent_registry_dialog_test#2: 'empty registry is inspectable without installing packages and failure offers refresh'
Source: `apps/client/test/features/experts/agent_registry_dialog_test.dart:80–107`; `testWidgets`; 1 expanded registration(s); SHA-256 `406a02da091bff234281b6af144b9c71d41100d1791c45c0f0fdc31939965887`.

- **agent_registry_dialog_test#2.1 — empty then failed registry refresh [P]**
  - Preconditions: Ready controller but registry snapshot null.
  - Input/action: Render empty registry; inject conflict into registry reads; press Refresh settings.
  - Expected outcome: Empty-abilities explanation shown and no mutations; failed refresh shows could-not-confirm explanation with no widget exception.
  - Failure/race and evidence limits: Inspection does not install packages; failed read is presented as unconfirmed rather than authoritative empty state.
  - Target classification/disposition: Keep non-mutating inspection and honest read failure; reassess empty/error wording.

### agent_registry_dialog_test#3: 'unknown Expert renders manifest metadata without a UI branch'
Source: `apps/client/test/features/experts/agent_registry_dialog_test.dart:109–161`; `testWidgets`; 1 expanded registration(s); SHA-256 `287547444f5fbbf81f3791100f501bcba82f8d8667f297e73b2bbe1a1bee8abb`.

- **agent_registry_dialog_test#3.1 — unknown manifest plus explicit source selection [D]**
  - Preconditions: Fixture installation/definition package changed to example.test.expert with custom name/description and required_attention minimum/maximum one, initially unselected.
  - Input/action: Load registry; open requirement; inspect Attention candidate; select it and Save sources.
  - Expected outcome: Manifest name/description render without generic fallback Specialized assistance; selection warning says choosing source does not grant access; resulting bindingRevision=2 and selectedCount=1.
  - Failure/race and evidence limits: No hardcoded Expert branch is needed; choice is configuration, not a grant; no denied/discovery race is injected.
  - Target classification/disposition: Preserve generic manifest rendering and revision-bound selection without authority expansion; reassess labels.

### agent_registry_dialog_test#4: 'saved source can be removed when candidate discovery fails'
Source: `apps/client/test/features/experts/agent_registry_dialog_test.dart:163–198`; `testWidgets`; 1 expanded registration(s); SHA-256 `7ae80182e91991d1461e77b3565ceb135fdbb07017c24acd33f954e70933f5cc`.

- **agent_registry_dialog_test#4.1 — remove selection when discovery unavailable [D]**
  - Preconditions: Existing selected candidate a×64 and selectedCount=1; candidate discovery throws capability_unavailable.
  - Input/action: Open required_attention; press Remove selection.
  - Expected outcome: Failure is retained as expertCandidateFailure; removal remains available; selected count becomes zero and fake candidate selection empty.
  - Failure/race and evidence limits: Failure to discover candidates must not trap a saved selection or require reconnecting to remove it; this is explicit removal, not automatic loss of binding.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/experts/agent_registry_test.dart`

Full read: lines 1–275; 9 registration sites / 9 expanded registrations / 16 scenarios.
File SHA-256: `ec1669759da7e0dc284429b680a182f72a43b4e24828b16bab7f966868430e7f`

Current owner: AgentRegistryView/Target decoder; NativeRegistryGateway; AgentController
Target owner/disposition: Experts owner and dedicated registry/controller split; canonical owner commands retain identity, revision and recovery semantics.
Harness (H): RegistryTransport (218–275) maintains synthetic retained operation, deep-copied snapshot, mutation count and lost-reply fault; shared registryFixture/TestRegistryGateway supply schema3 installation/assignment/candidate data. Broad TestVaultGateway/TestAgentGateway support does not imply their entire API is exercised.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/runtime/native_transport.dart` → `apps/client/lib/app/runtime/native_transport.dart`
- L2: `package:floe_client/app/runtime/local_owner_gateways.dart` → `apps/client/lib/app/runtime/local_owner_gateways.dart`
- L4: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`
- L6: `dart:async` → `SDK/package dependency`
- L7: `dart:convert` → `SDK/package dependency`
- L9: `package:floe_client/features/conversation/application/agent_controller.dart` → `apps/client/lib/features/conversation/application/agent_controller.dart`
- L10: `package:floe_client/features/experts/domain/agent_registry.dart` → `apps/client/lib/features/experts/domain/agent_registry.dart`
- L11: `package:floe_client/app/runtime/agent_vault_gateway.dart` → `apps/client/lib/app/runtime/agent_vault_gateway.dart`
- L12: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L14: `../../support/agent_registry.dart` → `apps/client/test/support/agent_registry.dart`

### agent_registry_test#1: 'Registry targets serialize only installation and assignment'
Source: `apps/client/test/features/experts/agent_registry_test.dart:17–24`; `test`; 1 expanded registration(s); SHA-256 `7aa6e338fba16b72c9244eada9537bc0d66b7dd161491e758dd0fcd90cc81012`.

- **agent_registry_test#1.1 — target wire enumeration [O]**
  - Preconditions: Registry target enum at baseline.
  - Input/action: Inspect values and wireName for each.
  - Expected outcome: Only installation and assignment variants, with corresponding strings.
  - Failure/race and evidence limits: No runtime owner mutation or failure covered.
  - Target classification/disposition: Retire exact old enum/string-shape assertion when canonical Experts commands replace it; preserve semantic distinction between installation and assignment if final contract requires both.

### agent_registry_test#2: 'overview validates identity, counters, and installation links'
Source: `apps/client/test/features/experts/agent_registry_test.dart:26–49`; `test`; 1 expanded registration(s); SHA-256 `4b36606b90e46a9750fc2648e19f9ceade4de50a398d3bd3dc33217257f0ab3f`.

- **agent_registry_test#2.1 — valid immutable snapshot [D]**
  - Branch evidence: L27–29 within this file.
  - Preconditions: registryFixture has matching UUID identities, installation link, revision 10 and assignment completion count 2.
  - Input/action: Parse then attempt assignments.clear.
  - Expected outcome: Completion count remains 2; attempting list mutation throws UnsupportedError.
  - Failure/race and evidence limits: Protects projection immutability; no backend call.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#2.2 — old schema [O]**
  - Branch evidence: L34–35 within this file.
  - Preconditions: Fresh otherwise valid registryFixture for mode 0.
  - Input/action: Change schema_version from 3 to 1. Parse registry view.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Reject this independently supplied invalid identity/counter/link case; fixture errors do not validate Rust persistence.
  - Target classification/disposition: Retire schema3-versus-schema1 compatibility enumeration under clean cutover; preserve strict canonical envelope validation.

- **agent_registry_test#2.3 — invalid instance [D]**
  - Branch evidence: L36–37 within this file.
  - Preconditions: Fresh otherwise valid registryFixture for mode 1.
  - Input/action: Set instance_id to invalid. Parse registry view.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Reject this independently supplied invalid identity/counter/link case; fixture errors do not validate Rust persistence.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#2.4 — broken installation link [D]**
  - Branch evidence: L38–39 within this file.
  - Preconditions: Fresh otherwise valid registryFixture for mode 2.
  - Input/action: Use registry instance UUID as assignment installation_id. Parse registry view.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Reject this independently supplied invalid identity/counter/link case; fixture errors do not validate Rust persistence.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#2.5 — negative state revision [D]**
  - Branch evidence: L40–41 within this file.
  - Preconditions: Fresh otherwise valid registryFixture for mode 3.
  - Input/action: Set assignment state_revision=-1. Parse registry view.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Reject this independently supplied invalid identity/counter/link case; fixture errors do not validate Rust persistence.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#2.6 — duplicate assignment [D]**
  - Branch evidence: L42–45 within this file.
  - Preconditions: Fresh otherwise valid registryFixture for mode 4.
  - Input/action: Append a duplicate assignment record. Parse registry view.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Reject this independently supplied invalid identity/counter/link case; fixture errors do not validate Rust persistence.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#3: 'native gateway sends only instance revision and explicit enablement target'
Source: `apps/client/test/features/experts/agent_registry_test.dart:51–73`; `test`; 1 expanded registration(s); SHA-256 `512e5847b2535aad1eddfc3cee913b0810838b7eeaca28abd35deeb6e009ed8a`.

- **agent_registry_test#3.1 — minimal revision-bound mutation [D]**
  - Preconditions: RegistryTransport contains revision 10 and enabled assignment.
  - Input/action: Read registry; configure assignment disabled.
  - Expected outcome: Change exactly contains instance_id, expected_revision 10, explicit target kind/id/enabled=false; returned revision 11 and assignment disabled; retained pending result released.
  - Failure/race and evidence limits: Asserts client CAS serialization and retained-operation drain, not a competing stale-writer conflict.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#4: 'lost mutation reply is drained and reread without replaying configuration'
Source: `apps/client/test/features/experts/agent_registry_test.dart:75–96`; `test`; 1 expanded registration(s); SHA-256 `6cab9e57a8375bb41c30500869186b45dea1bc7f28d69c0eb5d3e7efd3760c13`.

- **agent_registry_test#4.1 — lost mutation reply [D]**
  - Preconditions: Read valid revision 10, then transport will mutate successfully and throw StateError once instead of acknowledging.
  - Input/action: Configure disabled; catch error; read registry again.
  - Expected outcome: First future throws StateError; fresh read shows revision 11/disabled; changes count one.
  - Failure/race and evidence limits: Result is drained/reconciled without replaying effectful configuration after acknowledgement loss.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#5: 'foreign registry reply is never accepted by the native gateway'
Source: `apps/client/test/features/experts/agent_registry_test.dart:98–109`; `test`; 1 expanded registration(s); SHA-256 `ad12cd9243f34aa12352beefde19314cdec875fd27be6d27c5441fed3469cf91`.

- **agent_registry_test#5.1 — foreign person snapshot [D]**
  - Preconditions: Transport registry person_id replaced with registry instance UUID.
  - Input/action: Read for registryPerson.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Foreign-person reply is not accepted, even if other registry fields are valid.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#6: 'controller serializes changes and does not optimistically toggle grants'
Source: `apps/client/test/features/experts/agent_registry_test.dart:111–142`; `test`; 1 expanded registration(s); SHA-256 `39d58d155e71bbe74bf6fdf5d91a7c5c50a702230eba81f2eea5d6fe8d6ed99b`.

- **agent_registry_test#6.1 — serialized mutation and no optimistic grant [D]**
  - Preconditions: Loaded registry/Conversation, assignment enabled; gateway mutation held by completer.
  - Input/action: Configure assignment false; while pending attempt configure true; then release first mutation.
  - Expected outcome: During pending busy=true, canSend=false, displayed assignment still true; second request does not add change; one mutation total; after completion assignment false and canSend=true.
  - Failure/race and evidence limits: No optimistic authorization/enablement state from an unconfirmed write. Existing global Conversation send gating is product coupling to reassess with split controllers.
  - Target classification/disposition: Preserve serialization and authoritative commit before state change; do not require unrelated feature-wide blocking in final split.

### agent_registry_test#7: 'conflict clears stale registry without replacing or hiding the conversation'
Source: `apps/client/test/features/experts/agent_registry_test.dart:144–167`; `test`; 1 expanded registration(s); SHA-256 `6540703c1f76d2df2b4dbcaadb5a6e4207abfd47f7dd698fb0eeea81dcf2493a`.

- **agent_registry_test#7.1 — conflict isolates registry failure [D]**
  - Preconditions: Loaded registry and existing Conversation session.
  - Input/action: Make configure installation false throw conflict; later clear error and reload registry.
  - Expected outcome: Clear stale registry and show registryFailure=conflict while preserving same session object; later successful reload clears failure and restores registry.
  - Failure/race and evidence limits: Owner conflict does not erase or replace unrelated Conversation; no mutation replay is asserted.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#8: 'locking clears registry immediately and discards late read and mutation results'
Source: `apps/client/test/features/experts/agent_registry_test.dart:169–197`; `test`; 1 expanded registration(s); SHA-256 `f8a4b2d7bb8934d3ae14f1754c8807ed5b00f3824565c042a8b56a21dd2c018c`.

- **agent_registry_test#8.1 — late read [D]**
  - Branch evidence: L170–196 within this file.
  - Preconditions: Ready loaded registry; registryGate holds read response.
  - Input/action: Start loadRegistry; call closeView; inspect state; resolve gate and await both operations.
  - Expected outcome: Registry and session cleared immediately, remain cleared after late completion; Vault locked and gateway lock count exactly one.
  - Failure/race and evidence limits: Late result cannot repopulate sealed view. This calls explicit closeView; it is not evidence that ordinary widget/controller disposal should lock storage or cancel a Run.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#8.2 — late mutation [D]**
  - Branch evidence: L170–196 within this file.
  - Preconditions: Ready loaded registry; registryGate holds configure response.
  - Input/action: Start disable assignment; call closeView; inspect state; resolve gate and await both operations.
  - Expected outcome: Registry and session cleared immediately, remain cleared after late completion; Vault locked and gateway lock count exactly one.
  - Failure/race and evidence limits: Late result cannot repopulate sealed view. This calls explicit closeView; it is not evidence that ordinary widget/controller disposal should lock storage or cancel a Run.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_registry_test#9: 'key failure or worker interruption clears cached conversation and settings'
Source: `apps/client/test/features/experts/agent_registry_test.dart:199–215`; `test`; 1 expanded registration(s); SHA-256 `c5de2e6046264e96f053b61351949ecec87ea799db525f19522b49ec0e2f9333`.

- **agent_registry_test#9.1 — vault_unavailable [D]**
  - Branch evidence: L200–214 within this file.
  - Preconditions: Ready loaded registry and Conversation; fake maps vault_unavailable to reloadRequired=true and sealSession=true.
  - Input/action: Inject failure then loadRegistry.
  - Expected outcome: Registry and session null; Vault state unavailable.
  - Failure/race and evidence limits: The clearing is driven by synthetic explicit recovery/seal flags, not proof that every occurrence of this reason should globally seal in final architecture.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_registry_test#9.2 — interrupted [D]**
  - Branch evidence: L200–214 within this file.
  - Preconditions: Ready loaded registry and Conversation; fake maps interrupted to reloadRequired=true and sealSession=true.
  - Input/action: Inject failure then loadRegistry.
  - Expected outcome: Registry and session null; Vault state unavailable.
  - Failure/race and evidence limits: The clearing is driven by synthetic explicit recovery/seal flags, not proof that every occurrence of this reason should globally seal in final architecture.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/knowledge/agent_memory_review_test.dart`

Full read: lines 1–81; 2 registration sites / 2 expanded registrations / 2 scenarios.
File SHA-256: `9c85164a07f2504b9a416dc156af944f328aaa7f9571bc2472fad404c2f0e1ea`

Current owner: NativeMemoryGateway; AgentMemoryCandidate/Decision in knowledge/presentation/agent_memory_review.dart
Target owner/disposition: Knowledge owner; move pure review DTOs to knowledge/domain/memory_review.dart and gateway to knowledge/application/memory_gateway.dart without compatibility re-export.
Harness (H): _candidate (64–81) is synthetic pending create-memory with one source reference, confidence 900 and timestamp. Callback retains operation-style envelope but no real learner or persistence executes.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/runtime/local_owner_gateways.dart` → `apps/client/lib/app/runtime/local_owner_gateways.dart`
- L3: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`
- L5: `package:floe_client/features/knowledge/presentation/agent_memory_review.dart` → `apps/client/lib/features/knowledge/presentation/agent_memory_review.dart`
- L6: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### agent_memory_review_test#1: 'native gateway reads and decides pending memory candidates'
Source: `apps/client/test/features/knowledge/agent_memory_review_test.dart:9–54`; `test`; 1 expanded registration(s); SHA-256 `40ade65ab83c8abfbe5722a51c4fa8540ca62cfe21a41354cd338f886bd8bba5`.

- **agent_memory_review_test#1.1 — inspect then explicit approval [D]**
  - Preconditions: Fake Knowledge owner starts with one pending preference candidate, sourceCount=1.
  - Input/action: Read memory review for person-1, then approve candidate-1.
  - Expected outcome: Read sends only knowledge.memory.review and decodes statement/source count; decision sends knowledge.memory.decide, exact candidate ID and approve; returned candidate list empty.
  - Failure/race and evidence limits: No denial/rejection, lost decision, duplicate approval or owner conflict is injected; do not infer those from the gateway interface.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_memory_review_test#2: 'candidate parser rejects non-pending knowledge'
Source: `apps/client/test/features/knowledge/agent_memory_review_test.dart:56–61`; `test`; 1 expanded registration(s); SHA-256 `d98931d07bb755f47d63898d4bc08e064dfd2b0ed164ab0e741050287cf98620`.

- **agent_memory_review_test#2.1 — nonpending item rejected [D]**
  - Preconditions: Copy valid pending fixture.
  - Input/action: Change state to approved and parse as review candidate.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Approved knowledge cannot be misrepresented as pending review; rejection covers this one state, not every enum branch.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/knowledge/agent_memory_test.dart`

Full read: lines 1–79; 2 registration sites / 2 expanded registrations / 3 scenarios.
File SHA-256: `4ad83461f81bcfb8d9bc3c3dde6e0afdb063b0fc88112787039e98586ccb5d85`

Current owner: NativeMemoryGateway; AgentMemoryOverview/AgentMemory decoder
Target owner/disposition: Knowledge canonical bounded saved-memory projection and provenance validation; client displays owner truth.
Harness (H): _overview (61–79) is inline H fixture: one learned preference, two pending items, revision 2 and one source. Callback transport is H; label bounded does not include a boundary-limit rejection test.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/runtime/local_owner_gateways.dart` → `apps/client/lib/app/runtime/local_owner_gateways.dart`
- L3: `../../support/app_wire_transport.dart` → `apps/client/test/support/app_wire_transport.dart`
- L5: `package:floe_client/features/knowledge/domain/agent_memory.dart` → `apps/client/lib/features/knowledge/domain/agent_memory.dart`
- L6: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### agent_memory_test#1: 'native gateway reads a bounded saved memory overview'
Source: `apps/client/test/features/knowledge/agent_memory_test.dart:9–40`; `test`; 1 expanded registration(s); SHA-256 `caaaa317571453fde37b43fca48c40a3973663464ad18e0810f338130aeee704`.

- **agent_memory_test#1.1 — saved versus pending summary [D]**
  - Preconditions: Fake Knowledge owner returns one saved/two pending, learned preference record.
  - Input/action: readMemory person-1.
  - Expected outcome: Send only knowledge.memory.overview; savedCount=1, pendingCount=2, statement preserved, category Preference and origin learned.
  - Failure/race and evidence limits: No overflow/limit or foreign-person case here despite bounded label; decoding successful bounded fixture alone does not prove all bounds.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

### agent_memory_test#2: 'saved memory parser rejects duplicates and invalid metadata'
Source: `apps/client/test/features/knowledge/agent_memory_test.dart:42–58`; `test`; 1 expanded registration(s); SHA-256 `3d68848278f0fa42470f146984d132a02260c4ad868c4f56b719a88c6e9512b8`.

- **agent_memory_test#2.1 — duplicate target identity [D]**
  - Branch evidence: L46–53 within this file.
  - Preconditions: Overview has one valid memory; clone it.
  - Input/action: Supply two identical records and saved_count=2.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: Matching summary count does not legitimize duplicate target identity.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.

- **agent_memory_test#2.2 — missing provenance count [D]**
  - Branch evidence: L54–57 within this file.
  - Preconditions: Valid learned memory record.
  - Input/action: Set source_count=0 and parse.
  - Expected outcome: Throw FormatException.
  - Failure/race and evidence limits: A learned record cannot claim zero sources; source evidence validation beyond numeric count is not exercised.
  - Target classification/disposition: Re-prove the safety meaning through the canonical owner contract after S2; retire this old test and wire shape.
## `apps/client/test/features/settings/agent_memory_settings_test.dart`

Full read: lines 1–124; 3 registration sites / 3 expanded registrations / 3 scenarios.
File SHA-256: `1a79d382861231849246a2d2bb33d65dbf1a4a72254787fda517b8254bdcf3e7`

Current owner: AgentMemorySettings/Card; SettingsScreen; AgentController
Target owner/disposition: Knowledge controller/gateway owns reads/review; Settings owns navigation/presentation only.
Harness (H): _MemoryGateway (91–104) extends shared TestVaultGateway ready and counts readMemory; _overview (106–124) is synthetic learned preference. No approval occurs in these widget tests.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:floe_client/features/conversation/application/agent_controller.dart` → `apps/client/lib/features/conversation/application/agent_controller.dart`
- L3: `package:floe_client/features/knowledge/domain/agent_memory.dart` → `apps/client/lib/features/knowledge/domain/agent_memory.dart`
- L4: `package:floe_client/features/settings/presentation/agent_memory_settings.dart` → `apps/client/lib/features/settings/presentation/agent_memory_settings.dart`
- L5: `package:floe_client/app/runtime/agent_vault_gateway.dart` → `apps/client/lib/app/runtime/agent_vault_gateway.dart`
- L6: `package:floe_client/features/settings/presentation/settings_screen.dart` → `apps/client/lib/features/settings/presentation/settings_screen.dart`
- L7: `package:flutter/material.dart` → `SDK/package dependency`
- L8: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L10: `../../support/agent_vault_gateway.dart` → `apps/client/test/support/agent_vault_gateway.dart`

### agent_memory_settings_test#1: 'memory settings presents saved memory in user language'
Source: `apps/client/test/features/settings/agent_memory_settings_test.dart:13–38`; `testWidgets`; 1 expanded registration(s); SHA-256 `33d5c151afdc761e287c3cb706680c63be521a6f0bac3e5571b3dbe6b8cf1fb2`.

- **agent_memory_settings_test#1.1 — readable saved memory [P]**
  - Preconditions: Controller preloaded with one learned preference and two pending memories.
  - Input/action: Render dedicated memory settings page.
  - Expected outcome: Saved memories and statement visible; origin says Learned with your approval; technical confidence absent; exact memory row key present.
  - Failure/race and evidence limits: Copy asserts approval provenance but fixture contains only origin=learned, not evidence of actual user approval; reassess language against canonical Knowledge policy.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### agent_memory_settings_test#2: 'memory summary opens the management surface'
Source: `apps/client/test/features/settings/agent_memory_settings_test.dart:40–63`; `testWidgets`; 1 expanded registration(s); SHA-256 `3fe704209c63406a592ef7b5b81779225feea06351376ff15ff8833c1ca7df64`.

- **agent_memory_settings_test#2.1 — summary navigation callback [P]**
  - Preconditions: Controller preloaded with same overview; onManage records boolean.
  - Input/action: Click manage-memory.
  - Expected outcome: Callback fires; summary reads 1 saved · 2 pending.
  - Failure/race and evidence limits: Pure navigation; no read or decision mutation asserted.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### agent_memory_settings_test#3: 'data privacy opens the dedicated memory page'
Source: `apps/client/test/features/settings/agent_memory_settings_test.dart:65–88`; `testWidgets`; 1 expanded registration(s); SHA-256 `8bd41118e13b77356a8843c37810f2756a4f92a7326ad45bf0ac979a1dad7b5e`.

- **agent_memory_settings_test#3.1 — one read across dedicated page navigation [P]**
  - Preconditions: Ready _MemoryGateway and loaded Conversation controller; Settings starts with client null.
  - Input/action: Render Data/privacy then click manage-memory.
  - Expected outcome: Summary appears; dedicated page shows Saved memories and statement; gateway read count exactly one.
  - Failure/race and evidence limits: Navigation reuses loaded owner projection instead of issuing extra read; failure/race not injected.
  - Target classification/disposition: Preserve clear memory navigation; one-read count is an efficiency hypothesis, not a hard owner policy.
## `apps/client/test/features/settings/settings_screen_test.dart`

Full read: lines 1–792; 10 registration sites / 11 expanded registrations / 11 scenarios.
File SHA-256: `502580a16b56e17625eee5cd8d214bd60b65e37beae19d0fbe646bd20eef54d2`

Current owner: SettingsScreen and Dart parts; AgentController; CalendarActionController; LocalServerClient
Target owner/disposition: Settings feature composition with explicit Actions/Knowledge/Experts/Connections injection; Actions owns permission policy and locked availability.
Harness (H): Executor imported at line 18 from actions/calendar_action_execution_test.dart; _LockableActionExecutor (421–429) extends it. _AndroidContext (431–529), _AppleContext (531–573), synthetic health/calendar builders (575–743), _SettingsServerClient (745–792) are H. Shared TestRegistryGateway and MemoryServerCredentials are H. Gmail ready_snapshot.json is an external shared fixture consumed at line 268, not owned here.

Dependencies (read from actual imports; production imports are not deletion candidates):
- L1: `dart:convert` → `SDK/package dependency`
- L2: `dart:io` → `SDK/package dependency`
- L4: `package:floe_client/app/floe_selection.dart` → `apps/client/lib/app/floe_selection.dart`
- L5: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L6: `package:floe_client/features/conversation/application/agent_controller.dart` → `apps/client/lib/features/conversation/application/agent_controller.dart`
- L7: `package:floe_client/app/runtime/agent_vault_gateway.dart` → `apps/client/lib/app/runtime/agent_vault_gateway.dart`
- L8: `package:floe_client/features/actions/application/calendar_action_controller.dart` → `apps/client/lib/features/actions/application/calendar_action_controller.dart`
- L9: `package:floe_client/features/actions/domain/calendar_action.dart` → `apps/client/lib/features/actions/domain/calendar_action.dart`
- L10: `package:floe_client/features/connections/application/local_server_client.dart` → `apps/client/lib/features/connections/application/local_server_client.dart`
- L11: `package:floe_client/features/settings/presentation/settings_screen.dart` → `apps/client/lib/features/settings/presentation/settings_screen.dart`
- L12: `package:floe_client/infrastructure/native/android_context_gateway.dart` → `apps/client/lib/infrastructure/native/android_context_gateway.dart`
- L13: `package:floe_client/infrastructure/native/apple_context_gateway.dart` → `apps/client/lib/infrastructure/native/apple_context_gateway.dart`
- L14: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L15: `package:flutter/material.dart` → `SDK/package dependency`
- L16: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L18: `../actions/calendar_action_execution_test.dart` → `apps/client/test/features/actions/calendar_action_execution_test.dart` (show Executor)
- L19: `../../support/server_credentials.dart` → `apps/client/test/support/server_credentials.dart`
- L20: `../../support/agent_registry.dart` → `apps/client/test/support/agent_registry.dart`

### settings_screen_test#1: 'assistant permission management lives in Settings'
Source: `apps/client/test/features/settings/settings_screen_test.dart:23–53`; `testWidgets`; 1 expanded registration(s); SHA-256 `1e33a4566b40ce6af6b0c978abb669aa46f0dc69c817889bb377c8e910e91458`.

- **settings_screen_test#1.1 — privacy navigation surface [P]**
  - Preconditions: Loaded ready AgentController from registry fake, client null.
  - Input/action: Render Settings.
  - Expected outcome: Data & privacy labels twice and connections-privacy-navigation present; AI processing, Android source section and Schedule planning absent.
  - Failure/race and evidence limits: Rendering does not prove permission mutation or change source policy.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### settings_screen_test#2: 'Calendar consumer grants are absent from Data & privacy'
Source: `apps/client/test/features/settings/settings_screen_test.dart:55–79`; `testWidgets`; 1 expanded registration(s); SHA-256 `65006eb4bf28f0e6d457a83efea9ef6238400a6f8ad45a9efe1662a093c6ffa2`.

- **settings_screen_test#2.1 — old Calendar grant surface absent [O]**
  - Preconditions: Same ready controller and client-null Settings.
  - Input/action: Render.
  - Expected outcome: No Data Floe can use or AI processing label.
  - Failure/race and evidence limits: Negative UI assertion only; no check of canonical source-processing controls.
  - Target classification/disposition: Retire obsolete consumer-grant/AI-processing Settings representation; preserve canonical Connections review/navigation instead.

### settings_screen_test#3: 'Android source editors move out of Data & privacy'
Source: `apps/client/test/features/settings/settings_screen_test.dart:81–109`; `testWidgets`; 1 expanded registration(s); SHA-256 `06836a9830c4b94c8d84f1d483cbec74781d351596f9698079f7898f2159e8c7`.

- **settings_screen_test#3.1 — Android editors not invoked [D]**
  - Preconditions: Settings receives Android fake including Calendar, with zero permission counters.
  - Input/action: Render Data/privacy.
  - Expected outcome: No Android data-source, Calendar Allow or Health Refresh controls; both generic and Calendar permission request counters remain zero.
  - Failure/race and evidence limits: Rendering must not request unrelated OS permissions. Android fixtures are dormant evidence; no parity work implied.
  - Target classification/disposition: Preserve absence of incidental OS permission effects; obsolete Android surface checks may retire.

### settings_screen_test#4: 'Apple access controls move out of Data & privacy'
Source: `apps/client/test/features/settings/settings_screen_test.dart:111–145`; `testWidgets`; 1 expanded registration(s); SHA-256 `45ed7b75bb2da86b370dbe8ef3b15d1b1c252721cb6882e6fe1181a6167043cb`.

- **settings_screen_test#4.1 — Apple source reads not invoked [D]**
  - Preconditions: Settings receives Apple context fake with zero connection/wellbeing read counts and ready AgentController.
  - Input/action: Render Data/privacy.
  - Expected outcome: No Wellbeing access, Apple Contacts or Open Connections text; zero connections calls and wellbeing reads.
  - Failure/race and evidence limits: Privacy page does not read native health/source data just by rendering.
  - Target classification/disposition: Preserve owner-routed, deliberate native source acquisition; reassess exact navigation wording.

### settings_screen_test#5: 'settings navigation switches between separate pages'
Source: `apps/client/test/features/settings/settings_screen_test.dart:147–193`; `testWidgets`; 1 expanded registration(s); SHA-256 `f23b8926f76f95872627def86237ba05371bdd75d84f3c7f86c92148d6839feb`.

- **settings_screen_test#5.1 — separate settings destinations [P]**
  - Preconditions: 1200×900 surface; CalendarActionController using Executor, AgentController registry fake and LocalServerClient memory store.
  - Input/action: Render initial action page, navigate Data/privacy, then Remote server.
  - Expected outcome: Initially Allow all supported actions but no server connection; privacy displays connections navigation without action preset; remote server shows connection panel.
  - Failure/race and evidence limits: Only navigation is asserted; Executor state is in memory, no permissions or credentials persisted.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### settings_screen_test#6: 'desktop navigation and content scroll independently'
Source: `apps/client/test/features/settings/settings_screen_test.dart:195–218`; `testWidgets`; 1 expanded registration(s); SHA-256 `d5a08bf1c0e19d3d039e1c5078121d5f8db4286c62b96033adcbba1ed10dd66c`.

- **settings_screen_test#6.1 — independent scrolling [P]**
  - Preconditions: 1200×500 desktop with client-null Settings.
  - Input/action: Inspect navigation and content scroll widgets.
  - Expected outcome: Both vertical; distinct controllers; no widget exception.
  - Failure/race and evidence limits: Exact scroll implementation is presentation evidence rather than semantic authority.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### settings_screen_test#7: 'external model approval has no blanket Settings editor'
Source: `apps/client/test/features/settings/settings_screen_test.dart:220–259`; `testWidgets`; 1 expanded registration(s); SHA-256 `db8c1b8947932fa404b6361715e2cbc8e6292995323d41af4d8d3eb73e313196`.

- **settings_screen_test#7.1 — no blanket external-model consent [O]**
  - Preconditions: In-memory paired matching connection and _SettingsServerClient with old purpose availability/recipient fixture; ready AgentController.
  - Input/action: Render Settings and inspect serialized connection.
  - Expected outcome: No Allow external model providers or external-model-consent; connections privacy navigation remains; serialized connection lacks allow_external.
  - Failure/race and evidence limits: Pairing cannot encode blanket model/source consent. Legacy recipient-model fixture is not target source-processing policy.
  - Target classification/disposition: Retire exact-recipient/blanket-model Settings compatibility fixture under ADR0034; retain pairing and processing authority separation as D.

### settings_screen_test#8: 'server connection inventory stays out of Data & privacy'
Source: `apps/client/test/features/settings/settings_screen_test.dart:261–304`; `testWidgets`; 1 expanded registration(s); SHA-256 `2ae2e5273927428135a68f2d28bbe7f667017b0fc2fedb7feb0ca1d10dc1a5cd`.

- **settings_screen_test#8.1 — server source inventory absent in privacy [P]**
  - Preconditions: Read shared server/internal/connectors/gmail/testdata/ready_snapshot.json into fake client connectionSnapshots; pair matching memory-only connection.
  - Input/action: Render Settings with controller.
  - Expected outcome: Gmail, Ready and Open Connections text absent.
  - Failure/race and evidence limits: The fixture is parsed from disk but the test does not assert a connections() call; absence is presentation routing evidence, not validating the Gmail snapshot contract.
  - Target classification/disposition: Reassess Settings versus Connections information placement; preserve external fixture until all consumers are accounted for.

### settings_screen_test#9: 'remote server lives under Settings at $width'
Source: `apps/client/test/features/settings/settings_screen_test.dart:307–329`; `testWidgets`; 2 expanded registration(s); SHA-256 `0c3673ad7331606f2ef077862fcd272b382aa956460fab11ccfaf9668dae8e84`.

- **settings_screen_test#9.1 — width 390 [P]**
  - Branch evidence: L306–330 within this file.
  - Preconditions: Surface 390×900; LocalServerClient with empty memory store, no other controller.
  - Input/action: Mount Settings under scroll host and settle.
  - Expected outcome: Settings, Remote server, Remote server connection, address field and Pair this device all visible; no exception.
  - Failure/race and evidence limits: No pairing request sent; mobile/desktop rendering only.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

- **settings_screen_test#9.2 — width 1200 [P]**
  - Branch evidence: L306–330 within this file.
  - Preconditions: Surface 1200×900; LocalServerClient with empty memory store, no other controller.
  - Input/action: Mount Settings under scroll host and settle.
  - Expected outcome: Settings, Remote server, Remote server connection, address field and Pair this device all visible; no exception.
  - Failure/race and evidence limits: No pairing request sent; mobile/desktop rendering only.
  - Target classification/disposition: Reassess this product/presentation hypothesis; do not mechanically preserve keys, wording, widget structure or old routing.

### settings_screen_test#10: 'action permissions use presets and the Floe select'
Source: `apps/client/test/features/settings/settings_screen_test.dart:332–418`; `testWidgets`; 1 expanded registration(s); SHA-256 `ed46a3a384b31543ffb9b6fd1d7db6ab979d0442a89ee757076af2de82ca8681`.

- **settings_screen_test#10.1 — presets, customization and locked owner [D]**
  - Preconditions: 1200×900; lockable Executor starts with ask authority; Settings has only Action controller.
  - Input/action: Inspect selector; choose Allow all supported actions; Customize permissions; open Allow automatically and choose Do not allow; then make authority load throw vault_unavailable and reload.
  - Expected outcome: Uses FloeSelect rather than stock DropdownButton; initially enabled; all preset sets calendarCreate=allow and disables individual select; custom reenables; selection sets deny; locked load shows unlock guidance and disables select.
  - Failure/race and evidence limits: Lock/error must make Action authority uneditable. All supported actions currently means Calendar create only; no execution/persistence is proven and fixed widget/copy is P.
  - Target classification/disposition: Preserve canonical Actions policy changes and fail-closed locked editing; reassess preset scope and custom UI with encrypted single Actions store.

## Evidence limits and handoff

- No code/test removal or rewrite; only the two partition artifacts were written.
- No compiler, analyzer, formatter, build, tests, architecture checker, Git push/PR/deployment, data reset or credential operation was executed.
- Static callback/widget assertions are evidence of test intent, not proof that current implementation passes or final product behavior is correct.
- Registration expansion counts outer loops only; inner malformed-input/boolean tables are separately expanded behavior scenarios, not extra test registrations.
- Inherited file-level current owner, target owner and dependencies apply to every scenario; every scenario is bound to its containing exact registration span/hash and baseline file hash.
- All owned paths had empty incoming Dart import lists in the preparation map; this is lexical evidence, not a compiler-resolved global consumer proof.
- Shared fixture and helper ownership is not made safe for deletion by local semantic extraction alone.
