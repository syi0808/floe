# Checkpoint 04-F — Deletion, verification, and documentation convergence

- **Execution baseline:** 04-E completion
- **Depends on:** 04-A through 04-E
- **Goal:** prove one connection permission product path, run restart/failure acceptance, delete obsolete UI/wire, and converge durable documentation.

04-F may fix bounded residuals exposed by acceptance tests. It must not implement Checkpoint 05 interaction/resume semantics.

---

## 1. Expected final topology

~~~text
Connections / provider / OS
  account/source identity
  credentials
  current resources
  system access
          |
          | explicit connect/reconnect/resource product event
          v
App product composition
  exact first-party source policy
          |
          v
Access
  DataAccessGrant(s)
  source-specific policy authority
          |
          v
Context
  one or many exact source reads
  exact ContextDependency set
          |
          v
Builtin Expert / root consumer

Connection detail:
  Use with Floe
    = projection over current Connection + Access
    != durable authorization bit

Independent:
  Actions -> Act authority
  Inference -> exact external-recipient consent
~~~

---

## 2. Required product deletions

Expected gone or no longer product-facing:

~~~text
_ServerConnectionGrants
_remoteViewsFor
raw remote grant consumer selector
Attention consumer checkboxes
Settings AgentPersonalAccessSettings entry
Settings _AiProcessing / Allow external model providers
duplicate Calendar Observe review/pause/remove card
caller-supplied Personal/Contacts consumers
startup ensure that silently creates grants
~~~

Raw internal owner functions may remain only when they are canonical lower-level Access implementation behind connection product operation.

Public/wire variants with no legitimate caller must be deleted.

---

## 3. Residual audit

### Duplicate permission UI

~~~sh
rg -n '_ServerConnectionGrants|AgentPersonalAccessSettings|_AiProcessing|Allow external model providers|selectedConsumers|Allow attention data for' apps/client
~~~

Expected zero production matches.

### Caller-composed policy

~~~sh
rg -n 'consumers|GrantScope|consumer' apps/client/lib/features/connections apps/client/lib/app/runtime crates/bindings/protocol/src/dto
~~~

Inspect every match.

Forbidden:

- Flutter choosing Observe consumers;
- caller-supplied source GrantScope.

### Use-with-Floe duplication

~~~sh
rg -n 'use_with_floe|llm_enabled|source_enabled|allow_source|Use with Floe' crates apps server
~~~

Allowed: product labels/tests.

Forbidden: new durable bool/column/Registry field.

### Old grant ceremony

~~~sh
rg -n 'calendar_grant_preview|calendar_grant_review|view_grant_preview|view_grant_review|Review grant|Grant preview' apps/client crates/bindings
~~~

Prefer zero obsolete raw product-wire matches after migration.

### Registry

~~~sh
rg -n 'SourceGrants|BuiltinSourceBinding|CalendarExpertSetup|CalendarViewBinding|experts\.calendar\.' crates apps/client
~~~

Expected zero production matches. Rejection/history fixtures only.

### Consumer policy

~~~sh
rg -n 'calendar_first_party_consumers|first_party.*consumer|attention\.expert|contacts\.expert|GrantConsumer::builtin' crates/app crates/experts crates/modules/context
~~~

There must be one App default-policy composition path. Every exceptional runtime consumer must be justified.

---

## 4. Final behavior acceptance

### A. Fresh remote connection

1. no source connector;
2. Connect Gmail;
3. OAuth succeeds;
4. server connection current;
5. default first-party Observe grants reviewed;
6. connection detail shows Use with Floe Active;
7. supported source read succeeds without second grant ceremony.

### B. Fresh native Calendar

1. no EventKit connection;
2. allow system access;
3. choose calendars;
4. bind current source;
5. default Calendar Observe Active;
6. Schedule read succeeds under actual package consumer.

### C. Old connected profile

1. seed connection with no current Observe grant;
2. reopen/inspect;
3. connection remains connected;
4. Observe = NeedsReview;
5. no grant created;
6. explicit On performs fresh review.

### D. Off / On

Off:

- all current first-party Observe grants paused;
- credentials/resources unchanged;
- old dependency denied;
- Registry/Act/recipient consent unchanged.

On:

- source validation repeated;
- current resource set reviewed;
- stale source requires reconnect/review.

### E. Resource change

Active:

- expand/narrow resources explicitly;
- source owner records current selection;
- Access converges exact scope;
- removed resource unreadable;
- newly selected resource readable only after current review.

Paused:

- resources change;
- grants remain paused;
- On uses new selection.

### F. Disconnect

- local Observe invalidated first;
- provider credential/source removed second;
- reconnect creates new/current authority path;
- old dependency never resurrects.

### G. Multiple remote sources

Connect multiple sources for same logical view.

Verify:

- no Conflict merely because multiple sources authorized;
- bounded acquisition/merge follows 04-D;
- every contributing source has separate ContextDependency;
- reauthorization checks all dependencies;
- revoking one source cannot forge/erase another provenance.

### H. External model recipient

- source connection and Observe active;
- recipient consent absent;
- external model source release remains denied/consent-required;
- local/device processing may proceed when allowed;
- Use with Floe never changes recipient consent.

### I. Act

- source Observe active/paused does not alter Calendar create policy;
- approved action still requires current source/provider preconditions;
- no write scope granted by default Observe.

---

## 5. Persistence/restart acceptance

Use isolated profiles.

Verify:

- active connection + grants reopen Active;
- paused bundle reopens Paused;
- partial/corrupt bundle reopens NeedsReview or hard failure according to corruption semantics;
- source-authority drift fails closed;
- old no-grant connection is not auto-authorized;
- external recipient consent persists independently;
- ActionAuthority persists independently;
- legacy CP2/03 Calendar mappings remain rejected.

---

## 6. Architecture audit

Required final boundaries:

- floe-access does not import built-in catalogue;
- floe-experts does not own source permission;
- floe-connections/provider owner does not manufacture DataAccessGrant;
- App composition derives first-party policy;
- Context preserves every actual source dependency;
- Flutter decides presentation/user intent only;
- server connector connection owner does not become model-recipient authority.

Run tools/architecture/check_boundaries.py.

Do not add allow-list edges to preserve obsolete owner/UI paths.

---

## 7. Durable documentation convergence

### ADR 0028

Final text must state implemented policy:

- pairing alone: no connector grant;
- explicit concrete connection completion: default first-party Observe;
- one Use with Floe connection control;
- resource changes coordinate with current Observe;
- external model recipient and Act remain separate;
- old connections are not silently promoted.

If implementation materially changes this decision, amend ADR during implementing slice, not only here.

### Product integrations/privacy

Document user-visible meaning:

- connecting a supported source allows Floe to use selected data by default;
- connection detail can turn Floe use off without disconnecting;
- write actions and external processing remain separate approvals.

### Current architecture

Update only if final implementation changes durable ownership/path beyond current docs.

Do not copy checkpoint status into architecture docs.

### Execution plan

- mark Checkpoint 04 complete;
- record semantic commit SHAs;
- mark Checkpoint 05 next.

---

## 8. Full verification

Rust:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Server:

~~~sh
cd server
go test -race ./...
go vet ./...
~~~

Flutter/macOS:

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

Run focused macOS native tests if platform code changed.

Document baseline-only formatter/linter drift separately; do not hide new warnings in it.

---

## 9. Final definition of done

- [ ] one connection-level Observe product state;
- [ ] no duplicate Settings/source grant editor;
- [ ] no raw remote grant ceremony in normal product UI;
- [ ] no caller-selected first-party source consumer;
- [ ] new explicit connections default to current first-party Observe;
- [ ] old discovered connections never auto-grant;
- [ ] Off preserves connection/resources and pauses Observe;
- [ ] On fresh-validates and CAS-reviews;
- [ ] resource edits converge with active Observe;
- [ ] disconnect cannot leave current active Observe;
- [ ] overlapping remote sources are usable with exact multi-source provenance;
- [ ] external recipient consent remains separate/fail-closed;
- [ ] ActionAuthority remains separate;
- [ ] third-party consumers are excluded by default;
- [ ] restart/persistence cases pass;
- [ ] full Rust/server/FFI/Flutter/macOS gates pass;
- [ ] ADR/product/current docs match reality.

---

## 10. Required agent report

Report only after 04-A through 04-F are complete:

1. **Commit SHAs** by 04-A through 04-F.
2. **Final product model** — connection, Use with Floe, resources.
3. **Consumer policy** — exact owner and actual reader identities.
4. **Native result** — Calendar/Contacts/Attention/Wellbeing/Feasibility disposition.
5. **Remote result** — connect/scope/disconnect and removed raw grant surface.
6. **Multi-source result** — aggregation/routing and dependency proof.
7. **Separation result** — Observe vs Act vs external recipient.
8. **Residual audit** — exact searches and justified matches.
9. **Persistence/restart** — old profile, paused, drift, reconnect.
10. **Verification** — exact commands/results.
11. **Documentation** — ADR/product/architecture/plan updates.
12. **Blocker** — any unmet item keeps Checkpoint 04 open.

Do not mark Checkpoint 04 complete based on UI appearance alone.
