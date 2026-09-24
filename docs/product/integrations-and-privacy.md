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

Observe and Act are separate authority classes. Completing a concrete supported source connection enables Floe's exact, product-approved first-party readers for the selected resources by default. The connection's **Use with Floe** control pauses or freshly revalidates those Observe grants without disconnecting the account or changing its selected resources.

Pairing a server by itself grants no connector Observe access. Existing connections discovered during startup are never silently promoted; a connection without current grants requires explicit review. Adding or changing resources while Use with Floe is active coordinates source selection and Observe scope as one user interaction, while provider-side drift fails closed.

Observe does not authorize write/send/create actions and does not approve a new external model recipient. Actions remain separately authorized, and source-backed data may leave its approved processing boundary only under exact-recipient authority. Third-party Experts are never added to a connection's default first-party reader set.

Recoverable blocks surface as durable review cards bound to the blocked work, never as silent failures or implied approvals. A card offers only the safe actions the backend projects for its current state: approve what was reviewed, deny it, or choose Not now, which cancels the pending review without touching owner state. Returning from OS settings or OAuth never resolves a card by itself; an explicit refresh re-validates live owner evidence first, and anything that drifted is superseded rather than approved. When a model call is blocked, the person gets a deterministic explanation of what is missing and why, produced without issuing the unauthorized generation.

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
