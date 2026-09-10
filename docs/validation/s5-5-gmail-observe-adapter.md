# S5.5 Gmail Observe Adapter Foundation

> Date: 2026-09-10  
> Acceptance status: adapter contract and HTTP conformance only; live OAuth gate pending

## Delivered boundary

- Added a server-native Gmail REST adapter for bounded search, metadata reads, explicitly
  authorized on-demand plain-text body reads and incremental mailbox changes.
- Search and change cursors are opaque and bounded. Message, thread and history identifiers are
  validated before use; redirects, environment proxies, non-TLS remote endpoints, oversized
  envelopes and unexpected response shapes fail closed.
- Credential expiry, rate limiting, expired history checkpoints, invalid responses and provider
  unavailability remain typed. A Gmail history `404` requires a later full-sync recovery rather
  than being interpreted as an empty mailbox.
- The common connector descriptor publishes only Observe capabilities over
  `mail.communication` and ephemeral `mail.body` Views. It grants no draft/send/archive authority.
- Connection snapshots contain lifecycle, scope and typed failure metadata only. Source handles
  hash account/resource identifiers instead of projecting provider-native identifiers.

The request shapes follow Google's current
[messages.list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/list),
[messages.get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/get)
and [history.list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.history/list)
contracts.

## Automated evidence

```sh
cd server
go test -race ./internal/connectors/gmail
go vet ./internal/connectors/gmail
cd ..
cargo test -p floe-agent --test connected_context
```

The fixture server verifies exact GET-only paths, Bearer placement, metadata/body separation,
body authority checks, pagination, history additions/deletions, typed HTTP failures, endpoint
allowlisting and descriptor redaction. A shared JSON fixture also crosses the Go/Rust boundary and
passes the Rust connector conformance validator.

## Remaining gate

The adapter is not registered in the local console, has no Google OAuth flow or durable index and
has not run against a real mailbox. The Rust Agent runtime does not yet consume its Communication
View, and full-sync recovery after an expired history checkpoint is not implemented. S5.5-C1,
S5.5-C2 and implementation-order items 2–3 remain pending.
