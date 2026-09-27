# 08: Legacy purge and conformance closure

Prerequisite: 07 complete.

This checkpoint assumes the new product behavior works and removes every obsolete representation, special path and test that would make future agents support the old model.

## Exit state

1. one owner/path exists for connection resources, standing Observe, Expert connection selection and source acquisition;
2. Calendar-specific parallel grant stacks are gone;
3. consumer-policy epoch/state is gone;
4. no leaf Calendar permission/binding path remains;
5. no old storage/wire/test compatibility remains;
6. architecture checks prevent the principal regressions from returning.

## 08-A: production deletion list

Expected deletions (re-resolve before editing):

Whole files when caller-zero:
- crates/modules/access/src/application/remote_calendar.rs
- crates/adapters/vault/src/vault/remote_calendar_grants.rs
- crates/adapters/vault/src/vault/calendar_grant_policy.rs
- crates/modules/access/src/application/calendar_lease.rs if 03 fully genericized it
- Flutter native Calendar access grant model/gateway files if 07 replaced them.

Delete obsolete symbols:
- selected_shipped_consumers;
- native_calendar_policy_for_target;
- remote_policies_for_target target-selection variants;
- ConsumerPolicyAuthority;
- policy epoch/incarnation storage/wire fields;
- RemoteCalendarQuery/SignedCalendarPreview/RemoteCalendarSourceReference;
- Calendar leaf grant scope constructors;
- per-resource Calendar source candidate helpers;
- selected_resources/granted_resources dual projections;
- old Calendar grant mappings;
- remote Calendar special HTTP handlers.

Do not leave commented code, feature flags or deprecated aliases.

## 08-B: test deletion/rewrite audit

Delete tests whose only job is preserving old architecture:

- Calendar leaf candidate multiplicity;
- resource intersection consumer calculation;
- exact Calendar leaf IDs in GrantScope;
- four-calendar authorization maximum;
- ConsumerPolicyAuthority advance/replay;
- remote Calendar special grant store/protocol;
- Flutter old consumer_policy/granted_resources parsing;
- resource argument echo on ConnectionObserve.

Retain/rewrite safety invariants:

- Person/connection/owner isolation;
- producer pinning/signature;
- SourceAuthority drift;
- GrantAuthority CAS;
- pause/revoke;
- dependency freshness/provenance;
- exact-recipient transfer;
- linked interaction resume;
- Actions proposal/source fences;
- cancellation/deadline;
- uncertain external-write recovery.

A passing obsolete test is not a reason to preserve its production path.

## 08-C: dependency/public surface narrowing

Inspect Cargo manifests and public exports.

Remove dependencies that became unnecessary after:
- App first_party_observe stops reading Registry state;
- Day no longer owns connection authority;
- remote Calendar special modules disappear;
- Access no longer needs policy authority types.

Run tools/architecture/check_boundaries.py after every dependency removal group.

Narrow pub symbols to pub(crate)/private when no legitimate boundary uses them.

Do not create a shared utility crate for two functions that now have one owner.

## 08-D: machine conformance checks

Extend an existing architecture checker or add one focused companion checker only where source-level checks are valuable.

Useful invariants to enforce:

- ConsumerPolicyAuthority symbol absent;
- App first-party Observe production code does not inspect Expert binding selected resources;
- remote_calendar.rs / remote_calendar_grants.rs absent;
- Calendar source candidate production code does not loop Calendar leaf resources;
- product wire ConnectionObserve has no resource/calendar_ids argument;
- business modules do not import a built-in package to decide source permission;
- third-party extension IDs never enter default first-party consumer policy.

Do not encode fragile line numbers. Test the checker with positive/negative fixtures.

## 08-E: residual search matrix

Run concept searches over Rust, Go and Dart:

~~~
rg -n "ConsumerPolicyAuthority|consumer_policy|policy_incarnation|policy_epoch" .
rg -n "RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference|calendar_source_preview" .
rg -n "selected_shipped_consumers|native_calendar_policy_for_target" crates
rg -n "granted_resources|_observeResource" crates apps
rg -n "calendar_ids.*GrantScope|GrantScope.*calendar_id" crates
rg -n "calendar_addition_produces_a_new_candidate|maximum_sources.*calendar" crates apps
rg -n "calendar_grant_policies|remote_calendar_grant" crates
~~~

Every remaining match is one of:
- legitimate external/historical prose;
- current provider leaf acquisition detail;
- an explicit blocker that prevents checkpoint completion.

No "remove later" bucket.

## 08-F: targeted conformance scenario

Add one cross-owner test that would have failed under the original bug:

- one native Calendar connection;
- eleven resources;
- Schedule + another built-in with different historical per-leaf binding state fixtures removed from current Registry;
- one Use with Floe grant;
- current source read succeeds for the logical View;
- third-party Expert denied;
- resource change leaves grant/binding unchanged and stales old dependency.

This scenario should traverse Connections -> Access -> Context -> Expert host/App as close to product runtime as practical.

## Verification

Run affected package suites plus:

~~~
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
# new/extended source-semantic checker if added
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
(
  cd server
  go test ./...
  go test -race ./...
  go vet ./...
)
(
  cd apps/client
  flutter analyze
  flutter test
)
git diff --check
~~~

Do not declare 08 complete if residual legacy production symbols remain.
