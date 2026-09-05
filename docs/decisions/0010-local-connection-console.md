# ADR 0010 — Local model connection console

- Status: Accepted direction; local implementation, live provider evaluation pending
- Date: 2026-09-05
- Related: ADR 0011

## Decision

Bring forward a small connection-management console, not S4 accounts, hosted
administration or synchronization. Go serves embedded HTML/CSS/JS with no frontend
dependency/install step. Network inference remains in the existing gateway;
product features retain context minimization, consent and domain validation.

The initial operating boundary is one trusted user on one Mac, bound only to
`127.0.0.1`, with an editable port. The Flutter client accepts this local address,
pairs through explicit dashboard approval, and stores its address/credential in
macOS Keychain. Remote URLs and network-address aliases are rejected; the UI
normalizes `localhost` to `127.0.0.1`. Rust independently checks the connection.

The client presents this under **Settings → Remote server** rather than as an
inference provider or a primary connection. The deployment-neutral label reserves
the client/server boundary for the future Go service. The current implementation
continues to enforce loopback transport. Enabling a remote address requires a
separate HTTPS, server-identity, account and multi-user authorization decision.

## Separate authorities

- Management: private administrator token file, exchanged for a 12-hour,
  HttpOnly/SameSite=Strict management cookie. Mutation requests require exact
  Origin and session-bound CSRF token. Host is pinned to prevent DNS rebinding.
  HTTP cookies are loopback-only; this is not a deployable public-web auth design.
- Inference: distinct random bearer per paired app; hashes only in server state.
  Management cookies cannot call inference, and app tokens cannot administer it.
  Browser Origin requests remain rejected on inference and native pairing APIs.
- Pairing: one pending request, unguessable poll proof, eight-character comparison
  code, five-minute expiry, explicit authenticated approval, bounded client count.
  The comparison code is not the polling credential. Polling never returns an
  app credential before approval. Pairing does not authorize external context transfer.
- Provider keys: macOS Security.framework Keychain entries, opaque references in
  atomic 0600 configuration. No plaintext provider-key fallback or read-back API.
  Keys stay out of app/domain storage, model payloads and logs. Endpoint/provider
  changes clear inheritance; deleting a target does not revoke the provider-side key.
- Codex: the Go server owns a PKCE OAuth callback and stores access, refresh and
  identity tokens as a single macOS Keychain credential. Existing Codex credentials
  are never copied. Tokens are not exposed to Flutter or management APIs. The
  inference adapter fixes the ChatGPT Codex endpoint, sends no tools, requires
  structured output and retains Floe's per-request external-transfer consent.

## Codex OAuth boundary

Floe follows the Codex CLI OAuth wire contract directly rather than embedding
CLIProxyAPI or launching Codex App Server. The local callback uses PKCE and state,
expires after five minutes, and accepts only the fixed callback path. Token exchange,
refresh and inference endpoints are fixed in the binary; redirects and proxy
environment inheritance are disabled. Token responses and provider failures are
bounded and redacted.

This deliberately trades the App Server's credential ownership for server-owned
routing. The OAuth client and ChatGPT Codex backend are not a general public OpenAI
API contract, so compatibility, account eligibility, subscription usage, revocation
and refresh rotation require live regression checks. Other providers remain separate
adapters; no generic OAuth broker or token read-back API is introduced.

## Operations and limits

The persisted execution layer retains explicit targets, while the console groups
configuration by provider and edits class-specific model/reasoning presets. It runs only user-triggered synthetic
tests. It shows configured/unavailable state, not fabricated provider health or usage.
Registration alone performs no inference. Missing credentials do not prevent
management startup. No automatic fallback or provider selection is added.

State writes are atomic; API-key references and hashed app tokens persist across
restarts. Management sessions and pairing attempts do not. Revocation stops future
requests; it does not cancel a request already accepted by an immutable gateway
snapshot. One process must own a state directory. Launch-on-login, remote TLS,
multi-user permissions, OAuth client configurability, key rotation UI and
cross-platform secret stores are deferred.

## References

- [CLIProxyAPI Codex OAuth reference implementation](https://github.com/router-for-me/CLIProxyAPI/blob/main/internal/auth/codex/openai_auth.go)
- [CLIProxyAPI Codex token storage model](https://github.com/router-for-me/CLIProxyAPI/blob/main/internal/auth/codex/token.go)
- [Native OAuth browser/PKCE guidance](https://www.rfc-editor.org/rfc/rfc8252.html)
- [Validation evidence](../validation/local-connections.md)
