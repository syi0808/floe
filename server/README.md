# Floe Inference Gateway

Minimal Go network-inference module with no third-party Go dependencies and no
CLIProxyAPI dependency. Architecture: [ADR 0011](../docs/decisions/0011-inference-performance-classes.md).

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
5. Select a provider. Codex uses OAuth; OpenAI-compatible APIs use an endpoint and
   optional Keychain-backed API key. Claude OAuth is visible but not implemented.
6. Under that provider, set a model and reasoning effort for **High effort**. The
   Codex model field offers suggestions from the current public Codex catalog and
   also accepts a custom model identifier. The curated list was refreshed from
   OpenAI's `codex-rs/models-manager/models.json` on 2026-09-05; account-specific
   availability still requires a connection test.
   Saving makes this provider active for every non-empty class in the form.
7. Click **Check connection** in Floe to verify the paired credential and server
   reachability. Model selection remains in this dashboard.

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

After connecting, select Codex and enter or choose a model for each desired
performance class. Suggested model IDs are conveniences, not an account capability
check; custom IDs remain available. The endpoint and credential are fixed server-side. Requests go to
the ChatGPT Codex Responses backend with an empty tool list, `tool_choice: none`,
bounded context, structured output and the same per-request external-transfer
consent as other network providers. The server refreshes expiring access tokens and
stores the rotated result in Keychain. This integration follows the Codex CLI OAuth
wire contract directly rather than launching `codex app-server`; upstream protocol,
model availability and subscription limits can change and require live retesting.

### Gmail read-only OAuth

Create a Google **Desktop app** OAuth client in a project with the Gmail API enabled, then start
the local node with its client ID. The client secret is optional for installed-app clients and,
when supplied, must come from the process environment rather than repository configuration.

```sh
export FLOE_GOOGLE_OAUTH_CLIENT_ID='your-desktop-client-id'
export FLOE_GOOGLE_OAUTH_CLIENT_SECRET='optional-client-secret'
go run ./cmd/floe-server
```

In the Floe app, open **Connections**, select Gmail, and start the browser login. Floe opens no
embedded webview: the returned authorization URL must be opened in the system browser. The flow uses a
random loopback callback port, PKCE S256, five-minute state, offline access and only
`gmail.readonly`. Access and refresh tokens are stored together in macOS Keychain and are never
returned to the app. Disconnect first revokes the Google grant and then deletes the
Person-and-connection-scoped credential. Changing the configured client ID never inherits an older client's tokens.

The connector performs a bounded purpose-filtered metadata refresh every five minutes while
connected. `FLOE_GMAIL_QUERY` can replace the default
`newer_than:30d -in:spam -in:trash` query. OAuth setup and a live mailbox run are still required
before Gmail evidence is available to Floe clients.

The same Google client registration also enables a separate **Google Calendar** connection in the
app. That flow stores an independently scoped credential and requests only
`calendar.readonly`. Select one calendar ID in the app; paired clients may then request a
bounded `calendar.timeline` range of up to 32 days. Floe exports title and timing only—calendar IDs,
provider event IDs, descriptions, locations and attendees are excluded from Agent context.

### Microsoft Mail read-only OAuth

Register a Microsoft identity-platform application that allows public-client loopback redirects,
then configure the local node with its client ID. A client secret is optional and, when present,
must stay in the process environment.

```sh
export FLOE_MICROSOFT_OAUTH_CLIENT_ID='your-application-client-id'
export FLOE_MICROSOFT_OAUTH_CLIENT_SECRET='optional-client-secret'
go run ./cmd/floe-server
```

Use **Connections → Microsoft Mail → Connect** in the Floe app. The PKCE flow
requests only `Mail.Read` and `offline_access`, stores the resulting credential under a separate
Person-and-connection-scoped Keychain name, and binds stored tokens to the configured client ID. The paired communication
route uses Gmail first when both providers are configured and falls back to Microsoft on absence or
failure. Disconnect in Floe deletes the local credential; revoke the application's consent in the
Microsoft account when the remote grant must also be invalidated.

The same Microsoft application registration enables a separate **Microsoft Calendar** connection
with its own Person-scoped Keychain credential and exact `Calendars.Read` scope. Select
one calendar ID in the app. The paired timeline route tries configured Google Calendar first
and Microsoft Calendar second, returning the first healthy bounded View without exposing routing
policy, calendar IDs, bodies, locations or attendees to Agent context.

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

Edit the copied config **before** starting the service. Product features request a
product purpose; the gateway resolves it to an operator-managed target. Target,
model, provider and reasoning effort are operator-only configuration. Apps can see
only purpose availability and data-boundary metadata through authenticated
`GET /v1/inference-purposes`. Restart the gateway after changing configuration or credentials.
This headless path exists for fixtures and direct development clients; native Floe
uses console pairing instead. Device-local models execute inside the client runtime
and do not use this gateway.

## API-key target

An OpenAI-compatible target can be configured as:

```json
{
  "targets": {
    "api-model": {
      "provider": "openai_compatible",
      "base_url": "https://api.openai.com/v1",
      "model": "replace-with-supported-model",
      "api_key_env": "FLOE_PROVIDER_API_KEY"
    }
  },
  "routes": {
    "high_effort": {
      "target": "api-model",
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

Map the required performance class to `api-model` and explicitly allow external transfer for the
request. This permission is required for every OpenAI-compatible target, even a loopback proxy:
being on localhost does not mean its upstream runs locally. No automatic fallback,
retry or account rotation occurs. Provider errors do not expose raw response text.

Direct OAuth integration is **not** implemented by configuring a proxy. Supported
auth flows, credential ownership and refresh/revocation must be evaluated per
provider before adding an OAuth adapter. Existing Codex credentials are not read.

## Inference contract

Agent conversations now use `POST /v1/agent` (schema version 1), with native
`input.messages` and `input.tools`, rather than a model-authored JSON step envelope.
The gateway normalizes provider output; Rust alone executes tools and owns the
single output/schema correction retry. Structured generation uses `/v1/generate`, also with schema version 1. Legacy class-selected APIs are removed.
Build and deploy the matching server and client together; v2/v3 aliases are not supported. Native tool support is required
on the selected route; there is no automatic downgrade or provider fallback.
See [ADR 0016](../docs/decisions/0016-native-agent-model-protocol.md) for the migration
boundary and the remaining session/replay/UI work.

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
does not implement feature-specific domain validation. No tool or mutation endpoint is exposed.

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

Console/pairing specifics: [local connection validation](../docs/validation/local-connections.md).
