# Checkpoint 03-E — residual authority and Calendar access closure

- **Status:** complete
- **Baseline:** main at aaefc6592e4df8f0f4b04a9fd33653aded868419
- **Depends on:** 03-A through 03-D landed
- **Goal:** close the post-03-D behavioral and product-surface residuals without reopening the removed Expert/Calendar authority vertical.
- **Exit:** every 03-E acceptance item passed; Checkpoint 03 is re-closed and Checkpoint 04 is next. The 03-E commits are recorded in the [Checkpoint 03 index](03-expert-registry-and-access-authority.md).

03-A through 03-D established the correct ownership topology. Do not redesign it in this slice.

~~~
Registry
  package / installation / assignment / Expert private state

Connections
  current connection / selected resources / source health

Access
  DataAccessGrant
  CalendarGrantPolicy
  exact mutation / CAS semantics

Context
  source acquisition
  exact ContextDependency provenance
~~~

03-E is a bounded correctness and cutover closure. It fixes four findings from the post-implementation audit:

1. remote Calendar binding rejects valid sibling resource grants before resource matching;
2. native Calendar mutation APIs re-read the current GrantAuthority and use it as their own expected value instead of honoring the state the Person reviewed;
3. ConsumerPolicyAuthority lifecycle is not consistently tied to semantic review changes and one native path masks policy-load failures with Option conversion;
4. the old Calendar-Expert management UI/API was deleted, but native Calendar Observe management was not replaced by an Access-owned public path.

Remote Calendar already has an Access-owned product gateway. Preserve it and fix its exact-grant/CAS semantics. Native Calendar needs the minimal Access-owned explicit review/pause/remove surface that Checkpoint 03 originally required. Do not implement Checkpoint 04's final connection-time grant ceremony or Use with Floe switch.

---

## 1. Baseline anchors and confirmed defects

Symbols are authoritative. Re-find line numbers after each semantic commit.

| File | Current symbol / surface | Residual |
|---|---|---|
| crates/adapters/vault/src/vault/remote_calendar_grants.rs | remote_calendar_grant_binding | rejects a second grant sharing connector/connection/source authority before testing the requested resource |
| crates/modules/context/src/application/remote_sources.rs | read_remote_calendar_view | Access already selects one active resource/consumer grant before the binding lookup; the binding lookup must not reintroduce source-wide uniqueness |
| crates/modules/access/src/application/personal_read.rs | active_resource_grant | useful precedent: ambiguity is defined among grants that actually admit the requested resource/consumer |
| crates/modules/access/src/application/remote_calendar.rs | review_and_activate_remote_calendar_grant | always calls activate_calendar_grant with a fresh GrantId and no expected authority, so repeated review can create an exact-resource duplicate instead of updating the reviewed grant |
| crates/modules/access/src/ports/remote_grants.rs | RemoteGrantStore calendar methods | current port cannot express reviewed existing-grant expectation for Calendar review |
| crates/adapters/vault/src/vault/calendar_grants.rs | review_native_calendar_grant | existing grant mutation uses current.authority() loaded inside the operation rather than a caller-reviewed expected authority |
| same | pause_native_calendar_grant / revoke_native_calendar_grant | re-read current grant and use its current authority, so a stale product decision cannot be distinguished from a fresh one |
| same | previous_policy = calendar_grant_policy(...).await.ok() | treats missing policy and storage/decode failure as the same state |
| crates/adapters/vault/src/vault/remote_calendar_grants.rs | review_and_activate_remote_calendar_grant | writes the supplied ConsumerPolicyAuthority unchanged; semantic policy change is not centrally derived |
| crates/adapters/vault/src/vault/calendar_grant_policy.rs | CalendarGrantPolicy | correct minimal persistence shape; keep it minimal |
| crates/app/src/local_access_services.rs | LocalAccessCommand | Personal and Contacts mutations only; Calendar has subject preview query but no Observe mutation/overview |
| crates/bindings/protocol/src/dto/commands.rs | AppCommandDto | no Access-owned native Calendar configure command |
| crates/bindings/protocol/src/dto/queries.rs | AccessCalendarPreview | native Calendar exposes subject preview only |
| apps/client/lib/app/runtime/local_owner_gateways.dart | local Access owner gateway | no native Calendar Observe review/pause/remove operation |
| apps/client/lib/features/connections/application/remote_access_gateway.dart | RemoteAccessGateway | remote Calendar already has Access-owned preview/review/pause/status; retain this owner |
| apps/client/lib/features/connections/presentation/server_connector_panel.dart | remote Calendar grant controls | keep the remote owner surface; only change fields needed for exact CAS |

Do not restore any of:

~~~
CalendarExpertSetup
CalendarExpertOverview
CalendarViewBinding
SourceGrants
experts.calendar.*
AgentCalendarExpertController
AgentCalendarSettings
calendar_grant_mappings
remote_calendar_grant_mappings
~~~

---

## 2. Final 03-E contracts

### 2.1 Grant identity and ambiguity

A Calendar read or review must distinguish:

- source identity;
- exact Calendar resource;
- exact consumer;
- grant identity;
- grant authority;
- consumer-policy authority.

Sibling grants are valid:

~~~
source S
  grant A -> calendar A
  grant B -> calendar B
~~~

A read of calendar A must select grant A and ignore grant B.

Ambiguity is fail-closed only when more than one current grant can satisfy the same exact admission requirement. Do not treat two different resources on one remote source as an ambiguity.

### 2.2 Mutation expectation

A product decision that changes an existing grant must carry the exact grant identity and GrantAuthority that the Person reviewed.

Fresh create:

~~~
expected_grant_id = None
expected_grant_authority = None
~~~

Existing update/pause/remove:

~~~
expected_grant_id = Some(...)
expected_grant_authority = Some(...)
~~~

Mixed Some/None is invalid input.

The backend still reloads current owner state. Reloading current state is for validation, not for manufacturing a new expected value.

Forbidden pattern:

~~~
current = load_grant()
mutate(current.id, current.authority, ...)
~~~

when the operation represents an earlier user review.

Required pattern:

~~~
reviewed_expected = request.expected_grant
current = load_current_grant()
assert current.id/authority == reviewed_expected
mutate(reviewed_expected.id, reviewed_expected.authority, ...)
~~~

### 2.3 ConsumerPolicyAuthority

CalendarGrantPolicy remains:

~~~
CalendarGrantPolicy
  grant_id
  person_id
  consumer_policy
  reviewed_native_subject_fingerprint?
~~~

Do not add source, scope, Registry ids or duplicated consumer lists to this row.

For one existing grant:

- an exact semantic no-op review keeps ConsumerPolicyAuthority stable;
- a reviewed semantic policy change advances ConsumerPolicyAuthority;
- a new grant gets a new ConsumerPolicyAuthority;
- pause/revoke alone changes GrantAuthority, not consumer policy;
- missing/corrupt policy is an error, not a reason to silently mint a replacement authority.

For this checkpoint, define semantic review equality using the owner facts that were approved:

- same GrantSourceBinding;
- same GrantScope, including consumers/resources/purpose/processing;
- same reviewed native subject fingerprint for native Calendar;
- no native subject for remote Calendar.

This intentionally gives a simple rule: only a truly identical review is a policy no-op.

### 2.4 Native product surface

Native Calendar Observe management is Access-owned.

Conceptual owner contract:

~~~
CalendarAccessOverview
  provider
  connection_id
  selected_resources
  source_authority
  grant_id?
  grant_authority?
  consumer_policy?
  state
  review_required

CalendarAccessChange
  Review {
    connection_id
    selected_resources
    expected_source_authority
    expected_native_subject_fingerprint
    expected_grant_id?
    expected_grant_authority?
  }
  Pause {
    grant_id
    expected_grant_authority
  }
  Remove {
    grant_id
    expected_grant_authority
  }
~~~

Names may differ. The semantics may not.

Flutter never supplies the canonical Calendar consumer list or a caller-composed GrantScope. App product composition continues to derive the consumers through calendar_first_party_consumers().

OS Calendar permission and Day's Calendar connection/import state remain separate from Observe/DataAccessGrant.

---

## 3. 03-E1 — exact Calendar grant and policy mutation primitives

Start in Access/Vault. Do not add App or Flutter surface until the owner operations are correct.

### 3.1 Centralize policy evolution

Create one semantic helper at the Access/Vault ownership boundary that receives:

- previous DataAccessGrant, if any;
- previous CalendarGrantPolicy, if any;
- next source;
- next scope;
- next reviewed native subject, if any.

It returns the ConsumerPolicyAuthority to persist.

Rules:

1. no previous grant and no previous policy -> new authority;
2. previous grant exists but policy is missing -> fail closed;
3. policy exists for a different grant/person -> fail closed;
4. exact semantic no-op -> preserve authority;
5. same grant with changed source/scope/native subject -> advance authority;
6. advance overflow -> BudgetExceeded or the existing bounded authority failure;
7. no catch-all .ok() around policy reads.

Do not make ConsumerPolicyAuthority depend on Registry/package state.

### 3.2 Native grant mutation API

Rewrite native review persistence so it accepts an explicit expected grant pair.

Suggested store-level signature semantics:

~~~
review_native_calendar_grant(
  current source facts,
  target resources,
  canonical consumers,
  reviewed subject,
  expected_grant: Option<(GrantId, GrantAuthority)>
)
~~~

Selection:

- exact current source has zero grant:
  - expected must be None;
  - create and activate one grant;
  - persist a new CalendarGrantPolicy atomically.
- exact current source has one grant:
  - expected must be Some and match id + authority;
  - mutate that grant using the supplied expected authority;
  - evolve CalendarGrantPolicy by the rules above.
- exact current source has multiple grants:
  - fail closed; native Calendar is one selected-resource-set grant at this checkpoint.

Pause/remove must accept grant_id + expected authority and verify the loaded grant belongs to the current native Calendar source before mutation.

Do not pause/revoke by looking up the source and then substituting the freshly loaded authority.

### 3.3 Atomicity

Grant mutation and CalendarGrantPolicy mutation remain one transaction.

Do not hold a Vault transaction while waiting for EventKit/native subject I/O.

Native sequence remains:

~~~
load current connection
-> obtain fresh subject outside DB transaction
-> compare with reviewed expected subject
-> reload/validate current connection/source authority
-> transaction:
     validate reviewed grant expectation
     mutate/create DataAccessGrant
     evolve CalendarGrantPolicy
-> return overview
~~~

### 3.4 E1 tests

Add focused Vault/Access tests:

- fresh native review with no expected grant creates one active grant + policy;
- existing native review without an expected pair is rejected;
- mixed expected id/authority is rejected;
- stale expected GrantAuthority after pause is Conflict;
- stale pause/remove authority is Conflict;
- exact no-op review preserves ConsumerPolicyAuthority;
- resource/scope change advances ConsumerPolicyAuthority;
- consumer change advances ConsumerPolicyAuthority;
- native subject fingerprint change advances ConsumerPolicyAuthority;
- policy row corruption/missing row for an existing grant fails closed instead of being treated as first review;
- pause alone does not advance ConsumerPolicyAuthority;
- old ContextDependency fails after the relevant GrantAuthority/policy change.

### 3.5 E1 exit gate

Do not continue while native user mutations can use a freshly loaded authority as their expected authority.

Search the Calendar grant mutation path for:

~~~
grant.authority()
current.authority()
.await.ok()
~~~

Every occurrence used as mutation expectation or policy-load suppression must be justified or removed.

---

## 4. 03-E2 — remote Calendar resource-exact selection and reviewed CAS

Remote already has an Access-owned product path. Fix it; do not create a second local-style gateway.

### 4.1 Binding/read selection

The already-admitted resource grant selected by read_remote_calendar_view is the authoritative grant identity for that read.

Preferred direction:

- carry the admitted grant id into calendar_grant_binding; or
- otherwise filter source candidates by exact requested resource before checking uniqueness.

Do not fail because a sibling grant names another resource.

The binding must still verify:

- person;
- connector;
- connection;
- current source authority;
- requested resource;
- grant state/review flag;
- current CalendarGrantPolicy;
- later admit_remote_view_binding equality with the grant selected by Access.

If two grants can satisfy the same exact resource admission, return Conflict.

### 4.2 Existing-grant discovery for review

Repeated review of the same remote source/resource must not blindly create another GrantId.

Extend the RemoteGrantStore boundary with an owner-correct exact Calendar grant lookup/expectation. Do not make Access depend on Vault SQL details.

The review path must distinguish:

~~~
no exact current grant
  -> reviewed expectation must say none
  -> create

one exact current grant
  -> reviewed expectation must identify id + GrantAuthority
  -> mutate/review that grant under CAS

multiple exact current grants
  -> Conflict
~~~

A sibling resource grant is not an exact current grant for this operation.

### 4.3 Preview/review expectation

Remote preview is the user-review boundary. It must expose enough opaque current grant expectation for the subsequent review command to detect a stale decision.

Extend the existing RemoteCalendarGrantPreview/DTO/Flutter model with optional current grant expectation, for example:

~~~
grant_id?
grant_authority?
consumer_policy?
~~~

The review command echoes the reviewed expectation. The backend re-previews the signed producer/source and reloads the current exact resource grant.

Reject when:

- preview said no grant but one appeared before review;
- preview named a grant that disappeared;
- grant id changed;
- GrantAuthority changed;
- expected policy authority changed;
- producer/source/connection/resource changed.

Do not let Flutter construct or advance authority values. They are opaque expected values only.

### 4.4 Remote policy lifecycle

When updating the exact existing remote grant, use the 03-E1 policy evolution rules.

Fresh remote grant -> new ConsumerPolicyAuthority.

Exact no-op -> stable.

Changed reviewed source/scope/consumers -> advance.

Policy persistence remains in calendar_grant_policies and remains atomic with the DataAccessGrant mutation.

### 4.5 E2 tests

Required regressions:

- source S with resource A grant and resource B grant: read A succeeds with A;
- same fixture: read B succeeds with B;
- same source, same resource, two admissible active grants -> Conflict;
- unrelated sibling resource does not cause Conflict in calendar_grant_binding;
- reviewing resource A twice reuses/updates A rather than creating an exact-resource duplicate;
- preview says no grant, concurrent grant appears, review -> Conflict;
- preview names authority E1, concurrent pause advances to E2, review with E1 -> Conflict;
- source authority drift between preview and review -> fail closed;
- consumer-policy change advances policy;
- no-op re-review preserves policy;
- reopen preserves both sibling resource grants and their policy rows.

### 4.6 E2 exit gate

The following state must be valid:

~~~
connection/source S
  calendar A -> active grant GA
  calendar B -> active grant GB
~~~

and both reads must survive the before/after binding revalidation in Context.

---

## 5. 03-E3 — restore native Calendar Observe management under Access

03-C deleted the wrong Expert-owned surface correctly, but native Calendar currently exposes only subject preview. Add the minimum Access-owned replacement.

### 5.1 Access owner contract

Place Calendar Observe change/overview semantics under Access/App owner language, not Experts.

Do not reuse names/types from the deleted CalendarExpertSetup vertical.

The backend is authoritative for:

- Person/device from CallerContext;
- current CalendarConnection;
- current selected Calendar resources;
- canonical first-party consumers;
- current DataAccessGrant;
- current CalendarGrantPolicy;
- fresh native subject.

### 5.2 App local access surface

Extend LocalAccessCommand/LocalAccessInspection/LocalAccessResult with native Calendar Observe semantics.

Recommended public semantics:

~~~
query access.calendar.inspect
  -> CalendarAccessOverview

query access.calendar.preview
  -> existing fresh CalendarSubjectPreview

command access.calendar.configure
  -> Review / Pause / Remove
  -> CalendarAccessOverview
~~~

The exact wire names may remain those names because they are Access-owned. They must not carry Registry instance/revision/setup/view/assignment/install ids.

Review command carries only:

- connection id / selected resource ids;
- expected source authority;
- expected native subject fingerprint;
- optional reviewed grant id + GrantAuthority.

It does not carry consumers, purpose, processing or GrantScope.

### 5.3 Worker/Vault composition

Add a Calendar Access worker action only if needed by the existing owner-operation framework.

The action payload is an Access/App type. It is not floe_experts state.

Execution sequence:

1. verify CallerContext person/device;
2. load current Calendar connection;
3. derive canonical consumers in App through calendar_first_party_consumers();
4. inspect/observe fresh native subject as required;
5. invoke the 03-E1 CAS-bound grant operation;
6. return an Access-owned overview.

Do not hold the Vault transaction across native I/O.

### 5.4 Protocol / FFI

Add/update only the owner-aligned fields needed for:

- access.calendar.inspect;
- access.calendar.configure;
- LocalAccessResult calendar access overview.

Update protocol/FFI validation so callers cannot send:

~~~
setup_id
registry_revision
view_handle
expert_assignment_id
expert_installation_id
consumer list
GrantScope
~~~

Keep experts.calendar.* rejected.

### 5.5 Flutter

Do not recreate AgentCalendarExpertController or AgentCalendarSettings.

Add a narrow connection/access gateway, or extend the existing local owner Access gateway, for:

- inspect current Calendar Observe state;
- request subject preview;
- submit reviewed explicit grant;
- pause;
- remove/revoke;
- change selected resources through a fresh review.

ConnectorScreen may expose the minimal controls needed to preserve the explicit Checkpoint 03 behavior.

Do not implement:

- automatic Observe grant on connection;
- final Use with Floe toggle;
- chat permission interaction;
- interaction resume;
- external processing consent UX.

Those remain Checkpoints 04 and 05.

### 5.6 Distinguish OS/connection/Observe

Tests and UI state must keep these separate:

~~~
OS permission
Calendar connection/resource selection
DataAccessGrant Observe state
Action/write authority
~~~

A connected Calendar with no Observe grant is a valid state.

Pausing Observe must not disconnect Calendar or revoke OS permission.

Removing Observe must not delete external calendars, credentials or Day data.

### 5.7 E3 tests

Rust/App/protocol:

- inspect with connected Calendar and no grant -> NeedsReview overview;
- fresh reviewed preview creates active grant;
- configure payload with Registry/setup fields rejected;
- caller-supplied consumers/scope rejected by wire shape;
- stale GrantAuthority -> Conflict;
- pause leaves connection intact;
- remove/revoke leaves connection intact;
- scope change requires fresh reviewed subject and expected authority;
- Registry revision unchanged across all Calendar access operations;
- experts.calendar.install / inspect remain unknown.

Flutter:

- device Calendar detail can inspect Observe state through Access owner;
- explicit review uses the exact preview fingerprint and grant expectation;
- pause does not disconnect Calendar;
- resource change re-previews/reviews rather than silently mutating;
- no AgentCalendarExpert* production symbol returns.

### 5.8 E3 exit gate

There is one product path for native Calendar Observe management:

~~~
Connector / Access UI
-> App Access owner
-> current Connections state
-> fresh Context/native subject
-> Access/DataAccessGrant + CalendarGrantPolicy
~~~

No product caller reaches Vault Calendar grant methods directly.

---

## 6. 03-E4 — conversation and evidence regressions

The residual fixes must not regress the converged Expert runtime.

Required tests:

### Schedule with no Observe grant

- Schedule remains in Directory;
- delegation is still possible;
- Calendar Context read reports the existing recoverable access state;
- Registry revision is unchanged.

### Schedule with active grant

- actual consumer is floe.builtin.schedule;
- successful read records ContextDependency against exact grant/policy;
- evidence_id remains dependency.observation_id.

### /focus

- proposal inspection/publication reauthorizes current DataAccessGrant;
- reauthorizes current CalendarGrantPolicy;
- source/resource drift fails;
- stale policy authority fails;
- Act authority remains separate.

E4 investigation findings (recorded before fixing, per the stop-condition
rule). No governed (`calendar.observe`/`lease`/`timeline`) proposal path had
test coverage: `expert_proposal_dependency` reauthorized the DataAccessGrant
but never the CalendarGrantPolicy, so a subject-fingerprint re-review (which
advances consumer policy while leaving GrantAuthority untouched) stayed
publishable. The bounded E4 cleanup adds the `calendar.*` policy recheck to
`expert_proposal_dependency`, matching the execution-path dispatch
(`validate_current_authority_in_transaction`).

Writing those regressions exposed a second structural gap: the proposal gate
(`with_proposal_evidence`) holds an Immediate vault transaction while the
governed observe check — and, for inspection, the operation closure — resolve
the proposal dependency through `read_turn_coverage`, which opens a second
Immediate transaction on a new connection. Turso reports
`Busy("database is locked")`, so every governed proposal inspection failed
with StorageUnavailable before any reauthorization logic ran. (Publication
itself runs outside the gate after the evidence fetch, so only the gate-side
reads nest.) The bounded E4 cleanup performs that single coverage SELECT
without opening a write transaction — a WAL snapshot read with identical
results — so gate-nested reauthorization is pure reads. No gate, trait, or
isolation redesign: writers still serialize through Immediate transactions,
and the same checks run in the same order.

### Remote sibling resources

Run the complete Context path, not only Vault lookup:

~~~
active_resource_grant
-> signed preview
-> calendar_grant_binding
-> provider read
-> current grant/source/policy recheck
-> ContextDependency
~~~

A/B sibling grants must each complete this path independently.

---

## 7. 03-E5 — residual audit, verification and documentation convergence

### 7.1 Residual searches

Run from repository root.

No Expert Calendar authority revival:

~~~sh
rg -n 'CalendarExpertSetup|CalendarExpertOverview|CalendarViewBinding|SourceGrants|experts\.calendar\.|AgentCalendarExpert|AgentCalendarSettings' crates apps/client
~~~

Expected: zero production matches.

No caller-synthesized expected authority in Calendar mutation:

~~~sh
rg -n 'review_native_calendar_grant|pause_native_calendar_grant|revoke_native_calendar_grant|review_and_activate_remote_calendar_grant|calendar_grant_binding' crates
~~~

Inspect every mutation call. Existing-grant mutation must receive a reviewed expected authority rather than manufacturing one from a fresh load.

Policy error suppression:

~~~sh
rg -n 'calendar_grant_policy\(.*\)\.await\.ok\(\)|calendar_grant_policy' crates/adapters/vault
~~~

Expected: no policy read on a mutation/reauthorization path converts arbitrary failure to absence.

Wire ownership:

~~~sh
rg -n 'access\.calendar\.|calendar_access|CalendarAccess' crates/bindings crates/app apps/client
~~~

Every product management match must be Access/Connections-owned. No Registry/setup identity.

Consumer authority:

~~~sh
rg -n 'calendar\.expert|calendar_first_party_consumers' crates apps
~~~

calendar.expert may remain only in explicit rejection/history fixtures. calendar_first_party_consumers remains the single App product-composition source for first-party built-in consumers.

### 7.2 Targeted verification

Run after each semantic commit:

~~~sh
cargo test -p floe-access
cargo test -p floe-vault
cargo test -p floe-app
cargo test -p floe-protocol
git diff --check
~~~

Use actual package names if they differ and report the exact commands.

Run the focused remote/native Context tests that cover Calendar acquisition and dependency reauthorization.

### 7.3 Broad gate

Before re-closing Checkpoint 03:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

If server code changes:

~~~sh
cd server
go test -race ./...
go vet ./...
~~~

If EventKit/native provider code changes, run the repository's focused macOS native Calendar tests. Otherwise record why they were not separately required.

### 7.4 Persistence acceptance

Use an isolated fresh development profile.

Verify:

- fresh native review/reopen;
- native pause/review under stale and current authority;
- remote resource A/B grants/reopen;
- exact duplicate ambiguity fails closed;
- policy corruption/missing row fails closed;
- obsolete old mapping schema remains rejected and is never migrated.

Never reset the operator's normal profile.

### 7.5 Documentation

After code is green:

- mark 03-E complete;
- mark Checkpoint 03 complete again;
- record the 03-E completion commit(s);
- update this directory README current snapshot;
- mark Checkpoint 04 next;
- update current architecture docs only if implementation changed the durable ownership/path beyond what they already say.

Do not write the transient 03-E bug list into durable architecture docs.

---

## 8. Stop conditions

Stop and update this plan rather than adding a workaround if implementation appears to require any of the following:

1. restoring CalendarExpertSetup or any Expert-owned Calendar permission surface;
2. making Registry revision participate in source permission;
3. treating sibling remote resources as one source-wide grant;
4. silently selecting one of multiple exact-resource grants;
5. re-reading the current GrantAuthority and using it as the expected value for a previously reviewed mutation;
6. allowing Flutter to choose consumers, GrantScope, purpose or processing restriction;
7. duplicating source/scope/consumers into CalendarGrantPolicy;
8. masking CalendarGrantPolicy load/decode/storage failure as a missing policy;
9. holding a Vault transaction while waiting for native/provider I/O;
10. implementing Checkpoint 04 automatic connection-time Observe or Use with Floe;
11. implementing Checkpoint 05 chat permission interaction/resume;
12. changing ActionAuthority as part of Observe cleanup;
13. adding a compatibility experts.calendar.* alias.

If the existing remote preview contract cannot express reviewed existing-grant CAS, change that Access-owned preview/review contract directly. Do not bypass CAS to avoid a wire change.

---

## 9. Commit sequencing

Prefer small semantic commits:

1. **03-E1** — Calendar grant expectation + ConsumerPolicyAuthority lifecycle primitives;
2. **03-E2** — remote resource-exact selection and reviewed existing-grant CAS;
3. **03-E3** — native Access-owned management path through App/protocol/FFI/Flutter;
4. **03-E4** — integration regressions and any bounded cleanup exposed by them;
5. **03-E5** — residual audit, full verification and document convergence.

A compile break between commits is acceptable only within the active semantic slice. Do not add compatibility wrappers to keep obsolete callers compiling.

---

## 10. Definition of done

- [x] sibling remote Calendar resources can hold separate grants and each read succeeds under its own exact grant.
- [x] multiple grants that admit the same exact remote resource fail closed.
- [x] repeated remote review updates/reviews the exact current grant rather than blindly creating a duplicate.
- [x] remote review is bound to the grant/policy expectation the Person reviewed.
- [x] native review/update is bound to reviewed GrantId + GrantAuthority CAS.
- [x] native pause/remove is bound to reviewed GrantAuthority and current source ownership.
- [x] no Calendar mutation manufactures expected authority from a freshly loaded grant.
- [x] ConsumerPolicyAuthority is stable on exact semantic no-op review.
- [x] ConsumerPolicyAuthority advances on reviewed source/scope/consumer/native-subject semantic change.
- [x] CalendarGrantPolicy load/corruption failure is never converted to first-review absence.
- [x] grant + policy mutation remains atomic.
- [x] no DB transaction spans EventKit/provider I/O.
- [x] native Calendar has an Access-owned inspect/review/pause/remove public path.
- [x] native Calendar Access public payload contains no Registry/setup identity and no caller-supplied consumer policy.
- [x] remote Calendar remains on the existing Access-owned RemoteAccess path.
- [x] pausing/removing Observe does not disconnect the Calendar connection or alter OS permission.
- [x] Registry revision is unaffected by Calendar Observe mutations.
- [x] old Calendar-Expert APIs/wire/UI remain deleted.
- [x] Schedule missing-access and successful Calendar paths still pass.
- [x] /focus evidence and current dependency reauthorization still pass.
- [x] legacy grant mapping tables remain rejected and never migrated.
- [x] residual searches are clean with only explicitly justified historical/rejection matches.
- [x] Rust/architecture/FFI/Flutter/macOS gates pass.
- [x] Go gates pass if server changed. (Server unchanged in 03-E: no Go gate required.)
- [x] Checkpoint 03 index/README are reconverged and only then mark Checkpoint 04 next.

---

## 11. Required agent report

Report 03-E only after all items above are complete:

1. **03-E commits** — SHA and semantic scope for each commit.
2. **Remote selection result** — exact resource selection and duplicate behavior.
3. **CAS result** — native and remote reviewed expectation behavior, including stale-operation tests.
4. **Policy result** — exact ConsumerPolicyAuthority no-op/advance rules and corruption handling.
5. **Native product surface** — final App/wire/Flutter Access-owned path.
6. **Authority topology** — proof Registry/Experts did not regain source authority.
7. **Residual audit** — exact commands and every justified remaining match.
8. **Verification** — exact test/build commands and results.
9. **Persistence acceptance** — fresh/reopen, sibling resources, stale authority, old-schema rejection.
10. **Blocker** — any unmet definition-of-done item keeps Checkpoint 03 open.

Do not mark Checkpoint 03 complete merely because the broad test suite passes. The four post-03-D behavioral findings must be covered by direct regression tests.
