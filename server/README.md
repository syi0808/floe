# Floe Gateway

The Go Gateway is a single-operator service bound to a literal loopback address. It owns paired-client trust, source integrations, signed read admission/release, and operator-configured inference. Rust owns source-processing permission, model planning and tool execution. No Go Actions or provider-write API is exposed.

The server has no third-party Go dependencies. Run on macOS with Go 1.25+ and Xcode command-line tools for the Security.framework Keychain adapter:

```sh
cd server
go run ./cmd/floe-server
```

The service loads `.env` without overwriting process environment. `FLOE_ENV_FILE` selects another file. `FLOE_SERVER_ADDRESS` defaults to `127.0.0.1:8431`; `FLOE_SERVER_DATA` selects a private profile directory and otherwise defaults to `~/Library/Application Support/FloeServer`. The directory must be mode 0700; private files are 0600. Never run two processes against the same profile.

Open `http://127.0.0.1:8431/manage/` and use the administrator token in that profile's `admin-token` file. The token is never printed. Do not copy credentials into chat, logs or source control. The old shared-bearer/headless mode and `FLOE_INFERENCE_CONFIG`/`FLOE_INFERENCE_TOKEN` execution path have been removed.

## Pairing and trust

Floe generates and retains its issuer key privately, submits the public identity through `/pair/start`, verifies the signed producer challenge, and proves possession with `/pair/confirm`. All pairing operations use POST and schema 1. Start requires a stable `operation_id` and a privately staged random 32-byte base64url `proof`; subsequent requests use the exact `pairing_id`/`proof` fields. A bounded private receipt reserves each start identity before generating its challenge, so exact start replay returns the original challenge and proof after response loss. Changed identity or proof conflicts; an expired operation never creates another challenge. The operator compares the request and exact issuer fingerprint before approval. The uncommitted proof challenge expires after 30 seconds.

Trust atomically persists the app bearer hash and active issuer binding. Only after that commit may `/pair/poll` release the app token. A private Keychain pairing receipt supports the same approved polling readback after response loss or process restart. Missing private credential data returns an explicit repair error and never regenerates a bearer or issuer. `/pair/cancel` can cancel an uncommitted attempt; a committed binding must be explicitly revoked.

An interrupted `activating` receipt stays visible in the operator dashboard. Explicit Resume uses the original protected token slot, retained token digest, local proof, signed challenge and Trust revision; it never generates replacement inputs. Both Resume and Trust activation enforce the original 30-second expiry. Explicit Abort requires an authoritative Trust non-commit, retains an `aborted` receipt and leaves protected token data intact. If Trust already committed, activation wins and the active client must be explicitly revoked. Missing token/proof data, changed Trust or indeterminate persistence remain repair errors.

Rust keeps eligible RepairRequired operations observable through its bounded owner job and Ready activation. Eligibility requires the original confirmed handle/review and the exact Pending credential expectation; Forget removes that eligibility. The five-minute owner job covers every still-valid 30-second activation window. Later remote readback can rejoin an already committed operation, but cannot activate an expired challenge. Product get/Check status performs no recovery mutation.

Native credential observations honor their caller context and a three-second ceiling. One process-wide OS worker and at most eight waiting observers bound stalled Keychain work. A worker retains its lane through late mutation/readback settlement, so a timeout does not authorize deletion, absence, or an overlapping write. Security calls fail instead of displaying authentication UI and use nonsynchronizing device-only items. Pairing requests have a ten-second total observation bound; operator model probes start their 40-second bound before credential readiness.

App bearers cannot operate the dashboard, structured operator inference or traces. Operator sessions have their own expiry, rate limit, capacity and CSRF capability. Every client/issuer transition advances the shared trust generation, invalidating captured source and inference authority. Revocation persists issuer tombstones and an integration cleanup ticket before cleanup performs any provider I/O. The affected Person remains blocked until the exact durable cleanup receipt is acknowledged. A remaining same-Person client retains completed sources and unrelated attempts; last-client revocation removes that Person's sources.

Trust state, source lifecycle and operator inference configuration have separate private files: `trust.json`, `integrations.json` and `inference.json`. Producer identity is retained separately. File/key read errors never replace identities or reset data. Post-rename durability uncertainty latches the affected owner closed. This cutover requires a deliberately selected clean development profile; there is no old-format decoder or automatic reset.

## Sources

Product setup starts with a stable operation UUID and reviewed scope. Product responses contain only an operation ID, revision, setup state and `/manage/setup/<operation_id>` reference. The admitted Gateway operator completes provider OAuth or enters the provider secret on that hosted page; raw provider URLs, user codes and secrets never appear in the paired product setup response. Cancellation checks the exact remote operation revision.

Node factories open independent connection-scoped runtimes, with Keychain slots derived from connector namespace, Person and connection identity. OAuth completion, scope updates and disconnect use durable lifecycle records and revision checks. POST `/v1/connectors/{id}/disconnect` requires a stable `operation_id`, exact connection ID and revision. Its durable receipt reports `pending` or `completed`; exact replay does not issue a second source operation. Resources are canonicalized; a reordered resource set does not advance the source revision or epoch. Google and Microsoft Calendar may coexist. Each read remains bound to the exact admitted connection, revision, source incarnation/epoch and verified provider account; there is no cross-provider fallback.

Google Calendar validates the UserInfo subject, and Microsoft Calendar validates tenant/subject OIDC evidence. Missing account-identity support remains explicitly unverified and cannot authorize source reads. All source adapters use typed Readers and cached-only snapshots. Gmail, Drive, Microsoft Mail/Teams, GitHub, Slack and Home Assistant currently have no verified account-identity adapter, so those connections remain unverified and cannot authorize source reads. An unverified connection is never silently promoted or assigned a fabricated account identity.

Source catalog/list calls use copied cached metadata; they never fetch provider data or refresh OAuth. A Reader runs only after exact source admission and receives its caller cancellation/deadline and typed bounded query. Gmail index synchronization happens within that admitted read, and source cleanup clears its index before acknowledging durable runtime cleanup. Mail body acquisition remains an internal, explicitly authorized provider operation and is not advertised as an exposed View.

Day product refresh has a separate private `calendar.mirror` contract under POST `/v1/calendar/mirror/{source-preview,admit,read,release}`. Its signed operation tags are `day_calendar_source_preview`, `day_calendar_admission` and `day_calendar_release`, with purpose `day_refresh`. Product proofs contain no assistant grant or consumer; both workflows share the same Trust, exact source fences, bounded stage accounting and one-use proof engine. Preview authorizes no read. Rust local source/credential revisions are echoed separately from the actual Go source revision and issuer state.

A mirror page preserves actual provider event IDs, full bounded titles, timed timezone metadata, civil all-day dates and tagged external revision evidence. A provider revision remains opaque; a fallback observation fingerprint is equality evidence and never a conditional-write token. Remote records report no Calendar write capability. Go continuation tokens retain exact read/source/account/resource/interval and remaining budgets, and never expose or accept a caller-selected provider URL. Mirror records stay in the private product-read transport and cannot become assistant context or an Observe grant. Any source/identity drift drops the staged page.

Supported provider setup factories use:

- `FLOE_GOOGLE_OAUTH_CLIENT_ID` and optional `FLOE_GOOGLE_OAUTH_CLIENT_SECRET` for Gmail, Drive and Calendar
- `FLOE_MICROSOFT_OAUTH_CLIENT_ID` and optional `FLOE_MICROSOFT_OAUTH_CLIENT_SECRET` for Mail, Calendar and Teams
- `FLOE_GITHUB_OAUTH_CLIENT_ID` for GitHub App device authorization with Issues read-only permission
- `FLOE_SLACK_OAUTH_CLIENT_ID` and optional `FLOE_SLACK_OAUTH_CLIENT_SECRET` for Slack user authorization; its registered callback uses `http://localhost:1456/oauth/slack/callback`
- A separately supplied Home Assistant credential for the selected instance and entities

See [OAuth deployment configuration](../docs/deployment/oauth-configuration.md) for external registrations. Source credentials never enter normalized Views, Agent input, signed query payloads or diagnostics. Mail bodies remain outside the four exposed normalized View routes.

## Inference schema 2

Build the Gateway and Rust caller from the same snapshot. The wire has one schema and no compatibility decoder:

- `GET /v1/inference-purposes`: paired-client inventory with exactly `quick_response`, `everyday_assistance`, `deep_work`
- `POST /v1/agent`: paired-client invocation with purpose, opaque capability revision, fresh attempt UUID, admitted data classes, stable instructions, bounded canonical messages/tools, required `output_format` and output limit
- `POST /v1/generate`: operator-session structured invocation, using the same capability revision and accounting envelope
- `GET /v1/traces` and `/v1/traces/{trace_id}`: operator-only bounded diagnostics

Available inventory entries contain `status`, `capability_revision` and a sorted unique capability set containing `chat`, with optional `structured_output` and `tool_proposals`. The operator explicitly declares support for each configured model and endpoint; new forms initially select only chat. Provider families, model names and successful text probes do not establish those features. Private configuration requires the declaration and old undeclared configurations fail closed. Explicitly disabled or unconfigured purposes have only their status. Configured provider/account failures and missing required features are errors, never an absence or local-fallback signal. The opaque revision is derived with a private HMAC and binds the current purpose, configured target, effort, declared capabilities, configuration generation and provider/account identity. The service checks it before provider dispatch and before output release.

Successful Agent output is a typed array of `preamble`, `answer` and `call` steps. All metadata is echoed exactly; call IDs preserve output order. Usage is `{tokens:integer|null,cost_micros:integer|null}`: reported zero is valid and missing dimensions remain null. Structured output is an object validated against the requested bounded schema. The Gateway neither executes proposed tools nor fabricates usage. Provider-native replay state does not cross the paired wire, and a lost response never triggers an automatic resend.

Agent `output_format` is exactly `{"kind":"text"}` or `{"kind":"json","schema":...}` and must match the first canonical run frame's declaration. JSON mode requires empty tools and exactly one `answer` containing the validated object. Its portable schema is bounded to 16 KiB, depth eight, 256 nodes and closed objects; optional fields may be absent, but present nulls, nullable schemas, references and schema unions are unsupported. Domain schemas pass unchanged to provider output controls and are validated again before release. Codex Agent message interpretation stays separate from its operator structured-input mode. The existing operator `/v1/generate` schema remains separate. See the [DeviceModel contract](../docs/plans/2026-10-03-device-model-contract.md) for the exact portable vocabulary.

Inference request bodies are limited to 98,304 bytes, and responses to 65,536 bytes. Decoding rejects invalid UTF-8, duplicate keys, unknown fixed-shape fields, forbidden nulls, non-integral/out-of-range DTO numbers, trailing values and excessive nesting. Agent input is limited to 32,768 bytes; generated Agent steps to the requested limit up to 16,384 bytes. Inference errors have schema 2, a stable code, required nullable trace/attempt/purpose/capability-revision correlation and required nullable usage dimensions. Only an admitted execution can provide correlation and observed usage, including when its output fails schema validation; pre-admission errors report nulls. Provider error text, prompts, source content and credentials never enter traces.

Paired routes reject all nonempty Origin headers. Operator mutations require the admitted dashboard session, matching Origin and CSRF. No CORS is enabled. Provider endpoints require HTTPS except literal-loopback local Ollama; redirects and environment proxies are disabled. The operator controls targets/models in the dashboard; product callers cannot select endpoints, accounts, model names or reasoning effort. Explicit connection probes use the separate operator-only `ProbeTarget` action with fixed bounded synthetic Agent input. A target may be probed before any purpose route selects it; probes do not fabricate a product principal or bypass product capability checks. Startup performs local initialization checks and does not dispatch model probes.

## Ownership and verification

`trust` owns immutable principals, issuer/producer identity, sessions and durable revocation. `pairing` owns temporary proof/approval state and private committed readback. `integrations` owns connection lifecycle, scopes and cleanup. `views` owns normalized payloads, typed queries and bounded validation. `authority` owns one-use source admission/staging/release. `inference` owns durable operator configuration, purpose selection, account management and accounting; concrete adapters live under `inference/providers` and `inference/codex`. `transport/http` owns framing, routes, cookies and redacted DTOs. `node` composes these owners and real connector factories.

During the coordinated architecture cutover, formatting/compilation runs only at the completed slice or full structural checkpoint authorized by the execution plan. S2 changes wait for the full G2 boundary; tests remain deferred to S3. Final server qualification after the new S3 tests are written uses:

```sh
gofmt -w <changed-go-files>
go test -race ./...
go vet ./...
```

macOS Keychain and real provider OAuth behavior require their platform/provider prerequisites. A successful Linux compile cannot establish those behaviors.
