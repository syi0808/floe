# Integrations, privacy and distribution

## Integration fabric

Floe owns a provider-neutral integration boundary for authentication, source lifecycle, bounded observation, normalization, revisions, deletion/provenance and actions.

Integration is not a user-authored workflow graph.

```text
External source
  -> Connector / device provider
  -> bounded Floe View or domain mirror
  -> domain / Expert
  -> Manager
```

Observe and Act are separate authority classes. A successful read connection does not imply permission to use the data for AI processing or to mutate the source.

Credentials never become domain data or Expert input.

## Retention by domain

Do not mirror every external source into a global personal database.

| Data kind | Default direction |
|---|---|
| Calendar | normalized mirror where useful |
| Mail | bounded index + body/content on demand |
| Contacts | identity evidence/reference |
| Health | derived state only by default |
| current location / ETA | ephemeral context |
| weather | short-lived context cache |
| Floe Task / Note | Floe canonical data |

All derived records retain source/provenance appropriate to their use.

External content is untrusted. Mail bodies, Calendar descriptions, contact notes, documents and webhook payloads never become system instructions or persistent policy.

## Data sensitivity

Useful classifications include:

- **device-only** — e.g. voiceprint/wake audio/raw Health where practical;
- **highly sensitive** — relationship conflict, detailed health/personal episodes;
- **personal** — Timeline, Tasks, normal preferences and Memory;
- **temporary AI context** — minimized projections for an approved reasoning request.

Derived values can remain sensitive. Reducing raw data is not equivalent to declassifying it.

## Sensitive local compute

Prefer:

```text
Sensitive source
  -> local reduction
  -> bounded derived state / claim
  -> privacy projection
  -> approved consumer/model
```

Use deterministic/statistical processing, classifiers or local models according to the domain; do not assume one general local LLM is the privacy mechanism.

## Expert permissions

Third-party Experts are denied by default. Grants are semantic and scoped, such as bounded Timeline/People/Memory/derived-Health views or proposal capabilities.

- No direct database access.
- No raw OAuth/API credentials.
- No implicit unrestricted network/filesystem access.
- No direct authoritative Memory write.
- Broader package permissions require explicit re-approval.
- Action proposal permission does not bypass Actions authority.

## Device and server placement

OS/sensitive sources run on the device that owns them. Always-online SaaS connections may run on a Go server/control plane. Flutter owns connection and permission UX, not connector execution.

The server may provide pairing, identity/control-plane functions, SaaS connector runtime, bounded relay and future sync. It is not a raw device-context warehouse.

Hosted and self-hosted deployments use the same product authority boundaries. Floe Cloud can provide managed provider registrations and secrets; self-host operators bring provider registrations appropriate to their own external URL. Self-hostability should remain invisible to ordinary product usage.

For current implementation ownership, see [Architecture](../architecture/README.md) and [Authority and recovery](../architecture/authority-recovery.md).
