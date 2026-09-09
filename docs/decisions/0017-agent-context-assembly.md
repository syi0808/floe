# ADR 0017: Assemble Agent context from typed, governed layers

- **Status:** proposed
- **Date:** 2026-09-09
- **Scope:** S4 prompt simplification and S5 Memory/Playbook foundation

## Context

Floe currently passes a fixed Manager prompt, an `AgentContext` containing only
evidence, the full projected conversation and the capabilities available for the
turn. This is enough to validate the S4 loop, but not enough to add a configurable
Floe personality, user-authored profile, confirmed Personal Memory, retrieved
session history or progressively disclosed procedures without turning the system
prompt into an unbounded mixture of instructions and data.

The prompt must also stop carrying workflow details that belong in capabilities or
procedures. Manager and Expert role prompts should say who the model is, what it is
responsible for, what it may do, what it must not do and how it communicates.
Schemas, grants, call counts, freshness and output bounds remain host-enforced.

## Reference findings

The following implementations were reviewed as architecture references, not source
dependencies:

- Hermes separates stable, contextual and volatile prompt tiers. `SOUL.md` supplies
  identity, `USER.md` and `MEMORY.md` supply bounded snapshots, and a compact Skill
  index enables on-demand loading. Its frozen session snapshot improves prefix-cache
  stability but delays visibility of writes until a later rebuild.
- OpenClaw separates `AGENTS.md`, `SOUL.md`, `USER.md`, curated `MEMORY.md`, daily
  searchable notes and Skills. It rebuilds a system prompt each run, reports context
  contributors, loads only Skill metadata initially and reads bodies on demand.
  Directory nesting is organizational rather than a discovery hierarchy.
- Codex layers global and directory-scoped `AGENTS.md`, exposes only Skill name,
  description and location before loading `SKILL.md`, and keeps generated local
  memories separate from mandatory instructions. Memory use and generation can be
  controlled per chat; long conversations can be compacted.

Floe adopts explicit layers, progressive disclosure, retrieval and inspectability.
It does not adopt Markdown files as authoritative Personal Memory, silent
agent-authored writes, or a flat Skill index that exposes every nested procedure.

## Decision

### Separate six concepts

| Concept | Meaning | May grant authority? | Mutation |
| --- | --- | --- | --- |
| Product Constitution | safety, evidence and action invariants | no; it defines the boundary | signed product update only |
| Persona | Floe's character, values, tone and conversational boundaries | no | user edit or reviewed preset revision |
| Role | Manager or Expert responsibility | no | product/Expert package update |
| User Model | user-declared or confirmed profile and preferences | no | direct user edit or reviewed Memory decision |
| Personal Memory | temporal, source-backed facts, preferences, relationships and commitments | no | governed candidate, Review and tombstone |
| Playbook | reusable procedural guidance, equivalent to an agent-style Skill | no | versioned candidate, Review and rollback |

`SOUL.md` is the import/export and editing representation of Persona, not the safety
policy. Floe stores a parsed, versioned `PersonaProfile`; invalid or oversized source
text cannot enter a prompt. A default product persona is always available. Changing
Persona invalidates the stable prompt prefix but never changes grants, data policy or
Action Authority.

`USER.md` may likewise be supported as an import/export surface, but the authoritative
User Model remains typed Person-scoped records with active/superseded state,
observation time and provenance. Free-form user text is quoted as data until the user
confirms a parsed change.

### Context envelope

Every model call receives a versioned `ContextEnvelope`, assembled by Core rather
than by Flutter, a connector, an Expert or a model adapter.

```text
ContextEnvelope
├─ stable_instructions
│  ├─ Product Constitution
│  ├─ Persona projection
│  └─ Manager or Expert role
├─ scoped_instructions
│  ├─ turn purpose and response contract
│  ├─ granted capability descriptors
│  └─ loaded Playbook bodies
├─ contextual_data
│  ├─ compact User Model projection
│  ├─ retrieved confirmed Memory
│  ├─ authorized domain Views / current State
│  └─ retrieved Session Archive evidence
├─ conversation
│  ├─ compaction summary and recovery pointers
│  ├─ recent ordered messages and call/result pairs
│  └─ current user turn
├─ ephemeral
│  └─ local time, surface, budget and execution state
└─ manifest
   └─ source IDs, revisions, hashes, classes, expiry and token estimates
```

The serialized order optimizes stable-prefix caching; it does not determine trust.
Each section carries an explicit kind and trust label. `contextual_data` is always
quoted/structured evidence and can never become an instruction merely because it
contains imperative text. Model adapters must preserve these boundaries instead of
flattening every item into one undifferentiated string.

### Precedence and conflict rules

```text
Product Constitution
> host policy, grants and action authority
> Role
> Persona
> user-authored current request
> reviewed Playbook
> User Model and Personal Memory
> connector, document and session evidence
```

- Higher layers cannot be overridden by lower ones.
- Persona affects expression and judgment style, not factual truth or authority.
- Playbooks guide a task but cannot expand capabilities, data classes or View scope.
- Memory records facts and preferences; it is not an executable standing command.
- Current explicit user intent wins over an older preference when policy permits;
  the conflict is recorded as learning evidence rather than silently rewriting Memory.
- Expert context is a least-privilege projection. Experts do not receive the full
  Manager transcript, Persona, User Model or Memory unless their assignment declares
  and the host grants a specific projection.

### Assembly pipeline

```text
Turn request
→ resolve Person, session, role and purpose
→ freeze policy/grant/registry revisions
→ derive typed ContextQuery
→ retrieve candidate User Model, Memory, View, Archive and Playbook records
→ filter by Person, confirmation, tombstone, class, grant, freshness and scope
→ rank, deduplicate and allocate per-section budgets
→ project records into instruction or evidence items
→ append bounded conversation and ephemeral state
→ emit ContextEnvelope + ContextManifest
→ authorize again immediately before model dispatch
```

Retrieval uses the current request, explicit entities, session purpose and active
scope. Keyword/FTS and typed filters are the S5 baseline; embeddings may be added
later behind the same port. Ranking must prefer explicit user selections, current
scope, confirmed exact matches, recency and source quality. It must never retrieve a
pending, rejected, superseded, tombstoned, foreign-Person or ungranted record.

Context is frozen for one model call, not an entire session. A later iteration may
assemble a new envelope after a capability result, but the manifest records changed
revisions. This gives new turns fresh Memory while keeping the product-owned prefix
cacheable. A capability result is appended through the existing ordered call/result
protocol rather than secretly injected by the assembler.

### Budget and degradation

The caller supplies one total input budget. The assembler first reserves space for
the current turn, response headroom, Product Constitution, Role and capability
schemas. Remaining space is assigned to bounded sections; initial target shares are
configuration, not prompt instructions:

| Section | Initial ceiling of available retrieval budget |
| --- | ---: |
| User Model | 10% |
| Personal Memory | 25% |
| domain Views / State | 30% |
| Session Archive | 15% |
| Playbook indexes and loaded bodies | 20% |

Unused space may flow to another section. Degradation order is old conversation,
low-ranked Archive, low-ranked Memory, optional View detail and optional Playbook
references. The current request, accepted call/result pairs needed for causality,
loaded Playbook core body and product/scoped instructions are not silently truncated.
If they do not fit, assembly fails with `BudgetExceeded` or requests compaction.

### Hierarchical Playbooks

Floe retains its existing terminology: a **Skill** is an executable capability; a
**Playbook** is the procedural artifact called a Skill by Hermes, OpenClaw and Codex.
The UI may explain this as “workflow guidance,” but wire types remain unambiguous.

```text
PlaybookIndexEntry { id, revision, name, summary, triggers, scope }
PlaybookBody       { instructions, requiredCapabilities, references, children[] }
PlaybookChild      { id, revision, name, summary, triggers }
```

Only root entries eligible for the current Manager or Expert are included in the
initial index. Loading a root returns its body and summaries of its direct children.
A child becomes loadable and visible to the model only through its loaded parent;
loading it reveals the next level. This is the **nested Playbook** contract:

```text
root summaries
→ load parent body
→ direct child summaries become visible
→ load one child body
→ that child's direct child summaries become visible
```

The registry enforces one parent, acyclic ancestry, immutable `(id, revision)`, a
maximum depth and per-turn load budget. The initial S5 limits are depth 4, 32 visible
summaries, 8 loaded bodies and 32 KiB total Playbook content. Loading is recorded in
the context manifest and session trace. A Playbook cannot invoke another Playbook or
a Skill by itself; the model chooses the next load or capability call.

Manager and Expert common prompts contain no workflow. For example, “find focus
time” becomes an optional Calendar Playbook. The Schedule Expert instead receives
general calendar-read/search and free-window capabilities and decides how to use
them. A Playbook may recommend a sequence for a recurring task, but ordinary requests
remain model-directed.

### Context inspection and replay

S5 exposes a safe context inspection view showing section sizes, included record
labels, provenance, revision, freshness and exclusion reasons. Raw highly sensitive
evidence and hidden model reasoning are never shown in diagnostics. Each model attempt
stores the manifest, assembler version, policy/registry revisions and hashes needed to
reconstruct the authorized projection. Deletion follows provenance into indexes,
summaries and compaction artifacts; tombstones prevent replay from resurrecting data.

## S5 implementation order

1. Add `ContextEnvelope`, `ContextItem`, `ContextManifest` and assembler ports while
   adapting today's empty `AgentContext` without changing model behavior.
2. Add versioned Persona and typed User Model projections with inspect/edit/reset and
   strict separation from Product Constitution.
3. Add confirmed Memory retrieval, FTS session retrieval and per-chat use/generation
   controls; reject pending/rejected/tombstoned records before ranking.
4. Add root Playbook index, body loading and nested discovery, then move focus-time
   procedure out of Schedule Expert's common prompt and hard-coded input contract.
5. Add context inspection, compaction/recovery manifests, deletion propagation and
   replay evaluation for grounding, retrieval precision and prompt-budget regressions.

## Consequences

- Floe can be personalized deeply without turning personality or memory into authority.
- Context use becomes explainable and testable rather than prompt concatenation.
- New Memory is visible on the next relevant call/turn instead of waiting for a new
  session, at the cost of more assembler and cache-key complexity.
- Nested Playbooks reduce irrelevant summaries, but require cycle/version validation
  and make discovery quality part of acceptance testing.
- Typed records and manifests cost more implementation work than Markdown injection,
  but preserve Floe's provenance, deletion and Review requirements.

## References

- [Hermes prompt assembly](https://github.com/NousResearch/hermes-agent/blob/9e0dc4319ae2d59c60f5226b5c0af58d17755bfa/website/docs/developer-guide/prompt-assembly.md)
- [Hermes Memory](https://github.com/NousResearch/hermes-agent/blob/9e0dc4319ae2d59c60f5226b5c0af58d17755bfa/website/docs/user-guide/features/memory.md)
- [Hermes Skills](https://github.com/NousResearch/hermes-agent/blob/9e0dc4319ae2d59c60f5226b5c0af58d17755bfa/website/docs/user-guide/features/skills.md)
- [OpenClaw agent workspace](https://github.com/openclaw/openclaw/blob/7115c4b6fccfcbeb9c121ea7523c22207272effc/docs/concepts/agent-workspace.md)
- [OpenClaw context](https://github.com/openclaw/openclaw/blob/7115c4b6fccfcbeb9c121ea7523c22207272effc/docs/concepts/context.md)
- [OpenClaw system prompt](https://github.com/openclaw/openclaw/blob/7115c4b6fccfcbeb9c121ea7523c22207272effc/docs/concepts/system-prompt.md)
- [OpenAI Codex AGENTS.md](https://developers.openai.com/codex/guides/agents-md)
- [OpenAI Codex Skills](https://developers.openai.com/codex/skills)
- [OpenAI Codex Memories](https://learn.chatgpt.com/docs/customization/memories)
- [OpenAI response compaction](https://developers.openai.com/api/docs/guides/latest-model#4-compaction-extending-effective-context)
