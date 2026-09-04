# ADR 0010 — Local model connection console

- Status: Accepted direction; local implementation, live provider evaluation pending
- Date: 2026-09-05
- Extends: ADR 0009

## Decision

Bring forward a small connection-management console, not S4 accounts, hosted
administration or synchronization. Go serves embedded HTML/CSS/JS with no frontend
dependency/install step. Network inference remains in the existing gateway;
Rust retains context minimization, consent, proposal validation and non-mutation.

The initial operating boundary is one trusted user on one Mac, bound only to
`127.0.0.1`, with an editable port. The Flutter client accepts this local address,
pairs through explicit dashboard approval, and stores its address/credential in
macOS Keychain. Remote URLs and network-address aliases are rejected; the UI
normalizes `localhost` to `127.0.0.1`. Rust independently checks the connection.

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
- Codex: official App Server account RPCs only, isolated CODEX_HOME and working
  directory, OS credential store required, minimal environment, no inherited
  provider tokens. Existing Codex credentials are never copied. Unknown inbound
  RPCs are rejected. No thread/turn, shell, file, tool or inference methods are exposed.

## Codex evaluation gate

Official account APIs support browser login, completion notifications, cancellation,
status and logout. Floe uses these instead of reproducing OAuth endpoints/client IDs
or extracting subscription tokens. The callback and provider-token lifecycle belong
to Codex, not a new generic Floe OAuth broker.

The installed 0.153.2 runtime successfully initializes and reports no inherited
account in an isolated home. Fixture protocol tests cover login completion, including
notification/response ordering, logout and rejection of arbitrary RPCs. This is not
proof of live consent, refresh/revocation, account eligibility or inference isolation.
Codex inference is visibly disabled until those gates pass. Apple availability and
other providers' officially supported OAuth paths remain separate adapters.

## Operations and limits

The console edits explicit API/Ollama targets and runs only user-triggered synthetic
tests. It shows configured/unavailable state, not fabricated provider health or usage.
Registration alone performs no inference. Missing credentials do not prevent
management startup. No automatic fallback or provider selection is added.

State writes are atomic; API-key references and hashed app tokens persist across
restarts. Management sessions and pairing attempts do not. Revocation stops future
requests; it does not cancel a request already accepted by an immutable gateway
snapshot. One process must own a state directory. Launch-on-login, remote TLS,
multi-user permissions, key rotation UI and cross-platform secret stores are deferred.

## References

- [Official Codex App Server protocol](https://developers.openai.com/codex/app-server/)
- [Official credential-store configuration](https://openai.com/index/running-codex-safely/)
- [Native OAuth browser/PKCE guidance](https://www.rfc-editor.org/rfc/rfc8252.html)
- [Validation evidence](../validation/local-connections.md)
