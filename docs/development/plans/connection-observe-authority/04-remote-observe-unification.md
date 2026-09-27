# 04: Remote Calendar into the generic remote View path

Prerequisite: 03 complete.

This checkpoint deletes the parallel remote Calendar grant/preview/admission stack and makes calendar.timeline a normal connection-scoped remote View.

## Exit state

1. remote Calendar uses the same source preview, grant lookup, admission, read and release protocol as Mail/Work/Logistics;
2. remote Calendar grant resource is calendar.timeline:<connection-id>;
3. the server resolves the connection's current Calendar resources internally;
4. RemoteCalendarQuery, SignedCalendarPreview, RemoteCalendarSourceReference and calendar-specific RemoteGrantStore methods are deleted;
5. remote_calendar.rs and vault/remote_calendar_grants.rs are deleted;
6. no Calendar-specific grant HTTP route remains.

## 04-A: Context remote View model

Update crates/modules/context/src/application/remote_views.rs:

- include calendar.timeline in the canonical remote View set;
- validate CalendarViewQuery with existing bounded Calendar query rules;
- validate CalendarContextView with existing Calendar view validators;
- keep data class/categories correct for Calendar content/metadata semantics.

Update remote_sources.rs so Calendar goes through the same read_selected_remote_view/read_remote_view machinery and yields normal SourceView bindings.

Delete or fold read_remote_calendar_view/RemoteCalendarViewRead if they only duplicate generic source reading.

A Calendar query is domain-specific; authorization transport is not.

## 04-B: Access transport/store API deletion

Change crates/modules/access/src/ports/remote_grants.rs.

Delete:

- SignedCalendarPreview;
- RemoteCalendarQuery;
- RemoteGrantTransport::calendar_source_preview;
- RemoteGrantStore::verify_calendar_source_preview;
- activate_calendar_grant;
- find_calendar_grant;
- calendar_grant_policy;
- calendar_grant;
- pause_calendar_grant;
- calendar_grant_binding.

The generic View methods must handle calendar.timeline.

Delete crates/modules/access/src/application/remote_calendar.rs after hosted connector mapping/query semantics move to the correct Connections/Context/provider owners.

Keep producer pinning, signed source preview verification, stable source identity and current SourceAuthority checks.

## 04-C: Vault deletion

Delete:

- crates/adapters/vault/src/vault/remote_calendar_grants.rs;
- module exports/imports/tests for it.

Remote Calendar grants become ordinary DataAccessGrants discoverable by stable source + logical View resource. Do not replace the file with a forwarding wrapper.

Remote generic mapping may remain until 05 only where ConsumerPolicyAuthority still requires it. Do not add Calendar-specific columns to it.

## 04-D: Provider adapter cutover

Change:

- crates/adapters/providers/src/control/authorization.rs;
- crates/adapters/providers/src/sources/server.rs;
- AuthorizedSourceClient RemoteGrantTransport implementation.

Remove calendar_source_preview client/DTO behavior and call view_source_preview for calendar.timeline.

Authorized remote Calendar reads must use the same admitted View transport object as other Views.

## 04-E: Go server cutover

Primary files:

- server/internal/authorization/source_service.go;
- server/internal/transport/http/source.go;
- server/internal/application/calendar_admission_test.go and related tests;
- Calendar connector runtimes.

Delete the parallel Calendar authority protocol:

- CalendarAdmission;
- calendarAdmissionState;
- AdmitCalendar;
- ReadCalendar;
- calendar_source_preview operation;
- /v1/authority/calendar/source or equivalent special route;
- calendar-only admission map.

Extend generic View admission/read/release to calendar.timeline. Generic admission still carries the logical View grant resource, while the Calendar runtime resolves the connection's current configured calendar resources.

The server connection's resource configuration is authoritative. If current Calendar connectors support only one calendar_id, replace that internal scope with a bounded canonical calendar_ids resource set rather than perpetuating single-leaf permission. Connector configuration UI may still expose whatever connection setup the provider supports, but Observe permission does not add another selection.

Source/resource change advances the producer SourceAuthority/revision and invalidates old evidence. It does not require a new client grant.

## 04-F: remote multi-calendar acceptance

Add server + Rust integration coverage:

1. remote Calendar connection has at least two configured calendars;
2. one calendar.timeline grant is enabled;
3. one generic admitted read returns bounded merged timeline evidence for current resources;
4. resource set changes on server;
5. same grant remains;
6. old signed SourceAuthority/dependency fails;
7. fresh preview/read succeeds under new SourceAuthority;
8. Mail/Work/Logistics behavior is unchanged.

Also prove wrong connector/connection/producer/person and stale grant still fail before payload release.

## Residual audit

Required zero production matches:

~~~
RemoteCalendarQuery
SignedCalendarPreview
RemoteCalendarSourceReference
calendar_source_preview
activate_calendar_grant
find_calendar_grant
remote_calendar_grants
REMOTE_CALENDAR_RECIPIENT
AdmitCalendar
ReadCalendar
calendarAdmissionState
~~~

CalendarProvider/domain types and Calendar query/view validation are legitimate and remain.

## Verification

~~~
cargo test -p floe-access remote
cargo test -p floe-context remote
cargo test -p floe-provider-adapters
cargo test -p floe-vault remote
cargo test -p floe-app remote_observe
(
  cd server
  go test ./...
  go test -race ./...
)
python3 tools/architecture/check_boundaries.py
git diff --check
~~~
