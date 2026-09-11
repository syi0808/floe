# ADR 0026: Server-owned provider OAuth by deployment

## Status

Accepted

## Decision

The server that executes a network connector owns the provider OAuth flow, access and refresh
tokens, refresh lifecycle, and provider API calls. A paired Floe client may start authorization,
open the system browser, and poll an attempt, but provider credentials are never returned to it.
Every grant and stored credential is scoped to one Floe `Person` and connection.

Floe Cloud uses Floe-managed confidential OAuth application registrations. Each provider redirects
only to a fixed HTTPS endpoint on a Floe-owned domain. Client secrets and token-encryption keys live
in the deployment secret store and are never committed or included in a client build.

A self-hosted Floe server uses OAuth applications registered by its operator. Its configured,
trusted external base URL determines the exact callback URLs that the operator registers with each
provider. Floe does not attempt to add arbitrary private-server URLs to its shared provider apps,
and a central Floe callback relay is not a self-host dependency. Device Flow providers such as the
current GitHub App do not need a callback, but their resulting credentials remain owned by the
executing server.

The existing single-user local node and loopback callbacks remain a development and migration
profile, not the hosted OAuth contract. Provider configuration is supplied at deployment time.
Shared provider client IDs are not embedded into the Flutter application or the generic server
binary.

OAuth client IDs are public identifiers and can be observed in authorization URLs. Their secrecy
is not a security boundary. Hosted authorization instead relies on confidential client
authentication, exact redirect URI matching, one-time state, PKCE where supported, short-lived
attempts, and encrypted Person-scoped token storage.

## Consequences

- Floe Cloud can provide a one-click connection experience and background connector execution.
- Self-host operators must create and configure provider OAuth applications for their own domains.
- The client never transfers provider refresh tokens to a selected server.
- Local, hosted, and self-hosted profiles may use different provider client registrations.
- A future managed relay for self-hosting requires a separate security review and ADR.
