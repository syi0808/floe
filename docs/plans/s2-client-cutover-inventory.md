Historical S2 task handoff/inventory, retained for provenance. Its implementation is complete; current source and the central execution status govern remaining work. Do not restart the handoff from this file.

# S2 client caller cutover inventory

Read-only source inventory, 2026-10-03, after G1 passed. The canonical contract and owner-specific S2 decisions govern implementation. No client source changes, tests, formatter, compiler, build, checker or live operation were performed for this inventory. The following packages must migrate with their matching owner/binding contracts in the same source snapshot.

## 1. Actions product contract and UI

- `apps/client/lib/features/actions/application/calendar_action_controller.dart`: `canApprove` reconstructs source/freshness/expiry policy; `decide`, `direct`, `propose` and `run` chain approval, execution and collection; `canModify` parses EventKit/provider identifiers; `_collect` and `retryRead` own ephemeral collection status. Replace these with explicit owner intents, stable command replay, pure observation and owner-produced allowed actions.
- `features/actions/application/calendar_action_gateway.dart`, `infrastructure/native_calendar_action_gateway.dart`: expose separate direct/propose/execute/recover operations, global writes-enabled and `actions.read_result` worker polling. Replace with canonical submit/decide/reconcile/inspect/list/authority methods and typed results. Delete forwarding-only `application/calendar_action_facade.dart`.
- `features/actions/domain/calendar_action.dart`: raw provider/calendar/external identifiers and `mutation.original` leak into product policy. Current status lacks Cancelled/Expired/Failed and owner allowed actions. Replace with safe Action summary, immutable review reference, revision, outcome and recovery/collection state.
- `features/actions/infrastructure/app_wire_proposal_gateway.dart`, `domain/agent_proposal.dart`, `presentation/agent_proposal_card.dart`: proposal inspection still uses the worker envelope and assumes action ID equals invocation ID. Migrate against the exact new owner proposal reference/result.
- `features/actions/presentation/calendar_action_panel.dart`, `calendar_action_proposal.dart`, `features/settings/presentation/action_permissions.dart`, Day event controls: render execute-after-approval/retry-read and provider-derived affordances. Render the owner state/actions instead. Unknown remains inconclusive; timeout cannot authorize another write. Calendar Create standing authority remains an explicit revision-checked setting, not authority for every supported action.

Freeze first: exact safe action/operation/list/authority/proposal DTOs and wrappers; review reference; command/revision replay; collection and Unknown safe actions; opaque target selection for Create and Day event references for Update/Delete. Main currently leaves Actions uncomposed. Do not re-enable it until this whole path is replaced.

## 2. Native EventKit effect admission

- `apps/client/macos/CalendarActions/EventKitActions.swift:6,78` retains fixed development Person. `runAction` accepts raw action/state/approved_at JSON at lines 393–438; the C ABI returns only data/error at 481–493. There is no exact outstanding effect-admission registry or physical outcome readback.
- Lookup rejects Delete and accepts a matching Update postcondition. Freeze conservative causal evidence before replacing this logic: normal acknowledged Update/Delete remain supported; absence or matching later state cannot fabricate authoritative outcome.
- Matching Rust callers are `crates/adapters/providers/src/sources/native_calendar.rs:465–610` and `crates/app/src/action_facade.rs`. The provider's 15-second timeout leaves the native thread running. Swift and Rust must cut over together to exact admitted Person/device/executor/effect identity and Committed/NotApplied/Unknown evidence.
- Preserve permission, source fingerprint, revision, operation marker, expiry and prewrite checks. Track the actual save/remove boundary and late completion; readback/lookup cannot dispatch. Frozen contract must specify cache retention/acknowledgement, missing-cache semantics, cancellation, host closure and live invocation quiescence.
- Current `libfloe_eventkit.dylib` is bundled by `macos/build_native.sh`/macOS Xcode inputs. `ios/build_native.sh` currently bundles the model/Health dylib only. Do not imply iOS effect support without an explicit owner/platform implementation decision.

## 3. Day multisource display and native acquisition

- `features/day/domain/day_models.dart` and `infrastructure/app_wire_day_gateway.dart`: single-source Calendar mirror and raw calendar/external/provider fields remain. Replace with safe multisource coverage/display references and opaque Day event/action targets. External revisions stay private.
- `features/day/presentation/personal_day_screen.dart:93–109` selects one calendar source; `_collectAction` at 182–232 performs UI-owned refresh/source matching/external-ID collection verification. Remove that workflow after Actions owns durable collection.
- `features/connections/application/calendar_connection_view.dart` composes raw source authority with mirror status; `calendar_source_gateway.dart` is an explicitly frozen, uncomposed S2 dependency. Remove both obsolete product-authority routes after safe Day projection is wired.
- `features/day/application/personal_day_controller.dart`, `day_gateway.dart`, and the AppWire Day adapter already prepare stable refresh-command identity and pure operation observation. `day.refresh`/`day.refresh.get` remains uncomposed in main. Wire only when the durable Day owner is available; observer timeout/disposal must not cancel or replay acquisition.
- Native same-snapshot files: `ios/Runner/CalendarChannel.swift`, `macos/Runner/MainFlutterWindow.swift`, `lib/infrastructure/native/calendar_acquisition_broker.dart`, `eventkit_calendar_host.dart`, `native_context_host_transport.dart`, and Rust native/protocol DTOs. Both native acquisition implementations currently emit a string external_revision. Tagged external revision and any product-read correlation/limit additions must change together. Keep exact outstanding request/registration binding and before/after subject fences.

Freeze first: safe multisource Day snapshot, opaque creation/event targets, coverage/failure enums, native product-read request/response fields and bounds. Prepared refresh state union is pending/running/completed/failed/interrupted; it must remain owner-produced. Product refresh is not Observe permission.

## 4. Experts atomic configuration and binding review

- `features/experts/application/agent_registry_controller.dart:159–204` fans one capability toggle into installation/assignment writes and predicts revision increments. Replace with one owner command.
- `features/experts/infrastructure/app_wire_registry_gateway.dart` sends package/version/definition expectations in `experts.binding.replace`, uses unreviewed candidate lookup and polls `experts.read_result`. Replace with directory/set-installation/prepare-review/inspect-review/replace-binding owner methods.
- `features/experts/domain/agent_registry.dart` treats candidate IDs as 64-character hashes; the accepted review-bound opaque candidate references require a same-snapshot decoder replacement.
- `features/experts/presentation/agent_registry_dialog.dart` reconstructs expectations and can remove a selection without a current catalog. Migrate selection to an exact persisted BindingReview, including unavailable selected candidates, expiry and owner actions.
- `features/conversation/domain/agent_interaction.dart:87–136` currently carries assignment/package/version/requirement/capability without BindingReviewRef. Change this with Experts/Conversation review publication, resolution and Settings navigation.
- `features/conversation/domain/agent_session.dart:198–249` currently rejects Task state blocked. The proposed S2 trusted Task blockage must update the safe Task summary decoder with the owner projection, without exposing journal/coverage/authority evidence.

Freeze first: exact Directory and BindingReview DTOs/wrappers/allowed actions, reference types, interaction target/result shape and safe Blocked Task summary. Binding selection must never create Access permission.

## 5. Knowledge and final composition

- Knowledge domain/interface extraction is already complete in `features/knowledge/domain/memory_review.dart` and `application/memory_gateway.dart`.
- `features/knowledge/infrastructure/app_wire_memory_gateway.dart` still uses knowledge_operation/knowledge.read_result. Candidate decisions currently carry candidate ID and decision. Preserve Knowledge-owned semantics and migrate only after direct typed result/replay and any owner-required review/CAS fields are frozen.
- `features/knowledge/application/agent_memory_controller.dart` owns presentation read/review state; it does not run the Learner or activate memory locally. Keep that distinction.
- Settings `agent_memory_settings.dart`, `agent_memory_review_settings.dart`, `data_privacy.dart` and Experts Settings still take ConversationController. `features/conversation/application/conversation_controller.dart` constructs and forwards Experts/Knowledge controllers, proposal state and shared busy gating. Migrate to explicit feature controllers and Vault lifecycle dependencies without making query/view disposal cancel owner work.
- `app/runtime/app_runtime.dart`, `local_owner_gateways_scope.dart`, `app/floe_app.dart`, `main.dart`, and preview composition must inject the final explicit owner bundle and Actions/Day gateways.
- `app/runtime/owner_operation.dart` cannot be deleted after Actions/Experts/Knowledge alone: `features/vault/infrastructure/app_wire_vault_gateway.dart` also uses its worker observer. Final App worker deletion requires Vault caller closure too. Pure ownerCommand/ownerQuery helpers are separate from that obsolete observer.

Matching App semantic extraction points are `crates/app/src/vault_host.rs` Registry/ExpertCandidates/ExpertReplaceBinding/CalendarAction/InspectProposal/MemoryReview/Memory branches; `vault_host/expert_binding_settings.rs`; `vault_host/learner_worker.rs`; `vault_host/product_actions.rs`; `action_facade.rs`; `day_services.rs`; and `local_operations.rs`/`worker.rs`. These are other owners' implementation scope.

## Existing fixes and implementation barrier

Two older map findings are stale: FloeApp already forwards its Actions dependency to PersonalDayScreen, and macOS Calendar permission disclosure already describes authorized create/edit/delete. S1 source pumps already use Rust-issued registration/request identities rather than a fixed Person. The remaining fixed Person is the separate EventKit effect path above.

This inventory reserves five mechanical client/native packages; it does not freeze new contracts or authorize speculative code. No client edits begin until the corresponding owner and binding shapes are assigned. No T0 work is repeated.
