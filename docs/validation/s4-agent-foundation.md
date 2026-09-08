# S4 Agent contract foundation

Date: 2026-09-07. Evidence: deterministic fixture, macOS arm64.

This is the initial foundation checkpoint. The subsequent
[sample panel and incremental transport checkpoint](s4-agent-panel.md) supersedes
its statements about missing UI/stop transport; the privacy and live-model gates remain.

## Scope and status

This checkpoint starts the P0-I/S4 runtime foundation. It does not complete S4's
first assistant-panel increment or any of its 14 acceptance criteria. S3 has not
been Accepted and the P0-F session vault is not implemented. Preparatory work does
not promote S4 on the delivery board or change S1/S3 live evidence.

No new user-facing UI, model account, Calendar operation, native permission prompt,
Gmail request, private source collection or external transfer is introduced.

## Delivered contract

- `crates/floe-agent` owns a provider/UI-independent Agent runtime and version-1
  command, event, session, model and capability contracts.
- Host-owned identity instructions, scoped Person/policy, typed untrusted evidence,
  recent messages and ephemeral budgets are distinct fields in `ModelRequest`.
  There is no hidden-reasoning field. Prompt rendering remains adapter-owned.
- `SessionStore`, `ModelRunner` and `CapabilityHost` are semantic ports. Model
  adapters receive declared limits, Person/session/turn IDs, a cancellation token
  and a deadline; capabilities receive the corresponding bounded invocation.
- Turns enforce iteration, capability-call, token, integer-micro-cost, byte and
  wall-clock budgets. Identical reads are not reused or rejected solely by input;
  their underlying snapshot may have changed between calls.
- Capability descriptors are freshly checked, versioned and Person-scoped. Only
  read-only descriptors whose declared output class is authorized are advertised.
  Unknown or mutating calls never dispatch. This is not yet the full
  Tool/Expert/Connector package, installation and assignment registry.
- Cancellation/deadline interrupts pending cooperative futures. Dropping an
  in-flight future cancels the adapter token. Adapters must honor cancellation and
  drop; the runtime cannot terminate an independently spawned foreign worker.
- Byte limits bound accepted context, output and session records. Adapters must
  also enforce response/transport limits while receiving data, before allocation;
  reported token/cost usage is a trusted adapter contract, not provider billing
  verification.

## Persistence and recovery

The private core fixture store uses `agent_fixture_sessions`, separate from day
records. Its payload updates use Person-scoped compare-and-swap and monotonically
increasing revisions. It stores only fixed, synthetic prompts and results.

A completed user message and active turn are committed before dispatch. A complete
capability input/result pair is one durable message. Final assistant text and the
successful terminal outcome commit together. Progress events are emitted after
the relevant persistence succeeds; failed terminal persistence cannot emit success.

An abandoned turn retains an active-turn recovery pointer. After the host has
stopped the old execution, explicit revision-checked recovery marks it Interrupted
without replaying a model or capability call. Recovery is not a way to cancel an
active host. Retry is a new explicit user turn and leaves the failed turn intact.

Sessions currently stop at their configured size limit. Search, compaction,
branches, deletion, trace/replay archives and a session inventory are not delivered.

## Privacy boundary

`InferencePolicyDecision` is supplied by the trusted host/domain before any model
dispatch, never by `AgentCommand`, model output or external source content. The
host also owns truthful data classification and minimized projections; this is
not a content classifier or a substitute for projection review.

The runtime checks purpose, declared data classes, allowed placements, performance
class, projection version, freshness and transfer consent. A remote adapter cannot
replace a local-only route. Highly Sensitive remote input additionally needs an
explicit bounded-projection decision. Raw device data and credentials remain
outside the Agent even on the local route; their derivation belongs in native
source boundaries. Prior session classifications cannot be dropped in a later
turn to bypass placement policy.

`SessionProtection::Encrypted` is a requirement on a future trusted vault adapter,
not an encryption implementation. Tests simulate that adapter with synthetic
in-memory data. The only durable implementation is private and SyntheticOnly;
the public fixture bridge accepts the `today`, `follow_up` and `repeated_call`
presets, not arbitrary text, policy, context, credentials or a model selection.
There is no production personal-conversation entry point.

## Bridge

`floe_core_agent_fixture` uses the existing versioned JSON response envelope and
opaque C handle. Requests accept `start`, `get`, `turn` and `recover`; session and
event payloads use the same Rust contract consumed by the typed Dart gateway.
Foreign Person access, stale revisions, unknown fields and free-text prompts fail.

Example operation inside a version-1 request with `person_id`:

```json
{"kind":"turn","session_id":"<returned session UUID>","expected_revision":0,"prompt":"today"}
```

The existing Dart FFI isolate owns native handle and JSON memory lifetime. Symbol
lookup is lazy so loading Calendar does not require invoking the new fixture port.
This synchronous fixture bridge **returns a batch of events after completion**;
only the Rust runtime currently has live progress callbacks and cancellation.
It does not claim token streaming or Flutter stop support.

## Validation

- `cargo test --workspace`: 74 tests pass; 24 new Agent/core/ABI tests.
- `cargo build -p floe-ffi`: passes.
- `cargo clippy -p floe-agent --all-targets -- -D warnings`: passes.
- `cargo fmt --all -- --check` and `git diff --check`: pass.
- `flutter analyze`: no issues.
- Targeted Dart/C ABI suite: three tests pass. Full Flutter suite: 104 tests pass.
- The first Flutter compile ran out of disk space. A package-scoped `cargo clean`
  removed only reproducible Rust build artifacts; rebuilding and rerunning passed.

Run from the repository root:

```sh
cargo test --workspace
cargo build -p floe-ffi
cargo clippy -p floe-agent --all-targets -- -D warnings
cargo fmt --all -- --check
cd apps/client
flutter analyze
flutter test test/agent_fixture_gateway_test.dart
flutter test
```

Coverage includes ordered multi-turn events, restart durability, Person/CAS
isolation, malformed/versioned commands, recovery, persistence failure,
capability failure isolation, unknown/mutating calls, fixed identity versus
untrusted injected evidence, all implemented budgets, pending-model cancellation,
placement and consent denial, stale context, raw-source exclusion, sensitive
history retention and actual Dart/C ABI decoding. Outbound tests capture calls at
the model port; they are not packet captures or live provider privacy evidence.

Whole-workspace Clippy currently reports two pre-existing warnings in
`crates/floe-core/src/calendar_action.rs`: `too_many_arguments` on
`direct_calendar_action` and `collapsible_if` in mutation validation. They are
outside this checkpoint; no suppressions or unrelated Calendar edits are added.

## Next increments

1. Connect a visible assistant panel and cancellable asynchronous event transport.
2. Implement and validate encrypted session storage/key-unavailable behavior before
   enabling personal free text or real-source dogfood.
3. Complete Expert/Connector registries, granted View handles and typed proposals
   through the existing S3 authority/review executor, not direct mutations.
4. Validate a real local model and a supported remote adapter, then connected-source
   and physical-device gates. Live model, OAuth, Health and Screen Time acceptance
   remains entirely pending.
