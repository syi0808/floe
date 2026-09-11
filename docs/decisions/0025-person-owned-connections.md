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

Old `clients` entries stored as a bare token hash are loaded as `legacy_unscoped`. They retain the
existing read-only `/v1` access so an upgrade does not silently disconnect an installation, but are
rejected from connector mutations. Re-pairing with Person and device identity is the migration.
The current dashboard-managed connector credentials remain a single-Person compatibility path and
must be migrated to scoped credential names when their client-driven connect endpoint is added.

## Contract

Connection snapshots may carry `connection_id`, `person_id`, and an optional `device_binding`.
During the transition, old version-1 snapshots without ownership fields remain readable; if either
ownership field is supplied, both must be valid. A device binding on a server-executed connector or
one that disagrees with the execution descriptor fails conformance.

The paired `/v1/connections` response is stamped with its authenticated Person ownership. It never
accepts a caller-supplied Person override. New `/v1/connectors/*` mutation handlers must use the
authenticated pairing scope and must not accept legacy-unscoped credentials.

## Consequences

- Connection inventory, grants, and future mutation attempts have an explicit Person boundary.
- Device permission and server credential ownership are represented without conflating them.
- Existing unscoped pairings continue read-only operation and are visibly marked for migration.
- Cross-Person server operation remains unsupported until configuration and runtime instances are
  partitioned by connection ownership.
