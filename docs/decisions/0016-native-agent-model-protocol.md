# ADR 0016: Native agent model protocol and shared correction boundary

- Date: 2026-09-09
- Status: accepted; native transport, durable Manager replay and shared usage accounting implemented
- Amends: ADR 0011 and the model transport portion of ADR 0015

## Decision

Rust owns the Floe agent loop, capability authorization, Expert host and action
boundary. Go owns provider credentials and model transport, not tool execution.
Manager and bounded Experts share the model-attempt recovery function; their
domain execution loops and completion contracts remain distinct in this increment.

`POST /v1/agent` is separate from the purpose-routed `/v1/generate` structured generation API.
It uses schema version 1, a product purpose, data classes, explicit transfer
consent, instructions and an input containing actual messages and tool definitions.
It does not accept an output schema or legacy replay request. All public API and response schemas use version 1. There are no v2/v3 aliases,
class-selected generation endpoints or compatibility negotiation. Client and server
are developed and deployed together; paired credentials remain unchanged. Internal
database migrations are independent of public contract versions.

Codex Responses, OpenAI-compatible Chat Completions and local-only server Ollama
use native tools. The provider response is normalized by code into the existing
Rust ModelStep; the model no longer writes the answer/call envelope. There is no
automatic structured-output fallback or undisclosed provider switch. Device-local
Foundation Models retain their existing adapter and bypass the gateway.

Tool names are bounded aliases generated from capability identifiers, mapped back
against the currently advertised registry before dispatch. Rust validates arguments
against descriptor JSON Schema. Schema resolution has network and filesystem
features disabled. Invalid registry schemas fail before model dispatch.

The initial wire path is deliberately sequential: request parallel calls disabled
and reject multiple returned calls before any execution. Plain answer text and tool
calls are distinct. Text accompanying a tool call is not a final user answer.
Codex output is accepted only on terminal response.completed, never output_text.done.
Opaque Codex output items, including encrypted reasoning and assistant phase, and
provider call IDs are returned as typed replay metadata rather than cached in a
model adapter. Manager persists accepted call replay alongside encrypted execution
intent, keyed by host call ID. Continuation reconstructs it from settled executions
in the current turn, even with a new runner. Expert carries the same typed metadata
within its isolated invocation; its internal transcript is not durable yet.
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

AgentSession.usage contains the current/last turn totals at normal message,
capability and termination commits. It is not a lifetime session total or exact
provider billing. Unknown monetary cost remains zero; adapters may also supply
their own token estimates without an estimate marker. Abrupt process termination
between checkpoints still needs a durable per-attempt reservation journal; this
increment does not claim crash-exact accounting.

## Durable capability dispatch

Before Manager dispatches a capability (including an Expert invocation), Rust commits
its call ID, turn ID, capability ID and input in the encrypted session using CAS.
A failed intent commit prevents dispatch. The observed result and settled state
are committed together before another model call. A dropped future or failed result
commit leaves a started record; recovery marks it interrupted without rerunning it.
Interrupted means completion is unknown, not that the capability did nothing.
These records are bounded by the existing session size budget and are not model
context or gateway audit content. Expert-internal read attempts still need to join
the shared durable ledger in a later increment.

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

Codex replay must contain exactly the recorded function call with matching ID,
name and JSON arguments. Assistant phase and reasoning items retain their order.
Capability aliases use a specified FNV-1a mapping with collision checks, not Rust's
implementation-dependent default hasher. Rejected model attempts never commit
their replay. Session and gateway input limits bound retained bytes; successful
turn completion removes opaque replay while preserving semantic results and the
execution ledger. Interrupted work is never automatically replayed.

## Remaining migration work

This increment is not the entire agent-runtime redesign:

- Extend encrypted replay and execution records into Expert-internal reads; define
  a durable gateway replay identity if continuation across gateway restart is needed.
- Introduce ordered multi-item ModelResponse, then support multiple independent
  read calls without discarding preambles.
- Unify the remaining Manager/Expert loop mechanics without merging their contexts
  or authority; extend the shared usage ledger into durable per-attempt records.
- Expose model-attempt IDs, validation stages, correction events and settled outcomes
  through FFI/UI, distinguishing model synthesis from source execution failures.
- Add real-provider
  acceptance captures for the native transport.

Native function calling is not authority: fresh grants, Person isolation, view
expiry, consent, encrypted session storage and Review/Action Authority still apply.
Codex OAuth wire reuse remains experimental as described in ADR 0015.

## Validation

Offline tests cover native request projection, plain answers, call normalization,
malformed input rejection, terminal stream completion, opaque replay preservation,
one correction attempt, argument schema validation, preservation of completed tool
results, replay across session reload, encrypted WAL/checkpoint persistence,
route/account-switch rejection, parent/child budget enforcement, failed-attempt
accounting, continuation without double charging and authorization/cancellation behavior. Live Calendar/provider
acceptance is a separate gate, not implied by fixture tests.

## References

- https://developers.openai.com/api/docs/guides/function-calling
- https://learn.chatgpt.com/docs/app-server
- https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/agent-loop.md
- https://github.com/openclaw/openclaw/blob/main/docs/agent-runtime-architecture.md
