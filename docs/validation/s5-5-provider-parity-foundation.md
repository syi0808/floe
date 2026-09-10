# S5.5 Provider Parity Foundation

> Date: 2026-09-11
> Acceptance status: Microsoft Mail production path; live evidence and provider breadth pending

## Delivered boundary

- Added a server-native Microsoft Graph Mail adapter for the selected user's inbox. It performs one
  bounded GET, returns at most 100 messages, caps provider and View payloads, rejects redirects and
  non-TLS remote endpoints, and never follows provider pagination links.
- The adapter requests only message/conversation identity, receive time, sender, recipients, subject,
  body preview and categories. Provider IDs become opaque handles; URLs, full bodies, attachments,
  internet headers and action authority do not enter the View.
- Filtered reads use Graph search with escaped query text and omit ordering; unfiltered reads use a
  deterministic received-time order. Query, cursor, item and text bounds fail closed before network
  use or View publication.
- The connector descriptor declares one Observe-only `mail.communication.read` capability with the
  exact `Mail.Read` scope and emits the existing strict `mail.communication` View contract.
- Typed credential, permission, rate-limit, unavailable and invalid-response failures become common
  connection snapshots. A prior View is retained only in degraded state and only until its declared
  expiry.
- Static Microsoft Mail View and snapshot fixtures pass both the Go adapter tests and shared Rust
  Communication View/connected-context validators.
- Added a dedicated Microsoft OAuth 2.0 authorization-code runtime using a random loopback callback,
  PKCE S256, five-minute state and the exact `Mail.Read` plus `offline_access` request. Access and
  refresh tokens share one Microsoft-only Keychain credential bound to the configured client ID;
  refresh rotation retains required scope and rejected refresh clears the unusable credential.
- Startup installs the OAuth-backed Microsoft Mail service when its client ID is configured. The
  management dashboard exposes login, status, cancellation and local disconnect without returning
  credentials. Because Microsoft has no token-revocation endpoint in this flow, the dashboard
  states that remote consent must be revoked from the Microsoft account.
- Microsoft snapshots join the paired connection inventory. The paired Communication View route
  prefers Gmail deterministically and falls back to Microsoft only when Gmail is absent or fails,
  keeping provider selection outside model prompts and avoiding unnecessary mailbox fan-out.
- Added a server-native Google Calendar adapter for one selected calendar. It performs a bounded
  GET-only event read for at most 32 days and 128 items, uses canonical UTC query bounds, and exposes
  only opaque evidence, bounded untrusted title, start/end and all-day state. Descriptions,
  locations, attendees, provider IDs and write authority stay outside the View.
- Added a strict provider-neutral `calendar.timeline` wire View and Rust validator with freshness,
  range, cursor, item, duplicate and byte bounds. The Google adapter's static View and connector
  snapshot cross both Rust boundaries.
- Google Calendar OAuth uses a dedicated Keychain credential and the exact Calendar read-only scope,
  separate from Gmail and Drive credentials. Typed credential, permission, rate-limit and partial
  failures retain a prior View only while fresh.
- Startup creates the Calendar OAuth runtime alongside the other isolated Google profiles. The
  dashboard owns OAuth login and one selected-calendar configuration; only that selection persists
  in private server state and credentials remain in Keychain.
- Paired clients can request the selected source through `calendar.timeline` with bounded Unix-time
  range, cursor and item limit. Unknown fields and invalid ranges fail closed, source selection is
  never accepted from the request, and the connector snapshot joins the common inventory.
- Added a Microsoft Graph Calendar adapter for one selected calendar. It reads only `calendarView`
  with an explicit time range, item limit and selected fields, requests UTC projection, and emits the
  same common `calendar.timeline` View as Google Calendar. Cancelled events, bodies, locations,
  attendees and provider identities are not promoted.
- Microsoft continuation URLs must match the configured Graph origin and calendar-view path. The
  adapter extracts only bounded `$skiptoken`/`$skip` state and reconstructs the next selected-scope
  request rather than following a provider URL.
- Microsoft Calendar has a dedicated `Calendars.Read` OAuth profile and Keychain credential isolated
  from Microsoft Mail. Static Calendar View and descriptor fixtures pass the shared Rust validators,
  and typed failures preserve only fresh cached evidence.

## Automated evidence

```sh
go -C server test -race ./internal/connectors/microsoftmail
go -C server vet ./internal/connectors/microsoftmail
go -C server test -race ./internal/microsoftauth ./internal/console ./cmd/floe-server
go -C server vet ./internal/microsoftauth ./internal/console ./cmd/floe-server
go -C server test -race ./internal/connectors/googlecalendar ./internal/googleauth
go -C server test -race ./internal/connectors/common ./internal/connectors/microsoftcalendar ./internal/microsoftauth
go -C server vet ./internal/connectors/common ./internal/connectors/microsoftcalendar ./internal/microsoftauth
go -C server test -race ./internal/console ./cmd/floe-server
go -C server vet ./internal/connectors/googlecalendar ./internal/googleauth ./internal/console ./cmd/floe-server
cargo test -p floe-agent --test communication_context --test connected_context
cargo test -p floe-agent --test calendar_context
node --check server/internal/console/web/app.js
```

## Remaining gate

No live Microsoft Graph or Google Calendar evidence was used. Microsoft Calendar still needs its
startup/product route; Android Calendar/Contacts and Health Connect adapters are absent, and the
provider-neutral route table still needs the complete parity cohort. S5.5-C3 remains pending and the
slice stays **0/14**.
