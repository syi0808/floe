# Expert extensibility and preselected source bindings

## Authority, baseline and scope

This is the authoritative, task-specific execution plan for the next Expert extensibility change. It is a target and an ordered cutover, **not a claim that the target architecture is implemented**. The seven companion documents own checkpoint instructions; this file alone owns overall status and sequencing. Do not create another status file or parallel migration plan.

- Source baseline: `43005508338d7ae38d3247910361c733d7cdfe98` on `main`, rechecked on 2026-09-25.
- Earlier investigation baseline: `6bbaebd3d64efa3ab8a9127b0a20905921d27d7c`. Do not execute its obsolete Calendar/Schedule migration assumptions.
- Evidence level at plan creation: source and test inspection. The three regression findings below require executable reproduction; no implementation or runtime validation is claimed by this documentation commit.
- Authorized scope of this change set: execution-plan documentation. This plan's existence does not authorize an agent to implement, push subsequent code, deploy, reset profiles, or modify external accounts without the applicable task instruction.
- Repository prose remains English. Product copy follows the existing localization system.

Read [AGENTS.md](../../../../AGENTS.md), the [architecture-change skill](../../../../.agents/skills/architecture-change/SKILL.md), [current architecture](../../../architecture/README.md), [invariants](../../../architecture/invariants.md), and only the current checkpoint's owner-specific references. [Authority and recovery](../../../architecture/authority-recovery.md) is mandatory for authority, binding, settlement and resume changes. Use the [verification skill](../../../../.agents/skills/code-change-verification/SKILL.md) after code changes.

### Preserve what already converged

At the baseline, Schedule uses the common built-in endpoint; Calendar source permission is outside Expert Registry state; recoverable source/model blockages produce typed outcomes and trusted durable interactions; source-independent installed Expert discovery is intentional. Do not restore `ScheduleEndpoint`, Calendar Expert setup/view permission records, old Calendar Expert wire commands, or the removed Flutter Calendar Expert settings flow. The current runtime is documented in [runtime.md](../../../architecture/runtime.md).

The remaining work is different: fix consumer/producer contract mismatches, remove closed-world package/host/result semantics, and distinguish persistent source selection from current access authority.

## Target decisions

| Concern | Final owner and rule |
|---|---|
| Connector implementation, account and connection lifecycle | Product/Connections and real provider adapters. No Expert owns credentials or defines which product connectors exist. |
| Source identity, resource references, capability compatibility and acquisition | Context/Connections contracts and services. Keep legitimate typed Calendar/Mail/etc. contracts at their owners. |
| Read, processing and release permission | Access. Installation, a manifest declaration, availability or a saved binding is never a permission. |
| Expert package, installation, assignment and requirement selection | Experts. Store configuration and source references, not authoritative copies of grants or connection state. |
| Prompt, reasoning and domain result | The Expert package. Shared contracts do not enumerate individual Experts or Focus insights. |
| Task identity, admission and endpoint resolution | Experts/Directory/TaskCoordinator. Preserve exact task, invocation, definition and replay identity. |
| Model profile, attempt and usage | Inference. Keep runtime role separate from the exact source-consuming principal and recipient consent. |
| Interaction origin, reviewed target, decisions and linked resume | Conversation; authority mutations remain with their source owners, and binding mutations with Experts. |
| Consequential proposal and external effect | Actions. An Expert proposes; neither an artifact nor source read permission authorizes execution. |
| Persistence | Owner-defined repositories implemented by Vault. Physical storage does not transfer policy ownership. |

### Three independent questions

1. **Callable:** Is the package/installation/assignment/endpoint valid and enabled?
2. **Configured:** Which source/resource set is selected for each declared requirement?
3. **Readable now:** Do current connection identity, grant, policy, source freshness and processing/recipient checks allow this operation?

A missing required source does not hide an otherwise callable Expert. The Expert may report a bounded limitation and a trusted setup/review path. Optional-source failure is not empty successful evidence.

### Configuration is separate from runtime acquisition

```text
Setup / explicit settings change
  manifest requirement -> compatible candidates -> explicit or permitted default selection
  -> persisted assignment binding (CAS, exact command identity)

Invocation
  trusted assignment + immutable admitted binding snapshot
  -> requirement ID + bounded query
  -> Context exact-target acquisition -> Access fences -> real adapter
  -> SourceReadOutcome + every contributing dependency
  -> Expert result / trusted interaction -> Manager synthesis
```

Automatic selection is allowed only as a setup/configuration operation under existing product policy, without manufacturing access permission. Ambiguity goes to user selection. Connecting a new source never silently widens existing bindings. A broken binding never falls back to another account. Runtime may refresh credentials and revalidate the selected source; it may not discover a substitute.

Keep Manager's existing direct evidence tools as well as delegation. Its actual consumer must be admitted by the reviewed product policy. Manager source selection is product-owned and does not implicitly bind every Expert. A new installation or declared requirement does not retroactively join old grants, old reviews, or another consumer's consent.

Built-in provenance may affect distribution defaults and explicit trust policy, but not a separate execution path. An arbitrary ID does not prove first-party trust. Default installation, activation and binding use the same generic owner APIs as explicit configuration and never overwrite a user's disablement or selection.

### Limits and non-goals

Prefer existing `AgentEndpoint`, `ExpertReport`, `Artifact`, `TaskSnapshot`, `SourceReader`, `ExpertModel` and owner services. Introduce a type/port only for a real new boundary or invariant. No default new workspace crate, internal compatibility adapter, old/new decoder, optional migration state or versioned-next runtime.

Do not build dynamic library/WASM loading, a marketplace, arbitrary downloaded code execution, a connector plugin ecosystem, arbitrary remote endpoint installation or a fully dynamic View schema system. Existing capability reuse is the extensibility acceptance target. Preserve current supported multiplicity; resolve an exact assignment or reject ambiguity rather than silently choosing the first. Supporting multiple simultaneous assignments of one package under new public agent identities is not required by this plan.

macOS is the acceptance platform for this task. Android parity is out of scope. iOS simulator/device acceptance is deferred to the actual iOS implementation task and must be reported as not executed, not passed. Changes must not gratuitously damage shared Apple source.

## Checkpoint order and status

| Checkpoint | Document / responsibility | Status | Completion evidence |
|---|---|---|---|
| 00 | [Baseline, reproducible findings and owner contract decisions](00-baseline-and-contract-freeze.md) | Not started | None |
| 01 | [Current contract regressions](01-contract-regressions.md) | Not started | None |
| 02 | [Prompt, result and settlement contracts](02-result-and-prompt-contracts.md) | Not started | None |
| 03 | [Generic Registry and runtime registration](03-registry-and-runtime.md) | Not started | None |
| 04 | [Binding, authority, interaction and product cutover](04-binding-and-product-cutover.md) | Not started | None |
| 05 | [Deletion and extensibility conformance](05-deletion-and-conformance.md) | Not started | None |
| 06 | [Final verification and documentation convergence](06-verification.md) | Not started | None |

Order is `00 -> 01 -> 02 -> 03 -> 04 -> 05 -> 06`. Substeps are commit boundaries only when all affected production callers compile and the required tests/deletions are complete. Otherwise use one bounded checkpoint change set; do not keep an obsolete wrapper merely to split commits.

A checkpoint closes only after canonical implementation, caller migration, replacement tests, obsolete code/test deletion, residual audit and matching current architecture updates. 04 must include working product setup and recovery; a backend-only binding type is not completion. 03 may still use the one current source-selection implementation until 04 replaces it; this is an explicit remaining behavior, not a second compatibility route or completed binding support.

## 00: executable baseline and contract freeze

Detailed execution: [00-baseline-and-contract-freeze.md](00-baseline-and-contract-freeze.md).

Before implementation:

1. Record actual HEAD, worktree changes and the diff from the baseline. Preserve user work. If HEAD moved, recheck affected symbols and update this plan's baseline evidence before applying instructions. Paths/symbols below are source anchors, not immutable line offsets.
2. Inspect current manifests and [dependency policy](../../../../tools/architecture/module-dependencies.json). Confirm the concrete caller paths in 01-04. New path/type/test names in this plan are proposed locations, not existing APIs.
3. Reproduce 01's three findings through actual producers/consumers. Record failing test/command and observed cause. A finding that does not reproduce is revised with evidence, not forced into the code.
4. Before changing contracts, write the exact owner API/serialization decisions into the relevant checkpoint: generic artifact/action bridge, assignment identity, bounded source-reference shape, binding pin lifetime, consumer identity and review digest. Reuse existing contracts where sufficient. Do not leave an unresolved choice to be guessed independently by two implementers.
5. Classify each touched old test as delete, move/rewrite, or retain. Track those decisions in that checkpoint's test table/report, not a global second ledger. Search inline `#[cfg(test)]` modules as well as test directories and fixtures.
6. Capture baseline failures/warnings and available platform prerequisites. No product test is considered passed because it silently returned when a native fixture was missing.

00 closes only with concrete reproduction evidence (or a corrected finding) and a coherent ownership/dependency map. Merely writing this plan does not close 00.

## Execution discipline and reporting

Work on the first incomplete checkpoint unless explicitly tasked otherwise. Read this file, that checkpoint and its relevant current owners; do not read all historical plans as default context. Do not rewrite unrelated code, weaken a safety assertion, introduce a generic JSON execution bypass, or automatically reset storage to get green tests.

Every implementation report must state:

1. Start HEAD and resulting commit(s), plus exact checkpoint/substeps completed.
2. Changed owners/contracts and the canonical final path.
3. Migrated callers and deleted code/types/routes/tests/fixtures.
4. Safety assertions retained or moved, with their new executable locations.
5. Exact checks run and observed pass/fail/unavailable results; no invented counts.
6. Residual matches and narrowly justified exceptions, with a removal checkpoint where transitional.
7. Current architecture/ADR changes and remaining blocker; update only this status table for overall progress.

A documentation commit may report content/link/whitespace checks; it must not report Rust, Flutter, native or server execution as verified. Push/deployment permission is governed by the current task, not this template.

### Agent start instruction

```text
Read AGENTS.md, docs/development/plans/expert-extensibility/README.md,
and the first incomplete checkpoint document. Recheck current HEAD and worktree.
Execute only the requested checkpoint, including caller cutover, old code/test
removal, targeted tests, required broad gates and residual audit. Preserve the
existing Access/Context/Conversation safety and linked-resume boundaries. Do not
reintroduce Calendar Expert setup or a Schedule-specific endpoint. Report using
the plan's format and do not advance past an unverified deletion gate.
```

## Plan lifecycle

Implementation changes update current architecture when the actual path changes, not in advance. Add/amend an ADR only for a durable rationale change; explain preselected binding separately from grant authority and preserve durable interaction rationale. Current product and design documentation is updated with the real UI cutover. Root AGENTS.md does not become a checkpoint dashboard.

After 06 and user acceptance, remove this temporary plan bundle and its task-only entry from the documentation index in a bounded documentation cleanup. Git history is the archive. Do not retain a completed migration as default agent context.
