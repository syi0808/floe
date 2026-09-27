# 00: Baseline, executable regressions and contract freeze

Prerequisite: none.

This checkpoint turns the September 27 investigation into executable baseline evidence and freezes the final contract before production edits. It does not land a local workaround.

## Exit state

00 is complete only when:

1. actual main HEAD/worktree and source anchors are rechecked;
2. the two Calendar failures and the source-candidate coupling are reproduced through real owner paths or corrected with executable contrary evidence;
3. the final contracts in README are confirmed against current callers;
4. obsolete test/code surfaces are classified for later deletion;
5. no temporary red test patch remains committed;
6. the README status row records evidence and 01 remains not started.

## Current source anchors

Re-resolve these symbols on the execution HEAD.

| Finding | Current anchor | Meaning |
|---|---|---|
| Per-leaf consumer computation | crates/app/src/first_party_observe.rs::selected_shipped_consumers | Registry assignment/binding participates in first-party permission composition. |
| Calendar consumer intersection | first_party_observe.rs::native_calendar_policy_for_target | Consumers selected for every resource are intersected. |
| Empty consumer rejection | crates/contracts/context/src/lib.rs::GrantScope::try_new | Empty consumers produce MissingScope/InvalidInput. |
| Leaf Calendar candidates | crates/modules/context/src/application/source_candidates.rs::discover_source_candidates | calendar.timeline emits one SourceSelectionReference per CalendarSelection. |
| Four-calendar read cap | crates/modules/context/src/application/native_calendar.rs::admit_current_native_calendar_read and preview_native_calendar_subject | More than four current/selected calendar IDs are rejected. |
| Day owns connection authority | crates/modules/day/src/domain/calendar.rs::CalendarConnection and application/observations.rs | connection ID, scope, calendars, revision and SourceAuthority live in Day. |
| Native grant policy side table | crates/adapters/vault/src/vault/calendar_grant_policy.rs | ConsumerPolicyAuthority and reviewed subject are stored outside DataAccessGrant. |
| Remote Calendar parallel stack | crates/modules/access/src/application/remote_calendar.rs and vault/remote_calendar_grants.rs | Calendar duplicates generic remote View grant/preview/read concepts. |
| Generic remote View resource | crates/modules/context/src/application/remote_views.rs::remote_view_resource | Mail/work/logistics already use View + connection identity. |
| Flutter Calendar remote resource plumbing | apps/client/lib/features/connections/presentation/server_connector_panel.dart::_observeResource | UI sends a Calendar leaf resource to Observe grant operations. |

## 00-A: repository and toolchain baseline

Record:

~~~
git status --short --branch
git rev-parse HEAD
git log -1 --oneline
cargo metadata --no-deps --format-version 1
python3 tools/architecture/check_boundaries.py
rustc --version
cargo --version
flutter --version
go version
~~~

On macOS also record xcodebuild -version.

Do not reset, clean, stash automatically, delete profiles or change credentials.

Run the existing nearest tests before temporary repro patches:

~~~
cargo test -p floe-app first_party_observe
cargo test -p floe-context native_calendar
cargo test -p floe-context source_candidates
cargo test -p floe-access calendar
cargo test -p floe-vault calendar_grant
(
  cd apps/client
  flutter test test/features/connections/native_calendar_access_test.dart
  flutter test test/features/connections/server_connector_panel_test.dart
)
~~~

Filter names must be adjusted to current exact tests if necessary. Record zero-match filters as invalid evidence and rerun with a real target.

## 00-B: reproduce the 11-calendar permission failure

Add a temporary positive regression through the App/Vault owner path. Suggested name:

native_calendar_observe_accepts_connection_with_eleven_resources

Arrange:

- one EventKit connection;
- eleven valid calendar resources;
- Use with Floe review/enable through the same App operation used by the connection screen;
- shipped first-party Expert bindings that are not identical for every leaf resource, matching the current production shape that triggered the report.

Desired final behavior:

- enable succeeds;
- one logical calendar.timeline connection grant is active;
- no Expert/resource intersection is involved.

Expected current evidence is an InvalidInput/MissingScope caused by an empty consumer vector. Record the actual error and exact stack/owner function. Do not fix it in 00.

Revert the temporary production/test hunk after evidence capture.

## 00-C: reproduce the hidden four-calendar read failure

Add a temporary positive Context/provider fixture with eleven current resources and an otherwise valid native Calendar connection/grant.

Suggested name:

native_calendar_read_uses_all_current_connection_resources

Desired final behavior:

- all eleven current resources are admitted;
- provider read is bounded by item/byte/time limits, not an arbitrary resource-count-4 authorization limit.

Record whether current code fails in admit_current_native_calendar_read, preview_native_calendar_subject or a later adapter bound. If another lower provider/OS limit exists, distinguish it from Access/Context policy.

Revert the temporary red patch after capture.

## 00-D: prove Calendar binding currently changes with leaf resources

Using source_candidates.rs or the App Expert binding settings path, capture current behavior:

1. Calendar connection with A -> one candidate;
2. same connection resource set A,B -> two candidates;
3. candidate identity for B is a new Expert-bindable target.

This test is not a desired regression. It is evidence that the current binding model is leaf-scoped.

Freeze the replacement assertion for checkpoint 03:

~~~
resource set A -> one calendar.timeline:<connection> candidate
resource set A,B -> the same one candidate
binding revision unchanged when only resources change
~~~

## 00-E: freeze exact contract semantics

Confirm all of the following before 01 begins.

### Stable grant source

GrantSourceBinding keeps:

- PersonId;
- ConnectionId;
- ConnectorId;
- ExecutionOwnerId.

Remove SourceAuthority in 02. No replacement optional field.

### Dependency split

ContextDependency after 02 has:

- grant identity/authority;
- stable GrantSourceBinding;
- logical grant resources;
- current SourceAuthority;
- exact source_resources observed;
- categories, operation, purpose, consumer, processing;
- observation/query/lease/process/freshness identity.

Exact-recipient processing scope must bind the current source authority and exact observed source resources where source data is transferred.

### Connection resource semantics

Connections is the only owner of current standing leaf resources. Provider inventory or system permission changes update that owner state. Day mirrors and Expert bindings may reference connection identity but do not copy authoritative resource scope.

### Policy digest

The review digest is deterministic over the intended logical View grant policy. It is compare-only review evidence. It is never accepted as a grant or source authority.

### First-party consumers

A shipped trusted manifest declaring the View/capability may be in the default first-party consumer set. Registry source selection does not add/remove it. Extension manifests are never implicitly trusted.

## 00-F: disposition ledger

Classify, without preserving them for compatibility:

- first_party_observe resource-selection consumer logic -> DELETE in 03;
- Calendar leaf source candidate tests -> REWRITE in 03;
- native resource-count-4 tests/guards -> DELETE/REWRITE in 03;
- Day Calendar connection authority -> MOVE/DELETE in 01;
- remote_calendar.rs and remote_calendar_grants.rs -> DELETE in 04;
- ConsumerPolicyAuthority and policy epoch tests -> DELETE/REWRITE in 05;
- Flutter _observeResource Calendar branch -> DELETE in 07;
- selected_resources vs granted_resources UI expectations -> REWRITE in 07.

Record any newly found equivalent path in the owning checkpoint.

## Verification and close

00 may commit only factual plan/evidence updates, never a workaround. Run:

~~~
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Then update README with the 00 evidence commit and leave 01 Not started.
