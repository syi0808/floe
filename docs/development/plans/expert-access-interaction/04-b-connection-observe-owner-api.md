# Checkpoint 04-B — Connection-level Observe projection and owner API

- **Execution baseline:** 04-A completion on top of main 833b9f19
- **Depends on:** canonical first-party Observe policy
- **Goal:** create one connection-level product projection and mutation contract without adding a second authorization state.

---

## 1. Baseline problem

Today UI interprets several owner-specific structures:

- CalendarAccessOverview;
- PersonalAccessOverview;
- RemoteCalendarGrantOverview;
- RemoteViewGrantOverview;
- connection/provider/system status.

Each surface computes its own answer to "Can Floe use this connection?"

Checkpoint 04 needs one product meaning while keeping provider-specific authority underneath.

---

## 2. Product projection

Introduce an App-owned connection projection, conceptually:

~~~text
ConnectionObserveOverview
  connector_id
  connection_id
  status
  enabled
  selected_resources
  granted_resources
  opaque current/reviewed authorities needed for CAS
  issue/recovery kind
~~~

Names may differ.

### Status

Use a closed enum equivalent to:

~~~text
Active
Paused
NeedsReview
NeedsSystemAccess
ReconnectRequired
Unavailable
~~~

Do not expose raw AgentFailure as normal status.

### enabled

If included, enabled is derived from current admitted grants.

A partial bundle must be NeedsReview even if one member remains active.

### Resources

- selected_resources comes from Connections/provider/OS owner;
- granted_resources comes from Access;
- projection never treats one as the other.

---

## 3. No durable bundle authority

Do not add a stored bundle row only to back the switch.

A multi-grant connection bundle is reconstructed from:

- current connection identity;
- 04-A first-party policy;
- matching DataAccessGrant/policy records.

An operation receipt may exist for App operation lifecycle, but it is not authorization.

---

## 4. Product operations

Provide connection-level semantics equivalent to:

~~~text
inspect_connection_observe(connection_ref)

set_connection_observe(
  reviewed_overview,
  enabled
)

reconcile_after_connect(connection_ref)

reconcile_after_resource_change(
  reviewed/current expectation,
  current connection
)

prepare_disconnect_observe(
  reviewed/current connection
)
~~~

Do not create forwarding-only APIs; collapse operations if one owner command can safely express them.

### Inspect

Read-only. Never creates or activates grants.

Mandatory test:

~~~text
old connected profile + no grant
inspect twice
reopen
=> NeedsReview
=> zero new DataAccessGrant
~~~

### Disable

- compare reviewed connection/grant expectations;
- pause every current product-owned grant for the connection;
- use one Vault mutation boundary where multiple local rows are involved;
- return Paused;
- never alter connection credentials/resources/Act.

### Enable

1. load current connection/resources;
2. perform provider/native validation outside DB transaction;
3. derive exact policy;
4. compare reviewed expected source/grant authority;
5. atomically create/review/reactivate local grant set;
6. return current projection.

### Connect reconciliation

May create fresh grants only because the same explicit user flow just completed the connection.

It must not become an unconditional startup ensure function.

### Resource reconciliation

If connection owner commits resources but Access convergence fails:

- do not restore stale/wider source state;
- return NeedsReview;
- runtime remains fail-closed.

### Disconnect preparation

Invalidate/revoke local grant set under CAS before credential/source deletion.

Revoked grants are not reused after reconnect under a new source authority.

---

## 5. Bundle CAS

A connection may own multiple grants.

Reviewed overview may carry opaque expectations:

~~~text
grant id
grant authority
consumer-policy authority
source authority / connection revision
~~~

Flutter only echoes them.

Backend:

- computes expected member set from current product policy;
- rejects missing/extra/changed reviewed members when action depends on snapshot;
- never lets Flutter nominate an arbitrary grant outside the connection.

For disable/revoke across multiple grants, prefer one Vault transaction.

For enable, external/native validation happens first; local grant/policy commit happens after previews.

---

## 6. Provider-specific adapters

App coordinator may dispatch to:

- native Calendar access repository;
- personal device-source access;
- remote Calendar access;
- generic remote-view access.

Do not make one generic domain type carry provider-specific OAuth/EventKit evidence merely to appear unified.

Unified layer owns product status/intent; provider evidence stays at real boundaries.

---

## 7. Query-bound source exception

Apple feasibility currently persists a FeasibilityGrantQuery tied to a concrete event/destination.

Do not broaden this into a connection-wide wildcard merely to fit the switch.

For query-bound sources:

- connection detail remains canonical place to initiate review;
- projection may remain NeedsReview until a concrete query is reviewed;
- OS Location permission alone is not Active Observe;
- do not invent wildcard feasibility resource.

---

## 8. Wire strategy

Prefer owner-language product operations, e.g.:

~~~text
connections.observe.inspect
connections.observe.set_enabled
connections.observe.reconcile
~~~

or equivalent App-owner commands.

Requirements:

- no Registry fields;
- no consumer list;
- no GrantScope;
- no model recipient;
- no provider token/credential;
- reviewed authority values are opaque CAS expectations only.

04-C/04-D migrate Flutter and then delete obsolete raw product-wire variants with no legitimate callers.

---

## 9. Failure semantics

| Condition | Product state |
|---|---|
| exact current grant set active | Active |
| exact current set paused | Paused |
| no grant on old connection | NeedsReview |
| partial/stale grant set | NeedsReview |
| OS permission denied | NeedsSystemAccess |
| OAuth/source identity stale | ReconnectRequired |
| supported source temporarily unavailable | Unavailable |
| forged person/grant/authority | hard failure |
| corrupt Vault/policy row | hard failure |

Do not weaken integrity failures into friendly states.

---

## 10. Tests

### Projection

- fresh exact current grant set -> Active;
- all members paused -> Paused;
- partial active bundle -> NeedsReview;
- stale source authority -> NeedsReview/ReconnectRequired as owner semantics dictate;
- OS denied -> NeedsSystemAccess;
- connection absent -> ReconnectRequired/Unavailable.

### No silent migration

- seed connection without Access grants;
- inspect twice;
- reopen;
- assert no grant created.

### Toggle

- Off pauses all current bundle members;
- stale expectation -> Conflict;
- On performs fresh source validation;
- third-party/extra grant is neither adopted nor mutated as first-party bundle state.

### Separation

- toggle leaves ActionAuthority unchanged;
- toggle leaves external-recipient consent unchanged;
- Registry revision unchanged.

---

## 11. Exit gate

Do not proceed to UI cutover until:

- one backend projection answers product Observe state;
- inspect is side-effect free;
- default creation requires an explicit connection product event;
- multi-grant CAS is defined;
- no persisted Use-with-Floe bit exists.
