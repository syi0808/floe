# ADR 0033: Run-scoped Agent environment and grounded Manager

- **Status:** accepted
- **Date:** 2026-10-01
- **Amends:** ADR 0017 context lifetime/assembly semantics and ADR 0018 Manager–Expert discovery/delegation semantics
- **Scope:** root Agent readiness, Run-scoped Expert configuration, attempt-scoped Context, prompt-cache boundaries and Manager grounding
- **Amended by:** [ADR 0034](0034-gateway-reasoning-and-source-processing-authority.md) for live source-processing authority and Primary/Fallback grounding evaluation order

## Amendment by ADR 0034

Root readiness, immutable Run Expert configuration, attempt-scoped evidence, prompt identity and the factual-support rule remain accepted. References below to live model-recipient consent are superseded by live source-owned processing authority and verified Gateway identity; Run configuration still cannot grant or freeze authority.

The pre-cutover Foundation Manager Checkpoint 07 evaluation remains historical execution evidence, including the hard-gate failure. It is not evidence that the intended Gateway Primary fails. Do not start the old agent-execution-environment-grounding CP08 or a typed-grounding implementation from that result alone. The [reasoning/source-processing convergence plan](../development/plans/reasoning-source-processing-convergence.md) is the single migration sequence; its Checkpoint 11 evaluates Gateway Primary under the existing strict corpus/rubric and Foundation Fallback under a separately reported hard safety floor after the cutover.

Primary strict PASS leaves typed grounding deferred absent another concrete need. Primary strict FAIL requires a new post-cutover grounding design. Fallback-only safety failure marks that fallback not Ready and requires bounded mitigation, rather than weakening factual safety or forcing Primary-level intelligence from it. The original unconditional escalation wording below is amended by this evaluation order, not permission to accept unsupported claims. This ADR amendment does not claim that the new routing or evaluation has been implemented.

## Context

Floe has four state classes with different lifetimes but the current design does not
represent those lifetimes consistently.

Conversation Session state persists across user turns. Expert package installation,
Registry configuration and Directory publication form the root Agent's execution
environment. Source-backed evidence is observed for a bounded request and time.
Grants, source authority, exact-recipient consent, provider identity and OS permission
are live authority that may change while a Run is executing.

The existing implementation can nevertheless create or resume a Conversation Session
before the Expert Registry exists because first installation is attached to
Conversation `Start` while the normal product entry uses `Resume`. Turn execution then
uses an `ExistingOnly` Expert refresh that intentionally leaves an absent Registry
absent. A runnable Session can therefore be mistaken for a ready root Agent
environment.

The Manager path exposes a second mismatch. The model can return a structurally valid
direct answer even when a factual claim about the person's private, current or
changing external state has no observation behind it. Provenance and coverage can
reauthorize evidence the model actually consumed; they cannot prove that a claim
which bypassed evidence should have had a dependency.

Prompt caching adds another reason to distinguish lifetimes. A stable prompt prefix,
a Run configuration snapshot and attempt-scoped evidence should not be invalidated or
frozen as one unit. Cache identity is an optimization over a correctly assembled
input, never a substitute for freshness or authority.

## Decision

### Session and root Agent environment are separate lifecycles

Conversation owns Session/history continuity. It does not own Expert package or
Registry initialization.

After Vault create or unlock, App composition prepares the root Agent environment
before Conversation is runnable. Preparation reconciles the trusted shipped Expert
bundle, creates the Registry when genuinely absent, performs first-install-only
default source binding, and publishes the resulting Directory.

An absent/unprepared Registry and an intentionally configured environment with zero
active Experts are different states. The former cannot reach Conversation execution;
the latter is a valid ready configuration.

Conversation `Start` and `Resume` remain Session operations. Neither is a hidden
Expert-environment setup command, and ordinary turn execution is not a repair path
for missing Agent readiness.

### One immutable Expert environment per Run

Run admission samples the current eligible Expert configuration exactly once and
creates one immutable Run environment. That environment binds, for every advertised
Expert:

- the exact `AgentDefinition` and definition revision;
- the exact `ExpertAdmissionIdentity`;
- the exact `ExpertExecutionSelection`;
- the runtime endpoint selected for that definition;
- one deterministic environment revision/digest.

The Manager-visible discovery catalog and actual delegation target are derived from
that same environment. The runtime must not show one Directory snapshot to the model
and later re-resolve a different live Directory entry when executing the model's
delegation.

A Registry, assignment, enablement or source-selection configuration change made
after Run admission applies to a later Run. It does not reroute or reinterpret the
already admitted Run.

A same-Run crash/recovery path must either reconstruct the exact required environment
identity or fail closed. It must never substitute current configuration and present
that execution as the original Run.

### Configuration is not authority

The Run environment freezes configuration, not permission.

The current Run continues to re-check all authority owned outside Expert
configuration, including as applicable:

- current Connection/source existence and identity;
- `SourceAuthority` and observed physical resources;
- `DataAccessGrant` and `GrantAuthority`;
- model recipient consent and exact recipient;
- provider/current credential admission;
- OS/native permission;
- cancellation/deadline;
- source/model dependency coverage;
- Action authority and external-write preconditions.

Revoking or invalidating one of those authorities can deny the current Run
immediately even though its Expert configuration remains pinned.

A completed Task is historical evidence from its admitted environment. Later Expert
configuration changes do not rewrite that history. Any new source read, model
dispatch or external effect still passes the current authority owner.

### Run configuration and model-attempt Context have different lifetimes

The Run environment is fixed for a Run. A model `ContextEnvelope` is assembled for
one model attempt.

A later model iteration in the same Run may reauthorize retained history, admit a
fresh Expert result, retrieve different eligible Memory or observe newly available
Context. Those attempt-level changes do not mutate the Run's Expert environment.

The stable Manager program and Persona are likewise fixed for the Run unless the
runtime intentionally changes role, such as bounded finalization. The model adapter
must receive the same stable bytes for equivalent stable program identity.

### Discovery data is not instruction text

Expert cards and capability descriptors describe what can be selected. They do not
grant authority and they are not higher-precedence behavioral instructions.

The context contract therefore separates:

- stable instructions: Behavior Kernel, Role, Persona and stable capability-use
  protocol;
- Run instructions: purpose and response contract;
- discovery data: active Expert cards and capability descriptors;
- contextual data: Memory and admitted evidence;
- Conversation: retained causal history and current-turn exchanges;
- attempt context: correction and runtime/output bounds;
- manifest: safe identity, revision and hash metadata.

Adapters preserve these typed/trust boundaries rather than flattening discovery,
evidence and instructions into one undifferentiated prompt string.

### Prompt caching follows lifecycle boundaries

Prompt caching does not define semantics.

Canonical serialization is deterministic so an unchanged stable Agent program can
reuse its prefix while evidence changes. A new Run with changed Expert configuration
may change the Run/discovery frame without unnecessarily changing the stable prompt
prefix. A prompt or Persona revision intentionally changes that stable prefix.

Cache keys, cache-hit state and provider cached-token accounting never authorize data,
prove source freshness, freeze configuration or replace any Access/Context check.

### Manager factual eligibility is explicit

The Manager may state a factual claim about the person's private, current or changing
external state only when the claim is supported by at least one of:

- information the user supplied for the relevant scope and time;
- admitted current Context;
- a settled Expert result whose scope and time cover the claim.

General model knowledge, plausibility, prior assistant statements, Expert
descriptions, and failed, blocked or unavailable observations are not observations of
the person's current external state.

When required support is missing, the Manager selects a suitable advertised Expert
from the Run environment when delegation is permitted. If no suitable Expert is
active, delegation is disallowed, or the observation is blocked or unavailable, the
Manager returns a limitation and answers only what is supported. It does not convert
missing evidence into an empty result, all-clear result or plausible guess.

This is a model-visible decision rule, not a claim that natural-language truth can be
fully validated by the existing structural payload validator. Prompt behavior is
accepted through a fixed grounding evaluation. If prompting cannot satisfy the hard
grounding criteria, Floe must design a separate typed host-visible grounding contract
rather than add domain-specific prompt exceptions or brittle text heuristics.

## Consequences

- Vault-open readiness becomes the lifecycle boundary for root Agent environment
  preparation; Session entry no longer doubles as Expert setup.
- One Run has one Expert configuration truth for both discovery and delegation.
- Configuration edits have predictable next-Run visibility without weakening live
  authority revocation.
- Context may change between model attempts without making the Run's advertised
  Expert roster unstable.
- Discovery metadata moves to a data layer below behavioral instructions.
- Prompt-cache optimization can be measured with stable/run/attempt identities without
  becoming a source of authority.
- Manager grounding failures become explicit evaluation failures rather than
  acceptable structurally valid direct answers.
- Internal contracts and stored local development data may be replaced directly
  during this pre-stable cutover; no compatibility branch is required.

## Amendments to earlier decisions

[ADR 0017](0017-agent-context-assembly.md) remains authoritative for typed context,
Persona, Memory, Playbooks, evidence precedence and progressive disclosure except
where it previously grouped Registry/grant freezing together, treated the active
Expert index as scoped instruction text, or described one model-call freeze without a
separate Run configuration lifetime. Those points are superseded by this decision.

[ADR 0018](0018-manager-expert-a2a-delegation.md) remains authoritative for Expert
identity, natural-language A2A delegation, isolated Expert context and Task semantics
except where it previously re-resolved active Experts at each Manager model call or
allowed actual delegation to resolve against newer live Directory configuration.
Discovery and delegation now share the Run environment defined here.

Current architecture documentation must continue to describe implementation reality.
The runtime and authority documents are updated to these final semantics only as the
corresponding code cutovers land.

## References

- [ADR 0017 — Agent context assembly](0017-agent-context-assembly.md)
- [ADR 0018 — Manager–Expert A2A delegation](0018-manager-expert-a2a-delegation.md)
- [ADR 0029 — Pre-stable architecture convergence](0029-pre-stable-architecture-convergence.md)
- [Architecture invariants](../architecture/invariants.md)
