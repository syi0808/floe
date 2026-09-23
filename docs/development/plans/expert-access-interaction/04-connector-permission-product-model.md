# Checkpoint 04 — connector permission product model

## Goal

Converge the product permission ceremony on the connection that owns the source.

After this checkpoint:

- connecting a supported first-party source/account establishes the default Observe grant that makes the selected source usable by Floe;
- each connection has one clear “Use with Floe” control for Observe access;
- pausing that control pauses Access grants without destroying credentials or source selection;
- resource selection and Observe grant scope are coordinated as one user intent instead of two protocol-shaped ceremonies;
- action/write authority remains separate;
- Settings-level “LLM may use this data/connector” toggles are removed;
- exact external model/recipient consent remains enforced and will be requested contextually by checkpoint 05;
- remote and native connectors present the same product semantics even though their authentication/provider mechanics differ.

This checkpoint changes durable product policy. Implementation must amend/supersede the parts of ADR 0028 that currently require zero connector grants after pairing/connection and must update product integrations/privacy language in checkpoint 06.

## Baseline surfaces

Primary Flutter:
- apps/client/lib/features/connections/presentation/connector_screen.dart
- apps/client/lib/features/connections/presentation/server_connector_panel.dart
- apps/client/lib/features/connections/application/remote_access_gateway.dart
- apps/client/lib/features/settings/presentation/data_privacy.dart
- apps/client/lib/features/settings/presentation/ai_processing.dart
- apps/client/lib/features/settings/presentation/action_permissions.dart

Primary Rust owners/adapters:
- Connections owner and provider connection control APIs
- Access/DataAccessGrant owner
- crates/adapters/vault/src/vault/remote_calendar_grants.rs
- crates/adapters/vault/src/vault/remote_view_grants.rs
- native Calendar connection/EventKit adapter path
- App/FFI commands used by connection detail

Relevant decisions:
- ADR 0020 — Observe and Act are separate; UI is escalation surface
- ADR 0028 — connection-scoped permissions, but current proposed text says pair/connect creates zero grants
- product/integrations-and-privacy.md — current text says successful read connection does not imply AI-use permission

## 1. Define the product-level connection contract

### 1.1 Connection vs Observe vs Act

Keep three semantic layers internally:

~~~text
Connection
  authentication/system access + stable source identity

Observe
  DataAccessGrant permitting bounded read/use by approved first-party Floe consumers

Act
  ActionAuthority permitting/asking/denying external mutation
~~~

The UX should not force a user to manually coordinate those layers when their intent is obvious.

### 1.2 Connection completion semantics

For a supported first-party connector, “Connect” is the user’s intent to let Floe use the selected source for ordinary Observe behavior.

A connection becomes product-ready only when the connect flow has completed the applicable atomic/compensated steps:

1. authenticate / acquire OS system access;
2. establish stable connection/source authority;
3. select the resource scope where the connector exposes selectable resources;
4. create or activate the default first-party Observe grant for exactly that connection/resource scope;
5. return a connection snapshot whose effective Observe state is Active.

If step 4 fails, do not show “connected and usable”. Either:
- fail/roll back the product connection where the provider operation is safe to compensate; or
- show a recoverable connected-but-needs-attention state with one repair action.

Do not silently leave the current “connection exists, Registry looks Active, grant absent” disagreement state.

### 1.3 Empty selection

For sources such as Calendar where resource selection is explicit:

- no selected Calendar means no readable Calendar scope;
- connection credentials/system access may exist, but Use with Floe cannot be Active with an empty resource set;
- the UI asks the user to choose at least one resource before completing “usable” setup;
- do not manufacture a wildcard scope when the user selected none.

For whole-account sources such as a mail connection whose bounded resource is the account itself, the account resource can be created as the default Observe scope.

## 2. One “Use with Floe” control per connection/source capability

### 2.1 Meaning of ON

ON means:

- connection/source identity is current;
- selected resources are valid;
- the current default first-party Observe grant is Active;
- new reads still require normal runtime admission and fresh source checks.

ON is not a cached assertion that all future reads will succeed.

### 2.2 Meaning of OFF

OFF means:

- keep credential/system connection;
- keep stable connection identity;
- keep selected resource set;
- pause/revoke the applicable first-party Observe grants according to Access semantics;
- later reads return NeedsUserAction(EnableObserve);
- actions are governed independently by ActionAuthority.

If disabling must invalidate derived cached views, do that through existing provenance/freshness policy. Do not claim that already released model/provider data can be recalled.

### 2.3 Turning back ON

Turning ON must not simply flip a UI bit.

The owner path must:

1. load current connection;
2. validate source authority;
3. validate selected resources;
4. perform native subject/system permission check if applicable;
5. re-review/re-activate the grant with the current authority;
6. advance grant/policy authority/CAS state;
7. return the resulting effective access snapshot.

If source identity changed, return Needs review rather than silently activating stale consent.

## 3. Resource selection is part of the same user intent

Current designs can force:

~~~text
select calendars
then separately review Calendar Expert grant
~~~

Remove that split.

When the user explicitly edits selected resources from a connection detail screen, the same product operation should update the Observe grant scope to the selected set after validation.

Internally this may be two owner operations coordinated by App:

~~~text
Connections: set selected resources
Access: narrow/expand current Observe grant
~~~

but it is one user action.

Required safety rules:

- scope may never exceed selected resources;
- expansion happens only as part of the explicit resource-selection interaction;
- a background provider catalog change does not expand a grant;
- resource removal narrows/revokes access immediately;
- failed Access update must not leave UI claiming the broader scope is usable;
- use CAS/authority revisions so concurrent edits fail/reconcile cleanly.

This deliberately changes the proposed ADR 0028 rule that connection selection and grant expansion are always separate product ceremonies.

## 4. Reuse the canonical first-party consumer policy

Checkpoint 02 R4.5 establishes the Calendar consumer policy required for runtime cutover: product composition derives actual built-in consumer identities from canonical declarations and passes them into Access grant creation. Checkpoint 04 does **not** redesign or duplicate that policy; it changes the product ceremony that creates, pauses and reviews the grant.

Rules:

- connection creation obtains the current canonical policy from product composition;
- Flutter never sends or edits a consumer checkbox/string list;
- Access validates supplied scope but does not depend on built-in Expert declarations;
- Calendar reuses the policy established in Checkpoint 02;
- analogous source policies such as Mail follow the same owner pattern when introduced;
- third-party Expert/package ids are never included by default;
- a consumer may still have stricter processing/sensitivity policy than the connection-level Observe toggle;
- disabling Use with Floe blocks Observe for the canonical first-party set without changing Act authority;
- `calendar.expert` must never reappear as product-facing or security compatibility authority.

If a direct Manager/assistant source-read path exists, add its real consumer identity only through the canonical product policy and a focused runtime test.

## 5. Native Calendar connection

### 5.1 Connection detail composition

The macOS Calendar connection detail should present:

~~~text
macOS Calendar
System access          Allowed / Needs access
Calendars              Personal, Work
Use with Floe          On
~~~

The user should not see:

- “Calendar Expert installation”;
- tool assignment IDs;
- source fingerprint under normal presentation;
- DataAccessGrant terminology;
- separate Schedule activation required to make Calendar usable.

Security details may expose source authority/fingerprint under an advanced diagnostic section.

### 5.2 EventKit OS permission

System access remains OS-owned.

If EventKit permission is denied/revoked:

- connection effective state becomes Needs access/Unavailable;
- Use with Floe cannot create a successful read;
- clicking the recovery action invokes the native permission/system-settings path;
- after return, refresh connection state and revalidate the Observe grant;
- do not interpret an old grant as proof that OS permission remains present.

### 5.3 Selected Calendar changes

Selecting/deselecting calendars must:
- use the connection owner to update the selected set;
- update/narrow the Access Observe scope;
- bump source/grant authority as appropriate;
- invalidate stale derived source views;
- leave Schedule Registry untouched.

## 6. Remote server/SaaS connections

### 6.1 Remove protocol-shaped grant ceremony from server settings

server_connector_panel.dart currently exposes grant preview/review/pause operations too directly.

Move permission UX to each concrete connected source/account.

The server/pairing screen should focus on:
- server trust/connection health;
- pair/re-pair;
- optional security details;
- list/navigation to concrete source connections.

Do not make the user understand producer/audience/issuer/grant vocabulary for routine source use.

### 6.2 Pairing vs source connection

Pairing authenticates the app/server relationship. It does not itself create a grant for every possible SaaS account.

When the user later connects a concrete Google/Microsoft source:
- that connection operation creates the default Observe grant for that connection;
- pairing supplies trusted server authority infrastructure;
- source use remains connection-scoped.

If the current product treats server pairing and a single implicit source connection as one action, split the internal identities before exposing “Use with Floe”; a server instance is not a source account.

## 7. Remove the generic Settings LLM/source toggle

apps/client/lib/features/settings/presentation/ai_processing.dart currently presents external model use as a durable Settings toggle.

Remove that product control.

Do not remove the underlying processing policy.

### 7.1 New processing consent semantics

ProcessingRestriction / exact recipient remains an Access/Inference safety boundary.

When a requested operation needs a recipient not currently approved:

~~~text
source read/model dispatch
  -> NeedsUserAction(ApproveProcessingRecipient)
  -> Manager explains
  -> chat interaction / explicit review
  -> owner approves exact recipient/scope
  -> linked retry reauthorizes
~~~

This is checkpoint 05 UI/runtime work.

Checkpoint 04 should:
- remove the disconnected global toggle;
- preserve internal recipient authority;
- ensure no caller treats “Use with Floe = ON” as blanket approval for arbitrary external model providers.

### 7.2 Server-local connector processing is not model transfer

Do not conflate:
- a SaaS connector executing on Floe’s paired server to fetch data; and
- sending that data to an external model provider.

Connection runtime placement follows the connector boundary. Model recipient approval follows Inference/Access dispatch authority.

Tests should distinguish them.

## 8. Settings cleanup

Data & privacy should retain durable controls that are genuinely global and not owned by one connection.

Remove/edit source-specific controls that now belong on connection detail.

At minimum inspect and clean:

- Calendar Expert access card/section;
- Android source permission controls only where shared UI still exposes obsolete semantics; do not expand Android work;
- _AiProcessing external provider toggle;
- duplicate “Floe may use this source” controls outside Connections.

Action permissions remain because Act is a separate authority class.

A global Permissions overview may remain/read-only if it is useful for audit/emergency pause, but:
- it must not create or expand resource scope;
- it navigates to the owning connection for scope changes;
- do not implement a second full editor.

## 9. Connection/access read model

Flutter should not synthesize “Active” from unrelated booleans.

Expose one owner-produced effective access projection for each connection capability:

~~~text
ObserveAccessStatus
  Off
  Active
  NeedsReview
  NeedsSystemAccess
  ReconnectRequired
  Unavailable
~~~

The projection is derived from current connection + Access authority and may include:
- stable connection id;
- capability/source id;
- selected resource summary;
- whether inline enable is safe;
- non-secret reason code.

It must not become a second persisted authority.

UI “Active” is rendered only from this projection.

## 10. Protocol/App boundary

Replace old source-specific commands with connection-scoped intent.

A reasonable shape is:

~~~text
connections.access.inspect
connections.access.set_enabled
connections.resources.update
~~~

or equivalent owner-aligned names.

The command must identify:
- Person from CallerContext, not caller payload duplication;
- connection id;
- source/capability id if one connection exposes several capabilities;
- expected authority/revision;
- desired enabled/resource state.

Do not carry:
- Expert setup id;
- Expert Registry revision;
- grant id selected by Flutter unless the owner explicitly exposes it as an opaque current reference;
- native fingerprint chosen by Flutter.

App orchestrates Connections + Access and returns the effective projection.

## 11. Tests

### Product state

For native Calendar:
1. permission allowed + resource selected + connect -> Active;
2. Use with Floe Off -> connection remains, grant paused;
3. turn On -> fresh native subject check and Active;
4. OS permission revoked externally -> effective state NeedsSystemAccess;
5. source identity changes -> NeedsReview;
6. selection narrowed -> grant narrowed;
7. selection expanded explicitly -> grant expanded after fresh validation;
8. no selected calendars -> not Active.

For remote SaaS:
1. paired server alone does not create source account access;
2. source account connect creates default first-party Observe grant;
3. Use with Floe pause does not delete credential;
4. disconnect revokes source grants before deleting credential metadata;
5. re-connect creates current authority rather than reviving stale grant identity.

### Processing

- Use with Floe On does not authorize an unapproved external model recipient.
- exact recipient approval still fences model dispatch.
- server-local source acquisition can work while external model transfer remains denied.

### Flutter

- one Use with Floe control per concrete connection capability;
- no Calendar Expert permission control;
- no generic Settings LLM source toggle;
- effective Active state comes from backend projection, not local composition of Registry flags.

## 12. ADR/product decision update required by this checkpoint

Implementation cannot leave ADR 0028 saying “successful pairing leaves zero connector grants” while code makes first-party source connection establish Observe access.

Amend or supersede ADR 0028 with these durable decisions:

- pairing authenticates the server relationship but concrete source connection establishes default first-party Observe permission;
- resource selection and matching Observe scope update are one explicit product interaction;
- Use with Floe pauses/reactivates Observe without disconnecting;
- third-party consumers are still denied by default;
- Act remains separate;
- external model recipient consent remains separate and contextual.

Update product/integrations-and-privacy.md wording so “connection does not imply AI permission” is replaced by the new, more precise rule:
- connecting a first-party source with Use with Floe enabled grants bounded Observe to Floe first-party consumers;
- it does not imply Act authority or arbitrary external model recipient approval.

The actual docs edit may be committed in checkpoint 06 if implementation is staged, but the code and tests in checkpoint 04 must follow this decision.

## 13. Residual audit

Search:

~~~text
Calendar access
Calendar Expert
Allow external model providers
remote calendar grant
previewRemoteCalendarGrant
reviewRemoteCalendarGrant
pauseRemoteCalendarGrant
Data & privacy
Use with Floe
~~~

Classify all remaining UI controls. There must be one editing owner for source Observe scope: the connection detail.

Search wire/API names for:
- grant preview/review UI commands that no longer have a product caller;
- source access commands under Experts owner;
- duplicated enable flags.

Delete unused commands and gateways in the same checkpoint; do not leave backend ceremony after removing its UI.

## 14. Verification

Rust/protocol:
- connection owner tests;
- Access grant tests;
- remote source tests;
- native Calendar source/permission tests;
- protocol command/DTO tests;
- FFI build.

Flutter:
- flutter analyze;
- focused connector_screen_test.dart;
- focused server_connector_panel_test.dart;
- Data & privacy/settings tests;
- full flutter test.

Apple:
- macOS EventKit permission/list/read focused native tests if present;
- flutter build macos;
- manual smoke with a fresh development profile:
  - allow Calendar;
  - select calendar;
  - verify Use with Floe Active;
  - pause/resume;
  - revoke OS permission and observe repair state.

Do not run Android parity work beyond shared compile/test fallout unless explicitly requested.

## 15. Checkpoint exit criteria

Checkpoint 04 is complete when:

- concrete connection completion establishes the default first-party Observe grant;
- each source connection has one Use with Floe control;
- resource selection and Observe scope stay coherent under one product interaction;
- disabling Observe does not disconnect or alter Act authority;
- external model recipient consent remains separately enforced;
- the global Settings LLM/source-use toggle is gone;
- remote server settings no longer expose ordinary source grant protocol ceremony;
- backend effective access projection is the only UI Active source;
- ADR/product update requirements are queued for checkpoint 06 or already landed with this checkpoint;
- targeted Rust/Flutter/native checks and broad FFI/Flutter gates pass.

Checkpoint 05 can now create inline permission interactions that invoke exactly the same connection/access owner operations used by the connection screen.
