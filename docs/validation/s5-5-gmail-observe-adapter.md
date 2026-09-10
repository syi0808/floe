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
- Added a paired-client, read-only `GET /v1/connections` route. It publishes the same common Gmail
  snapshot without credentials or provider-native identifiers, does not accept browser origins or
  management authentication and redacts internal snapshot failures.
- The Flutter client validates a bounded v1 connection envelope and then applies the existing strict
  common snapshot parser. Data & privacy merges server Gmail and device Calendar status, preserves
  healthy cards when only one execution location fails and refreshes both sources together.
- Added paired `POST /v1/views/mail.communication` access to the metadata projection. Its strict
  request contains only schema version, query, cursor and limit; it is unavailable when the source
  is disconnected/revoked and exposes no body or mutation authority.
- Added a strict Rust Communication View parser plus a server-backed
  `mail.communication.read` Observe capability. The capability appears only with a paired server
  route, validates freshness, size, count, pagination, labels and unique evidence handles, and
  returns provider text as untrusted capability data for a model-selected read.

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
cargo test -p floe-agent --test communication_context
cargo test -p floe-ffi communication_view_read_is_authenticated_bounded_and_validated --lib
cd apps/client
flutter test test/features/server/local_server_http_test.dart \
  test/features/server/settings_screen_test.dart \
  test/features/agent/agent_connections_test.dart
flutter analyze
```

The fixture server verifies exact GET-only paths, Bearer placement, metadata/body separation,
body authority checks, pagination, history additions/deletions, typed HTTP failures, endpoint
allowlisting and descriptor redaction. A shared JSON fixture also crosses the Go/Rust boundary and
passes the Rust connector conformance validator. Index tests cover reopen, stale checkpoint
rejection, update/delete merge, paging, hashed provenance and private-file enforcement. Sync tests
cover bootstrap catch-up, label changes, deletions, atomic checkpoints and `404` full-sync recovery.

## Remaining gate

The authenticated console owns one local Gmail connection, scheduled synchronization, shared
connection-health presentation and an on-demand Communication View capability, but no live mailbox
run has been recorded. The Manager can select the bounded Observe capability; dedicated
Commitments/Communication Expert packages, their evaluation corpus and cross-source scenarios are
not yet implemented. S5.5-C1, S5.5-C2 and implementation-order items 2–3 remain pending.
