# ADR 0009: Go inference gateway and bounded focus suggestions

- **Date:** 2026-09-05
- **Status:** accepted architecture; live provider evaluation pending

## Context

The user requested S2 implementation, then clarified that CLIProxyAPI was a
reference rather than an approved drop-in dependency. The user approved a
Floe-owned thin Go inference module, deferred CLIProxyAPI adoption, explicit
device-local execution, and bringing forward only the minimum S2 gateway.

S1's live verification remains unfinished. Starting S2 is an implementation-order
exception, not an acceptance override. S1's unfinished implementation/verification
is Deferred, and S2 is the sole active implementation slice. S2 cannot become
Verified until S1 and real-model evaluation pass.

## Responsibility boundary

```text
Schedule / Memory / Manager
  ├─ select target, minimize context, specify allowed egress and capabilities
  ├─ Go inference gateway → network model providers
  └─ Native executor → device-only models (future adapter)

Rust host → validate returned proposal → Flutter presentation
```

- Domain services own model choice, sensitivity and business semantics. The
  gateway does not guess privacy, rewrite domain prompts, optimize model choice,
  or silently select a fallback.
- Go owns network provider authentication, transport, deadline/cancellation,
  protocol normalization and bounded execution. Future streaming/usage accounting
  belongs here, but is not implemented by the first non-streaming slice.
- Native models such as Apple Foundation Models execute on the device, without a
  mandatory trip through a remote gateway. The native adapter and capability PoC
  remain deferred, not a fake selectable provider.
- Rust owns Person-scoped preferences, calendar context, proposal validation and
  mutation authority. Flutter owns input, disclosure and presentation.
- CLIProxyAPI is not installed, embedded, forked or required. OAuth adapters need
  provider-specific supported-path, refresh/revocation and credential-isolation
  verification before adoption. We do not read existing Codex login files or
  assume a generic OpenAI-compatible endpoint proves OAuth support.

## First implementation

The Go module lives under `server/`, has no third-party Go dependencies, and
runs explicitly as a loopback-only developer service on `127.0.0.1:8431`.
This is not the hosted/multi-user server, resident Device Agent or S4 sync slice.

- Startup configuration registers named targets: provider, endpoint, model and
  optional environment-variable credential reference. Requests cannot supply
  arbitrary endpoints, credentials or dynamically register providers.
- OpenAI-compatible Chat Completions and loopback Ollama adapters are included.
  Only structured, non-streaming generation without tools is supported.
- Remote/API targets require per-request external-transfer permission, even when
  accessed through a local proxy. The app defaults this permission off.
- Ollama must be loopback-local; known cloud tags and remote-model metadata from
  `/api/show` are rejected before schedule context is sent. The local runtime
  operator remains trusted; disable Ollama cloud for local-only operation.
- No automatic retry, provider/account rotation, redirects, environment proxies
  or local-to-cloud fallback. HTTPS is required except literal loopback HTTP.
- A separate bearer token authenticates the app to the gateway. Provider API keys
  stay in the Go process and out of the Rust/Flutter protocol, model context and
  domain storage. Environment injection is developer provisioning, not the final
  Keychain/vault-backed product credential UI.
- Four concurrent requests maximum; 40-second Go deadline, 42-second HTTP client
  bound and 45-second domain timeout. Client cancellation propagates upstream.
  Raw requests, outputs, credentials and upstream errors are not logged.

## S2 domain contract

One user-entered focus-window preference per Person has start/end minutes and a
15–240 minute duration. Rust/Turso compare-and-swap revisions protect edits and
deletion. A deletion retains only a metadata tombstone, never the old value.
Without a saved preference the default is 09:00–18:00 / 60 minutes, not memory.

The host builds a bounded view of date, fixed timezone offset, busy intervals,
valid candidate slots, explicit preference, generic source IDs and cache warning.
Titles, event/Person/account IDs, notes, tasks and provider credentials are omitted
from model input. Local evidence labels retain recognizable source information.

The model chooses a supplied slot ID and emits a reason plus required source IDs.
The host rejects unknown fields, fabricated slots/sources, missing/duplicate
sources, malformed/oversized output and elapsed slots. It rereads the source
schedule and preference after inference and discards responses to changed state.
Canonical times/evidence come from the host. Suggestions never create or update
an event/task and are not S3 executable action proposals.

## Known limits and deferred work

- Live model quality, OAuth lifecycle, Apple model availability, credential vault
  integration and native UI review remain unverified.
- All-day events conservatively block the day. Candidate starts are sampled every
  15 minutes from the first available minute; this is not global optimization.
- The existing DayQuery fixed-offset contract does not provide IANA/DST handling.
- Missing/failed/out-of-range/older-than-15-minute Calendar cache produces a warning,
  not a guarantee of availability. Existing S1 collection limitations still apply.
- Free-text reason grounding needs real-model evaluation; schema validation and
  context provenance are not proof that the explanation is true.
- Proposals are ephemeral and not live-refreshed after display. S3 must revalidate
  at approval/execution time.
- The current FFI worker serializes requests. Network work does not block Flutter
  rendering but delays other core requests. Closing a dialog discards late results;
  it does not yet cancel the in-flight FFI request immediately.
- Gateway registration and startup are manual developer setup. No automatic
  installation, model downloading or launch-on-login service is introduced.
- The gateway token is for one trusted local operator, not Person-level server
  authorization. Remote deployment and multi-user credential isolation require
  separate design/PoCs before real data is used.

## References

- [Model layer](../planning/03-intelligence/model-layer.md)
- [Server stack](../planning/09-implementation/server-stack.md)
- [Gateway setup and wire contract](../../server/README.md)
- [S2 validation](../validation/s2-focus.md)
- [OpenAI Chat API](https://developers.openai.com/api/reference/resources/chat)
- [Ollama chat API](https://docs.ollama.com/api/chat)
