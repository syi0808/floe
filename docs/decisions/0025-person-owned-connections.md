# ADR 0025: Person-owned connections and device bindings

## Status

Accepted

## Decision

Every new connector connection is owned by one Floe `Person`. A connection record contains a stable
`connection_id`, its connector type, and `person_id`. Device-native sources additionally contain a
`device_binding.device_id`; the binding must match the device declared by the connector execution
descriptor. Apple Calendar is therefore a Person connection whose execution authority is bound to
the Apple device that holds EventKit permission, not a server-owned iCloud credential.

Paired app credentials are issued with the same `person_id` and a `device_id`. The local Go node is
currently deliberately constrained to one Person: another Person cannot pair while scoped clients
exist. This preserves the existing local-node product without pretending that global connector
configuration is safe for multiple people. Future multi-person support must partition runtime
instances and connector configuration before relaxing this gate.

Provider secrets use a credential-store name derived from both `person_id` and `connection_id`.
`credentials.ConnectionName` is the only supported constructor for new connector credentials; raw
secrets remain outside `state.json`. OAuth flow state and token exchange remain server-owned.

Every persisted `clients` entry must contain a token hash, `person_id`, and `device_id`. The server
rejects unscoped state instead of accepting or migrating it implicitly. Personal connector setup,
scope changes, OAuth actions, and disconnects are available only through the paired-client API.
The management dashboard configures non-person inference capability and approves or revokes paired
apps; it does not expose personal connector state or mutations.

## Contract

Connection inventory binds runtime snapshots to the authenticated Person record. A device binding
on a server-executed connector or one that disagrees with the execution descriptor fails
conformance.

Paired `/v1/connections` and `/v1/connectors/*` responses carry the authenticated `person_id` and
`device_id`. They never accept a caller-supplied ownership override, and unscoped credentials are
invalid for both reads and mutations.

## Consequences

- Connection inventory, grants, and future mutation attempts have an explicit Person boundary.
- Device permission and server credential ownership are represented without conflating them.
- Unscoped persisted pairings fail server-state validation and must be paired again explicitly.
- Cross-Person server operation remains unsupported until configuration and runtime instances are
  partitioned by connection ownership.
