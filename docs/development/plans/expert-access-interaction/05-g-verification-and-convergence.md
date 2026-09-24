# 05-G — failure matrix, verification and documentation convergence

- **Status:** complete. Verified code baseline `27299093263fb4998e59f800f663c5e604f22ac3` (05-F tip; 05-G adds no code).
- **Exit:** all rows below have direct regression evidence; 05 complete and 06 next.

## 1. Acceptance matrix

| ID | Required scenario | Evidence required |
|---|---|---|
| G01 | Direct tool source disabled | One durable origin-bound interaction, safe Manager observation, Completed Run |
| G02 | Delegated Schedule blocked | Task Completed with blocked judgment, Manager explanation, no fake Calendar evidence |
| G03 | Optional Calendar + other ready source | Requirement not discarded; actual contributing dependency coverage retained |
| G04 | Remote multi-source A/B | Separate source authority/dependencies; paused account is not a fake empty/complete aggregate |
| G05 | Missing/unknown target | Navigation-only recovery, no fabricated ids or inline grant |
| G06 | Wrong Person/Session/Task/consumer or forged artifact | No interaction-based authority; correct hard rejection |
| G07 | Invalid signature, corrupt/missing policy, duplicate exact source | No approval workaround |
| G08 | Publication crash/replay | Stable interaction/ref/message once; no actionable orphan |
| G09 | Fresh native and remote approval | Same canonical owner operations as Connections; reviewed bundle CAS |
| G10 | Drift/expected-absence race | Old approval cannot change new source/resource/policy or newly appeared grant |
| G11 | Duplicate/conflicting decision | Same command rejoins; different digest conflicts; one owner mutation |
| G12 | Owner committed, response lost | Durable reconciliation; no second grant/policy epoch advance |
| G13 | OS permission/settings/OAuth return | Backend evidence required; UI success flag alone never resolves |
| G14 | Out-of-band enable, removal, resource change | Read-only inspect unchanged; explicit refresh validates or supersedes |
| G15 | Denied/cancelled/expired review | No implicit grant or automatic child; correct resolving-operation semantics |
| G16 | First/final Manager model consent block | Zero unauthorized generation; deterministic explanation + card; usage truthful |
| G17 | Exact eligible recipient approval | Access-owned lineage/scope consent; fresh authorized model call |
| G18 | LocalOnly/forbidden class/unknown coverage/wrong recipient | Still fail-closed; consent never rewrites source processing policy |
| G19 | New source/profile/recipient after approval | No implicit consent scope expansion or hidden fallback |
| G20 | Consent revoke at admit/handoff/release | Correct transmission count and suppressed release; dispatched usage retained |
| G21 | Resolution/admission/restart races | One unique origin slot, canonical command and child Run |
| G22 | Several cards and concurrent decisions | One automatic follow-up after terminal group; all-denied gives none |
| G23 | Later user turn / archived origin | No silent stale restart; explicit current-Session CAS; no invented original text |
| G24 | Child execution | Fresh scope/budget/read/dependencies; not budget continuation/batch takeover |
| G25 | Existing confirmed/uncertain action | No duplicate write/reproposal dispatch; canonical Actions reconciliation |
| G26 | UI/FFI/events/resync | Real App route, safe actions, state updates after origin finished, no duplicate User message |
| G27 | Limits/privacy | Bounded refs/descriptors/listing/TTL; no secret/source content in model-safe metadata |
| G28 | Completed CP3/4 behavior | Registry independent, Use with Floe unchanged, atomic multi-view/reopen and `/focus` fences |

Use deterministic fixtures/fake transports and fault injection. Every race test checks durable identities, mutation/admission counts and final grant/policy revisions. A generic suite pass or a fabricated interaction fixture is not a substitute for the full app-composed path.

## 2. Crash injection matrix

Pause/restart at each boundary: before interaction persistence; after staged record before settled origin; after origin settlement before message; after decision claim before owner I/O; after owner commit before response; after resolution before child admission; after admission before execution dispatch; after child receipt before client acknowledgement; during provider handoff before response release.

For every boundary specify whether recovery rejoins, reconciles, suppresses stale action or fails closed. No blind retry after an uncertain mutation/effect. Retain existing model usage and validated pending-batch recovery tests.

## 3. Residual searches

Run from repo root; inspect all production matches, distinguishing negative fixtures and historical plans.

```sh
rg -n 'NeedsUserAction\(_\)|SourceAccessRequirement|USER_INTERACTION_MEDIA_TYPE|UserInteractionRef' crates apps/client
rg -n 'SavedConnectionRecipientAuthority|allow_external|external_recipients|allowExternal|externalRecipients' crates apps/client server
rg -n 'TurnMode::|EngineResumeState|continuation_of|retry_of|ResumeInteraction' crates apps/client
rg -n 'CalendarExpertSetup|SourceGrants|experts\.calendar\.|AgentCalendarExpert|calendar\.expert' crates apps/client
rg -n 'access_review_required|controller\.failure|prepareStartTurn|prepare.*Interaction' apps/client/lib/features/conversation apps/client/lib/app/runtime
rg -n 'Resolving|InteractionUpdated|interaction_id|resume_command' crates apps/client
```

Expected conclusions:

- no recoverable requirement is dropped or converted to generic hard failure without an owner-specific reason;
- no raw requirement artifact is trusted merely by media type;
- saved global consent fields are not a live approval authority; transport safety fences remain justified;
- no interaction resume maps to budget continuation or arbitrary user text;
- no second permission writer, Registry authority or hidden source/recipient fallback;
- no in-memory-only decision/resume idempotency or stale message status used as authority.

Review dependency manifests/allowlist for new reverse edges. New pure contracts are genuinely cross-owner values; actual mutation stays with its owner.

## 4. Full verification

Start with actual manifest package names and focused suites for contracts, Context, Access, Conversation, built-in Experts, Agent Runtime, Inference, Vault, provider adapters, App, protocol and FFI.

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

cd apps/client
flutter analyze
flutter test
flutter build macos
```

When provider/server generation, authorization or request contracts change, server gates are mandatory:

```sh
cd server
go test -race ./...
go vet ./...
```

Run the repository's focused native macOS permission/Calendar harness if native boundary behavior changes. Interactive OS/OAuth/live-model tests require separate explicit authorization and credentials; never use production accounts to make acceptance green. Record existing ignored tests as ignored with their reasons, not passed.

If rustfmt versions disagree with baseline, inspect pinned toolchain/current verification skill, run the matching formatter for edited files and report existing drift separately. Do not create unrelated formatting churn or hide a new warning behind historical warnings.

Use isolated tempdir/fresh development profiles for reopen/schema tests. Do not reset the operator's profile. Inability to run required macOS gates means completion is pending those gates, not silently waived. iOS is deferred by operator priority; Android parity/builds are out of scope.

## 5. Documentation in the same implementation

Update `docs/architecture/runtime.md`, `modules.md`, `authority-recovery.md` for actual Conversation interaction ownership, source-outcome publication, Access consent, fresh resume and event lifecycle. Amend/create the durable ADR selected in 05-A and the processing-consent rationale in 05-D. Update `docs/product/integrations-and-privacy.md` and relevant design/client docs for actual review, Not now, native recovery and no-model explanation behavior.

Do not state that recipient approval overrides LocalOnly or that grants are implicit in a message. Do not leave old saved-recipient-store instructions as current architecture. Keep progress/SHAs here, not in durable architecture docs.

After every acceptance row passes, update this plan and the 05 index with exact 05-A…G implementation SHAs and any final safety-fix SHA. Then update the directory README to 05 complete / 06 next with the final verified code baseline. Avoid “next commit” placeholders and self-referential completion SHA claims.

## 6. Agent final report

1. Stage SHAs, including any bounded plan-first corrective commits.
2. Final owner topology and canonical publication/resolve/resume paths.
3. Direct/delegated/optional/multi-source and no-model-blockage results.
4. Reviewed descriptor/CAS and operation-recovery proof.
5. Consent scope/expiry/route binding and prohibited-transfer results.
6. Unique child admission, newer-Session behavior, fresh execution and action safety.
7. Wire/Flutter/event/restart behavior.
8. G01–G28 evidence mapping plus crash injection outcomes.
9. Residual commands and every justified match.
10. Exact verification commands/results/warnings/ignored or unrun cases.
11. Architecture/ADR/product/plan documents updated.
12. Remaining blockers. Any unmet acceptance row or required gate keeps 05 open.

Plan-authoring validation is not implementation validation. Do not copy previous checkpoint test counts into this report as evidence for new behavior.

## 7. Completion evidence

Stage tips: 05-A `af28fee`, 05-B `c6e09b3`, 05-C `890c620`, 05-D `4e82bd4`, 05-E `23cef7b`, 05-F `2729909`. No bounded corrective commits were needed; 05-G is verification plus docs/plan closure.

Acceptance mapping (representative direct regressions; full mapping in the final report):

- G01 `direct_attention_tool_blocked_completes_turn_with_one_durable_ref`; G02 `production_builtin_expert_completes_blocked_task_with_durable_ref`, `common_schedule_review_requirement_completes_root_run`; G03 `optional_failures_are_explicit_and_never_masquerade_as_empty_data`; G04 `commitments_accept_bounded_multi_source_evidence_without_blurring_sources`, `multi_view_judgments_reject_cross_source_evidence_invention`.
- G05 `approve_on_navigation_only_is_rejected`, `refresh_navigation_settles_satisfaction_and_dead_connections`; G06 `builtin_endpoint_denies_forged_principal_without_touching_state`, `foreign_person_session_and_device_are_rejected`, `app_wire_interaction_rejects_forged_and_authority_fields`; G07 `gmail_bad_signature_never_mutates_nor_resolves`, `policy_identity_corruption_fails_closed_on_reopen`, `duplicate_exact_resource_grants_conflict_before_provider_io`; G08 `interaction_publishes_through_origin_journal_and_survives_compaction`, `exact_command_replay_does_not_dispatch_again_and_release_is_not_required`.
- G09 `native_allow_creates_exact_grant_and_resolves`, `remote_calendar_allow_resolves_through_hosted_connection`, `gmail_views_allow_enables_bundle_atomically_and_resolves`, `stale_revision_conflicts_without_owner_contact`; G10 `fresh_approve_with_drift_supersedes_without_mutation`, `gmail_authority_rotation_after_review_supersedes_without_mutation`, `sibling_grant_revoked_out_of_band_supersedes_with_absent_replacement`; G11 `double_allow_same_command_resolves_once`, `same_command_different_digest_conflicts_without_mutation`, `approve_precondition_mutates_once_with_stable_operation_id`; G12 `native_commit_then_crash_reopens_and_resolves_without_second_advance`, `gmail_commit_then_crash_reopens_and_resolves_without_second_mutation`.
- G13 `native_external_enable_refresh_resolves_inspect_does_not`, `native_os_denied_and_deselected_scope_never_falsely_resolve`; G14 `native_grant_paused_out_of_band_supersedes`, `native_deselected_scope_supersedes_without_mutation`, `refresh_reconciles_resolving_by_current_truth`; G15 `deny_records_denied_without_owner_contact`, `dismiss_cancels_pending_without_owner_contact`, `expired_interaction_persists_expired`, `observer_cancellation_never_revokes_a_recorded_decision`.
- G16 `canonical_root_unconsented_external_recipient_blocks_with_card_without_agent_post`, `blocked_model_call_reports_blocked_domain_judgment_without_artifacts`; G17 `consent_approve_grants_exact_review_and_resolves`, `authority_treats_revoked_and_expired_as_missing`; G18 `local_only_root_turn_starts_with_no_remote_connection`, `continuation_profile_mismatch_fails_closed`; G19 `same_request_id_with_different_profile_conflicts`, `gmail_reviewed_subset_supersedes_on_canonical_extras`; G20 `revoked_consent_blocks_child_fresh_without_stale_release`, `grant_rejoins_regrants_and_revokes`.
- G21 `resume_slot_rejoins_across_commands_without_redriving`, `consent_rejoined_command_rejoins_same_consent`, `resume_slot_survives_vault_reopen_and_rejoins`; G22 `allow_resolves_and_auto_child_runs_authorized_under_origin_lineage`, `deny_all_suppresses_automatic_child_without_dispatch`; G23 `newer_turn_suppresses_auto_but_explicit_continue_claims`, `prepare_resume_derives_origin_text_and_never_invents_it`, `worker_explicit_resume_claims_slot_at_current_revision`; G24 `linked_resume_runs_original_intent_with_marker_and_no_user_restatement`, `resume_child_never_redrives_origin_settled_tool_effect`, `child_resume_batch_mismatch_is_storage_fault`.
- G25 `governed_action_owner_approval_dispatch_and_recovery_are_durable`; G26 `app_wire_interaction_fixtures_are_stable`, Flutter `agent_interaction_test`/`conversation_runtime_gateway_test`; G27 `descriptor_bounds_reject_oversized_and_unsorted_targets`, `expiry_projects_without_writing`, `app_wire_validates_continuation_and_event_bounds`; G28 `multi_view_activation_rolls_back_as_one_local_set`, governed-focus tests, registry reopen tests.

Crash boundaries: before persistence (`builtin_endpoint_denies_forged_principal_without_touching_state`); staged-before-settled (`interaction_publishes_through_origin_journal_and_survives_compaction`); settled-before-message (`service_commits_encrypted_run_and_replays_after_vault_reopen`); claim-before-owner-IO (`refresh_reconciles_resolving_by_current_truth`, `owner_refusal_after_commit_resolves_and_other_failures_stay_resolving`); commit-before-response (native/gmail crash tests); resolution-before-admission (`resume_admission_recovers_resolved_group_after_restart_before_claim`); admission-before-dispatch (`open_vault_activation_interrupts_an_unfinished_conversation_run`, `activation_interrupts_orphan_and_releases_claim_without_replaying_work`); receipt-before-ack (`resume_slot_survives_vault_reopen_and_rejoins`, `worker_resolve_drives_auto_child_and_rejoins_retry`); provider handoff (`recovery_resumes_validated_batch_without_model_recall`, `dropped_attempts_keep_estimates_and_release_the_single_dispatch_slot`).

Residuals R1–R6: only current-architecture matches. Saved `allow_external`/`external_recipients` are validated transport scope, not approval authority (unconsented recipients still block with a card). Deleted `experts.calendar.*` wire kinds are rejection fixtures. No `InteractionUpdated` event exists; Flutter refreshes interaction state through snapshot get/list resync after origin completion. No `EngineResumeState`/`ResumeInteraction` legacy resume, no in-memory-only decision idempotency.

Verification at the baseline: `cargo check --workspace --lib` ok; `cargo test --workspace --no-fail-fast` ok; `cargo build -p floe-ffi` ok; `check_boundaries.py` 0 errors; `git diff --check` clean; `flutter analyze` no issues; `flutter test` 365 passed; `flutter build macos --debug` and `flutter build macos` (release) ok. Server gates not required (no `server/` changes in 05). Two ignored tests recorded as ignored (native response-loss shim, live Codex OAuth). Pre-existing rustc warnings retained; no new warnings introduced.
