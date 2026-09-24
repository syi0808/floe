# 05-G — failure matrix, verification and documentation convergence

- **Status:** planned; depends on 05-A through 05-F.
- **Exit:** all rows below have direct regression evidence; only then mark 05 complete and 06 next.

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
