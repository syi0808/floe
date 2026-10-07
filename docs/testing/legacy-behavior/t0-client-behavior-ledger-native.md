> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: Apple native and integration

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Static full-source extraction only. No test, compiler, build, formatter or architecture checker was run. No assertion is proof that old behavior is correct or passed. No test removal is performed by this ledger.

Classification: **D** durable safety/property to re-prove at the canonical owner; **P** product hypothesis requiring reassessment; **O** obsolete representation/compatibility evidence to retire; **H** test harness/support. Mixed entries retain the stated safety meaning without preserving the old shape. Exact file and registration hashes/spans are additionally frozen in `t0-client-behavior-ledger-sources.json`.

## apps/client/apple/FloeAppleContacts/Tests/FloeAppleContactsTests/AppleContactsProviderTests.swift

Full source read: lines1–248; SHA-256 `6631f5b3d050a40cb717d06602aba3737f4d33a200e98f6dd23dd16c56c96088`.

Current owner: AppleContactsProvider external Contacts boundary. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### `testConnectionSnapshotsCoverEveryAuthorizationState` (lines9–24; D)

With a deterministic 32-byte handle secret and epoch clock, inspect a mock store in all five authorization states: notDetermined→no scope/request allowed/read denied; limited→selected scope/no request/read allowed; authorized→full/no request/read allowed; denied and restricted→no scope/no request/read denied. This is a five-row authorization matrix, not five registered methods.

### `testRequestAuthorizationTransitionsFromNotDetermined` (lines26–32; D)

Begin with notDetermined and a mock authorization response of limited. Request authorization once; the returned snapshot must report limited and the store request counter must be exactly one. This does not exercise an actual OS prompt.

### `testDeniedRestrictedAndNotDeterminedCannotRead` (lines34–47; D)

For each notDetermined, denied and restricted store, reading a People view throws permissionRequired carrying that precise state before any fetch. Three denial branches fence unauthorized acquisition.

### `testUnresolvedSelectionDeniesBeforeProviderRead` (lines49–59; D)

An authorized store containing One receives an unresolved opaque identity handle. Reading the selected view must throw selectionUnresolved without fetching contacts; unknown handles cannot expand to all contacts.

### `testPermissionRevokedDuringProviderReadIsNotProjected` (lines61–73; D)

An initially authorized store revokes permission while its fetch returns One. The provider must reject the result as permissionRequired(denied) after one fetch instead of projecting data acquired across the revocation race.

### `testLimitedAndAuthorizedCanReadBoundedOpaqueProjection` (lines75–99; D/P)

For limited and authorized access separately, provide 70 records, request 64, and assert exactly 64 identities with incomplete coverage. Source and identity handles must omit raw provider IDs. Person 0 aliases normalize Friend to name:Friend, trimmed/case-normalized email to email:alex@example.com, and formatted phone to phone:+821012345678. Opaque identity/privacy meaning is durable; numeric limits and alias presentation are legacy contract choices to reassess.

### `testExplicitSelectionReturnsOnlyRequestedOpaqueIdentity` (lines101–116; D)

Read two authorized contacts to learn opaque handles, then select only Two. The result contains only Two with complete coverage and the store receives only provider identifier two; an opaque selection is resolved narrowly rather than treated as blanket permission.

### `testInspectedSubjectResolvesProviderIdentifiersAndIsStable` (lines118–132; D)

With limited access and two known contacts, inspect the same selected handles in forward and reverse order. Results must be identical, permission class limited, resolved handles sorted, and native provider identifiers exactly one/two. Selection order does not change reviewed subject identity.

### `testEncodedViewMatchesStrictFixtureShapeAndContainsNoForbiddenFields` (lines134–152; D/O)

Encode a one-contact authorized view and compare exact top-level and identity key sets to the package fixture. Encoded text must omit note, postal, birthday, raw-id, authority and write. Preserve privacy/authority exclusion; exact legacy schema shape is not automatically the target contract.

### `testMaximumProjectionIsReducedToRustPersonalContextBudget` (lines154–174; D/P)

Supply 64 contacts with long names and eight long aliases each. Encoded output must fit maximumSerializedViewBytes, remain nonempty, mark incomplete coverage, and trim at least one identity below eight aliases. Bounding and honest truncation are durable; current pruning policy and byte budget need target review.

### `testRejectsInvalidLimitsSelectionsAndShortSecrets` (lines176–187; D/P)

Construction with an empty secret must fail. The valid provider rejects limits 0 and 65, an empty selected-handle set, and a 129-character handle. Preserve validation before acquisition; concrete legacy bounds are hypotheses.

Dependencies/current owner: FloeAppleContacts provider, mocked AppleContactsStore, fixed clock/secret, Bundle.module resource. makeProvider/record/MockContactsStore are test-only; production provider/store abstractions remain.

## apps/client/apple/FloeAppleHealth/Tests/FloeAppleHealthTests/AppleWellbeingProjectionTests.swift

Full source read: lines1–109; SHA-256 `3fdc7655fbcf47c8d43b581769ea8839a5b96351f87c578df82535440f5ff5bb`.

Current owner: AppleWellbeingReducer and AppleHealthAvailability; legacy pre-transform projection. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### `testIPadRequiresVersion17AndAnAvailableHealthStore` (lines6–11; P)

Availability matrix: iPad with iOS major 16 is unsupported; iPad17 plus available Health store is supported; iPad17 with unavailable store is unsupported; macCatalyst17 remains unsupported. Product/platform boundary evidence, not device validation.

### `testReducerEmitsOnlyCoarseDerivedStateAndOpaqueEvidence` (lines13–34; D/O/P)

Reduce sleep8.25h, steps9100 and exercise35min at fixed time. Legacy output is strong/recovered, confidence700 and 30-minute expiry. Serialized keys are exactly the nine current view fields; sleep_hours, steps, exercise_minutes, samples, provider_id and metadata are absent. Preserve raw-data minimization, but this deterministic pre-transform reducer must not substitute for the accepted source-owned local Health privacy transform.

### `testSerializedFixtureMatchesTheSwiftBoundary` (lines36–48; O)

Using the same aggregate and fixture-specific source/evidence handles, serialized output must equal wellbeing_view.json exactly. This freezes the obsolete derived-view representation, not permission to reuse it as post-transform reasoning input.

### `testShortSleepReducesCapacityAndRequestsRecovery` (lines50–60; P/O)

Sleep5.5h plus steps1000 and no exercise produces reduced capacity, needsRecovery and confidence600. This heuristic is untrusted legacy product behavior subject to replacement.

### `testSleepOnlyLeavesCapacityUnknown` (lines62–73; P/O)

Sleep8.25h alone leaves capacity unknown but recovery recovered, confidence500 and only health.sleep.window evidence. Preserve uncertainty honesty while reassessing the inference heuristic and representation.

### `testStepsOnlyLeavesBothDimensionsUnknown` (lines75–86; D/P/O)

Steps9100 alone yields both dimensions unknown, confidence0 and no evidence. Missing supporting signals must not manufacture certainty; numeric heuristic and legacy model remain replaceable.

### `testExerciseOnlyLeavesBothDimensionsUnknown` (lines88–99; D/P/O)

Exercise45min alone likewise yields unknown/unknown, confidence0 and no evidence; it does not infer wellbeing from activity alone.

### `testNoSignalsProducesNoView` (lines101–108; D/P)

All three signals absent yields no view. No observed data cannot become fabricated health evidence.

Dependencies/current owner: FloeAppleHealth availability/reducer and Bundle.module fixture. The source-owned transform is the accepted future authority; these tests do not validate it.

## apps/client/ios/RunnerTests/RunnerTests.swift

Full source read: lines1–12; SHA-256 `5e8f4908dacea6f29b2a790e4d38bf5fded69096063f9a27a2f21584d62b30c0`.

Current owner: Empty iOS Runner XCTest harness. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### `testExample` (lines7–10; H)

Empty generated XCTest method has no setup, action or assertion. It exercises no product behavior and can retire with its exclusively test-only target after manifest reconciliation.

Dependencies/current owner: Runner product module normalization/attention reducer plus XCTest/FlutterMacOS on macOS; UIKit/Flutter and empty generated XCTest shell on iOS. Production reducers and bridges remain.

## apps/client/ios/ScreenTimeGate/Tests/FloeScreenTimeGateTests/ScreenTimeGateTests.swift

Full source read: lines1–108; SHA-256 `a55f7af11e21eb4942ec58da4ff76fc281528f800e361e2be87fca0a1c1b811d`.

Current owner: ScreenTimeGate / ScreenTimeAttentionReducer / ScreenTimeExport. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### `gateModelsEveryAvailabilityBoundary` (lines8–23; D/P)

Evaluate eight explicit capability rows at one timestamp: unsupported platform, unavailable API, missing entitlement, unavailable region, unknown region, notDetermined authorization, denied authorization, and approved/notRequired region. Each returns its matching outcome; only the final row is supported. Capability uncertainty cannot grant acquisition.

### `reducerExportsOnlyBoundedCoarseAttention` (lines25–48; D/P)

Given supported capability and a15-minute aggregate with720 active seconds/one interruption, reduction is focused, expires after120seconds and carries only aggregate evidence. Export must omit application, bundle, domain, notification, pickup, shield, active_seconds and interruption_count case-insensitively. Privacy boundary is durable; thresholds/expiry are product choices.

### `reducerRejectsUnavailableCapabilityAndInvalidInput` (lines50–70; D)

For a900second interval with1000 active seconds, an unavailable entitlement first yields capabilityUnavailable; with supported capability the same aggregate yields invalidAggregate. Capability denial and invalid bounds are distinct closed failures.

### `inconclusiveAggregateFailsClosedAsStrictUnknown` (lines72–86; D/P)

Supported capability plus400 active seconds/four interruptions in900seconds is inconclusive: strict unknown state, confidence0 and no evidence. Preserve uncertainty honesty; exact threshold is a hypothesis.

### `fixturesContainOnlyCapabilityOrCoarseView` (lines88–98; D/O)

Each of supported_attention, unknown_attention and unsupported_capability resource fixtures must decode as ScreenTimeExport, and text must omit bundle_id, application_name, domain, url, notification, pickup and shield. All three fixture branches are explicit; strict representation may change while privacy must survive.

Dependencies/current owner: FloeScreenTimeGate capability/reducer/export and bundled fixtures; supportedCapability is local test support. No actual Screen Time authorization/entitlement or device operation occurs.

## apps/client/macos/RunnerTests/RunnerTests.swift

Full source read: lines1–105; SHA-256 `64c09c8caac4f06240439c29e08a731849df0784c95f1511d6e0df94c6021d50`.

Current owner: macOS Runner all-day normalization/Attention reducer. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### `testAllDayEndAtLastSecondBecomesExclusiveNextDay` (lines8–19; P)

For Asia/Seoul Gregorian all-day September11 2026 whose end is23:59:59, normalize the end to the next civil-day midnight. This handles inclusive last-second native representation.

### `testAllDayEndAtMidnightRemainsExclusive` (lines21–30; P)

For the same start with end already at midnight two days later, retain that exclusive end instead of adding another day.

### `testAttentionReducerUsesOnlyCoarseActivationCounts` (lines32–47; D/P)

Record six coarse activations at10–60seconds and project at90seconds with2seconds idle. Output is high_interruption_pressure, confidence800, and only switch_pressure/recent_input evidence. No application identity is fed to this fixture; current scoring is a product hypothesis.

### `testAttentionReducerDiscardsActivityOlderThanFifteenMinutes` (lines49–57; D/P)

Record an activation at start, then project901seconds later. The retained activation count becomes zero, bounding retention to15minutes.

### `testAttentionReducerReturnsUnknownUntilObservationWindowExists` (lines59–68; D/P)

Project30seconds after startup with recent input but no full observation window. Return unknown, confidence0 and no evidence instead of inferring focus from insufficient history.

### `testAttentionReducerProducesFirstPresentSampleAtPublisherCadence` (lines70–80; P)

Project at44 then45seconds after startup. The former remains unknown; the latter becomes focused with confidence750, aligning the first sample with publisher cadence. Exact timing/scoring needs product review.

### `testAttentionReducerDoesNotTreatInactiveSessionAsInterruptible` (lines82–92; D)

Mark session inactive at60seconds and project at90seconds despite recent-input value2. Return unknown, confidence0 and no evidence; inactivity is not permission to declare interruptibility.

### `testAttentionReducerRequiresRecentPresence` (lines94–103; D/P)

After360seconds with300seconds idle, return unknown/zero/no evidence. Old presence cannot imply current availability; current idle threshold remains a hypothesis.

Dependencies/current owner: Runner product module normalization/attention reducer plus XCTest/FlutterMacOS on macOS; UIKit/Flutter and empty generated XCTest shell on iOS. Production reducers and bridges remain.
## Integration launchers and subprocess-host behavior

Each launcher below registers exactly one Flutter `test`; the host scenarios are additional executed behavior, not independently registered Flutter tests. All six integration sources were read completely. Their SHA-256 and registration spans are in the shared source ledger. Real execution would build, sign, start native processes and touch isolated filesystem/Keychain state; none occurred in T0.

### apps/client/integration/product_conversation_test.dart: main/test (lines6–100; H/D)

Preconditions: macOS and the same-source debug FFI dylib must exist; failure is an assertion, not a skip. Create a private temporary copy of Flutter tester and the validation Info.plist, build the `product_conversation_host.dart` bundle, build the same-source Swift6 LocalModel library for the host architecture, set its install name, ad-hoc sign library/bundle and verify the bundle. Every subprocess must exit0. Run it with `FLOE_VALIDATION_FFI`, a three-minute test timeout, and teardown that terminates/awaits the process and removes its private temporary bundle. Success requires exit0 plus PRODUCT_CONVERSATION_PASSED, VALIDATION_EXACT_VAULT_KEY_ABSENT and VALIDATION_PROFILE_REMOVED. These sentinels only assert the host contract; they do not independently verify models or cleanup. Dependencies include native product LocalModel source, FFI, Flutter build tooling and the production validation plist, which are not deletable merely because the launcher imports them.

### apps/client/integration/support/product_conversation_host.dart: exercise/rawRun/requireSecretFree (lines14–224; D/P/O/H)

Create a private profile, open a real AppRuntime, create/record its Vault and start a conversation. Two explicit branches run one ordinary greeting with automatic profile and one with `foundation-device`; profile selection is **O**, superseded by accepted Gateway-primary/local-fallback semantics. Each branch first synchronizes the read model, then admits a turn and records observed Runs. It requires a finished Run for the same session, completed/generated report with final message reference, at least one model attempt, no manufactured Task, nonempty assistant text, and a command receipt whose Run/runtime epoch matches admission. The terminal session must retain the person, have no active turn and a revision at least the receipt revision, and the observer must receive the same Run. **D**: recursive result-map scanning rejects keys matching bearer/token/base_url/route; no credentials/topology are exposed. **P**: exact response expectations assume an available local model and do not exercise provider failure.

After both branches, history must contain exactly two user/assistant pairs. Close/reopen the runtime, unlock the same Vault, load/synchronize the same session and compare every turn ID/kind/text, session revision and no-active-turn status. For both saved Runs, run/session/revision/executor-generation/state/report/attempt/task references must remain JSON-equal; each final message must preserve ID/role/text; both Runs must enter the synchronized read model. **D**: reopen is a read/recovery path and must not replay model work or duplicate messages. Finally invoke exact private-profile cleanup even on a failure. `main` initializes Flutter, requires the FFI environment value, runs exercise, exits0 on success or writes error/stack and exits1. `require` throws on a false condition; helpers are **H**, not production APIs.

### apps/client/integration/native_calendar_fixture_test.dart: main/test (lines6–92; H/D)

Require macOS/debug FFI or fail. Make a mode700 private copied tester bundle with the native-calendar validation plist. Build the Calendar host bundle, request the shared immutable Calendar native fixture through `build_test_fixtures.py calendar`, copy the resulting library into the private bundle, sign/verify, then run with FFI environment. Every build/sign/run step must exit0. A five-minute timeout bounds observation; teardown terminates/awaits the process and removes this temporary bundle. Require all three scenario sentinels and exactly three profile-removal sentinels. The copied native fixture must not leave `Contents/MacOS/creates.txt`: none of the scenarios authorizes a native Calendar write. Fixture builder and shared inputs have other Rust consumers and remain outside this deletion batch.

### apps/client/integration/support/native_calendar_fixture_host.dart: mirrorContinuity (lines84–160; D)

In a private local-person profile, a fake adapter exposes Home and one read-only event while the real runtime uses a native subject fixture. `establish` reviews a selected native source and requires Ready with the fixed fixture subject fingerprint. Sync once: the Day mirror refers to that exact source/provider event_kit, has one event and mirror revision1, while Connections source revision/authority remain unchanged. Change the adapter event to modifiable and sync again: content changes and mirror revision2, but source revision/authority still do not. Drain Day, close/reopen, then verify source identity, modifiable content, revision2, Home resources and unchanged authority persist. This separates projection revision from source authority. Always drain before private-profile cleanup; no actual EventKit write is invoked.

### native_calendar_fixture_host.dart: inventoryReconciliation (lines162–224; D)

Establish all_available authority over Home and sync. Add Work to inventory, then sync: the source now selects home/work, increments source revision exactly once and changes authority; two events import. Make Work reads fail permission_denied and sync: surface a Work status error but keep the already-reviewed source revision, authority and selected resources unchanged. Acquisition failure cannot silently narrow or erase authority. Drain/cleanup on every exit. The inventory change is an explicit source reconciliation behavior, not a permission grant from a successful read.

### native_calendar_fixture_host.dart: wideSource (lines226–259; D/P)

Expose maxCalendarCount+1 calendars, establish the selected source and sync. Source resources and imported event count must retain the whole inventory and Day Calendar error must remain null despite bounded Context publication. Authority/import scope is not limited by the projection cap; the precise cap is **P**. `main` invokes all three scenarios sequentially and exits1 on any thrown failure. `FixtureCalendarAdapter` returns configured inventory/records, can throw permission_denied for one calendar, and has a no-op Settings call; it is **H**, not proof of OS access or Settings behavior.

### apps/client/integration/local_server_pairing_test.dart: main/test (lines13–178; D/H)

Skip explicitly unless macOS plus debug FFI exists. Create a private validation profile and real native Vault/remote-pairing gateway bound to its person/device. Reserve a literal IPv4 loopback port, build the shared server fixture, start it with private node data and all OAuth/inference config blank, then await its listening stderr line. The HTTP helper forces DIRECT proxying, attaches same-origin/CSRF/session-cookie state, asserts200, and reads only this server's admin token. This is integration harness setup, not approval to access any existing account.

Prepare owner key, request strict server challenge, confirm through the native owner and inspect status: both local_confirmed replies must omit credentials. Approve the pairing through private manage API with issuer fingerprint, finalize, construct the approved connection and persist it only in MemoryServerCredentials. Reread JSON must exactly equal the approved connection before explicit release; preserve **D** credential-retention/release ordering and identity binding. Check authenticated connection, require every inference purpose present but unavailable and empty privacy activity. Delete the private server client and require the old credential to produce typed unauthorized. The two-minute test timeout does not itself establish backend cancellation. Teardown closes HTTP, terminates/awaits the server and invokes exact profile cleanup. This source references a Go-backed binary but no Go test meaning is inferred here.

### apps/client/integration/support/disposable_product_profile.dart (lines1–118; H/D)

`create` makes a fresh temporary root, mode700, random/default person UUID and distinct validation-device ID, creates a person directory and writes the exact local-device marker. `open` rejects double-open and opens a real runtime in that profile; `closeRuntime` closes then clears it. `recordVault` checks every root/person/Vault path is an actual directory with links not followed, and the marker is an actual file, then requires a lowercase version4 UUID that cannot change after first record. A malformed, linked or changed marker fails closed.

`cleanup` closes runtime first. If a Vault directory exists it revalidates ownership/identity, deletes only Keychain service com.floe.agent-vault.v1 and account personId/vaultId, accepts only success/already-absent44, then verifies absence with exit44 before emitting the exact-key sentinel. A recorded but disappeared marker, unexpected deletion status or inability to verify absence throws; no recursive profile removal follows. Only after this check may the helper remove its own root and verify absence. Every cleanup failure rethrows with retained-profile guidance, explicitly forbidding a shared-data reset. These safety semantics remain needed in a replacement validation harness; reading/deleting this source never authorizes actual credential deletion now.

### apps/client/test/app/runtime/disposable_product_profile_test.dart (three registrations; D/H)

- `unused disposable profile removes only its private files` (8–16): create an unused profile, read its local_device_id and require exact identity, cleanup and require root absent. No Vault exists, so cleanup must not touch Keychain.
- `malformed Vault marker retains evidence without Keychain deletion` (18–31): create an owned Vault directory but write invalid marker; cleanup throws StateError and root remains. Test teardown later removes only the private fixture directory; the failure itself must retain evidence.
- `symlink Vault marker cannot authorize Keychain cleanup` (33–45): link vault.id to a UUID-bearing different private file. Cleanup throws and target still exists, preventing link-following from authorizing key deletion. These tests do not simulate Keychain failures after valid identity.

### apps/client/test/support/app_host.dart (lines1–53; H/D)

`TestAppHost.open` persists a local-device marker adjacent to the requested database; if it exists with a different ID, reject before runtime open. It creates a localPersonId profile and composes real AppRuntime, NativeDayGateway and CalendarActionFacade. `close` drains Day before closing runtime. It is shared test-only support with real FFI/filesystem dependencies, not a production lifecycle owner or the stricter DisposableProductProfile cleanup helper.

## Native targets, resources and cross-language consumer graph

- `FloeAppleContacts/Package.swift` declares product library plus testTarget FloeAppleContactsTests with only that product dependency and processed Fixtures; `FloeAppleHealth` does the analogous library/testTarget pairing. `ScreenTimeGate/Package.swift` declares the library plus testTarget FloeScreenTimeGateTests and copied Fixtures. Remove only test targets/resources when approved; product platform declarations, source targets and libraries remain.
- iOS and macOS each have a RunnerTests.swift source reference/build file, RunnerTests group/product, native target, dependency/proxy to Runner, build configurations and configuration list in project.pbxproj, plus a non-skipped parallelizable TestableReference in the shared Runner scheme. Exact IDs are frozen with the full project hashes. Do not remove Runner launch/build/profile actions, Flutter preparation, native product frameworks or unrelated app configuration. Empty iOS test still has a real registered target.
- Apple Contacts people_view_shape.json is loaded by its Swift package test and packaged via `.process(Fixtures)`. The exact observed fixture contains opaque source/identity/evidence handles, display name and normalized email, no raw provider ID/authority. No external consumer was found by named-path search; deletion still waits for aggregate residual/consumer review.
- Apple Health wellbeing_view.json is consumed by Swift boundary equality and `crates/experts/builtin/tests/apple_wellbeing_projection.rs`. Keep the fixture while that Rust consumer exists, regardless of deleting Swift tests. The fixture freezes the legacy pre-transform derived view and is not a target Health requirement.
- Screen Time supported_attention.json and unknown_attention.json are consumed by Swift plus `crates/experts/builtin/tests/personal_context.rs`; unsupported_capability.json is consumed by Swift. Package resource-copy is an additional edge. Supported fixture has focused/750 and coarse evidence; unknown has zero/empty; unsupported contains entitlement-unavailable capability with no attention. None conveys permission independently.
- Shared Calendar/server fixture builder is used by these launchers and Rust provider/native-actions test consumers. Preserve `tools/validation/build_test_fixtures.py`, native signed-host plist inputs, production LocalModel/EventKit code and immutable cached fixture outputs until their own owner review. Private mutable profiles and credentials must never be shared or reset by extraction.
- Outside this native partition, Dart delegation reads `fixtures/expert-report/delegation-v1.json`, also produced/consumed by the Rust settlement test. Settings reads Gmail ready_snapshot.json, also consumed by Rust provider/Connections tests. These fixtures are not client-owned deletions.
- `apps/client/pubspec.yaml` uses flutter_test only as a dev dependency; flutter_lints still serves analysis_options.yaml and cannot be removed by association. Production fonts/assets, localization, ffi and other dependencies remain. Pubspec/lock changes await all Dart consumer accounting.

## Additional tracked diagnostic and dormant-platform audit

`apps/client/tool/mobile_vault_smoke.dart` was read fully (lines1–67; exact hash in source ledger) and is **production/opt-in diagnostic KEEP**, not a Flutter test registration. It uses application-support/mobile-vault-smoke, opens the same selected profile twice sequentially, creates only when status is missing otherwise unlocks, requires ready, and on first pass opens a contender that must fail conflict rather than admit a second Vault owner. Each runtime locks/closes; success/failure sentinel is printed and displayed in a Flutter app. It does not delete the profile or keys. This diagnostic touches real secure storage if invoked and must not be run or removed as incidental test cleanup.

The six dormant Android fixture JSON files were read fully. `calendar_view.json` feeds Rust builtin Calendar-context tests; `people_view.json` and `wellbeing_view.json` feed Rust builtin personal-context tests; Calendar/Contacts/Health-Connect snapshot JSON feeds Rust Connections connected-context tests. Snapshot descriptors retain provider identity, observe-only capabilities and bounded provenance; Health is explicitly highly_sensitive/derived_only. They are shared corpus **KEEP pending aggregate consumer migration**, not orphans after Dart tests disappear. No Android implementation/build/test work is authorized by this audit.

Android app/root/settings Gradle files and all three Windows CMake files were read fully. No tracked native Android test or androidTest source tree, JUnit registration/dependency, CTest add_test/enable_testing or Windows test target was found. Gradle buildRust and Windows Flutter/app build targets are product build tooling KEEP. Native package tests are only the three Swift testTargets and two Runner XCTest targets already listed; Swift Testing @Test is counted independently from XCTest method naming.

`pubspec.lock` was read fully (599 lines): flutter_test and flutter_lints are direct dev dependencies. test_api, matcher, fake_async, leak-tracker packages and other packages are transitive entries, but a lockfile does not encode a complete reverse-dependency graph. Do not delete transitive packages by their names; preserve entries until approved package resolution can establish exclusivity. `analysis_options.yaml` explicitly includes flutter_lints/flutter.yaml, so the lint dependency remains used without old tests. No package resolution/install was run.

Client README was read fully. It documents real opt-in diagnostic flows and same-source native prerequisites that must remain, but its claim that widget/geometry suites are not maintained contradicts the tracked tests; its catalog section also explicitly asks for widget tests. T0 records the conflict rather than using prose to exclude actual registered suites. References to removed tests/targets need later editorial reconciliation, not deletion of the diagnostic guides/product preview code.

## Exact native target object references

These source-read object spans and per-object hashes in the source ledger guide a future surgical target edit. They are not an edit script. References to these IDs inside surviving Groups/Products/Project target arrays and shared schemes must be removed with the target; product Runner targets, package products, entitlements and build phases remain.

### apps/client/ios/Runner.xcodeproj/project.pbxproj test-only object spans

- `331C808B294A63AB00263BE5`: lines 11–11
- `331C8085294A63A400263BE5`: lines 27–33
- `331C807B294A618700263BE5`: lines 52–52
- `331C8081294A63A400263BE5`: lines 53–53
- `331C8082294A63A400263BE5`: lines 87–94
- `331C8080294A63A400263BE5`: lines 148–164
- `331C8080294A63A400263BE5`: lines 203–206
- `331C807F294A63A400263BE5`: lines 239–245
- `331C807D294A63A400263BE5`: lines 330–337
- `331C8086294A63A400263BE5`: lines 353–357
- `331C8088294A63A400263BE5`: lines 456–472
- `331C8089294A63A400263BE5`: lines 473–487
- `331C808A294A63A400263BE5`: lines 488–502
- `331C8087294A63A400263BE5`: lines 666–675
### apps/client/macos/Runner.xcodeproj/project.pbxproj test-only object spans

- `331C80D8294CF71000263BE5`: lines 24–24
- `331C80D9294CF71000263BE5`: lines 35–41
- `331C80D5294CF71000263BE5`: lines 65–65
- `331C80D7294CF71000263BE5`: lines 66–66
- `331C80D2294CF70F00263BE5`: lines 88–94
- `331C80D6294CF71000263BE5`: lines 106–113
- `331C80D4294CF70F00263BE5`: lines 192–209
- `331C80D4294CF70F00263BE5`: lines 246–249
- `331C80D3294CF70F00263BE5`: lines 290–296
- `331C80D1294CF70F00263BE5`: lines 390–397
- `331C80DA294CF71000263BE5`: lines 412–416
- `331C80DB294CF71000263BE5`: lines 437–450
- `331C80DC294CF71000263BE5`: lines 451–464
- `331C80DD294CF71000263BE5`: lines 465–478
- `331C80DE294CF71000263BE5`: lines 718–727
