# Checkpoint 04-D — Remote SaaS connection convergence and multi-source reads

- **Execution baseline:** 04-C completion
- **Depends on:** 04-A policy, 04-B projection
- **Goal:** make remote connector completion/scope/disconnect coordinate default Observe, remove raw grant ceremony, and make default-on overlapping sources usable without losing provenance.

This is the highest-risk Checkpoint 04 slice. Preserve remote source trust, signed descriptors, producer pinning, CAS, cancellation, and exact recipient semantics.

---

## 1. Current product flow

Baseline:

~~~text
ServerConnectorPanel._connect
  -> server start connector
  -> OAuth / authorization operation
  -> AuthorizationSettled
  -> connected

then separately

_ServerConnectionGrants
  -> preview calendar/view grant
  -> choose consumer
  -> review
  -> pause
~~~

Scope update and disconnect are independent from local Access authority.

---

## 2. Target remote product flow

### New connection

~~~text
Connect
  -> server connection + credential/resource scope completes
  -> latest connector snapshot
  -> fresh signed source previews for product-supported views/resources
  -> App derives first-party readers
  -> local Access commits exact Observe grant set
  -> connection detail reports Active
~~~

If source connection succeeds but Observe commit fails:

- keep connection;
- status NeedsReview;
- do not auto-retry on every refresh;
- explicit On repairs it.

### Existing connection on startup

Read-only inspection only. No auto-grant.

### Off

Pause all current product-owned remote Observe grants for connection.

Do not disconnect OAuth/account.

### On

Freshly revalidate:

- pinned producer;
- pairing;
- signed source descriptor;
- connector/connection/resource;
- source authority;
- connection revision/provider identity;
- exact first-party consumer policy;
- source-transport recipient.

Then converge local grants/policy records under CAS.

### Scope update

When Active:

~~~text
server updateConnectorScope
  -> latest current connection/source snapshot
  -> reconcile exact grant resources
~~~

If Access reconciliation fails, status NeedsReview. Never keep a widened old authorization as current.

When Paused, update server scope and leave grants paused until On.

### Disconnect

Invalidate/revoke local Observe bundle first, then call server disconnect.

If server disconnect fails, source may remain connected but Observe stays off.

---

## 3. Replace raw Flutter grant UI

Delete product role of _ServerConnectionGrants.

Connection detail shows one Observe control from 04-B.

Delete from UI:

- grant preview/review buttons;
- grant id/authority display;
- consumer selector;
- separate Calendar/View grant sections;
- Flutter _remoteViewsFor policy mapping.

Do not recreate an advanced permission panel elsewhere.

---

## 4. Move connector/view policy to App

04-A policy chooses expected bounded view set for connector.

Caller cannot submit arbitrary view id for default Observe.

For each expected view:

- obtain signed source preview through current RemoteGrantTransport;
- verify connector/view admissibility;
- derive exact current resource;
- derive exact current first-party consumers;
- create/review grant under CAS.

Calendar uses its exact-resource Calendar policy path.

Generic remote views use generic remote view Access path, but product no longer supplies arbitrary consumer.

Raw Access functions may remain internal.

---

## 5. Remote view consumer policy

Current built-in remote reads use request.agent_id.

Default generic remote view grants must cover those actual package ids selected in 04-A.

If multiple built-ins read the same view, grant must admit exact bounded set without fallback to assistant.

Current remote_view_scope is single-consumer. Change owner contract if necessary to accept an exact bounded consumer set.

Prefer one source/view DataAccessGrant containing exact approved consumer set where current GrantScope already supports it.

Do not create duplicate grants per consumer unless runtime/persistence explicitly require it and ambiguity tests prove it correct.

ConsumerPolicyAuthority advances when consumer set changes.

---

## 6. Source-transport recipient vs model recipient

Generic remote view scope currently uses ProcessingRestriction::ApprovedRecipient from paired producer/audience review.

Preserve source-transport semantics.

Do not interpret it as permission for an arbitrary external model provider.

If one field is currently doing both jobs, stop and separate semantics instead of allowing Connect to broaden model-recipient consent.

---

## 7. Multi-source prerequisite

Default-on connectors make this normal:

~~~text
mail.communication:
  Gmail
  Microsoft Mail

work.context:
  Slack
  Teams
  GitHub

life.logistics:
  Gmail
  Home Assistant
  other supported sources
~~~

Baseline generic remote read calls active_resource_grant and expects exactly one source. This cannot remain final.

### Required behavior

One Expert request for a logical view may consume a bounded set of current source reads.

Every contributing source retains:

- its own DataAccessGrant;
- its own GrantAuthority;
- its own ConsumerPolicyAuthority;
- its own source authority;
- its own ContextDependency;
- its own reauthorization.

Do not create a synthetic aggregate grant or dependency.

### Contract cutover

Baseline:

~~~text
SourceRead
  payload
  one ContextDependency
  one GrantScope

AuthorizedRead / HeldGrant
  one dependency
  one scope
~~~

If aggregation happens below Expert host, change contract so merged payload carries all contributing dependency/scope bindings.

A valid direction is a bounded list of dependency/scope bindings attached to one authorized payload.

Requirements:

- single-source reads remain simple;
- every dependency individually validates;
- lease covers merged payload without erasing identity;
- Expert/Conversation recorder records all dependencies;
- model coverage contains all dependencies;
- proposal/evidence paths remain exact for source-backed evidence actually used.

Do not overload one ContextDependency with multiple grant ids.

### Stop condition

If correct multi-source provenance would require synthetic authority or dropping dependencies, stop and update plan before enabling overlapping default grants.

---

## 8. Bounded merge semantics

Merge at Context/source-view boundary, never Flutter.

### mail.communication

- execute same bounded query against each admitted current source;
- deduplicate by canonical evidence/source identity, never display text;
- deterministic ordering by existing time semantics;
- enforce global item/byte budget after merge;
- coverage_complete false if any source incomplete or merge truncates.

Pagination/cursor semantics must be explicit.

If current cursor is source-local and cannot resume aggregate query correctly, do not invent fake cursor. Introduce a bounded composite cursor owned by Context or change contract explicitly with all callers in same slice.

### work.context

- bounded deterministic merge of work items;
- preserve source evidence/provenance;
- no duplicate source item;
- global budget after merge.

### life.logistics

- bounded deterministic merge;
- preserve earliest expiry/freshness semantics;
- deduplicate stable evidence handles;
- coverage_complete false on truncation/partial source.

Reuse current domain validation after merge.

---

## 9. Failure/degradation semantics

One unavailable optional source need not destroy entire view if other current authorized sources succeed and view contract can express incomplete coverage.

Hard failures still fail read when they indicate:

- corrupted authority;
- forged producer/source;
- contradictory duplicate identity;
- invalid payload;
- impossible provenance.

Define degradation rule per view.

Do not silently convert denied/review-required source into empty success if product must repair it. Surface incomplete/review issue through existing source issue/outcome model where possible.

---

## 10. Remote wire cleanup

After connection-level path is live, remove/narrow product-facing raw variants with no legitimate caller:

~~~text
calendar_grant_preview
calendar_grant_review
calendar_grant_status
calendar_grant_pause
view_grant_preview
view_grant_review
view_grant_status
view_grant_pause
~~~

Internal Access functions remain.

If Checkpoint 05 needs connection access, it must target new connection-level owner operation, not raw grant variants.

Update:

- crates/app/src/remote_services.rs;
- protocol RemoteAccessOperationDto;
- FFI remote wire;
- Flutter RemoteAccessGateway;
- server connector panel tests.

No compatibility aliases.

---

## 11. Tests

### Connection ceremony

- fresh Gmail connect -> default exact grant set after authorization;
- fresh Google Calendar connect -> default Calendar grant for selected resource;
- existing connected/no-grant startup -> NeedsReview and zero new grants;
- scope update Active -> exact current grants;
- scope update Paused -> stays Paused;
- disconnect invalidates grants before credential removal;
- failed disconnect leaves Observe off.

### Consumer

- Flutter cannot nominate assistant or extension consumer;
- each remote view grant contains exactly 04-A current first-party readers;
- third-party consumer denied.

### Multi-source

At minimum:

- Gmail + Microsoft Mail active -> communication read bounded and carries two dependencies;
- current Slack/GitHub family active -> work context read carries all source dependencies;
- Gmail + Home Assistant logistics -> merged logistics with both dependencies;
- one stale source follows defined degradation/review behavior;
- duplicate exact source grant -> Conflict;
- all contributing dependencies reauthorize after reopen;
- revoking one source never revives or forges another source provenance.

### Safety

- producer fingerprint drift fails closed;
- source authority/revision drift fails closed;
- model-recipient consent unchanged;
- ActionAuthority unchanged.

---

## 12. Residual gate

~~~sh
rg -n '_ServerConnectionGrants|_remoteViewsFor|calendar_grant_preview|calendar_grant_review|view_grant_preview|view_grant_review|selectedView' apps/client
~~~

Expected no production product UI matches.

~~~sh
rg -n 'active_resource_grant' crates/modules/context crates/app
~~~

Any single-source selection in a logical multi-source remote path must be justified.

~~~sh
rg -n 'dependency\(&self\).*ContextDependency|SourceRead.*dependency' crates/contracts/context crates/modules/context
~~~

If authorized-read contract changes, no caller may silently drop all but one dependency.

---

## 13. Verification

This slice is expected to touch remote/server behavior:

~~~sh
cargo check --workspace --lib
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-app
cargo test -p floe-vault
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

cd server
go test -race ./...
go vet ./...

cd ../apps/client
flutter analyze
flutter test
~~~

Do not proceed to 04-E while a second connected source for one logical view causes authority Conflict or provenance loss.
