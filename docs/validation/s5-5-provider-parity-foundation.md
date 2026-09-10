# S5.5 Provider Parity Foundation

> Date: 2026-09-11
> Acceptance status: Microsoft Mail adapter contract; production OAuth/routing and provider breadth
> pending

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

## Automated evidence

```sh
go -C server test -race ./internal/connectors/microsoftmail
go -C server vet ./internal/connectors/microsoftmail
cargo test -p floe-agent --test communication_context --test connected_context
```

## Remaining gate

This is an adapter and conformance foundation, not a configured production connection. Microsoft
PKCE OAuth, startup/console lifecycle, provider-neutral product routing and live Microsoft Graph
evidence remain required. Google Calendar, Microsoft Calendar, Android Calendar/Contacts and Health
Connect adapters are also absent. S5.5-C3 remains pending and the slice stays **0/14**.
