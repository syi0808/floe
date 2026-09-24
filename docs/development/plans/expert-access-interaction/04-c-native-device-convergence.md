# Checkpoint 04-C — Native/device connection convergence

- **Execution baseline:** 04-B completion
- **Depends on:** connection Observe projection and canonical first-party policy
- **Goal:** make Apple/native connection detail the single source Observe editor and make explicit connection/resource completion establish default Observe where the source has stable grant semantics.
- **Platform:** Apple/macOS first. Do not expand Android parity.

---

## 1. Baseline anchors

### Calendar

- ConnectorScreen owns device Calendar connection/resource selection.
- AppWireNativeCalendarAccessGateway separately exposes inspect/preview/review/pause/remove.
- UI renders a native Observe card with explicit protocol-shaped grant actions.

### Apple/local connections

_AppleConnectionDetail already owns:

- system access/recovery;
- Contacts access card;
- Attention access card;
- Feasibility access card;
- Wellbeing access card.

This is the correct product location.

### Duplicate Settings surface

Data & privacy still contains source/system controls and Settings-owned source presentation types.

---

## 2. Move Observe UI ownership to Connections

Move/rename reusable source controls from settings-oriented paths into Connections.

A reasonable shape:

~~~text
features/connections/
  domain/connection_observe.dart
  application/connection_observe_gateway.dart
  presentation/connection_observe_control.dart
~~~

Exact file names may differ.

After cutover:

- Settings does not instantiate personal source Observe cards;
- connection detail is canonical editor;
- tests move with the owning feature.

Do not duplicate widgets merely to avoid import moves.

---

## 3. One Use with Floe presentation

For stable connection-level sources, connection detail presents:

~~~text
System access / account state
Resource selection (when any)

Use with Floe  [switch]
status / recovery detail
~~~

Switch reads/writes the 04-B projection.

Do not expose:

- grant id;
- grant epoch;
- consumer list;
- ConsumerPolicyAuthority;
- Review grant;
- Preview grant;
- LLM access.

Recovery copy may say:

- Allow system access;
- Reconnect;
- Review changed source;
- Choose resources.

---

## 4. Native Calendar

### Explicit fresh connection

Target:

~~~text
request EventKit permission
select calendars
bind/update current CalendarConnection
fresh subject observation
default first-party Observe review
return Active if all succeeds
~~~

The source connection must exist before grant binding.

If connection succeeds but Observe review fails:

- keep valid Calendar connection/resource selection;
- show NeedsReview;
- do not background-retry on every inspection;
- do not present Active.

### Existing old profile

Connected Calendar with no grant:

~~~text
inspect -> NeedsReview
~~~

No auto-grant.

Explicit On performs fresh review.

### Resource edit

When Active:

- update connection/source authority/revision under current owner;
- fresh subject validation;
- review exact new resource set;
- old dependency/grant authority fails current checks.

When Paused:

- update selection;
- keep Observe paused;
- later On reviews latest selection.

### Off

Pause only. Do not disconnect Calendar or change EventKit permission.

### Disconnect

Revoke/invalidate Observe first, then remove Calendar connection.

---

## 5. Attention

Baseline UI exposes consumer choice:

~~~text
assistant
attention.expert
~~~

Remove it.

04-A App policy supplies current first-party consumers.

After an explicit source enable/recovery with a stable current subject, default review may activate Observe.

Do not auto-grant an old ready attention connection discovered at startup.

Off preserves native source/presence state.

---

## 6. Contacts

Contacts has real resource selection: selected identity handles.

Product interaction:

~~~text
allow Contacts system access
select handles/resources
default Use with Floe on for this explicit selection
~~~

Requirements:

- client sends selected handles, never consumer list;
- App derives consumers;
- selected handles and grant metadata converge after native observation;
- removing handles invalidates old dependency;
- Off preserves handle selection/system permission.

If selected handles currently live only in Access policy metadata, do not add a second persisted UI copy. State clearly which owner owns selection.

---

## 7. Wellbeing / Health

Health/Wellbeing is a fixed bounded derived resource.

After explicit system permission/recovery and fresh subject validation, default Observe may become Active.

Remove caller-selected consumers.

Off pauses grant only.

Do not broaden from derived wellbeing to raw health records.

---

## 8. Feasibility

Feasibility is query-bound at baseline.

Do not fake connection-wide default Observe.

Connection detail remains its editor, but product state must accurately say a concrete feasibility query must be reviewed.

Remove duplicate Settings presentation.

Keep exact query/evidence binding until a separate authority decision changes it.

---

## 9. Personal access wire cleanup

After App owns consumers, remove consumers from product-facing:

- PersonalAccessChangeDto::Review;
- ContactsAccessChangeDto::Review;
- Flutter AgentPersonalAccessGateway review arguments;
- source cards/checkboxes.

Internal Access config receives canonical set from App.

If PersonalAccessOverview.consumers has no remaining product use, remove it from DTO/client domain. It may remain inside DataAccessGrant scope.

Do not keep a read-only consumer list only because old editor displayed it.

---

## 10. Android posture

Do not add Android parity.

When Settings ceases to be source editor:

- shared UI may stop exposing dormant Android-only source controls until Connections owns them later;
- keep shared Dart compiling;
- do not create new Android native implementation.

Existing dormant Android adapters may remain.

---

## 11. Tests

### Calendar

- fresh explicit connect/resource selection -> Active default Observe;
- old connected/no-grant profile -> NeedsReview without mutation;
- Off pauses without disconnect;
- On revalidates current source;
- resource expansion/narrowing changes exact grant scope;
- stale reviewed connection/grant -> Conflict/NeedsReview;
- disconnect revokes then removes connection;
- Registry revision and ActionAuthority unchanged.

### Contacts

- selected handles + explicit connection -> active grant with App-derived consumers;
- caller cannot forge consumer field because field is gone;
- removing handle invalidates old dependency;
- Off preserves handles.

### Attention/Wellbeing

- explicit recovery can establish default Observe;
- startup inspection cannot;
- no consumer picker;
- subject drift requires fresh review.

### Flutter

- one Use with Floe control for stable sources;
- no raw grant terminology;
- Settings no longer owns these cards;
- system access and Floe Observe are visually distinct.

---

## 12. Residual gate

~~~sh
rg -n 'selectedConsumers|Allow attention data for|Review and enable|Review again|AgentPersonalAccessSettings|consumers.*PersonalAccess|consumers.*ContactsAccess' apps/client crates/bindings crates/app
~~~

Every product-facing consumer editor must be gone.

~~~sh
rg -n 'device-calendar-observe|reviewCalendarAccess|pauseCalendarAccess|removeCalendarAccess' apps/client
~~~

Low-level methods may remain only behind canonical connection Observe coordinator. ConnectorScreen must not render a second raw grant editor.

---

## 13. Verification

~~~sh
cargo test -p floe-access
cargo test -p floe-app
cargo test -p floe-protocol
cargo build -p floe-ffi
git diff --check

cd apps/client
flutter analyze
flutter test
~~~

Run focused macOS/EventKit tests if Runner/native code changes.

Do not start 04-D until native/device product state has one editor and no caller-selected consumer policy.
