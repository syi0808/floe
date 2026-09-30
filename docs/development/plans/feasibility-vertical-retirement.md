# Dedicated Feasibility vertical retirement

- **Status:** implemented; awaiting operator acceptance
- **Execution shape:** one atomic architecture deletion task; no checkpoint series
- **Planning baseline:** `main` at `d33b1df5b51abacd2a0452ebb2134414d4864b6e`
- **Baseline date:** 2026-09-30
- **Primary owners affected:** Context contract, Access, Context, Vault, Native/Provider adapters, App, Protocol/FFI, Flutter, Apple native integration
- **Required workflow:** `AGENTS.md`, `.agents/skills/architecture-change/SKILL.md`, `.agents/skills/code-change-verification/SKILL.md`
- **Durable decision:** [ADR 0032](../../decisions/0032-retire-dedicated-feasibility-vertical.md)

This plan is the only active execution plan for the retirement. It removes the dedicated event/location/ETA/weather Feasibility vertical rather than moving it to Schedule, another Expert, or another source abstraction.

The line numbers below are locators for `d33b1df5`. Source files were unchanged from the immediately preceding Manager-delegation convergence; documentation/tool cleanup moved `main` after that commit. Before implementation, fetch current `origin/main`, record local/fetched HEAD, preserve operator work, and re-resolve each named symbol. If the Feasibility ownership model itself changed, reconcile this plan before editing.

## 1. Why this vertical is being removed

The current identifier `schedule.feasibility.read` looks like one bounded scheduling read, but its implementation is a separate cross-domain vertical:

```text
Calendar event
  + reviewed destination coordinates
  + current location
  + travel mode
  + MapKit directions/ETA
  + WeatherKit event-window weather
      ↓
FeasibilityGrantQuery
      ↓
Feasibility-only contextual Access review
      ↓
Feasibility-only Vault record and authority
      ↓
Apple native acquisition
      ↓
schedule.feasibility View
```

The root Manager no longer exposes this Tool after the Manager–Expert domain-acquisition convergence, and no shipped Expert declares or consumes Feasibility as a selected source. Keeping the substrate therefore preserves a special Access authority, persistence schema, internal wire shape, Flutter product API, location permission, WeatherKit entitlement and Swift package without a canonical conversational owner.

The final system should be smaller rather than retaining this dormant exception.

## 2. Final architecture contract

After completion:

1. There is no `schedule.feasibility` View or `schedule.feasibility.read` capability in production source, protocol, UI, native inventory or tests.
2. There is no `feasibility.apple` connector identity, `apple_feasibility` provider identity or `FloeFeasibilityProvider` package.
3. Schedule remains a Calendar/time-planning Expert. It does not own current location, directions, ETA or weather acquisition.
4. This task does not move travel/location/weather to Life Logistics or another Expert. A future mobility/travel capability must start from a new domain contract and authority design.
5. Access has no Feasibility-specific query review, source identity, grant store port or local-access product API.
6. Fresh Vaults do not create Feasibility review tables. Existing development Vaults containing the retired Feasibility review schema fail explicitly as unsupported local state; there is no migration or automatic deletion.
7. Context personal source handling covers current standing sources only. Feasibility-specific dependency reauthorization, query fingerprinting and acquisition helpers disappear.
8. Personal native acquisition supports People and Wellbeing only. Feasibility-only event/evidence/destination/time/travel fields disappear from the Rust/native/FFI/Dart request shape.
9. Apple context inventory no longer advertises a Feasibility connection. Runner no longer imports CoreLocation for this vertical, links the Feasibility Swift package, requests location permission, or carries WeatherKit entitlement for Floe.
10. Generic words such as “feasible”, Turso feasibility, model/provider feasibility gates and Screen Time feasibility are unrelated and must not be mass-deleted.
11. `GrantPurpose::Assistant`, assistant message/model identities, CryptoKit, and the shared Apple native-subject key remain where used by non-Feasibility owners.
12. No compatibility shim, deprecated command alias, optional legacy fields, no-op connector, hidden feature flag or fake replacement source is retained.

## 3. Baseline inventory and exact disposition

### 3.1 Context contract: delete the View itself

**`crates/contracts/context/src/views/personal.rs`**

Baseline locators:

- line 16: `FEASIBILITY_VIEW_ID`;
- lines 49–54: `WeatherImpact`;
- lines 58–65: `FeasibilityItem`;
- lines 69–76: `FeasibilityView`;
- line 163: `projection!(FeasibilityView)`;
- lines 199–232: `validate_feasibility_view`;
- line 21: Feasibility lifetime constant.

Delete all of these. Keep People, Attention, Wellbeing and the generic personal projection/evidence helpers.

Update tests/fixtures importing these values rather than preserving aliases.

### 3.2 Access: remove the contextual authority exception

**`crates/modules/access/src/application/personal_read.rs`**

- lines 259–299: `FeasibilityGrantQuery` and validation.

Delete it. Keep `PersonalReadRequirement`, `active_read_grant`, grant continuity and subject validation used by standing personal reads.

**`crates/modules/access/src/application/personal_sources.rs`**

- lines 11–13: `FEASIBILITY_CONNECTOR`, `FEASIBILITY_CONNECTION`, `FEASIBILITY_RESOURCE`;
- lines 51–62: `feasibility_source`.

Delete them. `apple_execution_owner` and standing personal-source constants remain.

**`crates/modules/access/src/application/personal_grants.rs`**

The file is otherwise the dedicated Feasibility review application service:

- lines 25 onward: `FeasibilityAccessChange`;
- line 40: `FeasibilityAccessConfiguration`;
- line 48 onward: `FeasibilityAccessOverview` / state;
- lines 177 onward: Feasibility consumer/scope review;
- line 224 onward: `apply_feasibility_access`.

The only non-Feasibility production helper is `attention_consumer` near the end. Move that small validation helper to `application/personal_read.rs` (or a smaller existing Access read owner if current source already has one), preserve its exact failure semantics, then delete `application/personal_grants.rs`.

Do not create a replacement “contextual personal grants” service.

**`crates/modules/access/src/ports/personal_grants.rs`**

Current contents mix two concerns:

- lines 29–33: `FeasibilityReviewRecord`;
- lines 36–41: `PersonalSubjectProbe`, including `Feasibility`;
- lines 61–87: `PersonalGrantStore`, entirely Feasibility-oriented;
- `PersonalSubjectEvidence` and `PersonalSubjectInspector` are still used by Contacts/Attention/Wellbeing review.

Converge the file boundary:

1. delete `FeasibilityReviewRecord`, `PersonalGrantStore` and the `Feasibility` probe variant;
2. move the remaining subject-inspection types to `ports/personal_subject.rs`;
3. change `ports/mod.rs` and `lib.rs` re-exports;
4. migrate callers without compatibility re-exports from the obsolete module path.

**`crates/modules/access/src/application/mod.rs` and `lib.rs`**

Remove every Feasibility type/function/constant re-export and the `personal_grants` application module. Preserve the generic personal-read and subject-inspection API needed by standing personal sources.

### 3.3 Context: remove acquisition, lineage and reauthorization

**`crates/modules/context/src/application/personal_sources.rs`**

Baseline Feasibility-owned sections include:

- the `FeasibilityQueryLineage` / `feasibility_query_fingerprint` imports near line 25;
- Feasibility constants/source re-exports near line 40;
- the private `PersonalRead`, `CompletedRead`, `admit_personal_read`, `acquire_personal_source` and `personal_dependency` helpers around lines 56–191. These helpers exist for the reviewed Feasibility query path; selected People/Wellbeing have their own standing-source path;
- `read_feasibility` beginning around line 194;
- Feasibility branch of `authorize_personal_dependency` around lines 446–500;
- `feasibility_identity` around line 514 and its blocker classification use;
- `read_feasibility_outcome` around lines 1281–1315;
- Feasibility-specific tests later in the module.

Delete the dedicated helpers and read APIs. Simplify `authorize_personal_dependency` to authorize only current supported standing personal connectors and fail closed for anything else; do not keep a generic “other means Feasibility” branch.

Preserve People/Attention/Wellbeing selected reads, source/grant continuity, OS-permission classification and dependency provenance.

**`crates/modules/context/src/application/personal_lineage.rs`**

- lines 94–102: `FeasibilityQueryLineage`;
- line 104 onward: `feasibility_query_fingerprint`.

Delete both. Keep People/Attention/Wellbeing fingerprints.

**`crates/modules/context/src/ports/personal_source.rs`**

- remove `FeasibilityGrantQuery` / `FeasibilityReviewRecord` imports;
- remove `PersonalGrantRecords::feasibility_review`;
- remove `PersonalDomain::Feasibility`;
- remove `PersonalAcquisition.feasibility`.

The final `PersonalAcquisition` carries only fields shared by People/Wellbeing: Person/device/host epoch/domain/selected handles/expected subject/deadline.

**`crates/modules/context/src/application/observations.rs`**

- remove `FeasibilityView` and validator imports at lines 25–26;
- remove `schedule.feasibility` from `ALLOWED_VIEW_IDS` at line 32;
- remove the Feasibility arm from `validate_view` at line 569.

Do not remove the trusted personal observation registry; it remains useful to standing native sources.

**`crates/modules/context/src/lib.rs`**

Delete:

- line 39 `ASSISTANT_CONSUMER` — after this retirement it has no production owner;
- Feasibility lineage re-exports around line 83;
- Feasibility constants/source/read exports around lines 87–91.

Keep `GrantPurpose::Assistant` elsewhere; that is not this constant.

### 3.4 Vault: remove dedicated persistence without migration

**`crates/adapters/vault/src/vault/feasibility_reviews.rs`**

Delete the file completely. It owns:

- `personal_feasibility_review_schema`;
- `personal_feasibility_reviews`;
- query/subject/source-authority decoding;
- Feasibility review CAS;
- pause/re-enable behavior;
- Feasibility-specific reopen tests.

**`crates/adapters/vault/src/vault.rs`**

Baseline:

- line 30: `mod feasibility_reviews`;
- lines 229 and 320: `initialize_feasibility_review_store`;
- line 334 onward: obsolete-schema rejection.

Remove the module and create/open initialization calls.

Extend `reject_obsolete_policy_schemas` so the retired `personal_feasibility_review_schema` and `personal_feasibility_reviews` tables cause `UnsupportedVersion` on old development profiles. This is an explicit stale-profile fence, not a migration. Do not DROP the tables, rewrite grants, delete a Vault automatically, or regenerate keys.

Move/replace the useful old-schema regression into a remaining Vault test location that can prove:

- a fresh Vault contains no Feasibility review tables;
- a synthetic old Vault carrying the retired table names fails reopen with `UnsupportedVersion`.

**`crates/adapters/vault/src/repositories/personal_grants.rs`**

Remove `feasibility_review` from `PersonalGrantRecords` implementation and delete the entire `PersonalGrantStore` implementation. The remaining adapter should only expose the grant records Context actually reads.

### 3.5 Native acquisition contract: shrink to People + Wellbeing

**`crates/platform/native/src/acquisition/personal.rs`**

Baseline:

- line 12: `PersonalDomain::Feasibility`;
- lines 32–38: `event_handle`, `evidence_handles`, destination coordinates, event window and `travel_mode`.

Delete the Feasibility enum case and all seven Feasibility-only request fields. Keep request/result identity, domain, selected handles, deadline and expected subject.

**`crates/adapters/providers/src/sources/personal_native.rs`**

Remove both `PersonalDomain::Feasibility` mappings and all translation of Feasibility-only query fields into native requests. Remove `PersonalSubjectProbe::Feasibility`.

People and Wellbeing continue through the same real native broker/subject boundary.

**`crates/app/src/local_context.rs`**

Remove the Feasibility domain-to-view arm around line 267. Personal completion must accept only People and Wellbeing.

### 3.6 App: delete the Feasibility-only local Access product surface

**`crates/app/src/local_access_services.rs`**

This file exists solely for Feasibility. Delete it.

**`crates/app/src/local_operations.rs`**

Remove `LocalAccessCommand` / `LocalAccessInspection` intent variants and their action mapping. Keep `LocalOperationOwner::Access`, which still owns `ConnectionObserve`.

**`crates/app/src/worker.rs`**

Remove:

- `WorkerAction::FeasibilityAccess`;
- its stage name and concurrency entry;
- `WorkerResult.feasibility_access`.

**`crates/app/src/vault_host.rs`**

Remove all `feasibility_access` progress/result slots and the `WorkerAction::FeasibilityAccess` execution branch around lines 2581–2596.

Do not alter Conversation, ConnectionObserve, Calendar Action, Registry or Memory scheduling.

**`crates/app/src/lib.rs`**

Remove the `local_access_services` module, Feasibility Access/Query exports and `LocalAccess*` public facades.

**`crates/app/src/vault_host/conversation_turn/interaction_publication.rs`**

Remove the `floe.source.feasibility` generic label. No Feasibility blocker can remain after the source path is gone.

**`crates/app/src/vault_host/review_snapshot.rs`**

Remove query-bound Feasibility comments/test inputs such as `feasibility.apple`. Preserve the general navigation-only behavior for unknown connectors and picker-owned sources.

**Manager regression fixtures**

`crates/app/src/vault_host/conversation_turn/engine_ports.rs`, `conversation_turn.rs`, and `tests/conversation_flows.rs` still mention `schedule.feasibility.read` only as a negative regression for the old seven Manager Tools. Remove that retired identity from those enumerations while keeping the Manager-empty-catalog/fail-closed assertions for the remaining retired domain Tool IDs and arbitrary unexpected Tools.

There is no need to introduce a permanent source-regex tombstone for Feasibility.

### 3.7 Protocol and FFI: remove the entire local Access wire

**`crates/bindings/protocol/src/dto/agent.rs`**

Delete:

- `FeasibilityAccessChangeDto`;
- `FeasibilityGrantQueryDto`;
- `FeasibilityAccessOverviewDto`.

**`crates/bindings/protocol/src/dto/commands.rs`**

Remove `access.feasibility.configure` / `AccessFeasibilityConfigure` and the corresponding `AppCommandResultDto::LocalAccessOperation`.

**`crates/bindings/protocol/src/dto/queries.rs`**

Remove:

- `access.feasibility.inspect`;
- `access.local.read_result`;
- `AppQueryResultDto::LocalAccessOperation`.

**`crates/bindings/protocol/src/dto/local_access.rs`**

Delete the file. Its small `identifier` helper is currently reused once from `dto/actions.rs`; move that validation locally to `actions.rs` or an existing truly shared validation helper before deleting the file. Do not keep `local_access.rs` for one unrelated helper.

Update `dto/mod.rs` and protocol `lib.rs` re-exports.

**`crates/bindings/protocol/src/dto/local_context.rs`**

Remove `LocalContextPersonalDomainDto::Feasibility` and all Feasibility-only personal acquisition request fields.

**`crates/bindings/ffi/src/conversion/owners.rs`**

Remove Feasibility access/query conversion and DTO projection. Remove `feasibility_access` from failure-stage classification/recovery matching.

**`crates/bindings/ffi/src/conversion/native.rs`**

Remove `schedule.feasibility` from allowed local views, the Feasibility personal-domain conversion and the retired request-field mappings.

**`crates/bindings/ffi/src/app_wire.rs`**

Remove command/query routing for Feasibility/LocalAccess, `local_access_result`, and `LocalAccessCommands/Queries` trait bounds.

Do not bump AppWire/protocol versions merely to preserve this pre-stable internal contract. Rust, Dart and bundled native callers move in the same snapshot; old Feasibility commands should fail decoding as unknown.

### 3.8 Flutter: remove feature state, UI and gateway plumbing

Delete:

- `apps/client/lib/features/settings/domain/feasibility_access.dart`;
- `apps/client/lib/features/connections/presentation/feasibility_access_card.dart`;
- `apps/client/test/features/settings/feasibility_access_gateway_test.dart`.

**`apps/client/lib/app/runtime/local_owner_gateways.dart`**

Delete `NativeFeasibilityAccessGateway` and all `access.feasibility.*` / `access.local.read_result` operations.

**`local_owner_gateways_scope.dart`, `app_runtime.dart`, `agent_controller.dart`**

Remove the `feasibilityAccess` owner slot and construction/wiring. Do not replace it with a nullable deprecated field.

**`connector_screen.dart`**

Remove:

- `feasibilityAccessGateway` and Feasibility-only `daySnapshot` plumbing where no longer used;
- `apple_feasibility` recovery/detail branch;
- `_requestQuery`;
- `FeasibilityAccessCard`;
- `_PersonalFeasibilityDialog`, destination latitude/longitude controls and travel-mode picker;
- `apple_feasibility` display name/description/icon mappings.

After this removal, re-check whether `DaySnapshot` imports/constructor fields are still needed by this screen. Delete them if not.

**`settings_screen.dart` / `data_privacy.dart`**

Remove Feasibility gateway and DaySnapshot parameters that existed only to feed this feature, including the unused `_RemoteServerSettings` forwarding field.

**`personal_day_screen.dart`**

Stop passing the removed gateway/day snapshot into Connections/Settings.

### 3.9 Dart native bridge: remove Feasibility request/validation path

**`apps/client/lib/infrastructure/native/apple_context_gateway.dart`**

Delete:

- `AppleContextApi.readFeasibility`;
- `AppleFeasibilitySubjectApi`;
- `AppleFeasibilityQuery`;
- `AppleTravelMode` if no other user remains;
- `readFeasibility`, `inspectFeasibilitySubject`, `requestFeasibilityPermission`;
- `validateAppleFeasibilityResult`.

Change Apple connection inventory validation from four entries to the final three supported entries.

**`local_context_publication.dart`**

Remove `_feasibilityViewId`, Feasibility subject API implementation and publish/revoke logic.

**`personal_acquisition_broker.dart`**

Remove `feasibility` from allowed domains and delete the seven retired request fields/validation. Final request validation should distinguish only:

- People: non-empty selected handles;
- Wellbeing: empty selected handles.

**`apps/client/lib/main.dart`**

Remove the Feasibility branch from `_applePersonalReader`. Keep People/Wellbeing branches and their subject continuity checks.

### 3.10 Apple native: delete location/ETA/weather implementation and platform requirements

Delete the complete directory:

```text
apps/client/apple/FeasibilityProvider/
```

including Package.swift, README, source, tests and fixtures.

**`apps/client/ios/Runner/AppleContextChannel.swift`**

Remove:

- `import CoreLocation`;
- `import FloeFeasibilityProvider`;
- `locationScope`;
- `AppleFeasibilityProvider`, governed provider and permission provider fields;
- initialization of CoreLocation/MapKit/WeatherKit providers;
- method-channel handlers for read/governed read/subject inspection/permission;
- subject fingerprint logic specific to location authorization;
- Feasibility connection snapshot and cached last-view state;
- `FeasibilityFailure` error mapping.

Keep `CryptoKit` and `nativeSubjectKey`: Wellbeing subject fingerprinting still uses them.

**`apps/client/ios/Runner.xcodeproj/project.pbxproj`**

Remove every local package reference, product dependency, Frameworks entry and build-file reference for `FloeFeasibilityProvider`. Do not edit Contacts/Health/ScreenTime package references.

**`Info.plist`**

Remove `NSLocationWhenInUseUsageDescription` after residual search confirms no other current iOS feature requests location.

**`Runner.entitlements`**

Remove `com.apple.developer.weatherkit`. Keep HealthKit.

**`apps/client/ios/APPLE_CONTEXT_SIGNING.md`**

Converge the current signing guide to HealthKit/current Apple context only. Remove WeatherKit enablement, provisioning and live-validation instructions. Historical ADRs retain why WeatherKit once existed.

### 3.11 Prompt and Expert tests

**`crates/experts/builtin/prompts/schedule_expert_role.txt`**

Baseline lines 3–5 mention feasibility, leave-by timing, weather and `schedule.feasibility`.

Rewrite narrowly:

- authorized calendar/task/note/coarse-capacity evidence;
- events, conflicts, availability, priorities, coarse capacity and possible schedule changes;
- `wellbeing.derived` remains optional;
- no travel, current-location, ETA, weather or leave-by facts.

Do not add a replacement Tool or workflow.

Delete `crates/experts/builtin/tests/apple_feasibility_fixture.rs`.

Remove Feasibility sections from `personal_context.rs` and `personal_context_freshness.rs` while preserving People/Wellbeing coverage.

### 3.12 Tests and fixtures that must be ported, not merely deleted

Update at least these current test surfaces:

- `crates/app/src/vault_host/tests/local_product.rs`: remove LocalAccess/Feasibility intent tests; preserve general local operation identity/owner tests.
- `crates/bindings/protocol/tests/local_owner_wire.rs`: remove positive Feasibility DTO/route tests and add a bounded regression that retired `access.feasibility.*` payloads are rejected as unknown; preserve unrelated owner-wire validation.
- `crates/app/tests/connected_calendar.rs`: rename generic Situation IDs such as `schedule.feasibility` to a non-retired scheduling scenario ID; their Calendar degraded/expiry behavior remains.
- `apps/client/test/infrastructure/native/apple_context_gateway_test.dart`: remove Feasibility methods and assert the three-entry Apple inventory.
- `apps/client/test/infrastructure/native/local_context_publication_test.dart`: remove Feasibility publication/revocation tests.
- `apps/client/test/features/connections/connector_screen_test.dart`: remove `apple_feasibility` fixtures/UI expectations.
- `apps/client/test/features/settings/settings_screen_test.dart`: remove Feasibility inventory/UI fixtures.
- `apps/client/test/features/connections/remote_authority_test.dart`: remove constructor plumbing for the deleted gateway if present.
- Swift Feasibility tests disappear with the package.

Do not reduce assertions for Contacts, Health, Calendar, Access, Vault reopen, A2A, provenance or other owner boundaries merely because shared fixtures become smaller.

## 4. Ordered execution

These are sequencing steps inside one task, not checkpoints.

### Step 1 — retire the semantic contracts and authority

1. Remove Context contract Feasibility types.
2. Remove Access query/source/review/store contracts.
3. Move `attention_consumer` and surviving subject-inspection types to accurate owners.
4. Delete the Feasibility application service.
5. Compile affected Rust crates to expose every remaining caller rather than adding compatibility aliases.

Expected fast checks after the bounded compile break is repaired:

```sh
cargo test -p floe-context-contract
cargo test -p floe-access
```

### Step 2 — remove Vault and Context runtime state

1. Delete `vault/feasibility_reviews.rs`.
2. Stop creating its schema on fresh Vaults.
3. Add explicit obsolete-table rejection for existing local development Vaults.
4. Simplify Vault grant-record adapters.
5. Delete Context Feasibility read/blocker/lineage/reauthorization paths and `ASSISTANT_CONSUMER`.
6. Remove Feasibility from observation validation.

Targeted checks:

```sh
cargo test -p floe-vault
cargo test -p floe-context
```

The Vault tests must prove both fresh-schema absence and old-schema fail-closed reopen.

### Step 3 — shrink native and App/FFI contracts

1. Remove Feasibility from Rust personal acquisition domain/request.
2. Migrate provider adapter and local Context host.
3. Delete App LocalAccess service, Worker variant/result and Vault-host branch.
4. Delete Feasibility Protocol DTOs/routes and the local-access DTO/result family.
5. Shrink the local personal acquisition DTO.
6. Remove FFI conversions/routing/trait bounds.
7. Keep all same-snapshot caller changes atomic; no protocol-version compatibility layer.

Targeted checks:

```sh
cargo test -p floe-native
cargo test -p floe-provider-adapters
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo test -p floe-app
cargo build -p floe-ffi
```

### Step 4 — delete Flutter and Apple product surface

1. Delete Feasibility domain/gateway/card/dialog.
2. Remove owner/runtime/controller/day/settings plumbing.
3. Shrink Dart personal acquisition request validation.
4. Remove Apple gateway Feasibility APIs and connection inventory.
5. Delete `apps/client/apple/FeasibilityProvider`.
6. Remove Xcode package/product/build references.
7. Remove location usage description and WeatherKit entitlement.
8. Update Apple signing documentation.

Do not modify signing accounts or provisioning profiles as part of implementation.

### Step 5 — converge prompt, current architecture and tests

1. Narrow Schedule prompt.
2. Port/delete Feasibility tests listed above.
3. Update current architecture:
   - `docs/architecture/modules.md`: remove App/Access/standing-source Feasibility exception;
   - `docs/architecture/runtime.md`: remove retained contextual Feasibility substrate statement;
   - `docs/architecture/authority-recovery.md`: remove Feasibility query authority/Vault semantics.
4. Do not rewrite historical ADR bodies. ADR 0032 and the ADR-index amendment notes created with this plan are the durable rationale.
5. Keep product docs aligned with the already-accepted retirement decision; do not re-add location/ETA/weather to Schedule as a replacement.

### Step 6 — residual deletion audit

Run from repository root after all edits:

```sh
rg -n   'schedule\.feasibility|schedule\.feasibility\.read|FeasibilityView|FeasibilityItem|WeatherImpact|FeasibilityGrantQuery|FeasibilityAccess|FEASIBILITY_(VIEW_ID|CONNECTOR|CONNECTION|RESOURCE)|feasibility\.apple|apple_feasibility|FloeFeasibilityProvider|PersonalDomain::Feasibility|LocalContextPersonalDomainDto::Feasibility|read_feasibility|readFeasibility'   crates apps tools docs
```

Expected:

- zero current production/test/config matches;
- this active plan and ADR 0032 may name retired identities;
- older ADR bodies may retain historical names as amended history.

Then:

```sh
rg -n 'CoreLocation|CLLocationManager|MapKit|WeatherKit|NSLocationWhenInUseUsageDescription|com\.apple\.developer\.weatherkit' apps/client
```

Expected: zero current app/native/signing/config matches after the dedicated provider is deleted. If another real owner appears at implementation time, classify it before retaining anything.

Then:

```sh
git ls-files 'apps/client/apple/FeasibilityProvider/**'
```

Expected: no output.

Finally run a broad semantic search:

```sh
rg -ni '\bfeasibility\b' crates apps tools docs
```

Do **not** require zero. Classify remaining matches:

- ADR 0032 / superseded historical ADR text;
- generic English feasibility;
- model/provider feasibility gates;
- Screen Time feasibility;
- other unrelated engineering terminology.

Any match that still denotes the retired event/location/ETA/weather vertical is a defect.

## 5. Documentation state

This planning commit performs the documentation cleanup that can be correct before code deletion:

- the completed Manager–Expert plan is already retired from current `main`;
- this plan becomes the sole active execution plan in `docs/README.md`;
- ADR 0032 records the accepted retirement decision;
- the ADR index points affected historical decisions to ADR 0032 instead of rewriting history;
- product intelligence/roadmap/privacy docs stop presenting the dedicated Feasibility vertical as current product direction.

Current architecture docs are intentionally not changed to a future state in this planning commit. They still describe the code that exists at `d33b1df5` and must be updated in the implementation change when the code disappears.

At accepted completion, remove this temporary plan and its `docs/README.md` active-plan pointer. Git history is the archive; do not keep a permanent completion ledger.

## 6. Verification

### Targeted Rust

Run the affected owners first:

```sh
cargo test -p floe-context-contract
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-native
cargo test -p floe-provider-adapters
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo test -p floe-app
```

If a package name has changed on current `main`, resolve the current Cargo package name rather than skipping its tests.

### Broad Rust / architecture

Use the current repository gate:

```sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
```

Do not recreate the recently retired one-off source-shape architecture checkers for this migration.

### FFI / Flutter

```sh
cargo build -p floe-ffi
cd apps/client
flutter analyze
flutter test
flutter build macos
```

If the known registry golden baseline still fails, reproduce/classify it rather than updating unrelated goldens.

### iOS/native boundary

Because the task removes an iOS local Swift package, Xcode package references, a usage description and an entitlement, verify the iOS target from the same snapshot when the local SDK/runtime is available:

```sh
cd apps/client
flutter build ios --simulator --no-codesign
```

If the environment lacks the required iOS SDK/runtime, report this gate as UNAVAILABLE with the exact blocker. Do not install runtimes, alter signing accounts or provisioning, reset TCC, or touch shared credentials merely to make the gate runnable.

A physical device, location permission and WeatherKit account are **not** required after removal; no live Feasibility acceptance remains.

### Persistence and safety

Tests must specifically cover:

- fresh Vault creation/reopen without Feasibility tables;
- old development Vault with retired Feasibility table names -> `UnsupportedVersion`;
- no automatic Vault/key reset;
- standing Contacts/Attention/Wellbeing grant and dependency behavior unchanged;
- no Feasibility source/capability can be admitted through AppWire/native personal acquisition;
- current Calendar/Expert/Action authority and provenance tests remain intact.

No Go or Android implementation/validation is required unless the implementation unexpectedly touches those boundaries.

## 7. Acceptance criteria

The task is complete only when:

1. The retired Feasibility View/capability/source/provider identities are absent from current production code.
2. No Feasibility Access/query/grant/store API remains.
3. No fresh Vault creates Feasibility review tables; old profiles with them fail explicitly.
4. Context contains no Feasibility read, blocker, query lineage or dependency reauthorization branch.
5. Personal native acquisition contains only People and Wellbeing.
6. App contains no LocalAccess facade created solely for Feasibility.
7. Protocol/FFI expose no `access.feasibility.*`, `access.local.read_result`, Feasibility DTO or Feasibility native-domain variant.
8. Flutter has no Feasibility gateway/card/dialog/connection entry or dead day-snapshot plumbing.
9. iOS Runner no longer links `FloeFeasibilityProvider`, advertises `feasibility.apple`, requests location access or carries WeatherKit entitlement.
10. `apps/client/apple/FeasibilityProvider` is deleted.
11. Schedule prompt contains no travel/ETA/weather/leave-by Feasibility capability claim.
12. Generic Manager Tool runtime/A2A architecture remains unchanged.
13. Contacts, Attention, Wellbeing, Calendar and other current sources keep their authority/provenance tests.
14. Current architecture docs describe the post-removal system; historical ADRs remain historical and point to ADR 0032 through the decision map.
15. Residual audit contains only explicitly historical/unrelated uses of the ordinary word “feasibility”.
16. Required targeted/broad/Flutter gates pass, with iOS simulator build either PASS or explicitly UNAVAILABLE for environment reasons.

## 8. Commit discipline

Implement this as one coherent retirement. Multiple local commits are acceptable for iteration, but the final local history must not leave an intermediate public design such as:

- disabled-but-still-advertised Feasibility;
- deprecated wire aliases;
- retained Vault tables “for compatibility”;
- a no-op location provider;
- a Schedule-to-Feasibility callback;
- a fake standing source;
- a feature flag retaining the old vertical.

Prefer one final implementation commit if practical. Do not push, open a PR, change signing accounts, delete user data or alter external credentials unless the operator explicitly authorizes it.

## 9. Agent report format

At completion report:

1. start HEAD / fetched origin-main / final local HEAD and commit SHA(s);
2. deleted Feasibility contracts/files/identities by owner;
3. final personal-source and native-acquisition shapes;
4. Vault fresh-schema and old-profile rejection evidence;
5. final AppWire/FFI surface and proof old Feasibility routes reject;
6. Flutter/Apple removals, including Xcode package, location permission and WeatherKit entitlement;
7. Schedule prompt/domain final wording;
8. residual-audit classifications;
9. exact targeted/broad/Flutter/iOS verification results;
10. architecture/docs convergence;
11. worktree status and any remaining blocker.

Record execution evidence in a final section of this plan only while the task is active. After operator acceptance, retire the plan and README pointer.

## 10. Execution evidence — 2026-09-30

### 1. Baseline and local history

- Start HEAD and fetched `origin/main`: `86df5a7898144c4970c7ae6ac316e92481d5e5df`; initial worktree clean. Current symbols were resolved against that snapshot rather than the planning locators.
- Implementation is one local atomic commit, whose parent is that baseline and whose tree includes this report. Its final SHA is reported in the operator handoff; no intermediate public design is committed.
- No branch creation, push, PR, deployment or external-account change was performed.

### 2. Deleted contracts and owners

- Context contract: Feasibility View/item/weather types, identifiers, validation and projection removed.
- Access: Feasibility query/source authority and personal-grant service/port removed. Surviving personal subject evidence lives in `personal_subject`; Attention consumer validation remains in personal read authorization.
- Context: contextual acquisition, blockers, review/query lineage and assistant-specific exception removed. One canonical standing personal-dependency authorization path remains.
- Vault: review module, records, initialization and store adapter removed, together with the retired transactional source lookup.
- App/Native/Providers: local Access service, intents, worker/result branches, Feasibility domain/probe and provider translation removed. No compatibility adapter or replacement source remains.

### 3. Final source and acquisition shapes

- Personal observations are People, Attention, Wellbeing and Calendar. Standing grant/provenance/CAS/recovery paths remain; the removed vertical is not a standing source.
- Native personal acquisition has People and Wellbeing only. People requires a nonempty subject selection; Wellbeing accepts empty selection. Attention retains its separate existing path.
- Event identifier, destination latitude/longitude, window start/end, travel mode and current-location purpose fields are absent from the native and wire acquisition contracts.

### 4. Persistence and profile safety

- Fresh Vault create/reopen tests prove neither retired review table is created.
- Opening a database containing retired review tables returns `UnsupportedVersion`; tests prove the original table, Vault identity and key slots remain unchanged. The existing obsolete personal-policy/query fences remain fail-closed too.
- No old decoder, migration chain, version bump, automatic reset, key replacement or user-database deletion was added. Tests use isolated stores, not an existing development profile.

### 5. AppWire and FFI

- Local Feasibility Access requests/results/DTOs and FFI conversions are deleted; wire versions are unchanged and same-snapshot callers are migrated.
- `retired_access_routes_are_unknown` proves configure, inspect and local-read-result routes reject as unknown variants, alongside arbitrary unknown routes.
- `personal_acquisition_accepts_only_current_domains_and_fields` proves current DTOs succeed while the retired domain and all seven retired fields reject. Remaining correlation and standing-authority assertions are retained.

### 6. Flutter and Apple

- Gateway, controller/scope plumbing, cards, review UI, stale labels/icons and native broker branches are removed. Inventory is Contacts/Health/ScreenTime (three entries); regression tests reject two/four entries.
- All tracked `apps/client/apple/FeasibilityProvider` files and its inspected ignored build cache are removed; the physical directory and tracked-file inventory are empty.
- Xcode package/product references, location API/permission declarations and WeatherKit entitlement are removed. HealthKit, Contacts/Health/ScreenTime packages, key identity and Calendar native integration remain.
- Source plist/entitlement/project checks pass; simulator output retains Contacts/Calendar/Health usage descriptions and has no location usage key. No signing account, shared credential or TCC state was changed.

### 7. Schedule scope

- Schedule is Calendar/time-planning: authorized calendar/task/note context, events/conflicts/availability/priorities, optional coarse `wellbeing.derived` capacity and possible changes.
- Unavailable capacity remains uncertain. No travel/location/ETA/weather/leave-by claim, replacement feature or retired tool fixture remains.

### 8. Residual audit

- Section 4's exact primary-symbol search has no current production/test/config matches; only this temporary plan, ADR 0032 and historical ADR text name those identities.
- Apple location/MapKit/WeatherKit search and retired-package tracked-file search return no matches. The package directory is physically absent.
- Expanded local Access/Feasibility symbol audit leaves only negative wire rejection fixtures. Obsolete table names remain solely as required fail-closed schema fences and preservation tests, not as authority or initialized tables.
- Broad ordinary-word matches are classified as temporary plan/README evidence, decision/history references, negative protocol fixtures and unrelated ScreenTime engineering prose. No executable retired vertical remains; no permanent regex checker was added.

### 9. Verification results

Environment: Rust 1.93.1; Flutter 3.47.2 stable / Dart 3.13.2; Xcode 26.0.1 (17A400).

- PASS: `cargo test -p floe-context-contract`, `cargo test -p floe-access`, `cargo test -p floe-context`, `cargo test -p floe-vault`, `cargo test -p floe-native`, `cargo test -p floe-provider-adapters`, `cargo test -p floe-protocol`, `cargo test -p floe-ffi`, `cargo test -p floe-app`.
- PASS: `cargo check --workspace --lib`; `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast`; `python3 tools/architecture/check_boundaries.py` (22 nodes, 105 edges, no errors/warnings); `git diff --check` and staged equivalent.
- PASS from `apps/client`: `flutter analyze` (no issues), `flutter test test/infrastructure/native/apple_context_gateway_test.dart test/infrastructure/native/local_context_publication_test.dart test/infrastructure/native/personal_acquisition_broker_test.dart test/features/connections/connector_screen_test.dart test/features/connections/remote_authority_test.dart test/features/settings/settings_screen_test.dart` (44 passing), `flutter build macos`, `flutter build ios --simulator --no-codesign`. Final builds follow `cargo build -p floe-ffi` from the same source snapshot (PASS).
- Full `flutter test`: 366 pass, one pre-existing failure in `agent_registry_dialog_test.dart` at width 520. The unchanged starting HEAD was archived outside the repository and the same test rerun: identical 29-pixel (0.02%) golden difference, identical isolated-diff SHA-256 `cd8d1fb2afc840a77357eeb9e0fda5f536e3845e838f0ad236fffb2b73b44caa`. The unrelated golden was not changed; generated failures were removed from the worktree.
- PASS: `swift test --package-path apps/client/apple/FloeAppleContacts` (11 tests), `swift test --package-path apps/client/apple/FloeAppleHealth` (8), `swift test --package-path apps/client/ios/ScreenTimeGate` (5), `bash tools/validation/calendar/check-native.sh` (32 deterministic assertions). Calendar validation performs no permission request or event read.
- PASS: `plutil -lint` on changed Apple plist/entitlement/project files and the residual audits above. Changed Rust/Dart files were formatted without a repository-wide formatting gate.
- During iteration, a missed obsolete positive protocol fixture was removed, not reaccepted. Initial App fixture failures under concurrent build load were rerun without weakening assertions/timeouts; final targeted App and broad workspace runs pass. The non-executable Calendar script was invoked through `bash` successfully.
- Local logs: `/tmp/floe-retirement-{context-final,protocol-final,app-final,check-final,workspace-test,boundaries-final,analyze-final,flutter-targeted,flutter-test-final,baseline-golden,native,calendar-native,ffi-snapshot,macos-snapshot,ios-snapshot,residual-final,apple-residual-final,broad-residual-final}.log`; other targeted crate logs use `/tmp/floe-retirement-floe-<crate>.log`. These are local diagnostic artifacts, not a permanent repository ledger.

### 10. Architecture and documentation convergence

- Current modules/runtime/authority-recovery documents describe the resulting owners, standing personal paths, schema rejection and Calendar/time-planning boundary.
- ADR 0032 and the decision-map amendments preserve the rationale; historical ADR bodies remain historical. No new progress document or replacement architecture was introduced.
- This report and the README pointer remain pending operator acceptance, after which both temporary artifacts should be retired as specified above.

### 11. Completion and safety audit

- The section 7 acceptance criteria are satisfied by source deletion, rejection/persistence regressions, residual audits and platform gates; the full Flutter golden exception is reproduced and classified as permitted by the verification policy.
- Final worktree must be clean after the single local implementation commit. No implementation blocker or unavailable required platform gate remains; the known baseline golden failure is explicitly not claimed as a passing full Flutter suite.
- No user data, uncertain external-operation record, provider data, connected-account state, shared credential, signing account or TCC permission state was deleted or changed. Only the exact retired package's inspected generated cache and test-generated golden failures were cleaned.
