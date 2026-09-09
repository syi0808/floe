# ADR 0016: Native agent model protocol and shared correction boundary

- Date: 2026-09-09
- Status: accepted; first migration increment implemented
- Amends: ADR 0011 and the model transport portion of ADR 0015

## Decision

Rust owns the Floe agent loop, capability authorization, Expert host and action
boundary. Go owns provider credentials and model transport, not tool execution.
Manager and bounded Experts share the model-attempt recovery function; their
domain execution loops and completion contracts remain distinct in this increment.

`POST /v3/agent` is separate from the existing v1/v2 structured generation APIs.
It uses schema version 3, a product purpose, data classes, explicit transfer
consent, instructions and an input containing actual messages and tool definitions.
It does not accept an output schema or legacy replay request. Existing paired
credentials and purpose routing remain valid. Deploy the server before the client.

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
provider call IDs are retained in bounded per-turn adapter memory for subsequent
calls. They are not displayed or written into content-free gateway audit records.

## Recovery

One shared Rust function handles model output validation and schema correction for
both Manager and Expert. There are at most two attempts per logical model step.
The second attempt adds product-owned protocol feedback without committing a new
user message. Previously observed tool results remain in context. A failed call is
not executed. Authorization failures, network failures, cancellation and deadline
expiry are not correction-retried. Go adapters do not independently retry.

The deadline and cancellation token are unchanged across attempts. Reported usage
for schema-rejected responses is added to the successful response; when an adapter
returns only an error, 4096 tokens are reserved as an estimate. This is not exact
billing: typed failure usage and durable attempt accounting remain follow-up work.

## Remaining migration work

This increment is not the entire agent-runtime redesign:

- Persist provider replay metadata in encrypted sessions with schema migration,
  route identity and retention rules. Current opaque replay memory does not survive
  runner recreation; old sessions retain their existing semantic transcript.
- Introduce ordered multi-item ModelResponse and durable pre-execution call records,
  then support multiple independent read calls without discarding preambles.
- Unify the remaining Manager/Expert loop mechanics without merging their contexts
  or authority; aggregate child usage into the parent ledger.
- Expose model-attempt IDs, validation stages, correction events and settled outcomes
  through FFI/UI, distinguishing model synthesis from source execution failures.
- Add route capability negotiation, safe rolling-upgrade behavior and real-provider
  acceptance captures before removing legacy transport.

Native function calling is not authority: fresh grants, Person isolation, view
expiry, consent, encrypted session storage and Review/Action Authority still apply.
Codex OAuth wire reuse remains experimental as described in ADR 0015.

## Validation

Offline tests cover native request projection, plain answers, call normalization,
malformed input rejection, terminal stream completion, opaque replay preservation,
one correction attempt, argument schema validation, preservation of completed tool
results and existing authorization/cancellation behavior. Live Calendar/provider
acceptance is a separate gate, not implied by fixture tests.

## References

- https://developers.openai.com/api/docs/guides/function-calling
- https://learn.chatgpt.com/docs/app-server
- https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/agent-loop.md
- https://github.com/openclaw/openclaw/blob/main/docs/agent-runtime-architecture.md
