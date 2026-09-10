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
- Added a private durable metadata index scoped by a hashed connection filename. Full replacement
  and checkpoint-CAS deltas are atomic; stale checkpoints cannot overwrite newer state.
- The index stores headers, labels and snippets but has no body field. It projects a bounded,
  paginated Communication View with hashed message/thread evidence handles and five-minute expiry.
  Public directories/files, symlinks, corrupt state and cross-connection state fail closed.
- Added bounded full/incremental sync orchestration. Bootstrap captures a mailbox checkpoint before
  its purpose-scoped search and then catches up from that checkpoint; incremental runs merge added,
  label-changed and deleted messages atomically. An expired checkpoint triggers a bounded full sync.
- Added a desktop Google OAuth runtime and authenticated local-console controls. It uses a random
  loopback callback, PKCE S256, five-minute state, offline access and only `gmail.readonly`.
  Tokens stay in macOS Keychain, refresh is serialized, invalid grants are cleared, changed OAuth
  client IDs cannot inherit tokens, and disconnect revokes upstream before local deletion.
- The local server now composes OAuth, sync and index as one Gmail service. It exposes authenticated
  status/manual-sync controls, refreshes every five minutes while connected, persists ready/degraded
  lifecycle evidence and removes indexed metadata after a successful revoke/disconnect.

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
go test -race ./internal/googleauth ./internal/console
cd ..
cargo test -p floe-agent --test connected_context
```

The fixture server verifies exact GET-only paths, Bearer placement, metadata/body separation,
body authority checks, pagination, history additions/deletions, typed HTTP failures, endpoint
allowlisting and descriptor redaction. A shared JSON fixture also crosses the Go/Rust boundary and
passes the Rust connector conformance validator. Index tests cover reopen, stale checkpoint
rejection, update/delete merge, paging, hashed provenance and private-file enforcement. Sync tests
cover bootstrap catch-up, label changes, deletions, atomic checkpoints and `404` full-sync recovery.

## Remaining gate

The authenticated console owns one local Gmail connection and scheduled synchronization, but no
live mailbox run has been recorded. Its snapshot is not yet transported to the client Connections
UI, and the Rust Agent runtime does not yet consume its Communication View. S5.5-C1,
S5.5-C2 and implementation-order items 2–3 remain pending.
