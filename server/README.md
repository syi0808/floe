# Floe Inference Gateway

Minimal Go network-inference module for S2. No third-party Go dependencies and no
CLIProxyAPI dependency. Architecture: [ADR 0009](../docs/decisions/0009-contextual-focus-suggestion.md).

This is a manually launched, single-operator loopback service, not the hosted
server. The local console adds target registration, macOS Keychain credentials,
app pairing, and server-owned Codex OAuth ([ADR 0010](../docs/decisions/0010-local-connection-console.md)).
Accounts, sync, Apple Foundation Models, general streaming and usage
accounting remain separate work. Native models will not be forced through a remote server.

## Local dashboard and app pairing

On macOS with Go 1.25+ and Xcode command-line tools (cgo/Security.framework):

```sh
cd server
env -u FLOE_INFERENCE_CONFIG -u FLOE_INFERENCE_TOKEN go run ./cmd/floe-server
```

Open `http://127.0.0.1:8431/manage/`. Unlock with the administrator token in
`~/Library/Application Support/FloeServer/admin-token`. Keep this file private;
do not paste its contents into chat, logs or committed files. The server prints
only its path. The containing directory must have mode 0700; generated files are 0600.

1. In Floe, open **Settings → Remote server**.
2. Enter `http://127.0.0.1:8431` (or `http://localhost:8431`, normalized to the literal address).
3. Click **Pair this device**, compare the eight-character code in the dashboard,
   and approve only the request you initiated. Requests expire after five minutes.
4. The app confirms connectivity and saves its credential/address to Keychain.
   Finder launches now work without shared shell environment variables.
5. Add an API or already-installed Ollama model target in the dashboard. No provider
   request occurs merely by saving a target. Use **Test connection** for an explicit
   synthetic request; external providers require confirmation and may charge for it.
6. Map **High effort** to that target under **Performance classes**. Floe chooses
   this class for focus suggestions without exposing models or routes in the app.
7. Click **Check connection** in Floe. The app reports whether the server has the
   required class configured; model selection remains in this dashboard.

App pairing tokens never grant management access. **Forget connection** removes
only the app's local copy; **Revoke** in the dashboard invalidates the token on the
server. Removing a target deletes its Keychain entry; this does not revoke an API
key at the provider. A changed provider or endpoint never inherits an old API key.

`FLOE_SERVER_ADDRESS=127.0.0.1:9431` changes the local port. `FLOE_SERVER_DATA` sets
a private state directory and explicitly selects console mode. No LAN, remote
deployment, TLS termination or multi-user authorization is supported in this slice.
Do not share one data directory between concurrently running server processes.

### Codex / ChatGPT OAuth

**Start browser login** runs a PKCE OAuth flow owned by the Go server, using a
five-minute callback listener on `localhost:1455`. Access, refresh and identity
tokens are stored as one macOS Keychain credential and are never returned through
the management or app APIs. **Disconnect Codex** deletes that credential.

After connecting, add a `Codex / ChatGPT OAuth` target and enter a model available
to the account. The endpoint and credential are fixed server-side. Requests go to
the ChatGPT Codex Responses backend with an empty tool list, `tool_choice: none`,
bounded context, structured output and the same per-request external-transfer
consent as other network providers. The server refreshes expiring access tokens and
stores the rotated result in Keychain. This integration follows the Codex CLI OAuth
wire contract directly rather than launching `codex app-server`; upstream protocol,
model availability and subscription limits can change and require live retesting.

## Legacy headless mode

The environment-configured mode below remains for existing development fixtures.
Set `FLOE_INFERENCE_CONFIG` and leave `FLOE_SERVER_DATA` unset to select it. It has
no browser dashboard and requires the shared gateway token as before.

## Run with server-side Ollama (legacy)

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
```

Edit the copied config **before** starting the service. The app's focus feature
requests `high_effort`; the gateway resolves that class to `local-focus`. Target,
model, provider and reasoning effort are operator-only configuration. Apps can see
only class availability and data-boundary metadata through authenticated
`GET /v1/inference-classes`. Restart the gateway after changing configuration or credentials.
This headless path exists for fixtures and direct development clients; native Floe
uses console pairing instead. Device-local models execute inside the client runtime
and do not use this gateway. Core/local preference CRUD needs no gateway.

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
  },
  "routes": {
    "high_effort": {
      "target": "api-focus",
      "reasoning_effort": "high"
    }
  }
}
```

Inject the actual key into the Go process environment using your secret manager
or an interactive shell. Do not put it in JSON or command-line arguments. Only
the environment-variable **name** belongs in config. The app receives no provider
key. The model must support non-streaming Chat Completions JSON-schema output;
compatibility is tested per model, not assumed for all APIs or subscription plans.

Map `high_effort` to `api-focus` and explicitly allow external transfer for the
request. This permission is required for every OpenAI-compatible target, even a loopback proxy:
being on localhost does not mean its upstream runs locally. No automatic fallback,
retry or account rotation occurs. Provider errors do not expose raw response text.

Direct OAuth integration is **not** implemented by configuring a proxy. Supported
auth flows, credential ownership and refresh/revocation must be evaluated per
provider before adding an OAuth adapter. Existing Codex credentials are not read.

## Inference contract

Bearer authentication is required for both endpoints. Console mode issues distinct
app tokens; legacy mode uses the shared developer token. The default is `127.0.0.1:8431`.
Requests with a browser Origin header are rejected. No CORS or public deployment
mode is provided. Anyone holding this local token can use registered targets:
it is not multi-user/Person authorization.

`POST /v1/generate`:

```json
{
  "schema_version": 1,
  "inference_class": "high_effort",
  "allow_external": false,
  "instructions": "Domain-owned instructions",
  "input": {"domain_owned": "minimal context"},
  "output_schema": {"type": "object", "properties": {}}
}
```

Success: `{"schema_version":1,"inference_class":"high_effort","output":"{...}"}`.
Failure: `{"schema_version":1,"error":{"code":"model_timeout"}}`.
Other codes include `unauthorized`, `validation`, `inference_class_unavailable`,
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
FLOE_TEST_KEYCHAIN=1 go test ./internal/credentials
```

Cross-language fixture and live evaluation procedures:
[S2 validation](../docs/validation/s2-focus.md).
Console/pairing specifics: [local connection validation](../docs/validation/local-connections.md).
