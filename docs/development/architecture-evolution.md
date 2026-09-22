# Architecture evolution during pre-stable development

Floe is still in active pre-stable development. Product requirements can change quickly, and agent-assisted implementation is used to move from a product decision to working code with high throughput. This makes structural discipline more important, not less important.

This document defines how Floe should change when requirements invalidate existing design assumptions. Current structure belongs in [architecture](../architecture/README.md); final-state properties belong in [architecture invariants](../architecture/invariants.md); durable rationale belongs in ADRs; temporary migration sequencing belongs in a task-specific execution plan when one is needed.

## Why this policy exists

Earlier high-velocity development repeatedly produced locally reasonable fixes that accumulated globally:

1. a requirement changed or an existing path blocked the immediate task,
2. the implementation preserved the old structure and added an adapter, port, optional field, fallback or branch,
3. the immediate tests passed,
4. the transitional route remained available to later work,
5. later changes had to understand and preserve both the old and new path.

This pattern optimizes for escaping the current obstacle with the smallest local disruption. Repeated across many tasks, it creates duplicated authority, widened contracts, ambiguous execution paths and failure modes that are difficult to trace.

Floe's current lifecycle permits a different optimization target: internal APIs, directory structure, module boundaries, abstractions and even implementation language may change when the product and system become simpler as a result. Compatibility is a requirement only when an explicit external or durable-data boundary makes it one.

## Primary optimization target

**Do not minimize the diff. Minimize the final system.**

Small diffs are useful only when they also leave a coherent final structure. A larger replacement is preferable to a small patch that permanently preserves two ways to represent or execute the same concept.

Before choosing an implementation, describe the desired final state:

- Who owns the state or behavior?
- What is the canonical contract?
- What is the canonical runtime path?
- Which callers should remain?
- Which old types, routes, fields or adapters should cease to exist?
- Which safety properties must still hold?

Implementation sequencing follows from that target. The existing shape is evidence to investigate, not a constraint that must always be preserved.

## Change classification

Use the lightest process that matches the architectural impact.

### Local change

A change is local when it stays inside one existing owner, does not alter a public or persisted contract, does not change dependency direction and does not require caller migration.

Examples include an internal algorithm fix, a private helper refactor or a local UI behavior adjustment.

Local changes may be implemented directly and then verified.

### Cross-component change

A change crosses components when more than one existing owner participates but the ownership model and durable boundaries remain intact.

Write a short plan in the task context when sequencing matters. Reuse current owner contracts rather than creating a new shared abstraction by default.

### Architectural change

Treat a change as architectural when it affects any of the following:

- semantic ownership or authority,
- package/crate/module dependency direction,
- public Rust API or FFI/wire contract,
- persistence meaning or durable schema semantics,
- runtime lifecycle, cancellation or recovery,
- authorization/provenance boundaries,
- provider/platform boundary placement,
- introduction or removal of a cross-module abstraction,
- migration of multiple callers from one contract/path to another.

For these changes, use .agents/skills/architecture-change/SKILL.md. If the task is large enough that execution must survive context loss or multiple checkpoints, create or update one authoritative execution plan rather than scattering TODOs across code and documents.

## Establish current reality before designing

Use the smallest relevant context and in this order:

1. actual source, manifests and current tests,
2. the architecture document for the affected owner,
3. repository-wide architecture invariants,
4. an ADR only when the rationale for a durable decision is needed,
5. the current task's authoritative execution plan when a staged migration is already in progress.

Do not start by recursively reading historical plans. Old implementation paths, validation counts and slice descriptions are evidence, not current state.

If current source and prose disagree, determine whether the source is violating accepted policy or the document is stale. Fix the discrepancy in the same change when it is in scope.

## Root cause before workaround

When a task becomes awkward, first ask whether the awkwardness is caused by the design itself.

Inspect:

- ownership: is the needed fact owned in the wrong place?
- contract: does the API expose implementation details or omit required semantics?
- lifecycle: is state created, validated or destroyed at the wrong boundary?
- dependency direction: is a caller reaching around the real owner?
- authority: are two values competing as sources of truth?
- abstraction: is a layer preserving an obsolete shape?
- persistence: is a transient migration concern leaking into durable state?

A local workaround is appropriate only when the architecture remains correct and the problem is truly local.

## Replacement is the default internal migration strategy

For an internal contract change from A to B, the default sequence is:

1. define B as the final canonical contract,
2. implement B at the correct owner/boundary,
3. migrate every in-scope caller,
4. port tests to the canonical path,
5. delete A and transition-only helpers,
6. search for residual symbols, variants, branches and aliases,
7. verify the whole affected surface.

Avoid leaving this completed shape:

~~~text
caller 1 -> A
caller 2 -> A -> compatibility adapter -> B
caller 3 -----------------------------> B
~~~

Prefer:

~~~text
caller 1 --+
caller 2 --+-> B
caller 3 --+

A deleted
transition adapter deleted
~~~

A compile break during a bounded replacement can be cheaper and safer than maintaining a false compatibility layer while callers are migrated.

## Compatibility is scoped, not assumed

### Internal contracts

Floe's internal Rust/Dart/Go APIs, test-only local data and development-only wire shapes do not require backward compatibility unless a current requirement explicitly says otherwise. Replace them directly and remove obsolete callers.

### External protocols and provider contracts

External provider versions, OAuth fields, signed challenge formats, A2A protocols and other independently versioned systems are real boundaries. Do not rename or break them merely because an internal alias is being removed.

### Durable user data and side effects

Pre-stable does not mean destructive by default. Existing data may be intentionally incompatible during local development, but resets must remain explicit and scoped to identified Floe-owned development data. Uncertain external operations, credentials and unrelated files are never deleted as a convenience fix.

When Floe gains stable public clients or migration guarantees, this policy must be revisited through an ADR rather than silently accreting compatibility layers.

## Abstraction policy

Introduce an abstraction when it represents a current semantic fact, for example:

- a real external/platform boundary,
- multiple existing implementations that share a meaningful contract,
- an ownership or authorization boundary,
- a policy seam that must be independently tested,
- a stable contract protecting a volatile implementation.

Do not add one primarily because:

- a future implementation might exist,
- old and new designs need to coexist,
- one caller wants a shorter import path,
- a facade can forward one method unchanged,
- naming something Port/Adapter/Service makes the dependency graph look cleaner without changing ownership.

Duplication that is local and obvious can be cheaper than a premature shared abstraction. Consolidate after the common semantic boundary is demonstrated.

## Stop signals

Stop adding local plumbing and reassess the architecture when any of these occur:

1. a second internal compatibility adapter or execution route is needed,
2. old/new behavior needs a permanent branch,
3. the same authority is independently represented in two places,
4. a domain/owner type needs provider-, storage- or transport-specific data,
5. an optional field is needed only so old callers can omit required canonical context,
6. a private/internal API must become public solely to bypass an owner,
7. a dependency edge would reverse the intended layer direction,
8. two modules define different versions of the same concept,
9. plumbing changes are materially larger than the feature semantics,
10. implementing the feature requires bypassing the abstraction that supposedly owns it.

These signals do not automatically require a large rewrite. They require a deliberate final-state decision before more code is added.

## Plan depth

Do not create heavyweight plans for every task.

Implement directly when the change is local and the final state is obvious.

Use a short task-local plan when several files or components must change but ownership remains unchanged.

Create or update a repository execution plan when the change has multiple caller migrations, changes ownership/authority, alters an FFI or persistence contract, changes runtime lifecycle, removes a substantial old path, or is likely to span multiple agent contexts/checkpoints.

A repository plan must define the final target, ordered checkpoints, deletion/residual gates and verification. It must not become a second global status board.

## Temporary complexity must be bounded

Architecture work may need temporary dual definitions or compile failures while the cutover is in progress. Temporary complexity is acceptable only when the same authoritative task plan names how and when it disappears.

"TODO remove later" is not a removal plan.

If a temporary compatibility layer is genuinely required, record at least:

- why direct replacement is blocked,
- exact remaining consumers,
- removal condition,
- responsible task checkpoint or bounded lifetime.

Do not encode temporary migration state into permanent domain contracts when a local execution-plan state can express it instead.

## Definition of done for architecture-affecting work

Passing tests is necessary but not sufficient.

A completed architectural change has:

- correct product behavior,
- one canonical owner and runtime path,
- all in-scope callers migrated,
- obsolete paths and transition-only adapters deleted,
- no unexplained public/FFI surface growth,
- no migration-only optional state,
- residual searches showing old symbols/branches are absent or explicitly justified,
- architecture dependency checks passing,
- safety/recovery invariants preserved,
- current architecture documentation updated,
- relevant full-surface verification passing.

The change is complete when the system has converged, not merely when the new path can execute.

## Continuous convergence

Do not postpone architectural cleanup until another large refactor.

A feature may temporarily increase complexity while being built, but completion should return the affected area to a single coherent design. Repeating this after every architecture-affecting feature is cheaper than allowing compatibility layers and duplicate authority to become the next feature's baseline.

The long-term discipline is:

~~~text
requirement
  -> impact classification
  -> final-state design
  -> implementation/caller cutover
  -> obsolete-path deletion
  -> residual audit
  -> verification
  -> documentation convergence
~~~
