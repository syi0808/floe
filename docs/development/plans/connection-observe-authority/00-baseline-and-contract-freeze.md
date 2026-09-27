# 00: Baseline, executable regressions and contract freeze

Prerequisite: none.

Status: Complete.

Planning base: main at eb55389dc66ce30ad9693f739569dc1b58f2b2f3 on 2026-09-28.

This checkpoint turns the investigation into executable evidence and freezes the contracts that checkpoints 01-09 will implement. It is deliberately not a production-fix checkpoint. Temporary tests may be used to expose current behavior, but all temporary code/test hunks must be removed before the checkpoint commit.

The original plan was written before eb55389dc66ce30ad9693f739569dc1b58f2b2f3. That intervening commit already removed fixed Calendar selection-count caps across Rust, wire, Flutter and Apple paths and added >4/>128 regression coverage. Therefore the old "reproduce the four-calendar failure" step is stale. Checkpoint 00 now verifies that convergence as existing green evidence and keeps the still-live empty-consumer/intersection and leaf-candidate coupling as the failures to prove.

Line numbers below are anchors on the planning base. The execution agent must re-resolve them on its actual start HEAD and record any drift rather than blindly editing by line number.

## Exit state

00 is complete only when all of the following are true:

1. actual main/origin-main/worktree/toolchain state is recorded;
2. the 11-Calendar Use with Floe failure is reproduced through the real App -> policy -> Vault -> GrantScope owner path, or executable contrary evidence proves the failure no longer exists;
3. the fixed Calendar count caps removed by eb55389d are confirmed absent from current native read/review/wire/UI paths and their regression tests pass;
4. current Calendar Expert source candidates are proven to be leaf-resource scoped;
5. the target contracts in the plan README are checked against current owners/callers and any contradiction is recorded before 01 begins;
6. all stale tests/code surfaces are assigned DELETE / REWRITE / MOVE / RETAIN dispositions to the checkpoint that owns their final replacement;
7. no temporary red/diagnostic test or production patch remains;
8. only factual evidence/status documentation is committed for checkpoint 00, and checkpoint 01 remains Not started.

If any safety invariant in docs/architecture/authority-recovery.md contradicts the target model, stop 00 and report the contradiction. Do not route around it.

## Planning-base code anchors

### A. Empty-consumer permission failure

| Path / lines on eb55389d | Symbol / behavior | Why it matters |
|---|---|---|
| crates/app/src/first_party_observe.rs:22-75 | selected_shipped_consumers | A shipped consumer is admitted only when an active Expert binding selects the exact connector + connection + execution owner + resource. |
| crates/app/src/first_party_observe.rs:182-219 | native_calendar_policy_for_target | Iterates every Calendar resource and intersects the per-resource consumer vectors. Any resource with no identical selected consumers can collapse the final set to empty. |
| crates/app/src/vault_host/calendar_access.rs:469-553 | CalendarAccessChange::Review branch in apply_calendar_access | Real native Use with Floe review path. It previews the native subject, recomputes current connection state, derives consumers through native_calendar_policy_for_target, then calls Vault. |
| crates/adapters/vault/src/vault/calendar_grants.rs:112-145 | review_native_calendar_grant | Receives the consumer vector from App and constructs the native grant binding. |
| crates/adapters/vault/src/vault/calendar_grants.rs:420-446 | calendar_binding | Converts leaf Calendar IDs into GrantScope resources and forwards consumers directly. |
| crates/contracts/context/src/lib.rs:288-310 | GrantScope::try_new | Empty consumers are rejected as MissingScope; Vault maps the resulting scope construction failure to AgentFailure::InvalidInput. |
| crates/app/src/vault_host/tests/native_calendar_access.rs:37-197 | Fixture, apply, review | Existing owner-level harness is the preferred temporary reproduction surface. It already installs shipped Experts and binds Calendar-capable assignments to the leaf resource "home". |

The current failure chain to prove is:

~~~
CalendarAccessChange::Review
  -> apply_calendar_access
  -> native_calendar_policy_for_target
  -> selected_shipped_consumers(resource A)
     intersect selected_shipped_consumers(resource B)
     ...
  -> consumers = []
  -> review_native_calendar_grant
  -> calendar_binding
  -> GrantScope::try_new
  -> MissingScope
  -> AgentFailure::InvalidInput
~~~

Do not "fix" the reproduction by adding assistant as a wildcard consumer, unioning consumers, skipping the grant write, or creating one grant per leaf Calendar.

### B. Leaf-scoped Expert source candidates

| Path / lines on eb55389d | Symbol / behavior | Why it matters |
|---|---|---|
| crates/modules/context/src/application/source_candidates.rs:150-175 | discover_source_candidates calendar.timeline branch | Loops over connection.calendars and emits one SourceSelectionReference per leaf Calendar ID. |
| crates/modules/context/src/application/source_candidates.rs:313-347 | calendar_addition_produces_a_new_candidate_without_changing_the_old_reference | Existing test proves A -> one candidate and A+B -> two candidates with a new candidate identity. |
| crates/app/src/first_party_observe.rs:22-75 | selected_shipped_consumers | Permission composition later treats those exact leaf selections as consumer authority inputs. |

This is current-state evidence, not desired behavior. Checkpoint 03 will replace it with one connection/View candidate whose identity is stable across leaf-resource changes.

### C. Fixed Calendar count-cap cleanup already on main

The following are current green evidence after eb55389d and must not be reimplemented in 00:

| Path / lines on eb55389d | Existing evidence |
|---|---|
| crates/modules/context/src/application/native_calendar.rs:108-191 | admit_current_native_calendar_read validates non-empty/unique/current identifiers but no longer enforces a fixed count of four. |
| crates/modules/context/src/application/native_calendar.rs:301-329 | preview_native_calendar_subject validates identifiers without a fixed count-of-four policy. |
| crates/modules/context/tests/native_calendar_read.rs:300-332 | native_admission_preserves_more_than_128_selected_calendars proves the current native admission path carries 129 selected Calendars. |
| crates/modules/context/tests/calendar_timeline.rs:660-668 | timeline_grant_accepts_more_than_four_exact_calendars proves 11 Calendar IDs pass the timeline grant contract. |
| crates/contracts/context/src/lib.rs:294 onward and test around 1127 | GrantScope no longer has MAX_RESOURCE_HANDLES count rejection; the contract test accepts 129 distinct resources within remaining validity/budget rules. |
| crates/bindings/protocol/tests/local_owner_wire.rs:255-271 | local Calendar preview/review wire accepts 129 Calendar IDs. |
| apps/client/test/features/connections/native_calendar_access_test.dart:93-145 | Flutter native subject preview forwards 11 Calendar IDs. |
| apps/client/test/features/connections/connector_screen_test.dart:208-282 | Device Calendar Use with Floe UI previews/reviews all 11 configured Calendar IDs. |

The architectural conclusion does not change: fixed resource-count policy was accidental and should remain absent. Actual provider/item/byte/time budgets remain valid.

### D. Contract/legacy anchors to freeze before cutover

| Path / lines on eb55389d | Current state | Owning checkpoint |
|---|---|---|
| crates/contracts/context/src/lib.rs:383-439 | GrantSourceBinding contains SourceAuthority. | 02 |
| crates/contracts/context/src/lib.rs:647-865 | ContextDependency carries ConsumerPolicyAuthority and obtains source authority through GrantSourceBinding. | 02, 05 |
| crates/modules/day/src/domain/calendar.rs:83-92 | Day CalendarConnection owns calendars + revision + SourceAuthority. | 01 |
| crates/adapters/vault/src/vault/calendar_grant_policy.rs:1-360 | CalendarGrantPolicy persists ConsumerPolicyAuthority and reviewed subject data in a side table. | 05 |
| crates/modules/access/src/application/remote_calendar.rs:37 onward | RemoteCalendarSourceReference and Calendar-specific remote grant/read stack exist in parallel with generic Views. | 04 |
| crates/modules/context/src/application/remote_views.rs:31-46 | remote_view_resource already gives generic View + connection identity for non-Calendar remote sources. | 04 |
| crates/app/src/vault_host/review_snapshot.rs:235-254 | Conversation review fingerprint for native Calendar is recomputed from leaf resources and per-target first-party policy. | 03, 05 |
| crates/app/src/vault_host/interaction_owners.rs:333-359 and 641-653 | Live interaction refresh repeats the same target-specific Calendar policy calculation. | 03, 05 |

## 00-A: repository, branch and toolchain baseline

Read only the context required by the active plan:

~~~
AGENTS.md
.agents/skills/architecture-change/SKILL.md
docs/development/plans/connection-observe-authority/README.md
docs/development/plans/connection-observe-authority/00-baseline-and-contract-freeze.md
docs/architecture/invariants.md
docs/architecture/authority-recovery.md
~~~

Then record:

~~~
git fetch origin
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
git log -1 --oneline
git merge-base --is-ancestor eb55389dc66ce30ad9693f739569dc1b58f2b2f3 HEAD
cargo metadata --no-deps --format-version 1
python3 tools/architecture/check_boundaries.py
rustc --version
cargo --version
flutter --version
go version
~~~

On macOS also record:

~~~
xcodebuild -version
uname -a
uname -m
~~~

Rules:

- do not reset, clean, stash, delete profiles, rotate keys or change credentials;
- preserve unrelated user work;
- if HEAD != origin/main, do not silently execute on a stale branch;
- if main moved beyond the planning base, re-resolve every line anchor and note semantic drift;
- a moved line number is not a design change; a changed owner/contract is.

## 00-B: verify the already-landed count-limit cleanup

First inspect the intervening commit when it is still an ancestor:

~~~
git show --stat --oneline eb55389dc66ce30ad9693f739569dc1b58f2b2f3
git show --format=fuller --no-ext-diff eb55389dc66ce30ad9693f739569dc1b58f2b2f3
~~~

Run the narrow regression evidence that should now be green:

~~~
cargo test -p floe-context --test native_calendar_read native_admission_preserves_more_than_128_selected_calendars
cargo test -p floe-context --test calendar_timeline timeline_grant_accepts_more_than_four_exact_calendars
cargo test -p floe-context-contract grant_scope_accepts_more_than_128_distinct_resources_within_byte_budget
cargo test -p floe-protocol --test local_owner_wire calendar_access_kinds_roundtrip_and_reject_stale_shapes

(
  cd apps/client
  flutter test test/features/connections/native_calendar_access_test.dart
  flutter test test/features/connections/connector_screen_test.dart
)
~~~

Run residual searches for the removed fixed-count policy:

~~~
rg -n "MAX_ACQUISITION_CALENDARS|MAX_RESOURCE_HANDLES|calendar_ids\.len\(\) > 4|calendarIds\.length > 4|calendarIDs\.count <= 4|availableCalendarIDs\.count <= 128" \
  crates apps/client
~~~

Expected result: no live fixed-count authorization guard for the native Calendar review/read path.

Do not treat unrelated provider pagination/item limits as a failure. If a residual Calendar selection-count policy is found in a live Apple/Rust/Flutter path, record it as unexpected evidence and stop 00 rather than making a production fix inside this checkpoint.

## 00-C: reproduce the 11-Calendar empty-consumer failure through the real owner path

Use crates/app/src/vault_host/tests/native_calendar_access.rs as a temporary diagnostic surface. Do not create a parallel test harness unless the existing Fixture cannot reach the owner path.

### Temporary arrangement

1. Start with Fixture::new().
   - It creates the EventKit connection.
   - It installs shipped Expert manifests.
   - Calendar-capable Expert bindings are currently selected only for leaf resource "home".
2. Through the existing FloeCore Calendar connection mutator, replace/widen the connection resource set to eleven valid CalendarSelection entries while leaving the Expert bindings unchanged.
3. Include "home" plus ten additional unique IDs. This guarantees at least one currently bound resource and at least one unbound resource without fabricating a third-party consumer.
4. Re-inspect Calendar access after the resource update so the request uses the current connection revision/SourceAuthority.
5. Use FixtureSubject's deterministic fingerprint behavior or explicitly seed the eleven-ID fingerprint. Do not bypass preview_native_calendar_subject.
6. Call Fixture::review / apply_calendar_access with all eleven current Calendar IDs.

Suggested temporary positive-regression name:

~~~
native_calendar_observe_accepts_connection_with_eleven_resources
~~~

The temporary test should express the desired future result (review succeeds) so current code fails. Capture the actual error with backtrace:

~~~
RUST_BACKTRACE=1 cargo test -p floe-app native_calendar_observe_accepts_connection_with_eleven_resources -- --nocapture
~~~

### Cause isolation

In the same temporary test or an adjacent temporary diagnostic assertion, call native_calendar_policy_for_target for the exact eleven IDs before the review mutation and record:

- consumer identifiers returned for "home";
- consumer identifiers returned for at least one unbound leaf, if inspected separately;
- final policy.consumers for the eleven-resource target.

Expected current result on the planning base:

~~~
per-resource selected consumers differ
-> intersection becomes []
-> calendar_binding forwards []
-> GrantScope::try_new rejects MissingScope
-> App observes AgentFailure::InvalidInput
~~~

If review succeeds, do not force a failure. Instead trace which current code changed the policy semantics and update the evidence/contract freeze before continuing.

### Cleanup gate

After capturing evidence:

- remove the temporary test/helper/diagnostic assertions;
- do not commit a red regression;
- do not commit a current-bug assertion that would preserve the old architecture;
- confirm crates/app/src/vault_host/tests/native_calendar_access.rs has no unintended diff.

Checkpoint 03 will add the permanent positive regression after the permission architecture is cut over.

## 00-D: prove leaf-resource candidate coupling without changing production

The existing test already gives executable evidence:

~~~
cargo test -p floe-context calendar_addition_produces_a_new_candidate_without_changing_the_old_reference
~~~

Record the observed facts from crates/modules/context/src/application/source_candidates.rs:

~~~
Connection calendars = [A]
-> 1 calendar.timeline candidate
-> candidate resource = A

Connection calendars = [A, B]
-> 2 calendar.timeline candidates
-> A candidate remains
-> B creates a distinct candidate_id
~~~

Also record the replacement assertion owned by checkpoint 03:

~~~
Connection resources = [A]
-> 1 calendar.timeline:<connection> candidate

Connection resources = [A, B]
-> the same 1 candidate
-> candidate identity unchanged
-> Expert binding revision unchanged solely because leaf resources changed
~~~

Do not rewrite this test in 00. Classify it REWRITE in 03.

## 00-E: freeze exact target contracts against current callers

This step is source inspection plus evidence, not implementation.

### Stable grant source identity

Current anchor: crates/contracts/context/src/lib.rs:383-439.

Freeze the 02 target:

~~~
GrantSourceBinding
  person
  connection
  connector
  execution_owner

SourceAuthority is not a field of the standing grant source identity.
No replacement optional/migration field is allowed.
~~~

Search:

~~~
rg -n "GrantSourceBinding|source_authority\(" crates
~~~

Record the callers that will require 02 migration.

### Dependency split

Current anchor: crates/contracts/context/src/lib.rs:647-865.

Freeze the 02/05 target:

~~~
ContextDependency
  grant identity + GrantAuthority
  stable GrantSourceBinding
  logical grant resources
  current SourceAuthority
  exact source_resources actually observed
  categories / operation / purpose / consumer / processing
  observation / query / lease / freshness identity
~~~

ConsumerPolicyAuthority is not part of the final dependency.

Exact-recipient processing must remain bound to the current source identity/authority and exact observed source resources where source data leaves the local authority boundary.

### Connection-owned leaf resources

Current anchor: crates/modules/day/src/domain/calendar.rs:83-92.

Freeze the 01 target:

- Connections owns current standing Calendar resources and SourceAuthority.
- Day keeps Calendar mirror/events/sync freshness/domain projection only.
- Expert binding and Access grant may identify a connection/View but do not own a second authoritative leaf list.

### Review digest

Current policy_fingerprint behavior is compare-only review identity but still includes the wrong target-derived consumer semantics.

Freeze the final meaning:

~~~
policy_digest =
  logical View
  + sorted trusted first-party consumers
  + categories
  + operation
  + purpose
  + processing

policy_digest excludes:
  leaf resource selection
  Expert assignment selection
  SourceAuthority
  GrantAuthority
~~~

The digest grants no authority.

### First-party consumer trust

Freeze:

- trusted shipped manifest + declared capability may participate in default first-party policy;
- active Registry binding selection is configuration, not permission authority;
- arbitrary extension packages do not enter existing first-party grants;
- assistant is included only for Views Manager actually reads directly; it is not an empty-consumer fallback.

## 00-F: disposition ledger

Record the following baseline dispositions. Add newly discovered equivalent surfaces to the same owning checkpoint rather than preserving them for compatibility.

| Current surface | Disposition | Owner |
|---|---|---|
| first_party_observe::selected_shipped_consumers used for grant consumer composition | DELETE from permission composition | 03 |
| native_calendar_policy_for_target per-resource intersection | DELETE / REPLACE with logical View first-party policy | 03 |
| Calendar leaf SourceSelectionReference generation | REWRITE to connection/View candidate | 03 |
| calendar_addition_produces_a_new_candidate_without_changing_the_old_reference | REWRITE | 03 |
| native Calendar fixed count-of-four / fixed resource-count guards | ALREADY REMOVED by eb55389d; RETAIN only real byte/item/time/provider budgets | 00 evidence / 03 final invariant |
| Day-owned Calendar connection authority | MOVE to Connections, then DELETE old authority/persistence path | 01 |
| GrantSourceBinding.source_authority | DELETE | 02 |
| ContextDependency source/consumer-policy shape | REWRITE | 02 / 05 |
| crates/adapters/vault/src/vault/calendar_grant_policy.rs | DELETE when GrantAuthority + SourceAuthority + policy digest replace its authority role | 05 |
| ConsumerPolicyAuthority and policy epoch tests/wire/storage | DELETE / REWRITE | 05 |
| crates/modules/access/src/application/remote_calendar.rs | DELETE after generic remote View cutover | 04 |
| crates/adapters/vault/src/vault/remote_calendar_grants.rs | DELETE after generic remote View cutover | 04 |
| selected_resources vs granted_resources product projection | REWRITE | 07 |
| Flutter remote Calendar _observeResource plumbing | DELETE | 07 |

Do not add compatibility wrappers, legacy decoders, dual schemas or migration-only optional fields to preserve any item in this table.

## 00-G: targeted baseline test matrix

Before closing 00, run the nearest tests that establish current behavior. Adjust only invalid filter names; zero-match filters are not evidence.

~~~
cargo test -p floe-app native_calendar_access
cargo test -p floe-app first_party_observe
cargo test -p floe-context source_candidates
cargo test -p floe-context --test native_calendar_read
cargo test -p floe-context --test calendar_timeline
cargo test -p floe-vault calendar_grant
cargo test -p floe-protocol --test local_owner_wire

(
  cd apps/client
  flutter test test/features/connections/native_calendar_access_test.dart
  flutter test test/features/connections/connector_screen_test.dart
)
~~~

Do not run the full workspace solely to close baseline evidence unless the targeted checks expose a broader break. Full repository verification belongs to checkpoint 09.

## 00-H: evidence write-up and close gate

Append a "Checkpoint 00 execution evidence" section to this file with:

1. execution date;
2. start local HEAD and origin/main HEAD;
3. worktree status before and after;
4. toolchain versions;
5. exact commands actually run and pass/fail/skip result;
6. 11-Calendar failure result and the exact owner chain where it failed;
7. count-limit cleanup verification and residual-search result;
8. leaf-candidate evidence;
9. contract contradictions or "none found";
10. final disposition-ledger additions;
11. temporary test cleanup confirmation;
12. checkpoint completion commit SHA.

Then update the parent README status row:

~~~
00 | Baseline, executable regressions and contract freeze | Complete
01 | Connection-owned source/resource authority | Not started
~~~

The 00 completion commit may contain only factual plan/evidence/status changes. No production workaround is allowed.

Close with:

~~~
python3 tools/architecture/check_boundaries.py
git diff --check
git status --short
~~~

Suggested completion commit subject:

~~~
docs: complete connection observe checkpoint 00
~~~

Stop after 00. Do not begin checkpoint 01 in the same execution.

## Checkpoint 00 execution evidence

Executed 2026-09-28 on macOS arm64. Start local HEAD and `origin/main` were both `51441b8f9c7900be4217f95280c0c0bc5f848c7e` after `git fetch origin`; `main` tracked `origin/main`, and `git status --short --branch` showed a clean worktree. `git log -1 --oneline` was `51441b8f docs: detail connection observe checkpoint 00`. The planning base `eb55389dc66ce30ad9693f739569dc1b58f2b2f3` is an ancestor. Actual symbols were re-resolved on the start HEAD; the listed owner paths and semantics had not drifted from the planning-base anchors.

Toolchain: `rustc 1.93.1`, `cargo 1.93.1`, Flutter `3.47.2` stable / Dart `3.13.2`, Go `1.25.5 darwin/arm64`, Xcode `26.2` build `17C52`; `uname -a` reported Darwin `25.5.0` / `RELEASE_ARM64_T6020`, and `uname -m` reported `arm64`. `cargo metadata --no-deps --format-version 1` succeeded. Initial `python3 tools/architecture/check_boundaries.py` returned zero errors and zero warnings.

### Executable results

| Command | Actual result |
|---|---|
| `git show --stat --oneline eb55389dc66ce30ad9693f739569dc1b58f2b2f3`; `git show --format=fuller --no-ext-diff eb55389dc66ce30ad9693f739569dc1b58f2b2f3` | Passed; inspected the 25-file fixed-count removal and regression additions. |
| `cargo test -p floe-context --test native_calendar_read native_admission_preserves_more_than_128_selected_calendars` | Passed, 1 test. |
| `cargo test -p floe-context --test calendar_timeline timeline_grant_accepts_more_than_four_exact_calendars` | Passed, 1 test. |
| `cargo test -p floe-context-contract grant_scope_accepts_more_than_128_distinct_resources_within_byte_budget` | Passed, 1 test. |
| `cargo test -p floe-protocol --test local_owner_wire calendar_access_kinds_roundtrip_and_reject_stale_shapes` | Passed, 1 test; the targeted test includes 129-ID preview/review wire assertions. |
| `flutter test test/features/connections/native_calendar_access_test.dart` | Passed, 5 tests, including 11-ID preview. |
| `flutter test test/features/connections/connector_screen_test.dart` | Passed, 13 tests, including 11-ID Device Calendar review UI. |
| `rg -n 'MAX_ACQUISITION_CALENDARS\|MAX_RESOURCE_HANDLES\|calendar_ids\.len\(\) > 4\|calendarIds\.length > 4\|calendarIDs\.count <= 4\|availableCalendarIDs\.count <= 128' crates apps/client` | No matches (expected `rg` exit 1); no live fixed-count selection guard found in Rust, wire, Flutter or Apple paths. Remaining identifier, byte, item and provider budgets are not count-cap regressions. |
| `RUST_BACKTRACE=1 cargo test -p floe-app native_calendar_observe_accepts_connection_with_eleven_resources -- --nocapture` | Expected red diagnostic: 1 failed, `Err(InvalidInput)`; temporary test removed afterward. |
| `cargo test -p floe-context calendar_addition_produces_a_new_candidate_without_changing_the_old_reference` | Passed, 1 matching test (other targets had zero matches). |
| `cargo test -p floe-app native_calendar_access` | Passed, 11 matching tests. |
| `cargo test -p floe-app first_party_observe` | Passed, 8 matching tests. |
| `cargo test -p floe-context source_candidates` | Passed, 4 matching tests. |
| `cargo test -p floe-context --test native_calendar_read` | Passed, 12 tests. |
| `cargo test -p floe-context --test calendar_timeline` | Passed, 22 tests. |
| `cargo test -p floe-vault calendar_grant` | Passed, 28 matching tests. |
| `cargo test -p floe-protocol --test local_owner_wire` | Passed, 6 tests. |

The temporary diagnostic used the existing `Fixture::new()` and `FloeCore::set_calendar_scope` to set revision 2 with `home` plus `calendar-1` through `calendar-10`, kept Expert bindings on `home`, re-inspected the current SourceAuthority, and used `FixtureSubject`'s deterministic 64-character fingerprint through the real preview. Direct policy probes returned `home` consumers `floe.builtin.commitments`, `floe.builtin.focus-attention`, `floe.builtin.schedule`, `floe.builtin.wellbeing`; `calendar-1` returned `[]`; the eleven-ID target returned `[]`. `Fixture::review` then returned `Err(InvalidInput)`. The source-level owner chain is `CalendarAccessChange::Review` → `apply_calendar_access` → `native_calendar_policy_for_target` → per-resource `selected_shipped_consumers` intersection → `review_native_calendar_grant` → `calendar_binding` → `GrantScope::try_new`: the last rejects empty consumers as `MissingScope`, and Vault maps that validation failure to `AgentFailure::InvalidInput`. The diagnostic result is consistent with this path; no production path was altered.

`discover_source_candidates` still loops over `CalendarConnection.calendars` for `calendar.timeline`. Its passing test proves `[A]` emits one candidate with resource `A`, while `[A,B]` emits two: the `A` candidate remains equal and `B` has a distinct `candidate_id`. Checkpoint 03 must instead assert one `calendar.timeline:<connection>` candidate whose identity and Expert binding revision remain unchanged by leaf additions.

### Contract freeze and disposition

No contradiction with the target contracts or `docs/architecture/authority-recovery.md` was found. Current `GrantSourceBinding` still stores `SourceAuthority`, and constructors/source-authority access span Access, Context, Vault, App, providers and runtime/provenance fixtures; checkpoint 02 must remove that field and migrate the callers rather than wrap it. Current `ContextDependency` still carries `ConsumerPolicyAuthority` and lacks separate exact observed `source_resources`; checkpoints 02/05 must split logical grant resources from current source authority/resources while retaining exact-recipient provenance. Day's `CalendarConnection` still owns calendars, revision and SourceAuthority; checkpoint 01 moves standing source authority to Connections. `policy_fingerprint` is compare-only review identity but native Calendar target calculation still derives consumers from leaf selection; the final digest excludes leaf and assignment selection and never grants authority. Trusted shipped manifests/capabilities may contribute first-party consumers; arbitrary extensions cannot, and `assistant` is not a Calendar fallback. Existing generic `remote_view_resource(view_id, connection_id)` demonstrates the logical View contract, while Calendar-specific remote grant paths remain for checkpoint 04. Access exact-recipient consent, Context provenance, Act authority and uncertain-write recovery remain separate.

Additional dispositions beyond the baseline table:

| Current surface | Disposition | Owner |
|---|---|---|
| `crates/app/src/vault_host/review_snapshot.rs` native Calendar per-leaf review members and target-derived fingerprint | REWRITE to logical View review digest/member | 03 / 05 |
| `crates/app/src/vault_host/interaction_owners.rs` live Calendar target/inline per-leaf fingerprint refresh | REWRITE to logical View policy comparison | 03 / 05 |
| `crates/contracts/context/src/processing.rs` reviewed processing source scope deriving source authority from `GrantSourceBinding` | REWRITE to explicit current SourceAuthority and exact observed source resources; RETAIN exact-recipient restriction | 02 / 05 |
| `crates/adapters/vault/src/vault/access_grants.rs` source-epoch columns and source-bound grant lookup/update | REWRITE around stable source identity and standing GrantAuthority; DELETE source-epoch-as-grant-identity semantics | 02 |
| `crates/app/src/vault_host/remote_observe.rs` target policy fingerprint/member review composition | REWRITE to canonical logical View digest in remote cutover; RETAIN compare-only review drift check | 04 / 05 |

The temporary `native_calendar_observe_accepts_connection_with_eleven_resources` hunk was removed after evidence capture. `git diff --exit-code -- crates/app/src/vault_host/tests/native_calendar_access.rs` passed; no temporary red test, diagnostic assertion, helper or production patch remains. The final worktree/close-gate results and completion commit SHA are reported at handoff after committing this self-describing evidence (a commit cannot contain its own SHA). Checkpoint 01 remains Not started.

Immediately before the completion commit, `git status --short --branch` showed only this file and the parent plan README modified. The close gate `python3 tools/architecture/check_boundaries.py` passed with zero errors/warnings; `git diff --check` passed. The post-commit worktree and exact commit SHA are captured in the execution handoff.

## Required agent report

Report exactly these categories:

1. start HEAD / origin-main / final HEAD;
2. baseline environment and worktree state;
3. 11-Calendar reproduction result;
4. exact empty-consumer owner chain or contrary evidence;
5. >4/>128 cleanup verification result;
6. leaf-candidate coupling result;
7. contract-freeze findings and any contradiction;
8. disposition-ledger additions;
9. commands run with real outcomes;
10. temporary-hunk cleanup confirmation;
11. documentation files changed;
12. checkpoint-00 commit SHA;
13. confirmation that checkpoint 01 was not started.
