# Agent Runtime and Governed Learning

> Status: S4/S5 implementation contract
>
> Reference reviewed: NousResearch/hermes-agent commit
> `693641aa8b4359c602283bdbbc14041e03bc47bc` on 2026-09-07

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
wall-clock, model-token/cost, tool-call and output-size budgets. Identical failed
calls and read-only calls with no progress trigger warnings and then a controlled
halt. Cancellation never records partial model text as a completed assistant turn.

Parallel calls are allowed only when their descriptors declare them read-only and
order-independent. Mutations and interactive Review remain sequential.

## Prompt and context layers

```text
1. Stable: Floe identity, safety, tool/Expert protocol
2. Scoped: Person policy, session purpose, granted capabilities
3. Retrieved: relevant Memory/Playbooks/domain Views with provenance handles
4. Recent: bounded conversation tail and compression summary
5. Ephemeral: current time, budget and fresh execution state
```

Stable identity and safety are product-owned and cannot be edited by the Agent.
Memory and Playbooks are data, never higher-priority instructions. External content is
quoted/typed as untrusted evidence. Prompt snapshots are versioned so a trace can
be replayed without logging hidden reasoning or raw sensitive content.

## Sessions and chat

`AgentSession`, `AgentMessage`, `CapabilityCall`, `ExpertInvocation` and
`AgentOutcome` are durable, Person-scoped records. Session history supports resume,
branch/compaction lineage and on-demand search. Compaction preserves user messages,
tool/result pairing, identifiers and recovery pointers to archived turns.

The UI renders final messages, grounded evidence, capability progress, Review links,
stop/retry and recoverable errors. It does not expose private chain-of-thought.

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
