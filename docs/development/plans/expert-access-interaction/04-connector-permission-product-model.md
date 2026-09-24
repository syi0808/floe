# Checkpoint 04 — Connector permission product model

- **Status:** complete
- **Baseline:** main at 833b9f191fa6d1c14a11efa07f6f382e48627d9d
- **Precondition:** Checkpoint 03 complete. Registry is source-independent; Calendar Observe is Access-owned; native and remote Calendar CAS and policy reauthorization are converged.
- **Goal:** make a successful first-party source connection the product ceremony that enables that source for Floe by default, expose one connection-level **Use with Floe** control, and remove duplicate/protocol-shaped permission editors without collapsing Observe, Act, or external-processing authority.
- **Compatibility posture:** pre-stable. Do not preserve obsolete permission UI or wire solely for compatibility. Existing connected profiles must not be silently promoted into new active grants.
- **Platform priority:** Apple/macOS first. Shared contract fallout may update Android code; do not add Android parity work.

This file is the authoritative Checkpoint 04 index. Execute these child plans in order:

1. [04-A — product policy and first-party consumer authority](04-a-product-policy-and-consumers.md)
2. [04-B — connection-level Observe projection and owner API](04-b-connection-observe-owner-api.md)
3. [04-C — native/device connection convergence](04-c-native-device-convergence.md)
4. [04-D — remote SaaS convergence and multi-source reads](04-d-remote-saas-convergence.md)
5. [04-E — Settings, processing, and Act separation](04-e-settings-processing-act.md)
6. [04-F — deletion, verification, and documentation convergence](04-f-verification-doc-convergence.md)

Do not start 04-B until 04-A fixes the durable product decision and canonical first-party consumer policy. Do not default-enable remote source grants until 04-D has a correct multi-source read story for overlapping views. Do not implement Checkpoint 05 chat interactions in this checkpoint.

Completion commits:

- 04-A: `543d0ec8def8c85affe098efacdb8430a0856a8a`
- 04-B: `8ab92b9152145727afce4beb9be9267c318fd578`
- 04-C: `755081d9fd6d28b66a83ec4022c20e7f3b4c309e`
- 04-D: `64f5adc2cd361f49880ec1c70331d601ef954ac9`
- 04-E: `d4ac4b78d3bc7bcbc584d2bc62a97268f96f0fef`
- 04-F: this completion commit; Checkpoint 05 is next and has not started.

---

## 1. Why this checkpoint exists

Checkpoint 03 fixed authority ownership but intentionally preserved explicit review ceremonies. The current product still exposes implementation details:

~~~text
Remote SaaS:
Connect
  -> OAuth/source connection
  -> separate grant preview/review/pause panel
  -> user may choose a raw consumer

Native Calendar:
OS/source connection + resource selection
  -> separate Observe preview/review/pause/remove card

Personal device sources:
connection detail AND Settings > Data & privacy
  -> duplicate Observe editors
  -> some clients submit consumer lists

External processing:
Settings > Data & privacy
  -> generic "Allow external model providers" toggle
~~~

Target product model:

~~~text
Connect / allow system access / select resources
  -> connection becomes current
  -> App derives exact first-party readers
  -> Access creates or reviews exact Observe grant set
  -> Floe may use the source

Connection detail:
  Use with Floe [on/off]

Off:
  -> pause Observe grants
  -> keep credentials
  -> keep connection
  -> keep resource selection
  -> keep Act authority unchanged

On:
  -> fresh source/system validation
  -> fresh exact grant review
  -> current selected resources only
~~~

The UI becomes simpler, but authority remains separate:

- **Connections / provider / OS** — account/source identity, credentials, resource selection, system permission.
- **Access Observe** — DataAccessGrant, consumer, source/resource scope, purpose, processing restriction, policy authority.
- **Actions Act** — create/send/write authority and proposal approval.
- **Inference / recipient authority** — whether source-backed data may leave its currently approved processing boundary.

No new connection flag may become a second authorization truth.

---

## 2. Baseline findings at 833b9f19

### Native/device

- apps/client/lib/features/connections/application/native_calendar_access_gateway.dart exposes access.calendar.inspect/preview/configure.
- ConnectorScreen still renders a protocol-shaped native Calendar Observe card with separate review/pause/remove actions.
- _AppleConnectionDetail already hosts Contacts, Attention, Feasibility, and Wellbeing access cards.
- Data & privacy still contains overlapping source/system controls.
- PersonalAccessChangeDto::Review and ContactsAccessChangeDto::Review still accept caller-supplied consumers.
- Attention UI still lets the Person choose assistant / attention.expert directly.

### Remote SaaS

- ServerConnectorPanel._connect completes connector authorization but does not establish default Observe.
- _updateScope mutates server connection scope without coordinating Observe scope.
- _disconnect deletes the connection without first converging local Observe authority.
- _ServerConnectionGrants exposes raw preview/review/pause controls and a consumer selector.
- Flutter hard-codes _remoteViewsFor(connectorId).
- Remote grant commands are Access-owned, but product wire is still grant-oriented rather than connection-intent-oriented.

### External processing and Act

- _AiProcessing in Settings edits ServerConnection.allowExternal.
- Inference still enforces exact-recipient consent separately; this enforcement must remain.
- ActionPermissionsSection controls external side-effect authority and is correctly separate from Observe.

### Runtime consequence of default-enabling every connection

Remote connectors can overlap on logical views:

~~~text
mail.communication:
  Gmail
  Microsoft Mail

work.context:
  Slack / Teams / GitHub family

life.logistics:
  Gmail
  Home Assistant
  other supported providers
~~~

Generic remote read currently selects one active source grant and conflicts when multiple sources admit the same logical view. Default-on connections make multi-source normal, so 04-D must fix routing/provenance before remote default Observe is complete.

---

## 3. Final product semantics

### 3.1 No persisted Use-with-Floe boolean

Use with Floe is a product projection over owner state.

Do not add durable authority such as:

~~~text
connection.use_with_floe
connector.llm_enabled
registry.source_enabled
settings.allow_source
~~~

Effective state is derived from:

- current connection/source/system state;
- current selected resources;
- expected first-party Observe grant set;
- current grant/source/policy authority.

### 3.2 Effective states

Connection-level projection must distinguish at least:

~~~text
Active
Paused
NeedsReview
NeedsSystemAccess
ReconnectRequired
Unavailable
~~~

If an enabled boolean is exposed, it is derived from actual admitted current grants, never persisted independently.

A partial multi-grant bundle is **NeedsReview**, never clean Active.

### 3.3 Which events may create default Observe

Only an explicit product event may create or review default Observe:

- successful new connection;
- explicit reconnect;
- explicit resource/scope update while Use with Floe is enabled;
- explicit toggle from off/review-required to on.

Merely discovering an existing connection during startup or inspection must not create a grant.

Therefore an old connected profile with no current grants becomes NeedsReview, not Active.

### 3.4 Off

Off:

- pauses all current product-owned first-party Observe grants for the connection;
- preserves credentials and provider connection;
- preserves selected resources;
- preserves OS permission;
- preserves ActionAuthority;
- invalidates old dependencies through normal GrantAuthority changes.

### 3.5 On

On:

1. reload current connection/resources;
2. perform fresh native/signed source validation;
3. derive exact first-party policy in App;
4. compare reviewed expected connection/source/grant authority;
5. create/review/reactivate exact grant set under CAS;
6. return current derived status.

Never revive a stale source merely because an old grant exists.

### 3.6 Resource changes

When Active, resource selection and Observe scope are one product interaction:

~~~text
change selection
  -> connection owner commits current selection
  -> fresh source/resource validation
  -> Access reviews exact current scope
  -> return final effective state
~~~

If Access convergence fails after the connection owner commits the resource change, fail closed as NeedsReview. Do not widen an old grant to make the UI appear successful.

When Paused, resource selection may change while grants stay paused. Turning On later reviews the latest selection.

### 3.7 Disconnect

Invalidate or revoke Floe-owned Observe grants before deleting source credentials/connection where owner sequencing permits.

If provider disconnect later fails, the safe residual is:

~~~text
connection still exists
Observe disabled or revoked
~~~

Never leave:

~~~text
credential/source removed
old active grant still appears current
~~~

---

## 4. First-party default policy

Default connection Observe is only for product-approved first-party readers.

- built-in Expert consumers come from actual production reader identities;
- root assistant consumers are included only where a real current root path reads the source;
- third-party and extension consumers are never included automatically;
- Flutter never submits the approved consumer list;
- Access and Vault never import the built-in catalogue;
- App composition joins connection capability and current first-party readers.

Do not add an assistant wildcard to every grant.

04-A owns exact inventory and tests.

---

## 5. Multi-source rule

A default-on connection model means multiple current connections may legitimately serve the same logical view.

Checkpoint 04 must not solve that by:

- silently choosing a hidden preferred source;
- denying the second connection permission;
- leaving both grants active while generic reads always Conflict;
- inventing a synthetic aggregate DataAccessGrant or ContextDependency.

Every contributing source retains its own DataAccessGrant and ContextDependency.

04-D owns the smallest correct bounded multi-source acquisition/merge contract. If SourceRead/HeldGrant cannot represent this without provenance loss, change that contract instead of collapsing authority.

---

## 6. External processing is not Observe

Default source Observe does **not** approve a new model recipient.

A source-backed request may still be blocked because the chosen model route would transmit data to a recipient that has not been approved.

Checkpoint 04:

- removes the generic Settings ceremony;
- preserves exact-recipient enforcement and stored authority;
- does not default external processing to allowed;
- does not treat Use with Floe as external-recipient consent.

Checkpoint 05 will turn a missing contextual recipient/source decision into a Conversation-owned interaction.

A fresh profile may therefore have a connected, Observe-active source while remote model processing still requires later contextual approval. This is intentional fail-closed behavior.

---

## 7. Act is separate

Observe operations never mutate:

- Calendar create authority;
- send/write/delete authority;
- proposal approval;
- provider write scopes.

ActionPermissionsSection may remain in Settings because it is cross-connection Act policy, not source Observe.

---

## 8. Execution slices

### 04-A — product policy and first-party consumer authority

Exit:

- ADR/product policy reflects the new connection ceremony;
- App has one canonical first-party Observe policy derivation;
- actual production readers and supported connector/view mappings are tested;
- caller-supplied consumer editing has a deletion path.

### 04-B — connection Observe projection and owner API

Exit:

- no persisted Use-with-Floe authorization bit;
- one effective connection access projection exists;
- inspect is side-effect free;
- connection-level set-enabled/reconcile semantics are owner-correct;
- bundle CAS/failure semantics are defined;
- old profiles are never auto-granted on inspect.

### 04-C — native/device convergence

Exit:

- supported Apple connection completion creates/reviews default Observe where stable resource semantics exist;
- connection detail owns source controls;
- duplicate Settings editors are removed for migrated sources;
- client consumer selection is gone;
- native Calendar resource edits and Use-with-Floe share one product state.

### 04-D — remote SaaS convergence

Exit:

- successful new remote connection establishes current first-party grant set;
- scope edit and disconnect coordinate Observe;
- raw grant ceremony/consumer picker is gone from Flutter;
- Flutter _remoteViewsFor is gone;
- overlapping source views read correctly with exact per-source provenance.

### 04-E — Settings, processing, and Act separation

Exit:

- generic external-model source toggle is removed from Settings;
- recipient enforcement remains fail-closed;
- Action permissions remain independent;
- Settings contains only true cross-connection concerns.

### 04-F — deletion and verification

Exit:

- residual search proves one product permission path;
- restart/failure/reconnect/resource-change cases pass;
- full applicable Rust/Go/FFI/Flutter/macOS gates pass;
- current architecture, ADR/product docs, and this plan converge;
- Checkpoint 05 becomes next.

---

## 9. Global invariants

- Registry is never reintroduced into source access.
- DataAccessGrant remains Observe authority.
- Connection state is not authorization.
- Use with Floe is derived/product intent, not a second durable authority.
- App derives first-party policy; Access remains catalogue-agnostic.
- Flutter may echo opaque expected authority but never composes GrantScope/consumers/recipient policy.
- Existing connected profiles are not silently authorized.
- Native/provider I/O occurs outside Vault transactions.
- Resource/source drift fails closed.
- ContextDependency records actual admitted source and consumer.
- No third-party source consumer is default-granted.
- Observe never changes ActionAuthority.
- Observe never implies external-model recipient consent.
- Disconnect cannot leave a current active Floe Observe grant.
- Default-enable must not make multi-source views unusable.

---

## 10. Stop conditions

Stop and update the plan instead of adding a workaround if implementation appears to require:

1. a persisted use_with_floe authorization bit;
2. Registry source permission revival;
3. Access importing floe-experts-builtin;
4. Flutter sending consumer lists or GrantScope;
5. automatic grant creation merely because an old connection is discovered at startup;
6. treating paired-server enrollment alone as connector Observe consent;
7. turning external model consent on as a side effect of Connect;
8. mutating ActionAuthority from Use with Floe;
9. preserving _ServerConnectionGrants as a second permission editor;
10. preserving Settings source Observe editors after connection-detail replacement is live;
11. hidden source priority to avoid multi-source conflicts;
12. synthetic aggregate grants/dependencies that erase per-source provenance;
13. holding a database transaction across OAuth/EventKit/provider I/O;
14. implementing Checkpoint 05 inline permission interactions early.

---

## 11. Broad verification

Targeted gates live in each child plan. Final broad gate in 04-F:

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

Server connector code is expected to change in 04-D. If it does:

~~~sh
cd server
go test -race ./...
go vet ./...
~~~

Run focused macOS/native tests when native provider or Runner code changes.

Use isolated development profiles for connection/grant persistence acceptance.

---

## 12. Definition of done

- [x] explicit new/reconnect connection completion enables supported first-party Observe by default.
- [x] merely inspecting an old connected profile never creates a grant.
- [x] every migrated connection detail exposes one Use with Floe control.
- [x] Use with Floe is derived from current owner state, not persisted separately.
- [x] Off pauses Observe while preserving connection/resources/credentials/system permission.
- [x] On performs fresh source/resource validation and CAS-bound review.
- [x] resource selection and active Observe converge in one product interaction.
- [x] disconnect invalidates Observe before source credential removal.
- [x] Flutter sends no first-party consumer list or GrantScope.
- [x] App is the single product-composition source for first-party consumer policy.
- [x] actual production consumer identities are covered; third-party consumers are excluded.
- [x] remote grant consumer picker and raw grant ceremony are gone from connection UI.
- [x] Flutter _remoteViewsFor product policy is gone.
- [x] overlapping remote sources can coexist and be read without provenance loss.
- [x] Settings no longer edits migrated source Observe permissions.
- [x] Settings external-model toggle is gone.
- [x] exact-recipient processing enforcement remains.
- [x] ActionAuthority is unchanged by Observe operations.
- [x] old connected state is not reinterpreted as authorization.
- [x] legacy Calendar-Expert/source Registry surfaces remain deleted.
- [x] full Rust/architecture/FFI/Flutter/macOS gates pass.
- [x] Go gates pass for server changes.
- [x] durable ADR/product/current architecture docs match final behavior.

Checkpoint 05 may start only after all items above are true.
