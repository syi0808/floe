# 03: Native Calendar vertical cutover

Prerequisite: 02 complete.

This checkpoint is the first complete product vertical of the new model. It removes Calendar leaf resources from Expert configuration and Access permission, resolves all current resources from Connections at read time, and removes the consumer-intersection defect and four-calendar authorization cap.

## Exit state

1. one native Calendar connection exposes one calendar.timeline source candidate;
2. Expert bindings select the connection/View, never individual calendar IDs;
3. first-party Calendar grant consumers do not depend on Registry source selection;
4. one logical calendar.timeline grant authorizes the connection View;
5. Context resolves the connection's current resources at acquisition time;
6. 11 current calendars work through grant + read;
7. resource changes advance SourceAuthority but leave grant and Expert binding unchanged.

## 03-A: Calendar source candidate/binding

Change crates/modules/context/src/application/source_candidates.rs.

Current behavior loops connection.calendars and emits one candidate per calendar. Replace it with exactly one candidate per usable Calendar connection:

- connector_id: current native Calendar connector;
- connection_id: current connection;
- execution_owner_id: connection owner/device;
- capability_id: calendar.timeline;
- resource: canonical calendar.timeline:<connection-id>;
- contract_version unchanged.

Candidate title/detail identifies the connection/provider, not a leaf calendar.

Rewrite the current candidate-addition test. Required regression:

- resources [A] -> candidate X;
- resources [A,B,...] -> the same candidate X;
- source candidate ID does not change solely because the resource set changes.

Update expert_binding_settings and default first-party setup so Calendar binding remains stable across resource changes.

Delete leaf-calendar candidate/picker UI/test semantics. A future explicit restricted subset is out of scope and must not be simulated with old candidates.

## 03-B: first-party policy simplification

Refactor crates/app/src/first_party_observe.rs.

Delete:

- selected_shipped_consumers;
- RegistrySnapshot/AgentRegistry restore dependency for consumer composition;
- assignment enabled/binding exact-target scan;
- native Calendar loop over resources;
- consumer intersection;
- resource-specific policy fingerprint generation.

Build the default built-in consumer set only from trusted shipped manifests that declare the capability/View. Extension packages never join this set.

Manager assistant is included only for Views the Manager product policy directly reads. Do not add assistant to Calendar merely to avoid an empty list if Calendar is not a direct Manager View.

The policy digest is derived from the intended logical View GrantScope/product policy and contains no Expert assignment selection state.

A newly installed third-party Expert must leave the digest and consumer set unchanged.

## 03-C: native Calendar grant semantics

Update native Calendar grant review/admission in Access/Vault/App.

Grant resource:

~~~
calendar.timeline:<connection-id>
~~~

not calendar IDs.

The review operation obtains current source/resource truth from Connections. The client does not submit a grant resource list.

Grant activation must reject:
- wrong Person/connection/connector/owner;
- stale reviewed grant expectation;
- current source unavailable/unverified.

It must not reject because two leaf calendars have different Expert bindings; such bindings no longer exist.

selected_resources/granted_resources projection remains temporarily until 07 only if needed by unchanged wire, but granted_resources must no longer be treated as an independent permission list. Mark it for deletion and do not add new callers.

## 03-D: native read path

Change:

- crates/modules/context/src/application/native_calendar.rs
- native_calendar_view.rs
- crates/modules/access/src/application/calendar_read.rs
- crates/modules/access/src/application/calendar_lease.rs
- provider/native Calendar access request construction.

Remove:
- expected_calendar_ids/selectable subset from Expert reads;
- selected_calendar_ids from NativeCalendarViewRead;
- calendar_ids <= 4 authorization limit;
- grant scope comparison against every calendar ID.

Read flow:

1. read current Connections Calendar source;
2. require usable connection/source authority;
3. copy its full sorted resource set for this acquisition;
4. ask native provider to validate/read exactly that set;
5. validate native subject/generation against Connections source identity;
6. authorize logical calendar.timeline grant for the requesting consumer;
7. re-read Connections state after acquisition;
8. record dependency:
   - logical calendar.timeline grant resource;
   - current SourceAuthority;
   - exact calendar IDs in source_resources.

Budgets remain item/byte/query/time budgets. Add a defensible connector/provider resource-count budget only if an actual OS/API limit requires it; do not restore 4 as permission semantics.

## 03-E: lease simplification

calendar_lease.rs currently serializes Calendar IDs into a special dependency helper. After ContextDependency has source_resources, move the generic provenance/lease binding into Context's SourceLeaseRegistry/SourceView path where possible.

Target:
- delete calendar_lease_dependency if it only duplicates ContextDependency construction;
- keep a Calendar query fingerprint struct only if query-specific identity needs it;
- do not retain an Access-owned Calendar lease abstraction solely for historical layout.

If complete genericization is blocked by a real native adapter difference, record the exact remaining function and remove it in 08; do not leave both generic and special dependency authorities.

## 03-F: executable acceptance

Required tests:

1. Use with Floe enable with eleven Calendar resources succeeds.
2. Schedule/another built-in consumers can share the logical Calendar grant without per-resource intersection.
3. resource [A] -> [A,B] advances SourceAuthority only.
4. grant ID/GrantAuthority unchanged across that resource update.
5. Expert binding revision/selection unchanged across that update.
6. next read requests A and B.
7. historical dependency on A-era SourceAuthority becomes stale.
8. third-party Expert install/binding does not gain Calendar grant.
9. paused grant blocks all current resources.
10. revoked/disconnected connection blocks read.
11. native subject/generation drift still fails closed.

Delete old tests asserting per-calendar Expert refs or four-calendar maximum.

## Residual audit

Production matches for these concepts must be zero after this checkpoint unless explicitly deferred to 07 presentation cleanup:

~~~
selected_shipped_consumers
native_calendar_policy_for_target
selected_calendar_ids
expected_calendar_ids
calendar_addition_produces_a_new_candidate
len() > 4
calendar resource intersection
SourceSelectionReference ... calendar_id
~~~

## Verification

~~~
cargo test -p floe-context source_candidates
cargo test -p floe-context native_calendar
cargo test -p floe-access calendar
cargo test -p floe-vault calendar
cargo test -p floe-app calendar
cargo test -p floe-app first_party_observe
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

On macOS run the real native Calendar adapter fixture required by the verification skill.
