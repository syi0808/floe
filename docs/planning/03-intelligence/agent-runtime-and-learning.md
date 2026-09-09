# Agent Runtime and Governed Learning

> Status: S4/S5 implementation contract
>
> Reference reviewed: NousResearch/hermes-agent commit
> `693641aa8b4359c602283bdbbc14041e03bc47bc` on 2026-09-07

The first native-model migration increment is described in
[ADR 0016](../../decisions/0016-native-agent-model-protocol.md). Remote adapters now
send actual provider-native messages and tools, while Manager and Expert share one
bounded model-output/schema correction attempt. This supersedes the immediate
hard-stop rule below for the first such failure only; subsequent failure remains a
hard stop. The full durable replay and multi-item event migration is not complete.

## Product boundary

Floe has one user-facing **Manager Agent**. Experts provide bounded domain judgment;
Tools provide capabilities; neither becomes an independent user-facing assistant.

```text
Chat / Voice / System Invocation
              ↓
       Manager Agent Runtime
        ├─ Context Assembler
        ├─ Model Port
        ├─ Tool Registry
        ├─ Expert Host
        ├─ Policy / Budgets
        └─ Event Stream
              ↓
        Review / Activity
              ↓
   Deterministic Domain Executors
```

The Agent may reason and propose. It does not acquire authority from the model and
does not bypass typed domain commands, fresh validation or Review.

## Runtime ports

The core is independent from Flutter, voice, model provider and deployment.

```text
AgentRuntime
├─ SessionStore
├─ ContextProvider[]
├─ ModelRunner
├─ ToolRegistry
├─ ExpertRegistry / ExpertHost
├─ ConnectorRegistry / ViewProvider
├─ PolicyEngine
├─ LearningSink
└─ AgentEventSink
```

Required invariants:

- Flutter chat, future voice, tests and server transport consume the same typed
  AgentCommand/AgentEvent stream.
- Model adapters converge on one internal message/tool-call representation.
- Tools and Experts register through versioned descriptors with availability and
  permission checks; core code does not maintain provider-specific switch lists.
- Connectors register capabilities and provider-neutral Views. Agent context never
  embeds OAuth credentials, native framework objects or unrestricted raw datasets.
- ExpertPackage, Installation and Person Assignment remain separate.
- Every call receives a deadline, cancellation token, Person, session, granted
  view handles and a trace identifier.
- Optional subsystems fail locally. Missing Memory, voice or one Expert cannot
  corrupt the session or disable unrelated capabilities.

S4 dogfoods the same port with a local Go Gmail connector, Contacts,
location/ETA/weather, Apple device context providers and fixtures. Execution
location is descriptor metadata, not a different Agent API. Apple Health and
Screen Time stay local-derived; without S8 sync their physical-device validation
does not make those Views available to macOS.

## Turn lifecycle

```text
append UserMessage
→ assemble bounded context
→ model step
→ validate structured output
→ zero or more Tool/Expert calls
→ append typed results
→ repeat within budget
→ persist AssistantMessage + outcome
→ emit optional LearningObservation
```

The loop is interruptible between model and capability calls. It enforces iteration,
wall-clock, model-token/cost, tool-call and output-size budgets. Identical reads are
not cached or rejected because the underlying snapshot may change between calls.
Cancellation never records partial model text as a completed assistant turn.

Durable messages are not sent to models in their storage representation. The model
projection omits historical Tool evidence and reconstructs every current result as an
ordered `assistant.tool_calls` and `tool.tool_call_id` pair. JSON arguments and output
are structured values, while success and failure are explicit Tool states. This keeps
the causal call/result transition intact instead of presenting persisted
`Result<String, Failure>` records as ordinary prompt data.

Iteration, wall-clock, token, cost and Tool-call exhaustion is a soft stop at a safe
boundary. The session persists the stopped turn ID, cumulative usage, placement and
continuation level. A user-visible Continue action resumes that same turn without
adding another UserMessage. Each continuation raises the exhausted execution budget
to 1.75 times the previous ceiling, up to three user-approved continuations. Physical
model context limits, invalid output, policy/consent failures, cancellation and
uncertain external mutations remain hard stops. A continuation must preserve the
recorded placement and pass fresh policy, revision and capability checks.

Parallel calls are allowed only when their descriptors declare them read-only and
order-independent. Mutations and interactive Review remain sequential.

## Prompt and context layers

The concrete S5 assembly, Persona/User Model separation, retrieval manifest and
hierarchical Playbook decision is defined by
[ADR 0017](../../decisions/0017-agent-context-assembly.md). The outline below remains
the product-level model.

```text
1. Stable: Behavior Kernel, Role, optional Persona, base capability protocol
2. Scoped: purpose, available capability guidance, eligible/loaded Playbooks
3. Contextual data: User Model, Memory, authorized Views and Archive evidence
4. Conversation: compaction summary, ordered recent events, current request
5. Runtime: current time, surface, budget and fresh execution state
```

Host Policy is enforced outside the prompt. Its minimal model-visible Behavior Kernel
is product-owned and cannot be edited by the Agent. Persona is user-editable but
cannot change Role, policy or authority. Playbooks are scoped procedural instructions;
Memory and external content are quoted/typed evidence, never instructions. Prompt
snapshots are versioned so a trace can be replayed without logging hidden reasoning
or raw sensitive content.

### Product-owned instruction files

Product-owned Behavior Kernel, base capability protocol, Manager Role, built-in Expert
Roles and preset turn templates are maintained as separate UTF-8 resources under
`crates/floe-agent/prompts/`. Runtime code embeds those reviewed files at compile time;
it must not duplicate prompt literals. Role files define responsibility, success
criteria and durable behavior, not representative workflows, complete tool manuals or
host-enforced limits.

Persona is a separate, typed and versioned user configuration with `SOUL.md` as an
import/export surface. It is composed for the Manager and omitted from Experts unless
an assignment explicitly needs it. Conditional capability guidance comes from the
granted registry descriptors, and Playbook bodies come from the governed registry.
Dynamic values enter only typed, bounded context components; free-form user text
remains session data. The Context Assembler records every component revision and
builds the stable/cacheable prefix before scoped and volatile data.

## Sessions and chat

`AgentSession`, `AgentMessage`, `CapabilityCall`, `ExpertInvocation` and
`AgentOutcome` are durable, Person-scoped records. Session history supports resume,
branch/compaction lineage and on-demand search. Compaction preserves user messages,
tool/result pairing, identifiers and recovery pointers to archived turns.

The UI renders final assistant messages as selectable GitHub-Flavored Markdown while
preserving user-authored messages as literal text. Headings, emphasis, lists, quotes,
links and code are presentation only: Markdown cannot grant capabilities or bypass
Review. Remote and local Markdown images are reduced to their alt text so a model
response cannot initiate a network or filesystem read. Links are styled but remain
inert until a separately reviewed navigation policy exists. The UI also renders
grounded evidence, capability progress, Review links, stop/retry and recoverable
errors. It does not expose private chain-of-thought.

## Expert extensibility

Experts are invoked like typed advisors, not given the entire Agent transcript or
ambient tool access.

```text
ExpertInvocation {
  apiVersion, invocationId, expertId, assignmentId,
  trigger, personRef, deadline, budget,
  grantedViewHandles, grantedCapabilities, input
}

ExpertResult {
  insights[], actionProposals[], memoryCandidates[],
  stateUpdates[], diagnostics
}
```

The host validates schema/version, budgets and grants before and after invocation.
Native, declarative, future Wasm and remote Experts adapt to this contract. Experts
cannot render arbitrary UI, read credentials/DBs, mutate authoritative Memory or
execute external actions. Private state is namespaced per assignment and migrated
transactionally with rollback.

## Three durable knowledge classes

| Class | Meaning | Context policy | Mutation policy |
| --- | --- | --- | --- |
| Personal Memory | facts, preferences, relationships, commitments | compact projection + relevant retrieval | source-backed candidate and Review |
| Session Archive | what was said/done and observed outcomes | search on demand | append/compact; user deletion propagates |
| Procedural Playbook | how Floe should handle a recurring class of task | compact index, body loaded on demand | versioned LearningCandidate and Review |

Identity, safety policy, permission grants, Expert trust and model configuration are
not learnable knowledge classes.

## Governed self-improvement loop

Floe self-improvement means improving externalized Memory, retrieval and Playbooks;
it does not mean autonomous model-weight or safety-policy modification.

```text
conversation + tool/Expert outcomes + user correction
→ immutable LearningObservation
→ isolated Learner pass over a bounded digest
→ MemoryCandidate or PlaybookChangeCandidate
→ policy/static checks/evaluation
→ Review with source or diff
→ versioned activation
→ later outcome comparison
→ retain, revise, rollback, pin or archive
```

Initial defaults:

- foreground and background inferred writes are staged, never silently activated;
- the Learner has read-only session/evaluation access and candidate-write tools only;
- reviews coalesce by session/evidence and run after the foreground turn;
- a new user turn can preempt/defer local-model review work;
- Playbook bodies use progressive disclosure and capture reusable rules, not incident logs;
- all mutations have actor, source, before/after hashes and rollback material;
- pinned artifacts cannot be changed by background maintenance;
- automated maintenance may mark stale and recoverably archive, never hard-delete;
- consolidation is opt-in until quality and cost evidence exists.

Activation requires deterministic schema/security checks plus replay evaluation on
the affected scenario set. The system records whether the next comparable outcome
improved; usage alone is not proof that a learned change is good.

## Hermes reference: adopt, adapt, reject

Hermes informed this design through its platform-agnostic core, registry pattern,
interruptible loop, searchable sessions, bounded factual memory, progressive Skills,
background review, write-approval gates and recoverable Curator lifecycle.

| Hermes mechanism | Floe adaptation | Slice |
| --- | --- | --- |
| platform-agnostic `AIAgent` + callbacks | `AgentRuntime` + typed Command/Event adapters | S4 |
| tool registry + availability checks | separate versioned Tool and Expert registries | S4 |
| interruptible tool loop + budgets | cancellation, capability policy, cost/stall hard stops | S4 |
| SQLite session persistence/search/compaction | encrypted Person sessions, recovery pointers, searchable archive | S4/S5 |
| bounded `MEMORY.md` / `USER.md` | typed, temporal, source-backed Personal Memory | S5 |
| progressively disclosed Skills | reviewed procedural Playbooks; Floe Skill remains capability | S5 |
| isolated background review | restricted Learner producing candidates only | S5 |
| memory/skill write approval | unified Review, enabled by default for inferred writes | S5 |
| Curator ledger/pin/archive/rollback | versioned activation, outcome evaluation and recoverable maintenance | S5 |

Floe deliberately adapts or rejects these parts:

- use typed domain records and provenance graphs, not `MEMORY.md`/`USER.md`, for
  Personal Memory;
- default inferred durable writes to Review rather than opt-in approval;
- map Hermes procedural Skills to Floe Playbooks; keep Floe Skill as an executable
  capability and keep Expert, Playbook, Skill and Tool distinct;
- prohibit the learning fork from shell/network/domain mutation;
- do not inject all Memory every session; use compact safe context plus retrieval;
- do not expose raw reasoning, accept arbitrary code Playbooks or let Agent-authored
  content change higher-priority identity and safety instructions.

Primary references:

- [Architecture](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/developer-guide/architecture.md)
- [Agent loop](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/developer-guide/agent-loop.md)
- [Prompt assembly](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/developer-guide/prompt-assembly.md)
- [Persistent Memory](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/user-guide/features/memory.md)
- [Skills](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/user-guide/features/skills.md)
- [Curator](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/user-guide/features/curator.md)
- [Memory provider contract](https://github.com/NousResearch/hermes-agent/blob/693641aa8b4359c602283bdbbc14041e03bc47bc/website/docs/developer-guide/memory-provider-plugin.md)

Ideas are architecture references, not a source-code dependency. Any future code
reuse requires a separate license, security and maintenance review.
