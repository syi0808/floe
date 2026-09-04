# Floe Inference Gateway

Minimal Go network-inference module for S2. No third-party Go dependencies and no
CLIProxyAPI dependency. Architecture: [ADR 0009](../docs/decisions/0009-contextual-focus-suggestion.md).

This is a manually launched, single-operator loopback service, not the hosted
server. Accounts, sync, OAuth login/refresh, Keychain UI, Apple Foundation Models,
streaming and usage accounting remain separate work. Native-only models will not
be forced through a remote server.

## Run with a local model

Go 1.25 or newer. Copy `config.example.json` to `config.local.json` and replace
the placeholder with an already-installed Ollama model. Floe does not install or
download models. Start Ollama with cloud disabled and verify it listens on the
configured literal loopback address.

From the repository root, in one shell:

```sh
cp server/config.example.json server/config.local.json
export FLOE_INFERENCE_CONFIG="$PWD/server/config.local.json"
export FLOE_INFERENCE_TOKEN="$(openssl rand -hex 32)"
(cd server && go run ./cmd/floe-server) &
cd apps/client
flutter run -d macos --dart-define=FLOE_INFERENCE_TARGET=local-focus
```

Edit the copied config **before** starting the service. The app's focus panel
accepts `local-focus`, the target ID, not the raw model name. The target's model
and provider are operator-configured and available through authenticated
`GET /v1/targets`. Restart the gateway after changing configuration or credentials.
An app opened from Finder does not inherit this shell's token; packaged credential
provisioning is not implemented yet. Core/local preference CRUD needs no gateway.

## API-key target

An OpenAI-compatible target can be configured as:

```json
{
  "targets": {
    "api-focus": {
      "provider": "openai_compatible",
      "base_url": "https://api.openai.com/v1",
      "model": "replace-with-supported-model",
      "api_key_env": "FLOE_PROVIDER_API_KEY"
    }
  }
}
```

Inject the actual key into the Go process environment using your secret manager
or an interactive shell. Do not put it in JSON or command-line arguments. Only
the environment-variable **name** belongs in config. The app receives no provider
key. The model must support non-streaming Chat Completions JSON-schema output;
compatibility is tested per model, not assumed for all APIs or subscription plans.

Select `api-focus` and explicitly allow external transfer for the request. This
permission is required for every OpenAI-compatible target, even a loopback proxy:
being on localhost does not mean its upstream runs locally. No automatic fallback,
retry or account rotation occurs. Provider errors do not expose raw response text.

Direct OAuth integration is **not** implemented by configuring a proxy. Supported
auth flows, credential ownership and refresh/revocation must be evaluated per
provider before adding an OAuth adapter. Existing Codex credentials are not read.

## Contract

Bearer authentication is required for both endpoints; a separate random token
of at least 32 bytes is required. The listener is fixed to `127.0.0.1:8431`.
Requests with a browser Origin header are rejected. No CORS or public deployment
mode is provided. Anyone holding this local token can use registered targets:
it is not multi-user/Person authorization.

`POST /v1/generate`:

```json
{
  "schema_version": 1,
  "target": "local-focus",
  "allow_external": false,
  "instructions": "Domain-owned instructions",
  "input": {"domain_owned": "minimal context"},
  "output_schema": {"type": "object", "properties": {}}
}
```

Success: `{"schema_version":1,"target":"local-focus","model":"configured-model","output":"{...}"}`.
Failure: `{"schema_version":1,"error":{"code":"model_timeout"}}`.
Other codes include `unauthorized`, `validation`, `unknown_target`,
`external_transfer_denied`, `model_busy`, `model_unavailable`, `invalid_proposal`.

The gateway normalizes provider envelopes and checks output bounds/JSON, but
does not implement domain validation. Rust validates slots, timestamps, evidence
and source freshness after generation. No tool or mutation endpoint is exposed.

Configuration is the endpoint allowlist; clients cannot submit URLs or credentials.
HTTPS is required except literal loopback HTTP. Redirects and environment proxies
are disabled. The administrator and configured endpoint remain trusted; this is
not a sandbox around a malicious local proxy or Ollama process.

Limits: four active requests, 96 KiB request envelope, 32 KiB input and schema
each, 1 MiB provider envelope, 8 KiB returned JSON, 40-second provider deadline.
Client disconnection and process shutdown cancel upstream work. Logs contain
startup/failure summaries only, not prompts, outputs or credentials.

## Validate

```sh
go test -race ./...
go vet ./...
```

Cross-language fixture and live evaluation procedures:
[S2 validation](../docs/validation/s2-focus.md).
