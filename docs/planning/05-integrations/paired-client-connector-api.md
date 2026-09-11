# Paired-client connector API

> Status: implemented local-node contract

## Ownership and authority

The Flutter client owns the connection experience: it presents the catalog, starts a connection,
opens the returned authorization URL, polls the attempt, changes a selectable source scope, and
requests cancellation or disconnection. The local Go node owns connection records, OAuth flow
state, PKCE verifier and state values, token exchange and refresh, and credential storage.

Every request derives `person_id` and `device_id` from the bearer credential issued by pairing.
Neither value is accepted in a request body, and unscoped credentials are invalid. Attempts are
bound to both the paired Person and device; connections are owned by the Person. The current local
node remains single-Person, as specified by ADR 0025.

## Catalog

`GET /v1/connectors` returns configured and unconfigured server connector types. Each item contains:

- `id`, `name`, `auth_kind`, `available`, and `status`
- fixed provider `required_scopes` and selectable `scope_fields`
- `capabilities.connect`, `cancel`, `disconnect`, and `scope_update`
- `connection_id` and the non-secret `scope` when the authenticated Person has a connection

An OAuth provider whose server client configuration is absent is returned as `unavailable`, not
omitted. Device-native connectors are supplied by the Flutter/native catalog and are not claimed
as server-executed providers by this endpoint.

## Lifecycle

All request bodies use `schema_version: 1` and reject unknown fields.
Every successful response includes the bearer-derived `person_id` and `device_id`; the client must
reject a response whose ownership differs from its saved pairing.

| Operation | Endpoint | Result |
| --- | --- | --- |
| Start | `POST /v1/connectors/{connector_id}/connect` | `201` attempt with `status` and optional `authorization_url` |
| Poll | `GET /v1/connectors/{connector_id}/connection-attempts/{attempt_id}` | Current attempt status |
| Cancel | `POST /v1/connectors/{connector_id}/connection-attempts/{attempt_id}/cancel` | Cancelled attempt; OAuth only |
| Scope | `PATCH /v1/connectors/{connector_id}/scope` | Updated selectable resource scope |
| Disconnect | `DELETE /v1/connectors/{connector_id}` | Revoked credential and removed connection |

OAuth start bodies contain only provider resource selection in `scope`. The response exposes the
provider authorization URL but never exposes the PKCE verifier, authorization code, access token,
or refresh token as API fields. The opaque provider URL necessarily carries its generated `state`
parameter; the Go runtime creates, retains, and validates that value. `cancel` and `scope_update`
return `capability_not_supported` when the catalog declares the operation unavailable.

Secret-based connectors accept the raw credential once in the start request's `secret` property.
The node stores it under a vault name derived from `person_id` and `connection_id`; the raw value is
not written to `state.json`, returned by any response, or included in application logging. GitHub,
Slack, and Home Assistant source selection is stored separately as non-secret scope metadata.

OAuth runtimes bind to the same derived Person-and-connection vault namespace before starting the
provider flow. On restart, the persisted connection record rebinds the runtime before it reads or
refreshes a token. Paired-client flows never read, write, migrate, or delete an unscoped credential
name.

Connection inventory and every `/v1/views/*` execution select runtimes by the authenticated
`person_id` and persisted `connection_id` before invoking provider code. A runtime without a live
owned connection is never used as an implicit source. Revoking the Person's last paired client
removes that Person's connections, attempts, runtimes, and scoped credentials; a subsequently
paired Person cannot inherit cached observations. Gmail clears its local index even when remote
grant revocation fails.

## Connector identifiers and selectable scope

| Connector | Authentication | Selectable scope |
| --- | --- | --- |
| `gmail` | Google OAuth PKCE | none |
| `microsoft.mail` | Microsoft OAuth PKCE | none |
| `github.issues` | one-shot PAT | `owner`, `repository` |
| `slack.conversations` | one-shot token | `channel`, optional `thread` |
| `google_drive.files` | Google OAuth PKCE | `folder_id` |
| `calendar.google` | Google OAuth PKCE | `calendar_id` |
| `calendar.microsoft` | Microsoft OAuth PKCE | `calendar_id` |
| `microsoft.teams` | Microsoft OAuth PKCE | `team_id`, `channel_id` |
| `home_assistant.states` | one-shot token | `base_url`, `entities` |

The fixed `required_scopes` are least-privilege read grants and cannot be enlarged through this API.
The management dashboard has no personal connector state or mutation endpoints. Server operators
provide non-person OAuth application capability through process configuration; only the paired
Flutter client starts, scopes, or disconnects a personal provider connection.
