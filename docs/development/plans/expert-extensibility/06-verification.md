# 06: final verification and documentation convergence

Prerequisite: checkpoint 05 is complete.

This is the authoritative execution plan for checkpoint 06. It replaces the earlier high-level 06 verification sketch with a latest-main, code-line-level final acceptance plan. Writing or committing this document does **not** execute any gate, does not make checkpoint 06 complete, and does not retire the temporary Expert-extensibility plan bundle.

## Plan authority and baseline

Plan baseline rechecked on 2026-09-27:

- repository: \`syi0808/floe\`
- branch: \`main\`
- inspected HEAD: \`5628495718c8644ff2bb6028eb8ee52b33e4c9c8\`
- checkpoint 05: Complete
- checkpoint 06: Not started
- 05 serial workspace evidence: 1,240 passed, 2 ignored, 0 failed across 93 suites
- 05 Flutter evidence: 368 passed
- proposal-card assertion blob: \`1a58b8188b739278f03eef2e4d6e5a3b233d4b12\`
- proposal-card PNG blob: \`e870ef8b73e0858777d5325e95bc50f63e884405\`

Those counts and blob IDs are **baseline evidence**, not expected values to invent for the 06 run. Re-measure every count on the final 06 HEAD.

Read before execution:

1. \`AGENTS.md\`
2. \`docs/README.md\`
3. \`.agents/skills/architecture-change/SKILL.md\`
4. \`.agents/skills/code-change-verification/SKILL.md\`
5. \`docs/development/plans/expert-extensibility/README.md\`
6. this document
7. \`docs/architecture/README.md\`
8. \`docs/architecture/modules.md\`
9. \`docs/architecture/invariants.md\`
10. \`docs/architecture/runtime.md\`
11. \`docs/architecture/authority-recovery.md\`
12. \`docs/product/intelligence.md\`
13. \`docs/product/integrations-and-privacy.md\`
14. \`docs/product/experience.md\`
15. ADR 0018 and ADR 0030 only for durable rationale comparison
16. \`apps/client/README.md\` for current macOS/product validation commands

Line numbers below are anchors observed at the plan baseline. Re-resolve symbols on the actual implementation HEAD before acting.

## Checkpoint-06 posture

06 is an evidence/convergence checkpoint. The default production-code diff is **zero**.

If static audit, end-to-end acceptance or documentation comparison discovers a real defect:

1. stop claiming verification for that surface;
2. fix the defect at its semantic owner in one bounded 06 corrective commit;
3. run the nearest affected tests;
4. rerun every broad/product gate whose result could have been invalidated by that fix;
5. update current architecture only if the implemented boundary changed.

Do not:

- weaken a safety assertion to get green;
- introduce an old/new compatibility path;
- change timeouts merely to mask parallel flakiness;
- add a new source/provider feature during final verification;
- re-add the temporary E1 ninth Expert;
- run ignored real-EventKit or live-Codex tests without their explicit operator authorization;
- reset a Floe profile, delete a shared Keychain credential, clear a saved server connection or alter an external account to satisfy a test;
- begin a post-plan cleanup before user acceptance.

Authoritative Rust broad validation remains the repository-qualified **serial** run. A separately attempted default-parallel run is extra evidence only.

## Final state checkpoint 06 must establish

| Concern | Final evidence required |
|---|---|
| Expert registration | One generic registration/Directory/Task path for shipped and statically supplied implementations; no App package-ID dispatch. |
| Binding | Per-assignment requirement-key selection remains configuration only; admitted Task selection is immutable and exact. |
| Source read | Manager current-selection and Expert selected-read paths remain distinct; both use current Access authority and canonical provider transport. |
| Model handoff | Access and Expert binding fences run at actual provider handoff; refusal performs zero forbidden transport/fallback. |
| Task completion | New successful terminal writes are transactionally fenced; failure terminalization and historical replay remain valid. |
| Interactions | Reviewed owner state is immutable/CAS-bound; resolution may create a fresh linked Run, never mutate the old Task selection. |
| Extensibility | Permanent non-builtin conformance remains green; supplied extensions receive no trust from package name, Manager grants or another Expert binding. |
| Actions | Only Actions-owned proposal media/evidence can become an external intent; uncertainty is lookup/reconciliation only. |
| Wire/client | Rust DTO -> FFI/protocol -> Dart parser/UI remains same-snapshot and package-generic. |
| macOS product | Same-source FFI/native libraries build, sign and load; safe native/local-model/Keychain gates pass; product Conversation passes when the required Foundation runtime is available. |
| Go source path | Full Go race/vet gate passes and canonical source admit/read/release behavior remains intact. |
| Documentation | Architecture describes current implementation; product describes durable meaning; ADRs retain rationale rather than progress state. |
| Temporary plans | 06 may become Complete, but the plan bundle and \`docs/README.md\` pointer remain until the user explicitly accepts the completed work. |

---

## 06-0: baseline freeze and prerequisites

No production edits belong in 06-0.

### 06-0A: source and worktree baseline

Record:

\`\`\`sh
git status --short --branch
git rev-parse HEAD
git fetch origin main
git rev-parse origin/main
git log -1 --oneline
\`\`\`

Expected plan baseline is \`5628495718c8644ff2bb6028eb8ee52b33e4c9c8\`. If local or origin moved, inspect the diff from this baseline and re-resolve all affected anchors before continuing.

Preserve user work. Do not use \`git reset --hard\`, \`git clean\`, automatic stash or profile/key reset.

### 06-0B: toolchain/prerequisite record

Record actual output, not inferred availability:

\`\`\`sh
uname -m
sw_vers
rustc -V
cargo -V
go version
flutter --version
xcodebuild -version
xcrun swiftc --version
codesign --version
\`\`\`

Classify prerequisites:

- macOS host: required for 06 product acceptance;
- Xcode/Swift/codesign: required;
- Go: required for final server/source gate;
- FoundationModels runtime: required for \`product_conversation_test.dart\`; lack of model/runtime is a concrete blocker unless the user explicitly accepts that exception;
- external Codex OAuth: **not required**; the ignored live-Codex test stays ignored without explicit approval;
- real EventKit response-loss calendar: **not required**; the ignored real Calendar test stays ignored without explicit approval.

### 06-0C: ignored-test inventory

Current known Rust ignored tests are:

- \`crates/app/src/vault_host/tests/native_actions.rs::authorized_eventkit_response_loss_recovers_exact_disposable_event\`
  - requires explicit authorization, copied debug app and real EventKit response-loss shim;
- \`crates/adapters/providers/tests/live_server_access.rs::live_codex_model_uses_canonical_inference_and_exact_recipient\`
  - requires an explicitly approved existing Codex OAuth credential/model.

The final workspace report must list every ignored test actually observed. If the count or names differ from this baseline, classify the change rather than silently carrying the old count forward.

### 06-0D: immutable artifact baseline

Record final-start blobs:

\`\`\`sh
git rev-parse HEAD:apps/client/test/features/actions/agent_proposal_card_test.dart
git rev-parse HEAD:apps/client/test/goldens/agent_proposal_card.png
\`\`\`

At the inspected baseline they are the two blob IDs listed above. Do not enable/delete the commented golden assertion, regenerate the PNG or run an update-goldens command.

---

## 06-A: static architecture, public-surface and artifact audit

Run this before the expensive product gates. A static failure is a real blocker.

### 06-A1: dependency and source-semantic machine gates

Current machine-enforced boundaries:

- \`tools/architecture/check_boundaries.py\` — Cargo/dependency topology;
- \`tools/architecture/check_expert_extensibility.py\` — source-level Expert deletion/generic-runtime invariants.

Run both checkers and their negative-fixture suites:

\`\`\`sh
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_expert_extensibility.py
\`\`\`

The semantic checker currently covers nine rule families, including:

- deleted source-read helpers;
- legacy source forwarders;
- builtin source identity in production App;
- builtin/package-ID dispatch in production App;
- common modules depending on \`floe-experts-builtin\`;
- Tool-shaped Registry;
- removed shared result API;
- removed Calendar wire API;
- package-specific generic Flutter Expert branches.

Do not add a broad allowlist if a new violation appears. Fix or narrowly classify the owner boundary.

### 06-A2: final residual selectors

Re-run 05's deletion selectors on the actual final checkout:

\`\`\`sh
rg -n 'read_communication_view|read_work_context_view|read_calendar_context_view|read_confirmed_interaction_view|read_logistics_view|read_people_view|read_attention_view|read_wellbeing_view|read_personal_view' crates
rg -n 'ExpertBudget|MAX_EXPERT_VIEW_BYTES|PackageKind::Tool|BuiltinSourceRequirement|from_package_id|TestScheduleHost|mod builtin_setup' crates apps/client
rg -n 'BuiltinContextSource::|BuiltinExpertKind::|from_package_id' crates/app/src
rg -n 'ScheduleEndpoint|CalendarExpert|AgentCalendarExpert|experts\.calendar\.install' crates apps/client server
rg -n 'ExpertInput|ExpertInsight|ExpertFocusProposal|StatefulFocusProposal|PackageImplementation|FindFocusWindow' crates apps/client
rg -n 'kind == .tool.|Connected information' apps/client/lib/features/experts apps/client/test/features/experts
rg -n 'e1_temporary_bundled|conformance-tasks' crates apps/client
rg -n '/v1/views/(mail\.communication|work\.context|calendar\.timeline|life\.logistics|relationships\.confirmed_interactions|people\.identity|attention\.coarse|wellbeing\.derived)[\"\x27 ]' crates server
\`\`\`

Classify every residual by meaning:

- package-local \`BuiltinExpertKind\` in \`crates/experts/builtin/**\`: legitimate;
- \`floe_experts_builtin::registrations()/manifests()\` at the App composition/product-trust boundary: legitimate;
- Go unsuffixed View strings that assert legacy-route rejection: legitimate negative protocol coverage;
- historical plan/ADR prose: historical, not runtime;
- any executable compatibility caller or package-ID common dispatch: defect.

Use checkout \`rg\` and current-ref file content as authoritative. GitHub code-search indexing may still return deleted historical blobs.

### 06-A3: public Registry/wire/client shape

Inspect these current anchors together:

- \`crates/contracts/agent/src/expert.rs\`
  - \`PackageKind\` is Expert-only;
- \`crates/bindings/protocol/src/dto/experts.rs:19+\`
  - binding command carries assignment/package/definition/requirement/revision plus opaque candidate IDs;
- \`crates/bindings/ffi/src/app_wire.rs:191+\`
  - FFI converts that DTO mechanically to \`ExpertBindingSelectionIntent\`;
- \`apps/client/lib/features/experts/domain/agent_registry.dart:72+\`
  - client accepts only \`kind == 'expert'\`;
- \`apps/client/lib/app/runtime/local_owner_gateways.dart:675-724\`
  - Flutter sends \`experts.binding.replace\` with opaque candidate IDs, not \`SourceSelectionReference\`;
- \`apps/client/lib/features/experts/presentation/agent_registry_dialog.dart:222-281\`
  - saved unavailable selections remain visible/removable and new choice requires available candidates.

Required conclusions:

- no technical connector/connection/execution-owner/resource reference is client mutation input;
- no Tool Registry fallback exists;
- no package-specific common result parser exists;
- unknown Expert package metadata renders generically.

### 06-A4: Rust-produced delegation fixture -> Dart parser

The tracked product-boundary fixture is:

- producer/authority test: \`crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs:301+\`
  - \`schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency\`;
- bytes: \`fixtures/expert-report/delegation-v1.json\`;
- consumer: \`apps/client/test/features/conversation/agent_delegation_fixture_test.dart\`.

Run:

\`\`\`sh
cargo test -p floe-app --lib schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency -- --test-threads=1
cd apps/client
flutter test test/features/conversation/agent_delegation_fixture_test.dart
cd ../..
git diff -- fixtures/expert-report/delegation-v1.json
\`\`\`

The fixture must remain Rust-produced. Do not hand-edit it to satisfy Dart.

### 06-A5: proposal-card exception and temporary E1 cleanup

Verify:

\`\`\`sh
git diff --exit-code -- \
  apps/client/test/features/actions/agent_proposal_card_test.dart \
  apps/client/test/goldens/agent_proposal_card.png

rg -n 'e1_temporary_bundled|conformance-tasks' crates apps/client
\`\`\`

The temporary ninth Expert was a 05 experiment and must remain absent. Do not re-add it just to repeat E1 in 06. Permanent E2 plus machine enforcement are the repeatable conformance evidence; the recorded 05 E1 diff remains historical acceptance evidence.

---

## 06-B: executable owner/end-to-end matrix

Run focused evidence first. The commands below name existing tests on the plan baseline; if a test is renamed on a newer HEAD, resolve the actual containing test and record the replacement. Rust library/unit commands intentionally use a unique substring filter rather than harness `--exact`, because lib tests are module-qualified. Confirm the harness reports the intended non-zero test count; a zero-test filter is not evidence.

### 06-B1: source target isolation and product policy

Context:

\`\`\`sh
cargo test -p floe-context same_connection_mail_and_logistics_grants_are_not_duplicate_authority -- --nocapture
cargo test -p floe-context work_view_ignores_unrelated_mail_grants -- --nocapture
cargo test -p floe-context selected_a_ignores_unselected_b_even_if_b_is_blocked -- --nocapture
cargo test -p floe-context selected_missing_a_does_not_adopt_live_b -- --nocapture
cargo test -p floe-context admitted_sources_keep_their_own_bindings -- --nocapture
cargo test -p floe-context --test native_calendar_read selected_native_calendar_subset_survives_read_and_reauthorization -- --nocapture
\`\`\`

App:

\`\`\`sh
cargo test -p floe-app --lib manager_mail_read_requires_assistant_in_reviewed_product_policy -- --test-threads=1
cargo test -p floe-app --lib registered_runner_nonbuiltin_extension_chain_reads_only_its_exact_selection -- --test-threads=1
cargo test -p floe-app --lib registered_runner_extension_cannot_read_another_experts_selection -- --test-threads=1
cargo test -p floe-app --lib registered_runner_builtin_prefix_does_not_grant_first_party_observe -- --test-threads=1
cargo test -p floe-app --lib registered_runner_extension_does_not_change_first_party_observe_policy -- --test-threads=1
cargo test -p floe-app --lib hosted_calendar_settings_use_product_connection_and_pinned_producer -- --test-threads=1
cargo test -p floe-app --lib expert_settings_resolve_only_current_candidate_ids_and_rejoin_exact_save -- --test-threads=1
\`\`\`

Acceptance:

- Gmail mail/logistics grants on one connection do not become duplicate authority;
- unrelated Views/grants do not become candidates;
- selected A never widens to B;
- native Calendar keeps exact subset through dependency reauthorization;
- Manager \`assistant\` grant does not authorize a supplied Expert;
- supplied/builtin-looking extensions receive no default first-party trust;
- source settings use opaque current candidates and exact CAS.

### 06-B2: binding/model/terminal race fences and replay

Run:

\`\`\`sh
cargo test -p floe-inference execution_fence_rejects_handoff_without_transport_or_fallback -- --nocapture

cargo test -p floe-app --lib read_a_then_rebind_b_fences_expert_model_dispatch -- --test-threads=1
cargo test -p floe-app --lib rebound_selection_discards_runner_result_before_final_release -- --test-threads=1
cargo test -p floe-app --lib completed_task_replays_historical_result_after_rebinding_and_disable -- --test-threads=1
cargo test -p floe-app --lib binding_operation_rejoins_exactly_and_stale_task_admission_never_reroutes -- --test-threads=1
cargo test -p floe-app --lib stateless_completed_cas_rejects_rebound_selection_but_records_failure -- --test-threads=1
cargo test -p floe-app --lib settlement_rolls_back_registry_and_task_on_failure_and_stale_cas -- --test-threads=1
\`\`\`

Acceptance:

- rebind after outer validation but before provider handoff produces zero provider payload and no fallback;
- rebind after runner result blocks usable release/completion;
- new Completed CAS is transactionally fenced;
- Failed terminalization remains recordable;
- historical Completed replay does not execute again;
- exact operation retry rejoins instead of double-mutating.

### 06-B3: reviewed authority, crash/rejoin and linked resume

Run:

\`\`\`sh
cargo test -p floe-app --lib superseded_expert_selection_cannot_enable_reviewed_source -- --test-threads=1
cargo test -p floe-app --lib gmail_views_allow_enables_bundle_atomically_and_resolves -- --test-threads=1
cargo test -p floe-app --lib gmail_commit_then_crash_reopens_and_resolves_without_second_mutation -- --test-threads=1
cargo test -p floe-app --lib nonbuiltin_model_consent_resolves_into_fresh_linked_task -- --test-threads=1
cargo test -p floe-app --lib revoked_consent_blocks_child_fresh_without_stale_release -- --test-threads=1
cargo test -p floe-conversation linked_resume_runs_original_intent_with_marker_and_no_user_restatement -- --nocapture
cargo test -p floe-vault resume_slot_survives_vault_reopen_and_rejoins -- --nocapture
\`\`\`

If the final \`floe-vault\` filter does not match because that test is exposed only through the repository integration target, run its actual containing test target and report the exact command. Zero-test filters are not evidence.

Acceptance:

- old source-selection review cannot mutate after rebind;
- reviewed bundle enable is atomic;
- crash after owner mutation rejoins rather than mutates twice;
- recipient consent is exact and time/lineage scoped;
- linked child is a fresh Run/Task with fresh admission and no duplicate user text;
- revoked consent does not leak through stale release;
- resume slot survives restart/rejoin.

### 06-B4: Actions provenance and uncertain effects

Run:

\`\`\`sh
cargo test -p floe-app --lib schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency -- --test-threads=1
cargo test -p floe-app --lib proposal_publication_rejects_forged_and_ambiguous_artifacts -- --test-threads=1
cargo test -p floe-app --lib governed_action_owner_approval_dispatch_and_recovery_are_durable -- --test-threads=1
cargo test -p floe-app --lib rebinding_after_proposal_fences_new_dispatch_without_erasing_intent -- --test-threads=1
cargo test -p floe-app --lib cancellation_after_publication_reports_uncertainty_without_replacing_the_intent -- --test-threads=1
cargo test -p floe-app --lib old_calendar_receipt_cannot_be_published_against_a_new_connection_revision -- --test-threads=1
\`\`\`

Also run the Tier-B native response-loss test named by \`tools/s3-validation/README.md\`:

\`\`\`sh
cargo test -p floe-app --lib native_executor_uses_rust_ledger_and_lookup_only_after_response_loss -- --test-threads=1
\`\`\`

Acceptance:

- package-looking JSON is inert unless Actions recognizes its own exact media type;
- proposal contributor/Task/selection evidence is exact;
- read permission never becomes Act permission;
- durable intent survives later binding change;
- response loss/uncertainty uses lookup/reconciliation, never blind retry.

### 06-B5: generic client and wire behavior

Run:

\`\`\`sh
cargo test -p floe-protocol
cargo test -p floe-ffi

cd apps/client
flutter test test/features/experts
flutter test test/features/conversation/agent_delegation_fixture_test.dart
flutter test test/features/conversation/agent_interaction_test.dart
flutter test test/features/actions
cd ../..
\`\`\`

Required named client regressions include:

- \`agent_registry_dialog_test.dart:115\` unknown Expert renders manifest metadata without a UI branch;
- \`agent_registry_dialog_test.dart:169\` saved unavailable source can be removed when discovery fails;
- \`agent_interaction_test.dart:152\` linked child parsing;
- \`agent_interaction_test.dart:213+\` reviewed decision sends only bound identity;
- \`agent_interaction_test.dart:240+\` resume omits caller text;
- \`agent_interaction_test.dart:291+\` identical decision rejoin after transport loss;
- generic Actions UI/attribution tests.

No client test may require a source-reference technical field or package-specific Expert parser.

---

## 06-C: broad, Go and macOS product acceptance on the final code HEAD

Do not start 06-C until any 06-A/B corrective code commits are complete.

If any code or test changes after one of these broad gates, rerun the affected broad gate and report the final run only as acceptance.

### 06-C1: authoritative Rust workspace gate

From repository root:

\`\`\`sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_expert_extensibility.py
git diff --check
\`\`\`

Record:

- source HEAD;
- number of suites/tests;
- pass/fail;
- ignored tests by name;
- whether a default-parallel run was attempted separately.

Do not describe the serial pass as default-parallel reliability.

### 06-C2: final Go server/source gate

The Expert work changed the client-side source protocol consumers even though 05 did not require a Go production change. 06 runs the full Go gate:

\`\`\`sh
cd server
go test -race ./...
go vet ./...
cd ..
\`\`\`

This is also the final check that canonical admit/read/release, pairing, provider identity and legacy unsuffixed-route rejection still agree.

No live external provider account is required for this gate.

### 06-C3: FFI + Flutter broad gate

Build the debug FFI before native Flutter integration so the dylib and Dart code come from the same HEAD:

\`\`\`sh
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test
flutter build macos
cd ../..
\`\`\`

Record actual Flutter test count. Do not copy the 05 value of 368 unless observed again.

### 06-C4: safe native/macOS automated gates

These commands do not request personal Calendar access or external-account mutation:

\`\`\`sh
tools/s3-validation/check-native.sh
tools/validation/check-local-model.sh
tools/validation/run-local-model-smoke.sh --availability
tools/validation/run-vault-keyring-smoke.sh --exercise
\`\`\`

Semantics:

- \`check-native.sh\` runs pure EventKit payload/conflict/default-gate assertions without reading Calendar;
- \`check-local-model.sh\` compiles/tests the Swift host plus provider-adapter/Inference owners;
- \`--availability\` validates the signed local-model smoke bundle/transport availability path;
- Vault Keychain \`--exercise\` creates only a disposable private temp profile/random key, verifies encrypted create/reopen/key-loss fail-closed, deletes that exact temporary key and directory, and touches no shared Floe credential.

If Vault smoke reports retained disposable artifacts after a cleanup failure, use only the script's exact \`--cleanup <retained-root> <Person UUID>\` path after inspecting that it is the validation-owned private root. Never delete an unrelated Keychain item.

### 06-C5: disposable local-server integration

Run the non-external-account server integrations:

\`\`\`sh
cargo test -p floe-provider-adapters --test live_server_access \
  live_pairing_current_connection_access_is_denied_after_server_revocation \
  -- --exact --nocapture

cd apps/client
flutter test integration/local_server_pairing_test.dart
cd ../..
\`\`\`

These tests use disposable loopback servers and test-owned/memory credentials. They must not read/write the shared saved-server Keychain slot.

### 06-C6: product Conversation macOS acceptance

Immediately after building the same-source debug FFI:

\`\`\`sh
cargo build -p floe-ffi
cd apps/client
flutter test integration/product_conversation_test.dart
cd ../..
\`\`\`

This is the required macOS product Conversation gate when FoundationModels is available. It builds a private signed Flutter test host with the same-source Foundation dylib, exercises Auto and explicit \`foundation-device\` turns plus durable reopen, then removes only its exact validation-owned Vault key/profile.

Environment rule:

- a conflicting shared saved-server credential is a fail-closed blocker; do not clear it;
- missing/unsupported FoundationModels runtime is \`NOT_RUN/UNVERIFIED\`, not pass;
- 06 remains incomplete on this required product gate unless the user explicitly accepts the recorded environment exception.

### 06-C7: final built bundle ABI/sign/load audit

After \`flutter build macos\`, verify the release app from the actual Flutter output path (normally \`apps/client/build/macos/Build/Products/Release/floe_client.app\`).

At minimum:

\`\`\`sh
APP="apps/client/build/macos/Build/Products/Release/floe_client.app"
test -d "$APP"
codesign --verify --deep --strict "$APP"

for dylib in \
  libfloe_ffi.dylib \
  libfloe_eventkit.dylib \
  libfloe_local_model.dylib
do
  test -f "$APP/Contents/Frameworks/$dylib"
  codesign --verify --strict "$APP/Contents/Frameworks/$dylib"
  otool -D "$APP/Contents/Frameworks/$dylib"
done
\`\`\`

For \`libfloe_ffi.dylib\`, confirm the current header's local C ABI exports are present:

- \`floe_core_open\`
- \`floe_core_command_v2\`
- \`floe_core_query_v2\`
- \`floe_core_events_v2\`
- \`floe_core_remote_pairing_v2\`
- \`floe_core_remote_access_v2\`
- \`floe_protocol_version\`
- \`floe_string_free\`
- \`floe_core_free\`

Use \`nm -gU\` or the platform-equivalent export inspection. Also classify any obsolete owner-specific FFI export if found; do not add an alias to retain it.

### 06-C8: explicitly excluded opt-in validation

Do not run these without new explicit user/operator authorization:

- real EventKit response-loss:
  \`authorized_eventkit_response_loss_recovers_exact_disposable_event\`;
- live Codex OAuth:
  \`live_codex_model_uses_canonical_inference_and_exact_recipient\`.

Report them as \`NOT_RUN\` with their prerequisites. The automated Tier-B native uncertainty test and disposable local-server access test are the required non-external substitutes.

iOS device/simulator remains deferred to active iOS implementation work. Android remains out of scope.

---

## 06-D: documentation and rationale convergence

Documentation work happens **after** the final implementation/verification HEAD is known.

### 06-D1: current architecture

Inspect, against source rather than checkpoint prose:

- \`docs/architecture/modules.md\`
  - Experts owns Registry/assignment/Task;
  - Context owns source acquisition/provenance;
  - Access owns source/model recipient authority;
  - App is composition/product policy only;
- \`docs/architecture/runtime.md\`
  - registration -> Directory -> exact Task admission;
  - requirement key -> exact selected refs;
  - canonical authorized source transport;
  - provider-handoff binding fence;
  - transaction-fenced successful Task completion;
  - durable interaction/fresh linked resume;
- \`docs/architecture/authority-recovery.md\`
  - binding is not grant;
  - Manager and Expert actual consumers are checked separately;
  - arbitrary supplied extensions get no automatic first-party consumer;
  - proposal/read permission is not Act authority;
- \`docs/architecture/invariants.md\`
  - dependency and Expert semantic checkers are listed as machine enforcement;
- \`docs/architecture/README.md\`
  - both architecture checkers remain discoverable.

Update only stale facts. Do not copy checkpoint test counts or commit SHAs into architecture docs.

### 06-D2: product meaning

Compare current behavior to:

- \`docs/product/intelligence.md\`
  - Experts are A2A domain judgment agents;
  - generic source settings;
  - source selection, source approval, model-recipient approval and Action authority stay distinct;
  - third-party/supplied Experts are distinct principals;
- \`docs/product/integrations-and-privacy.md\`
  - active trusted first-party Experts may join reviewed product Observe only for exact selected sources;
  - third-party Experts never enter the default first-party reader set;
- \`docs/product/experience.md\`
  - blocked work yields durable review request;
  - owner resolution may start one fresh linked Run;
  - review is not an Action proposal.

These documents are already broadly aligned at the plan baseline. Do not add implementation-progress prose. Edit only if final source contradicts durable product meaning.

### 06-D3: ADR review

Inspect:

- ADR 0018 — Manager–Expert A2A delegation;
- ADR 0030 — Conversation-owned durable interactions and origin-linked resume.

Expected baseline conclusion: their durable rationale is still valid.

Do **not** rewrite ADR 0018's historical “Initial migration” section merely because the migration is now complete; ADRs preserve decision history. Do not add 06 status/counts to either ADR.

Amend/supersede/add an ADR only if a 06 corrective change actually changes durable rationale.

### 06-D4: runbook and documentation checks

Ensure current commands remain accurate in:

- \`apps/client/README.md\`;
- root \`README.md\`;
- relevant server/validation READMEs.

Run:

\`\`\`sh
python3 tools/docs/check_docs.py
git diff --check
\`\`\`

The docs checker validates active links, ADR index coverage, retired roots/references and retired architecture code paths. Manually inspect semantic accuracy as well; link success alone is not convergence.

### 06-D5: checkpoint status and retirement handoff

After every required 06 gate is green on the final HEAD:

1. append actual verification evidence to this document;
2. update \`docs/development/plans/expert-extensibility/README.md\`:
   - 06 -> Complete;
   - record final commit(s), exact broad/product evidence and unavailable opt-in surfaces;
3. keep the plan bundle in place for the completion report/user review.

**Do not retire the plan during checkpoint 06.**

The active documentation router currently contains the temporary pointer at:

- \`docs/README.md:52-58\`.

Only **after the user explicitly accepts the completed checkpoint 06** should a separate bounded documentation cleanup:

- remove \`docs/development/plans/expert-extensibility/\` from the active tree;
- remove the task-specific pointer from \`docs/README.md\`;
- run \`python3 tools/docs/check_docs.py\` and \`git diff --check\`;
- leave architecture/product/ADR/runbook documents as the durable context;
- rely on Git history as the archive.

That post-acceptance cleanup is not checkpoint-06 implementation and must not be performed speculatively by the coding agent.

---

## 06-E: final completion gate

Checkpoint 06 may be marked Complete only when all of the following hold on one final source HEAD:

1. 06-A machine checks and residual audit are clean/classified.
2. Rust-produced delegation fixture and Dart parser agree byte-for-byte.
3. proposal-card assertion and PNG are unchanged from 06 start.
4. all permanent E2/non-builtin isolation tests pass; temporary E1 code remains absent.
5. source isolation/native subset/product consumer tests pass.
6. model handoff, Task terminal, replay and CAS race tests pass.
7. review drift/crash/rejoin/linked-resume tests pass.
8. Actions proposal/provenance/uncertainty tests pass.
9. protocol/FFI/Flutter targeted tests pass.
10. authoritative serial Rust workspace gate passes with ignored tests explicitly named.
11. both architecture checkers and both checker fixture suites pass.
12. full Go \`-race\` and \`go vet\` pass.
13. Flutter analyze/full test/macOS build pass.
14. safe native/local-model/Keychain/disposable local-server gates pass.
15. product Conversation macOS integration passes, or the user explicitly accepts its concrete environment exception.
16. built macOS app and three bundled dylibs pass sign/install-name/export inspection.
17. docs checker and semantic documentation review pass.
18. current architecture/product/ADR/runbook documents converge with the final code.
19. 06 completion evidence is recorded and plan README says Complete.
20. no user data/profile/shared credential/external account was reset or changed merely for validation.
21. final worktree is clean.
22. no push occurs unless explicitly authorized.
23. plan retirement is left for post-acceptance cleanup.

A failure in a required gate is a blocker, not a documentation exception. The only exception mechanism is explicit user acceptance of a specifically recorded unavailable environment prerequisite.

## Recommended commit strategy

06 should normally have very few commits:

1. **06-A/B corrective commit(s), only if necessary** — bounded owner fix discovered by final audit/acceptance, with targeted evidence.
2. **06-D documentation convergence** — only actual current architecture/product/runbook corrections.
3. **06-E completion evidence/status** — append final verification evidence and mark 06 Complete.

Do not create a commit merely to record that a command was about to run. Do not split a corrective change by adding compatibility wrappers.

The post-user-acceptance plan-retirement cleanup is a separate later documentation change, not one of these commits.

## Checkpoint 06 execution evidence (2026-09-27)

### Baseline and audit

- Clean `main` start HEAD and fetched `origin/main`: `7a596363b1a4dd6e38bc1ce6f4b38f3724994392`. No production-code correction was needed. Host: arm64 macOS 26.5.2; Rust/Cargo 1.93.1; Go 1.25.5; Flutter 3.47.2/Dart 3.13.2; Xcode 26.2/Swift 6.2.3. `codesign --version` is unsupported; signature verification passed below.
- `python3 tools/architecture/check_boundaries.py` passed (22 nodes, 102 edges, zero errors/warnings); `test_check_boundaries.py` passed 10/10. `check_expert_extensibility.py` passed; `test_check_expert_extensibility.py` passed 7/7. All 06-A2 residual selectors ran: no executable old source helper, Tool Registry, package-ID dispatch, temporary E1 or obsolete result/Calendar path remains. App `BuiltinExpertKind` matches are test-only identities; `experts.calendar.install` and unsuffixed Go View paths are negative rejection fixtures. The remaining App unsuffixed View string is a test fixture.
- Public-shape inspection found Expert-only `PackageKind`, opaque candidate IDs through Dart/DTO/FFI, generic unknown Expert rendering, no Tool fallback and no client `SourceSelectionReference` mutation input. The Rust-produced delegation fixture test passed 1/1 and its Dart consumer passed 1/1; `fixtures/expert-report/delegation-v1.json` did not change. Proposal-card assertion/PNG blobs remained `1a58b8188b739278f03eef2e4d6e5a3b233d4b12` / `e870ef8b73e0858777d5325e95bc50f63e884405`.

### Targeted matrix

Every named filtered Rust command below ran **one test passed, zero failed**; library filters did not use `--exact`.

| Command form | Filters actually run |
|---|---|
| `cargo test -p floe-context <filter> -- --nocapture` | `same_connection_mail_and_logistics_grants_are_not_duplicate_authority`; `work_view_ignores_unrelated_mail_grants`; `selected_a_ignores_unselected_b_even_if_b_is_blocked`; `selected_missing_a_does_not_adopt_live_b`; `admitted_sources_keep_their_own_bindings` |
| `cargo test -p floe-context --test native_calendar_read <filter> -- --nocapture` | `selected_native_calendar_subset_survives_read_and_reauthorization` |
| `cargo test -p floe-inference <filter> -- --nocapture` | `execution_fence_rejects_handoff_without_transport_or_fallback` |
| `cargo test -p floe-conversation <filter> -- --nocapture` | `linked_resume_runs_original_intent_with_marker_and_no_user_restatement` |
| `cargo test -p floe-vault <filter> -- --nocapture` | `resume_slot_survives_vault_reopen_and_rejoins` (lib target) |
| `cargo test -p floe-app --lib <filter> -- --test-threads=1` | `manager_mail_read_requires_assistant_in_reviewed_product_policy`; `registered_runner_nonbuiltin_extension_chain_reads_only_its_exact_selection`; `registered_runner_extension_cannot_read_another_experts_selection`; `registered_runner_builtin_prefix_does_not_grant_first_party_observe`; `registered_runner_extension_does_not_change_first_party_observe_policy`; `hosted_calendar_settings_use_product_connection_and_pinned_producer`; `expert_settings_resolve_only_current_candidate_ids_and_rejoin_exact_save`; `read_a_then_rebind_b_fences_expert_model_dispatch`; `rebound_selection_discards_runner_result_before_final_release`; `completed_task_replays_historical_result_after_rebinding_and_disable`; `binding_operation_rejoins_exactly_and_stale_task_admission_never_reroutes`; `stateless_completed_cas_rejects_rebound_selection_but_records_failure`; `settlement_rolls_back_registry_and_task_on_failure_and_stale_cas`; `superseded_expert_selection_cannot_enable_reviewed_source`; `gmail_views_allow_enables_bundle_atomically_and_resolves`; `gmail_commit_then_crash_reopens_and_resolves_without_second_mutation`; `nonbuiltin_model_consent_resolves_into_fresh_linked_task`; `revoked_consent_blocks_child_fresh_without_stale_release`; `schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency`; `proposal_publication_rejects_forged_and_ambiguous_artifacts`; `governed_action_owner_approval_dispatch_and_recovery_are_durable`; `rebinding_after_proposal_fences_new_dispatch_without_erasing_intent`; `cancellation_after_publication_reports_uncertainty_without_replacing_the_intent`; `old_calendar_receipt_cannot_be_published_against_a_new_connection_revision`; `native_executor_uses_rust_ledger_and_lookup_only_after_response_loss` |

`cargo test -p floe-protocol` passed 32 tests/six suites and `cargo test -p floe-ffi` passed 26/three suites. Flutter targeted commands from `apps/client`: `flutter test test/features/experts` 20 passed; `flutter test test/features/conversation/agent_delegation_fixture_test.dart` 1 passed; `flutter test test/features/conversation/agent_interaction_test.dart` 13 passed; `flutter test test/features/actions` 42 passed. These suites include the named unknown-Expert, unavailable-selection, linked-child, decision/rejoin and generic Actions regressions.

### Broad and product gates

- On source HEAD `7a596363`, `cargo check --workspace --lib` passed with two existing App dead-code warnings. `CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast` passed **93 suites, 1,240 passed, zero failed, two ignored**. Ignored: `vault_host::tests::native_actions::authorized_eventkit_response_loss_recovers_exact_disposable_event` and `live_codex_model_uses_canonical_inference_and_exact_recipient`. No default-parallel workspace run was attempted or claimed. Both architecture checkers and both negative-fixture suites passed on that same source HEAD.
- From `server/`, `go test -race ./...` and `go vet ./...` passed. `cargo build -p floe-ffi`, `flutter analyze`, `flutter test` (**368 passed**) and `flutter build macos` passed. `bash tools/s3-validation/check-native.sh` passed 32 native assertions; `bash tools/validation/check-local-model.sh` passed the Swift host plus Rust suites (132 passed, one ignored live-Codex test); `bash tools/validation/run-local-model-smoke.sh --availability` reported Foundation `Available`; `bash tools/validation/run-vault-keyring-smoke.sh --exercise` passed all private-key/profile checks. Direct script invocation returned permission denied (exit 126), so these scripts were run with `bash` without changing their file modes.
- `cargo test -p floe-provider-adapters --test live_server_access live_pairing_current_connection_access_is_denied_after_server_revocation -- --exact --nocapture` passed 1/1; `flutter test integration/local_server_pairing_test.dart` passed 1/1 with exact private key/profile cleanup.
- The first two product Conversation attempts timed out during session start while a pre-existing shared saved-server Keychain slot existed. The user then explicitly requested server-Keychain deletion; only Floe's exact `app.floe.local-server` / `connection-v1` slot was deleted and its absence verified (`security find-generic-password` exit 44). The first post-deletion attempt failed transiently at Run read after a `VaultUnavailable` turn failure. A diagnostic rerun and the final plain `flutter test integration/product_conversation_test.dart` both passed 1/1: Auto and explicit `foundation-device` generated one-attempt/no-Task turns, durable reopen passed, and exact validation-owned key/profile cleanup completed. No other Keychain item, personal Calendar, source profile or external account was changed. Ordinary saved-server use requires re-pairing.
- The release `apps/client/build/macos/Build/Products/Release/floe_client.app` passed deep strict codesign verification; bundled FFI, EventKit and local-model dylibs passed strict signature verification and had matching `@rpath/<name>` install names. `nm -gU` found all nine required exports: `floe_core_open`, `floe_core_command_v2`, `floe_core_query_v2`, `floe_core_events_v2`, `floe_core_remote_pairing_v2`, `floe_core_remote_access_v2`, `floe_protocol_version`, `floe_string_free`, `floe_core_free`. No obsolete owner-specific export was found.

### Documentation and exclusions

Architecture, product, ADR 0018/0030 and root/client/server/validation runbooks were compared to source and remain semantically current; no durable rationale or production boundary changed. `python3 tools/docs/check_docs.py` passed 114 Markdown files (a Python regex `FutureWarning` only), and `git diff --check` passed. Real EventKit response-loss and live Codex OAuth tests were **NOT_RUN** without operator approval; iOS device/simulator and Android were **NOT_RUN** by scope. No push occurred. The plan bundle and `docs/README.md` task pointer remain pending explicit post-checkpoint acceptance.

## Required agent completion report

Report:

1. start local HEAD, fetched origin/main and worktree state;
2. final source HEAD and every 06 corrective/documentation/status commit SHA;
3. toolchain/macOS/Go/Flutter/Xcode versions;
4. final residual-search classifications;
5. dependency checker + fixture results;
6. Expert semantic checker + fixture results;
7. public Registry/wire/client audit result;
8. Rust-produced delegation fixture -> Dart parser result;
9. proposal-card blob comparison;
10. exact 06-B source isolation test commands/results;
11. exact race/replay/terminal-fence test commands/results;
12. exact review/crash/rejoin/linked-resume test commands/results;
13. exact Actions/provenance/uncertainty test commands/results;
14. protocol/FFI/Flutter targeted commands/results;
15. serial workspace command, suite/test/ignored counts and ignored names;
16. any optional parallel run, reported separately;
17. full Go \`go test -race ./...\` and \`go vet ./...\` results;
18. Flutter analyze/full test/macOS build result and count;
19. \`check-native.sh\`, local-model validation/smoke and Vault Keychain smoke results;
20. disposable live-server Rust + Flutter pairing integration results;
21. product Conversation integration result or exact unavailable prerequisite plus explicit acceptance status;
22. built app/dylib codesign, install-name and C ABI export audit;
23. iOS/Android/live-Codex/real-EventKit NOT_RUN classification;
24. architecture/product/ADR/runbook docs changed or explicitly confirmed current;
25. docs checker and diff check results;
26. 06 status in the plan README;
27. final worktree state;
28. whether anything was pushed;
29. confirmation that the plan bundle and \`docs/README.md\` pointer remain pending explicit user acceptance.
