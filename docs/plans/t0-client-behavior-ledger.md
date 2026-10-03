# T0 client behavior ledger and coverage audit

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`, extraction branch `refactor/architecture-20261002`.

This is the client pre-removal behavior record required by architecture-refactor §7. It replaces neither the authoritative execution plan nor product requirements. Old assertions are source evidence, not an accepted future specification and not a claim that they passed. This extraction performs no test deletion, data reset, credential operation, package install, compilation, formatting, build, executable test or architecture checker.

## Behavior partitions

- [Apple native/integration/targets/resources](t0-client-behavior-ledger-native.md)
- [Runtime, Vault and AppWire](t0-client-behavior-ledger-runtime.md)
- [Conversation, interactions and recovery](t0-client-behavior-ledger-conversation.md)
- [Actions, review and external-write recovery](t0-client-behavior-ledger-actions.md)
- [Connections, Experts, Knowledge and Settings](t0-client-behavior-ledger-connections.md), with [structured scenarios](t0-client-behavior-ledger-connections.json)
- [Day/Calendar projection and refresh](t0-client-behavior-ledger-day.md)
- [Native acquisition/publication and diagnostics](t0-client-behavior-ledger-infrastructure.md)
- [Presentation/design-system/preview/App shell](t0-client-behavior-ledger-presentation.md), with [structured scenarios](t0-client-behavior-ledger-presentation.json)

[Source/read/registration/dependency evidence](t0-client-behavior-ledger-sources.json) binds each file to complete source bytes and each executable registration to an exact symbol, mechanism, inclusive span and SHA-256. Every source-ledger registration is interpreted in its linked prose partition; inner input/failure tables are narrated, rather than treated as extra independently registered tests. Where a test name overstates its assertions, the ledger states the narrower observed evidence.

## Completed static coverage

All121 audited artifacts have full-source reads. All359 direct registration sites have behavior records:201 Dart test calls,125 Flutter testWidgets calls,28 XCTest methods and5 Swift Testing @Test methods. Outer parameter loops expand the326 Dart sites to370 registrations; together with33 Swift methods this yields403 statically declared registration instances. Inner assertion tables are explicitly narrated but are not separate framework registrations. These numbers are not execution/pass counts.

The87 Dart test/support sources reconcile exactly to18,090 baseline lines; they include77 registered-test files and10 helper-only files. The five Swift suite sources add33 registrations. Supplemental manifests, resources, README and retained smoke diagnostic explain the121-artifact audit scope. File hashes match both the frozen baseline and current bytes; missing behavior registrations, unread artifacts and duplicate paths are zero. See the [machine-readable audit](t0-client-behavior-ledger-audit.json).

Extraction is complete for review. Source deletion remains paused for the coordinator’s explicit covered-batch authorization. No implementation test result or structural verification is claimed.

## Method and exclusions

The frozen87 Dart files under test/ and integration/ reconcile to18,090 lines and to all87 preparation hashes without discrepancy. Fresh source displays were read through the end, including setup/teardown/fakes/helpers; truncated displays were reread in smaller contiguous ranges. Five Swift test source files were likewise read fully. Manifests/resources/diagnostic context are separately classified and hashed rather than counted as executable tests.

Registration accounting uses actual Flutter `test` and `testWidgets` calls beneath main/group/outer parameter loops, XCTestCase test methods and Swift Testing `@Test`. Comment/string occurrences, assertion calls, helper methods and diagnostic main entrypoints are not registrations. Token-aware inventory was cross-checked against complete source and actual imports/target declarations, not just filename or raw substring counts. Exact dynamic outer-loop counts are distinguished from inner scenario branches and from executed results. No compiler resolution or runtime discovery was performed.

Tracked-path and source-registration searches found no additional integration_test/, test_driver/, Linux native test tree, Android/JUnit test source/dependency or Windows/CTest target. Mobile Vault smoke is a real opt-in diagnostic KEEP. Three Swift package test targets and two Xcode Runner XCTest targets are explicit; the Screen Time suite's five @Test methods must not be omitted by a func-test-prefix scan. Production lib/preview, main_preview, main_design_system, fake implementations, localization, fonts/assets and native acquisition/packaging code remain KEEP.

## Safety meaning retained without obsolete representation

Preserve verified identity, person/device/producer/native-subject correlation, source-owned authority and CAS, no permission escalation through views or navigation, key continuity/no automatic replacement, minimal privacy-safe projection/diagnostics, cancellation direction, stable effect identity, durable pre-dispatch intent and uncertain-write recovery. In particular, lost admission/result/release and collection failure cannot become a second external write or model replay. A screen closing, observer timeout or Day refresh does not implicitly cancel a backend Run.

Retire exact model-recipient consent and concrete profile-selection expectations; use accepted source-processing/Gateway-primary semantics. Direct pre-transform Health-derived publication is obsolete; raw and pre-transform aggregates must stay out of reasoning and the required source-owned local transform must fail closed without declassifying HighlySensitive. Geometry, strings, widget implementation types, control timing and heuristic scores remain product hypotheses.

## Shared consumers and future removal boundary

All92 source files listed below are candidates for a covered recoverable removal batch only after coordinated review of the completed aggregate ledger. They contain legacy executable tests or exclusively test helpers/hosts; no deletion has happened. The list includes all87 Dart test/integration files plus five Swift suite sources. Removing source code never authorizes running teardown against any real profile or credential.

- The Actions test helper chain Gateway/action/connection → Executor → Conversation/Settings must be removed together or all live importers otherwise reconciled.
- Retain all five Apple/Screen Time JSON resources and six Android JSON fixtures until aggregate cross-language consumer closure; several have live Rust include_str paths. Retain root delegation and Go connector fixtures, shared fixture builder/compiled artifacts and production diagnostic hosts.
- Edit only the three Swift testTarget declarations and two Runner test-target object/reference closures after approval; keep product library targets and Runner/native build graph. Exact Xcode test-only object spans are in the native partition/source ledger.
- flutter_test is the only direct test framework dependency. flutter_lints remains used by analysis_options. Lockfile transitive packages must not be pruned by names alone; approved dependency-resolution work must reconcile them without losing shared product packages.
- Client README and active guide references need same-snapshot reconciliation. Preserve real opt-in diagnostic guidance and future verification policy; do not use the README's stale no-widget-tests claim to hide actual suites.

## Covered source-removal candidates (pending aggregate review)

- `apps/client/apple/FloeAppleContacts/Tests/FloeAppleContactsTests/AppleContactsProviderTests.swift`
- `apps/client/apple/FloeAppleHealth/Tests/FloeAppleHealthTests/AppleWellbeingProjectionTests.swift`
- `apps/client/integration/local_server_pairing_test.dart`
- `apps/client/integration/native_calendar_fixture_test.dart`
- `apps/client/integration/product_conversation_test.dart`
- `apps/client/integration/support/disposable_product_profile.dart`
- `apps/client/integration/support/native_calendar_fixture_host.dart`
- `apps/client/integration/support/product_conversation_host.dart`
- `apps/client/ios/RunnerTests/RunnerTests.swift`
- `apps/client/ios/ScreenTimeGate/Tests/FloeScreenTimeGateTests/ScreenTimeGateTests.swift`
- `apps/client/macos/RunnerTests/RunnerTests.swift`
- `apps/client/test/app/app_shell_test.dart`
- `apps/client/test/app/runtime/agent_vault_controller_test.dart`
- `apps/client/test/app/runtime/agent_vault_gateway_test.dart`
- `apps/client/test/app/runtime/app_read_model_test.dart`
- `apps/client/test/app/runtime/app_wire_transport_test.dart`
- `apps/client/test/app/runtime/disposable_product_profile_test.dart`
- `apps/client/test/app/runtime/floe_client_test.dart`
- `apps/client/test/app/runtime/local_context_gateway_test.dart`
- `apps/client/test/app/runtime/native_transport_error_test.dart`
- `apps/client/test/app/runtime/owner_operation_test.dart`
- `apps/client/test/design_system/design_system_usage_test.dart`
- `apps/client/test/design_system/feedback_layout_test.dart`
- `apps/client/test/design_system/floe_action_card_test.dart`
- `apps/client/test/design_system/floe_calendar_popover_test.dart`
- `apps/client/test/design_system/floe_design_system_test.dart`
- `apps/client/test/design_system/floe_input_test.dart`
- `apps/client/test/design_system/floe_loading_test.dart`
- `apps/client/test/design_system/floe_selection_test.dart`
- `apps/client/test/design_system/floe_switch_test.dart`
- `apps/client/test/design_system/floe_theme_test.dart`
- `apps/client/test/design_system/floe_toast_test.dart`
- `apps/client/test/features/actions/agent_proposal_card_test.dart`
- `apps/client/test/features/actions/agent_proposal_test.dart`
- `apps/client/test/features/actions/calendar_action_execution_test.dart`
- `apps/client/test/features/actions/calendar_action_gateway_test.dart`
- `apps/client/test/features/actions/calendar_action_ui_test.dart`
- `apps/client/test/features/actions/calendar_direct_interaction_test.dart`
- `apps/client/test/features/connections/agent_connections_test.dart`
- `apps/client/test/features/connections/app_wire_calendar_source_gateway_test.dart`
- `apps/client/test/features/connections/connection_observe_gateway_test.dart`
- `apps/client/test/features/connections/connector_screen_test.dart`
- `apps/client/test/features/connections/local_server_http_test.dart`
- `apps/client/test/features/connections/local_server_test.dart`
- `apps/client/test/features/connections/native_personal_source_gateway_test.dart`
- `apps/client/test/features/connections/personal_connection_cards_test.dart`
- `apps/client/test/features/connections/personal_contact_selection_test.dart`
- `apps/client/test/features/connections/remote_authority_test.dart`
- `apps/client/test/features/connections/remote_owner_operation_test.dart`
- `apps/client/test/features/connections/remote_pairing_gateway_test.dart`
- `apps/client/test/features/connections/server_connector_panel_test.dart`
- `apps/client/test/features/conversation/agent_controller_test.dart`
- `apps/client/test/features/conversation/agent_conversation_controller_test.dart`
- `apps/client/test/features/conversation/agent_delegation_fixture_test.dart`
- `apps/client/test/features/conversation/agent_interaction_card_test.dart`
- `apps/client/test/features/conversation/agent_interaction_test.dart`
- `apps/client/test/features/conversation/agent_panel_test.dart`
- `apps/client/test/features/conversation/conversation_runtime_gateway_test.dart`
- `apps/client/test/features/conversation/owner_failure_controller_test.dart`
- `apps/client/test/features/day/calendar_agenda_interaction_test.dart`
- `apps/client/test/features/day/calendar_context_rail_test.dart`
- `apps/client/test/features/day/calendar_day_boundary_test.dart`
- `apps/client/test/features/day/calendar_event_details_test.dart`
- `apps/client/test/features/day/calendar_observation_publisher_test.dart`
- `apps/client/test/features/day/calendar_observation_refresh_test.dart`
- `apps/client/test/features/day/calendar_panel_test.dart`
- `apps/client/test/features/day/day_loading_test.dart`
- `apps/client/test/features/day/fake_day_gateway_test.dart`
- `apps/client/test/features/day/native_day_gateway_test.dart`
- `apps/client/test/features/experts/agent_registry_dialog_test.dart`
- `apps/client/test/features/experts/agent_registry_test.dart`
- `apps/client/test/features/knowledge/agent_memory_review_test.dart`
- `apps/client/test/features/knowledge/agent_memory_test.dart`
- `apps/client/test/features/settings/agent_memory_settings_test.dart`
- `apps/client/test/features/settings/settings_screen_test.dart`
- `apps/client/test/infrastructure/diagnostics/app_diagnostics_test.dart`
- `apps/client/test/infrastructure/native/android_context_gateway_test.dart`
- `apps/client/test/infrastructure/native/apple_context_gateway_test.dart`
- `apps/client/test/infrastructure/native/attention_acquisition_broker_test.dart`
- `apps/client/test/infrastructure/native/calendar_acquisition_broker_test.dart`
- `apps/client/test/infrastructure/native/local_context_publication_test.dart`
- `apps/client/test/infrastructure/native/macos_context_gateway_test.dart`
- `apps/client/test/infrastructure/native/personal_acquisition_broker_test.dart`
- `apps/client/test/preview/design_feedback_overlay_test.dart`
- `apps/client/test/preview/design_system_catalog_test.dart`
- `apps/client/test/support/agent_gateway.dart`
- `apps/client/test/support/agent_proposal.dart`
- `apps/client/test/support/agent_registry.dart`
- `apps/client/test/support/agent_vault_gateway.dart`
- `apps/client/test/support/app_host.dart`
- `apps/client/test/support/app_wire_transport.dart`
- `apps/client/test/support/server_credentials.dart`
