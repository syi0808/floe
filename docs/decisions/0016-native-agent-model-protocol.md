# ADR 0016: Native agent model protocol and shared correction boundary

- Date: 2026-09-09
- Status: accepted; native transport, replay, shared usage, durable model attempts, scoped capability execution and ordered output implemented
- Amends: ADR 0011 and the model transport portion of ADR 0015

> The implemented Expert-as-capability transport is superseded as the target design
> by [ADR 0018](0018-manager-expert-a2a-delegation.md). Its model-attempt, usage,
> journal and authorization guarantees remain requirements of the A2A-aligned path.

## Decision

Rust owns the Floe agent loop, capability authorization, Expert host and action
boundary. Go owns provider credentials and model transport, not tool execution.
Manager and bounded Experts share the model-attempt recovery function; their
domain reasoning loops and completion contracts remain distinct. Manager and
Expert now also share a durable capability-dispatch function.

`POST /v1/agent` is separate from the purpose-routed `/v1/generate` structured generation API.
It uses schema version 1, a product purpose, data classes, explicit transfer
consent, instructions and an input containing actual messages and tool definitions.
It does not accept an output schema or legacy replay request. All public API and response schemas use version 1. There are no v2/v3 aliases,
class-selected generation endpoints or compatibility negotiation. Client and server
are developed and deployed together; paired credentials remain unchanged. Internal
database migrations are independent of public contract versions.

Codex Responses, OpenAI-compatible Chat Completions and local-only server Ollama
use native tools. The provider response is normalized by code into the existing
Rust ModelResponse.output list of ModelStep items; the model no longer writes an answer/call envelope. There is no
automatic structured-output fallback or undisclosed provider switch. Device-local
Foundation Models retain their existing adapter and bypass the gateway.

Tool names are bounded aliases generated from capability identifiers, mapped back
against the currently advertised registry before dispatch. Rust validates arguments
against descriptor JSON Schema. Schema resolution has network and filesystem
features disabled. Invalid registry schemas fail before model dispatch.

The native wire path permits multiple independent calls; host execution remains
sequential. Each response contains an ordered list of at most 16 items and at most
eight read-only calls. Preamble text is distinct from a final answer. A response
with calls cannot also contain a final answer; without calls it must end with
exactly one answer. The shared correction boundary validates the complete list,
JSON object arguments, advertised read-only capabilities and every input schema
before any preamble is published or tool is dispatched. Both loops preflight
their entire batch against remaining call budgets.

Manager checkpoints a multi-item output list in encrypted pending_output before
processing it. Every preamble is committed as a non-final message; every tool uses
the shared durable execution boundary. The checkpoint is cleared only after the
whole list is consumed. Stops or recovery abandon an incomplete list and disable
continuation of that partial batch, even when its already-executed calls are all
settled. They never rerun completed reads or forward incomplete provider groups.
Expert batches remain bounded by the in-flight parent capability and their own
tool budget. Authority is rechecked at execution; this is not an atomic transaction
across tools.

Device-local Foundation Models still emit one item through their adapter.
Preambles stream as committed v1 message events and Flutter renders them without
ending the running turn. This is item-level progress, not token streaming.
Codex output is accepted only after terminal response.completed, never
output_text.done. Completed output items are accumulated in order from
response.output_item.done because the production Codex stream may leave the
terminal response.output array empty. When both representations are present they
must be semantically identical. A terminal event without either representation is
invalid. Only completed items are retained for replay; partial deltas are ignored.
Opaque Codex output items, including encrypted reasoning and assistant phase, and
provider call IDs are returned as typed replay metadata rather than cached in a
model adapter. Manager persists accepted call replay alongside encrypted execution
intent, keyed by host call ID. Continuation reconstructs it from settled executions
in the current turn, even with a new runner. Expert carries the same typed metadata
within its isolated invocation and persists internal read inputs, results and replay
under that invocation's execution scope. This is a recovery journal, not automatic
reconstruction or resumption of an interrupted Expert conversation.
Replay is not displayed or written into content-free gateway audit records.

## Recovery

One shared Rust function handles model output validation and schema correction for
both Manager and Expert. There are at most two attempts per logical model step.
The second attempt adds product-owned protocol feedback without committing a new
user message. Previously observed tool results remain in context. A failed call is
not executed. Empty answers and response schema mismatches pass through the same
correction boundary. Output and usage limits are enforced before correction;
budget violations are not retried. Authorization failures, network failures, cancellation and deadline
expiry are not correction-retried. Go adapters do not independently retry.

The deadline and cancellation token are unchanged across attempts. Manager and
Expert share one host-owned usage ledger through capability invocation, not through
model-written Expert results. Every attempt reserves up to 4096 tokens before
dispatch; reported usage replaces that reservation even when validation rejects the
response. Errors, cancellation and dropped model futures retain the estimate.
The ledger tracks attempts and estimated tokens separately from total tokens.

Both child-local limits and the parent's remaining tokens/cost constrain each
request. Usage from failed Experts is charged even if no ExpertResult is returned.
The current sequential runtime permits only one outstanding model attempt per
ledger. A second concurrent attempt fails before dispatch rather than overspending.
Continuation seeds the ledger from the preceding turn checkpoint, so completed
child work is neither rerun nor counted twice.

AgentSession.usage contains the current/last turn totals at attempt, message,
capability and termination commits. It is not a lifetime session total or exact
provider billing. Unknown monetary cost remains zero; adapters may also supply
their own token estimates without an estimate marker.

## Durable model attempts and progress

Each runtime-managed model attempt has a unique ID, scope ID, model turn ID,
one-based correction attempt number, placement, usage and typed outcome.
Manager and child Expert requests share a journal channel. The runtime commits
the started record and reservation through encrypted session CAS before
acknowledging model dispatch. A failed start commit blocks dispatch and refunds
the undispatched reservation. Validation acceptance/rejection is committed before
a tool may execute or a correction may start. Neither prompts nor model output
are copied into this journal.

If the process or future stops before settlement is durable, the persisted started
record retains its estimated usage. Recovery marks it interrupted without
reissuing the model request; observed usage that was never committed remains
unknown, not free and not claimed to be exact billing. These records are bounded
by the existing session byte budget. Expert-internal model attempts and tool reads
use the same acknowledged journal; Expert private-state publication still has its
separate authority-checked transaction.

Committed attempt records stream as v1 model_attempt events through the existing
sequenced, bounded FFI progress transport. Logical model_started events still mark
Manager iterations, while attempt events distinguish actual provider attempts
and child scopes. Flutter distinguishes Expert analysis from a correction retry.
Attempt metadata is not appended as a conversation message or interpreted as a
successful source read. Final cancellation and recovery use the existing typed
turn outcome. Controlled stops publish interrupted attempt events after the halt
commit; restart recovery retains them in the encrypted session without replay.

## Durable capability dispatch

Manager capability calls, Expert timeline views and Expert-internal schedule reads
use the same dispatch boundary. Rust commits scope ID, call ID, turn ID, capability
ID, input and optional provider replay through encrypted session CAS before polling
the tool future. A failed intent commit prevents dispatch. Cancellation and deadline
are checked again after acknowledgment. The observed bounded result and settled
state are committed before returning evidence to either reasoning loop. Manager's
public capability message is committed atomically with its settlement; child results
remain in their execution scope, not in Manager messages or progress events.

Execution settlement finds the matching call ID and scope, not the last journal
entry: nested Expert reads must not prevent their parent capability from settling.
Storage/journal failures propagate as runtime failures, not ordinary tool evidence.
A dropped future, failed result commit or oversized result leaves a started record;
recovery marks all affected scopes interrupted without rerunning them. Interrupted
means completion is unknown, not that the capability did nothing. A settled child
does not imply its parent Expert or private-state transaction completed.

Fresh view validation, grants, consent and final Expert publication checks remain
in their existing authority boundaries. Manager's replay projection selects only its
own scope; child replay cannot enter a Manager provider request. Completion prunes
opaque replay from every scope. Execution inputs and semantic results remain in
the encrypted, session-size-bounded ledger. Root capability-call budgets and Expert
view/tool budgets remain separate; model token/cost accounting is shared.

The journal is attached by AgentRuntime. Standalone ExpertHost invocations retain
the bounded execution path but do not acquire durable storage implicitly.

## Replay isolation and retention

The gateway provides an opaque HMAC route fingerprint covering provider, endpoint,
model, reasoning effort, credential, purpose and Codex account identity. It rejects
a different fingerprint before provider dispatch. Codex also checks the bound
account against the actual credential selected for the outgoing request, closing
the account-switch gap between gateway routing and dispatch. Rust checks gateway address,
purpose and transfer placement before sending replay. Gateway identity rotation
(including a console restart with a new internal token) deliberately invalidates
old fingerprints: start a fresh turn instead of silently transferring or downgrading
provider state. This is not a promise of arbitrary provider-session resumability.

Replay records bind all original call IDs in a response and its preamble to each
host execution. Projection reconstructs one assistant call group followed by all
matching results, inserting opaque provider items only once. Missing, reordered
or inconsistent group records are rejected before transport. Codex validates
every recorded call's ID, name and JSON arguments, including uniqueness and order.
Assistant phase, interleaved text and reasoning items retain their original order.
Capability aliases use a specified FNV-1a mapping with collision checks, not Rust's
implementation-dependent default hasher. Rejected model attempts never commit
their replay. Session and gateway input limits bound retained bytes; successful
turn completion removes opaque replay while preserving semantic results and the
execution ledger. Interrupted work is never automatically replayed.

## Remaining migration work

The core ordered-output and durable-dispatch migration is implemented. Remaining
operational or optional extensions are:

- Define a durable gateway replay identity only if continuation across gateway
  restart is required; interrupted Expert conversations are not automatically resumed.
- Extend committed-item progress into token-level streaming and more detailed
  validation stages without exposing private reasoning.
- Add hermetic redacted real-provider capture fixtures for the native transport;
  the production-shaped incremental stream is covered by a synthetic fixture.

Native function calling is not authority: fresh grants, Person isolation, view
expiry, consent, encrypted session storage and Review/Action Authority still apply.
Codex OAuth wire reuse remains experimental as described in ADR 0015.

## Validation

Ordered-output tests cover whole-list rejection with one correction, zero dispatch
on malformed later calls or insufficient batch budget, preamble ordering,
cancellation between settled calls, Expert-local batch budgets, grouped provider
replay reconstruction, incomplete/mismatched group rejection and non-final Flutter
progress. Server tests cover native multi-call normalization and Codex replay of
interleaved text with multiple call/result pairs.

Scoped execution tests cover pre-dispatch/result acknowledgment failures, oversized
result rejection, dropped tool futures, recovery across parent and child scopes,
Expert replay exclusion from Manager requests, atomic parent settlement after child
reads, and encrypted child result persistence through WAL/checkpoint reopen.

Manual synthetic acceptance against the configured Codex OAuth route verified a
complete native call, opaque replay, tool result and final answer round trip from
incremental output-item streams, with completed content-free traces. Provider HTTP
failures are classified as credential, quota,
request rejection or availability failures without recording response bodies.
Dashboard state exposes the last 20 content-free traces. Credential readiness is
checked outside the console mutex so a blocked Keychain lookup cannot stall paired
inference authentication.

Offline tests cover native request projection, plain answers, call normalization,
malformed input rejection, terminal stream completion, opaque replay preservation,
one correction attempt, argument schema validation, preservation of completed tool
results, replay across session reload, encrypted WAL/checkpoint persistence,
route/account-switch rejection, parent/child budget enforcement, failed-attempt
accounting, durable pre-dispatch acknowledgments, interrupted reservation recovery,
Dart/native attempt decoding, correction progress, continuation without double
charging and authorization/cancellation behavior. Live Calendar/provider
acceptance is a separate gate, not implied by fixture tests.

## References

- https://developers.openai.com/api/docs/guides/function-calling
- https://learn.chatgpt.com/docs/app-server
- https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/agent-loop.md
- https://github.com/openclaw/openclaw/blob/main/docs/agent-runtime-architecture.md
