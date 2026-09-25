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
