# 05: Eliminate ConsumerPolicyAuthority and duplicate policy storage

Prerequisite: 04 complete.

This checkpoint removes the second permission epoch. GrantAuthority becomes the standing grant-policy authority; SourceAuthority remains the source/resource authority. Review policy digest remains compare-only evidence.

## Exit state

1. ConsumerPolicyAuthority no longer exists in contracts, Access, Context, Vault, provider wire, App, FFI, Flutter or Go;
2. ContextDependency carries no consumer_policy field;
3. CalendarGrantPolicy storage is deleted;
4. remote_view_grant_mappings policy epoch storage is deleted, and the mapping itself is deleted if logical resource lookup makes it redundant;
5. source admission/release proves GrantAuthority + SourceAuthority + consumer/purpose/processing directly;
6. pending review drift uses policy_digest, not a policy authority.

## 05-A: remove the contract type

Primary:

- crates/contracts/context/src/lib.rs

Delete ConsumerPolicyAuthority and all constructors/accessors/advance/default behavior.

Remove consumer_policy from ContextDependency and update:
- constructor;
- validation;
- canonical serialization;
- equality/identity;
- persisted-size tests.

No deprecated alias.

## 05-B: Access/replay/release simplification

Update:

- application/admission.rs;
- dependency.rs;
- release.rs;
- calendar_read.rs;
- remote_view.rs;
- personal_read.rs;
- ports whose bindings currently return a policy authority.

Delete:
- ReplayRequest.consumer_policy;
- ReplayTrust::consumer_policy_is_current;
- RemoteGrantBinding.consumer_policy;
- CalendarReadAccessAdmission.consumer_policy;
- policy-specific dependency matching.

Replace every safety check with the authority that actually owns the fact:

- grant scope/consumer/purpose/processing drift -> GrantAuthority + current grant contents;
- source/resource drift -> SourceAuthority/current connection;
- exact recipient drift -> contextual recipient authority;
- observation freshness -> Context lease/dependency.

Do not silently drop a check; map it to the correct owner first.

## 05-C: delete Calendar policy storage

Delete crates/adapters/vault/src/vault/calendar_grant_policy.rs and its schema/tables:

- calendar_grant_policy_schema;
- calendar_grant_policies;
- policy_incarnation/policy_epoch fields.

Native subject fingerprint has already moved to Connections in 01. There is no remaining semantic fact requiring this table.

Remove initialization/validation/export code and tests.

## 05-D: simplify remote grant persistence

Inspect crates/adapters/vault/src/vault/remote_view_grants.rs.

With stable GrantSourceBinding + logical View ResourceHandle, a separate mapping that duplicates:

- grant_id;
- view_id;
- source;
- scope

should be unnecessary.

Preferred final state:

- find the exact DataAccessGrant by stable source + logical resource;
- detect ambiguity directly in Access-grant queries;
- activate/mutate via generic access_grants transactional primitives;
- delete remote_view_grant_mappings and the file if no real transport/storage boundary remains.

If one tiny remote repository implementation remains necessary, it must not persist duplicated scope/source or a second epoch.

No compatibility read of old mapping tables.

## 05-E: provider/server protocol

Remove policy incarnation/epoch from:

- Rust AuthorizedViewRead / authorization request;
- provider adapter request DTOs;
- Go ViewAdmission / authority Request PolicyReference;
- signed admission/release material;
- tests/fixtures.

The server still receives:
- exact grant ID/GrantAuthority;
- source identity/SourceAuthority;
- requested consumer/purpose;
- logical resource;
- query digest/bounds.

That is sufficient to bind the release to the client's current authority when server issuer/proof checks remain intact.

## 05-F: review policy digest

Retain/rework App first-party Observe policy fingerprint as policy_digest with one meaning:

~~~
digest(canonical intended logical View GrantScope + product View identity)
~~~

It must not include:
- Expert assignment selection;
- current source resources;
- SourceAuthority;
- GrantAuthority.

Those are separately reviewed expectations.

Internally prefer a fixed 32-byte digest. Wire may serialize canonical lowercase hex if needed. Conversation stores it as opaque review identity.

Rename policy_fingerprint where practical to policy_digest to stop implying authority.

## 05-G: Conversation interaction shape

Change ReviewedBundleMember/LiveMember and resolution:

Delete policy_authority.

Keep:
- member logical View ID/resource;
- policy_digest;
- expected grant state;
- source revision at the connection/target level or per member only where multiple distinct sources genuinely exist.

Do not duplicate one connection SourceAuthority into every member if the bundle is one connection.

Review resolution still fails on:
- changed connection/source identity;
- changed policy digest;
- changed expected grant;
- changed producer/native source evidence.

## Tests

Required replacements:

1. grant scope consumer change advances GrantAuthority and invalidates old dependency without a policy epoch;
2. source change invalidates old dependency through SourceAuthority only;
3. no-op re-review preserves GrantAuthority;
4. policy digest change invalidates pending review before mutation;
5. random extension install does not change digest;
6. remote admit/release rejects stale GrantAuthority;
7. remote admit/release rejects stale SourceAuthority;
8. old persisted policy table/profile is not migrated;
9. FFI/Flutter no longer parse consumer_policy.

Delete all tests whose only invariant is policy epoch advance/equality.

## Residual audit

Zero production matches:

~~~
ConsumerPolicyAuthority
consumer_policy
policy_incarnation
policy_epoch
calendar_grant_policies
calendar_grant_policy_schema
evolve_calendar_consumer_policy
~~~

String occurrences in historical ADR text may remain until 09 only if clearly historical.

## Verification

~~~
cargo test -p floe-context-contract
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-provider-adapters
cargo test -p floe-conversation
cargo test -p floe-app
(
  cd server
  go test ./...
)
python3 tools/architecture/check_boundaries.py
git diff --check
~~~
