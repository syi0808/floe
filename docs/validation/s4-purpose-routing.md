# S4 purpose routing and remote inference foundation

Date: 2026-09-08. Implementation checkpoint; S4 remains **0/14**.

## Implemented

- The authenticated Go v2 API accepts `quick_response`, `everyday_assistance` or
  `deep_work`. It maps those purposes to operator-owned internal performance routes
  and rejects a v2 request that attempts to select an inference class or model.
- The Flutter app has no conversation model picker. It probes purpose availability,
  passes only `everyday_assistance` for general chat and keeps the paired-app token in
  Keychain.
- The native host selects available Apple Foundation Models first, then the configured
  Go route. A server-local model is treated as local-machine placement. An external
  provider is admitted only when the user has enabled the separate external-transfer
  switch; disabling it withdraws consent for later turns.
- The Go gateway supports the existing OpenAI-compatible API-key adapter and Ollama.
  The API key remains server-owned. The dashboard now identifies API-key auth as the
  supported production path and labels the direct Codex-client OAuth experiment as
  non-official rather than presenting it as a Floe OAuth integration.
- Each attempted provider generation receives a random trace ID. A bounded in-memory
  audit record exposes purpose, placement, transfer fact, outcome and request/response
  digests without prompt, output, credential, provider or model contents. Re-execution
  with `replay_of` is accepted only when the complete request digest matches.

## Automated evidence

```text
go test ./...
all packages passed

cargo test -p floe-protocol -p floe-ffi
48 passed; 0 failed

flutter analyze
No issues found
```

Focused Flutter tests cover free-text Calendar submission and consent persistence;
Go/Rust tests cover route serialization. Golden tests are intentionally not regenerated
by this checkpoint.

## Remaining acceptance work

- Persist trace receipts in the encrypted Person vault and add an app trace/replay
  surface. The current server ring is diagnostic only and is lost on restart.
- Validate a real remote API-key request and external outbound capture, plus signed-app
  Keychain, EventKit and Apple Foundation Models behavior on supported hardware.
- Add unavailable/consent/credential/quota-specific conversation copy and complete the
  Gmail, Contacts, location/weather, Screen Time and Health source gates.
