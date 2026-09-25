# Checkpoint 06-D — final verification and execution-plan archive

- **Status:** blocked on 06-C.
- **Baseline:** 06-C completion.
- **Goal:** prove the complete refactor under current source, then delete the temporary execution-plan directory.
- **Exit:** repository has no remaining checkpoint task or active plan for this refactor.

## 1. Verification-only posture

06-D may expose a missing defect. If so:
- reopen the owning 06-A/B/C plan;
- implement/fix semantically there;
- return to 06-D.

Do not hide new architecture work inside a verification commit.

## 2. Residual matrix

R1 old Schedule/Calendar Expert vertical:

~~~sh
rg -n 'ScheduleEndpoint|run_calendar_expert_endpoint|CalendarExpertSetup|CalendarExpertOverview|CalendarAccessConfiguration|CalendarViewBinding|AgentCalendarExpert|experts\.calendar\.|calendar\.expert' crates apps server docs
~~~

Production authority/runtime expected zero; explicit rejection/history fixtures named in report.

R2 Registry source authority:

~~~sh
rg -n 'SourceGrants|BuiltinSourceBinding|BuiltinSourceState|assignment_source_grant|assignment_has_mandatory_source|granted_view_handles' crates apps
~~~

R3 source-history heuristic:

~~~sh
rg -n 'SourceHistoryBoundary|ConservativeSourceHistoryBoundary|narrow_by_source_boundary|carries_source_history|bounded_source_history_start' crates apps
~~~

Expected zero.

R4 canonical provenance:

~~~sh
rg -n 'projection_coverage|read_turn_coverage|project_model_conversation_history|ContextDependency' crates/modules/conversation crates/modules/context crates/runtime/agent
~~~

Inspect for exact recorded coverage only.

R5 legacy inference:

~~~sh
rg -n 'ModelTransport|ModelTransportRequest|ModelTransportResponse|FoundationModelRunner|ServerModelRunner|ModelRouteConfig|RemoteRoute|RoutePairing|LEGACY_INFERENCE_CONSUMER' crates apps
~~~

Expected no legacy matches; PreparedModelTransport is current.

R6 saved recipient authority:

~~~sh
rg -n 'allowExternal|externalRecipients|withExternalConsent|coversExternalRecipient|external_recipients' apps/client crates
rg -n 'allow_external|expected_recipient' crates server
~~~

First search: no saved/product authority. Second: request-scoped fence/synthetic tests only.

R7 interaction/resume:

~~~sh
rg -n 'WaitingForUser|InteractionUpdated|ResumeInteraction|Resolving|resume_command_id|TurnMode::Resume|InteractionResumeRef' crates apps/client
~~~

No WaitingForUser or fake event route; current durable resolving/resume remains.

R8 duplicate permission UI:

~~~sh
rg -n 'Observe with Floe|Feature permissions|Review and enable|Allow external model providers|selectedConsumers|connection-view-consumer' apps/client/lib
~~~

R9 dead helper/public warning audit:
- search known 06-C candidates;
- inspect rustc/analyzer warnings;
- no refactor-introduced caller-zero production helper survives without justification.

R10 plan links:

~~~sh
rg -n 'expert-access-interaction' . --glob '!docs/development/plans/expert-access-interaction/**'
~~~

Expected zero before archive.

## 3. Cross-owner acceptance

A01 eligible connection establishes bounded default Observe; startup no-grant remains review-required.
A02 Use with Floe Off/On affects Observe only, not Act/recipient.
A03 multi-source keeps one dependency per source and deterministic merge.
A04 direct/delegated source block creates one durable interaction and completes origin.
A05 stale interaction decision cannot widen changed target.
A06 missing exact recipient creates no unauthorized generation; exact approval works; wrong/revoked/expired denies.
A07 resolution creates/rejoins one fresh linked Run; user text not duplicated.
A08 budget continuation: Independent and current dependent replay; stale/Unknown cannot execute; revoke-before-release cannot leak Answer.
A09 resumed interaction cannot duplicate confirmed/uncertain external Action effect.
A10 pending/resolving/resolved interaction and resume slot survive reopen.
A11 old Calendar mapping / old saved consent never becomes current authority.
A12 disconnect/revoke blocks later source admission/release without affecting unrelated connection.

## 4. Crash boundaries

H01 source revokes after continuation gate but before pending-step execution -> no step.
H02 stored Answer reached in memory then source revokes before terminal release -> no committed/released answer.
H03 contextual recipient consent consumed/revoked before handoff -> no provider call.
H04 provider handoff then revoke before release -> response suppressed, usage retained.
H05 old saved credential shape on restart -> explicit invalid/re-pair; no auto-delete, no consent migration.

Retain Checkpoint 05 crash/rejoin matrix as well.

## 5. Full gates

Rust:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Flutter/macOS:

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos --debug
flutter build macos
~~~

Run focused EventKit/native tests only if native code changed.

iOS validation is deferred to active iOS development by project policy. Android parity/build is out of scope.

Server, only if server changed:

~~~sh
cd server
go test -race ./...
go vet ./...
~~~

Otherwise report SKIPPED_UNCHANGED.

Report ignored/live credential tests as ignored unless explicitly authorized and run.

Avoid broad unrelated rustfmt churn; report any existing fmt baseline accurately.

## 6. Persistence acceptance

Use an isolated development profile.

Verify:
- current grants/policies survive reopen;
- contextual consent survives only within its TTL/lineage semantics;
- pending/resolving interaction survives;
- auto-resume slot rejoins;
- old Calendar mappings remain rejected;
- old saved consent fields never become contextual authority;
- explicit reset/re-pair touches only identified Floe-owned test state.

Never reset normal operator data or provider data.

## 7. Durable docs final pass

Verify:
- docs/README.md;
- docs/architecture README/invariants/modules/runtime/authority-recovery;
- affected docs/product files;
- docs/decisions index, ADR 0028, ADR 0030 and any status change made in 06-C;
- component READMEs/examples touched by 06.

No current document should instruct a reader to resume Checkpoint 01–06.
No current architecture doc should contain completion SHA tables.

## 8. Archive the plan

Only after code/docs/gates/residuals are accepted, delete:

~~~text
docs/development/plans/expert-access-interaction/
~~~

in one final archive commit.

Do not replace it with COMPLETED.md, STATUS.md, another archive folder or migration ledger. Git history is the archive.

After deletion:

~~~sh
git diff --check
rg -n 'expert-access-interaction' README.md docs AGENTS.md .agents
~~~

Expected links: zero.

## Verification record (2026-09-25)

Source after semantic commits: 06-A `04dba03a`; 06-B `36f404d7`, `7a59f47c`; 06-C `2794b27b`, `5832a575`. The 06-D audit reopened 06-C for a positive Go fixture using the removed Calendar consumer and 06-B for direct contextual-consent revoke races. Both fixes were committed in their owning semantic stages before final gates.

Residuals: R1 has only explicit legacy Calendar rejection fixtures and the current `CalendarAccessConfiguration` owner command; R2/R3/R8/R9/R10 removed-symbol/link searches are zero. R4 uses recorded `projection_coverage` and current resolver. R5 has only canonical `PreparedModelTransport`. R6 has old saved fields only in negative fixtures and request-scoped server fields. R7 has durable interaction/resume symbols, no `WaitingForUser` or fake event route.

Direct acceptance evidence:

- A01: `review_then_enable_binds_gmail_bundle_atomically`, `inspect_connected_calendar_without_grant_needs_review`.
- A02: `calendar_review_and_pause_use_current_grant_authority`, Flutter connection controls, no saved-recipient fields or ActionAuthority mutation.
- A03: `admitted_sources_keep_their_own_bindings`, `communication_merge_is_bounded_deterministic_and_partial`, `sibling_resources_complete_independent_reads_and_rechecks`.
- A04: `direct_attention_tool_blocked_completes_turn_with_one_durable_ref`, `production_builtin_expert_completes_blocked_task_with_durable_ref`.
- A05: `same_command_different_digest_conflicts_without_mutation`, `native_commit_then_crash_reopens_and_resolves_without_second_advance`.
- A06: `missing_consent_returns_requirement_with_zero_transport_calls`, `exact_contextual_consent_produces_consumed_target`, `external_recipient_mismatch_is_denied`, `authority_treats_revoked_and_expired_as_missing`.
- A07: `worker_resolve_drives_auto_child_and_rejoins_retry`, `linked_resume_runs_original_intent_with_marker_and_no_user_restatement`.
- A08: Independent/current/revoked/Unknown/multi-source pending-batch tests in Conversation; H01/H02 below.
- A09: `resume_child_never_redrives_origin_settled_tool_effect` plus `native_executor_uses_rust_ledger_and_lookup_only_after_response_loss`; Action effects remain in the separate idempotent Actions owner.
- A10: `reopen_recovers_every_state_and_rejoins_decisions`, `resume_slot_survives_vault_reopen_and_rejoins`, `consent_survives_vault_reopen`.
- A11: `legacy_grant_mapping_tables_are_rejected_not_migrated_on_reopen`, `obsolete_saved_recipient_list_is_not_decoded_as_authority`.
- A12: `disconnect_and_reconnect_are_durable_and_never_restore_old_views`, `revoked_grant_cannot_release_a_recorded_answer`, `sibling_resources_bind_independently`.

Direct crash-boundary evidence: H01 `revoked_pending_answer_never_executes_or_recalls_model`; H02 `revoked_after_pending_answer_execution_suppresses_terminal_output`; H03 `contextual_consent_revoked_before_consume_never_reaches_transport`; H04 `contextual_consent_revoked_after_handoff_suppresses_output_and_keeps_usage`; H05 `obsolete_saved_recipient_list_is_not_decoded_as_authority` plus Flutter's preserved old-credential negative fixture and no keychain deletion path. These five tests were also run as individual filtered commands.

Final gates: `cargo check --workspace --lib` passed; `RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast` passed (1,177 passed, 0 failed, 2 ignored); `cargo build -p floe-ffi` passed; `python3 tools/architecture/check_boundaries.py` passed; `git diff --check` passed. `flutter analyze`, `flutter test` (366 passed), `flutter build macos --debug` and `flutter build macos` passed. The changed Go test required and passed `go test -race ./...` and `go vet ./...`. EventKit response-loss and live Codex OAuth tests remain ignored for missing explicit authorization/runtime credentials; iOS and Android validation are out of scope. A parallel-only App test run hit Vault-job timeout assertions; the required full workspace gate passed with serial Rust test threads. No normal user data was reset; tests used isolated stores.

## 9. Final report

1. 06-A/06-B/06-C semantic SHA(s).
2. 06-D verification SHA and final plan-archive SHA.
3. final owner/runtime topology.
4. continuation provenance results.
5. model transport/recipient results.
6. deleted caller-zero/public surfaces.
7. R1–R10 results.
8. A01–A12 evidence.
9. H01–H05 evidence.
10. exact Rust/Flutter/macOS/Go commands/results/skips/ignored tests.
11. durable docs/ADR changes.
12. proof plan directory is absent.
13. blocker.

Any unmet item means the refactor remains open and the plan directory stays until closed.
