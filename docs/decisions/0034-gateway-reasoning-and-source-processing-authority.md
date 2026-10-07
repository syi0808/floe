# ADR 0034: Gateway reasoning and source-owned processing authority

- **Status:** accepted
- **Date:** 2026-10-02
- **Amends:** ADR 0011 inference boundary and fallback policy; ADR 0015 sensitive inference policy; ADR 0024 Health processing and onward reasoning; ADR 0028 connection processing permissions; ADR 0030 interaction kinds and automatic resume; ADR 0033 live processing authority and grounding evaluation order
- **Preserves:** ADR 0031 source/standing-grant separation, Run-pinned Expert configuration, live provenance and authority checks, and Actions-owned external-effect recovery

## Context

Product purpose was intended to hide provider/model configuration from ordinary Floe usage. The implementation nevertheless puts profile selection in Conversation intent and binds processing approval to an exact downstream recipient, profile, consumer, purpose, source scope and lineage. The resulting model-review card exposes deployment details and makes ordinary reasoning a separate permission ceremony.

Reasoning placement is also being used as a substitute for source privacy. First-party Observe policies currently use LocalOnly, built-in Experts have different placement restrictions, and Learner is local-only. Apple Health performs deterministic bounded reduction but has no mandatory local semantic privacy transform. The Wellbeing manifest declares HighlySensitive while actual Context evidence and the common Expert policy can classify it as Personal.

These are separate boundaries: product inference intent, source-processing authority, source-local privacy transformation, and reasoning capability selection. They must not be represented by one model-recipient permission mechanism.

This decision fixes the governing architecture. Acceptance of this ADR is not a claim that its runtime cutovers are implemented. Current source remains implementation truth; the [active architecture plan](../plans/2026-10-02-architecture-refactor.md) owns the remaining ordered migration. The [original convergence plan](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/development/plans/reasoning-source-processing-convergence.md) is historical rationale and evaluation evidence.

## Decision

### 1. Product purpose and the verified Gateway are the client boundary

Product policy selects quick_response, everyday_assistance or deep_work. A client may show Device versus Floe Gateway when that processing distinction matters to the person. Ordinary Conversation intent, AppWire and client UI must not select or expose provider, model, profile, exact downstream recipient, reasoning effort or a server-internal route.

The paired Floe Gateway is the external reasoning trust boundary. The Gateway owns its configured downstream provider/model routing; Gateway processing can include those configured providers. Removing their identities from product authority does not mean that the data is necessarily processed only inside the Gateway process. Operator configuration and diagnostics may retain those details.

Gateway identity remains authenticated and pinned through the current saved connection, verified Person/device and pairing evidence. An arbitrary endpoint, cached availability result or prepared transport is not authority. Private credentials stay inside their existing provider/key boundaries.

Ordinary model invocation is not a per-request approval event. There is no replacement model/provider consent card or Learner-specific background model grant. Internal profile IDs may remain for Inference/prepared-transport integrity and evaluation, not for product intent or user permission.

### 2. Access owns source-processing authority

A source grant expresses boundary-oriented processing policy:

```text
DeviceOnly
GatewayAllowed { categories }
```

GatewayAllowed permits the admitted bounded View to be processed on device and through the verified Floe Gateway. It does not identify a downstream model or authorize an Action. Gateway-internal route changes do not change source-grant identity or require another model approval.

The combined connector permission review defaults new source access to GatewayAllowed and discloses that processing may use the verified Gateway and its configured downstream models before the person allows access. For an initial inline Observe review, Access derives the requested categories from the trusted View only after proving grant absence; the stored review binds the combined permission. This avoids a second processing-only approval after the first authorized read. Existing grants, including paused DeviceOnly grants, retain their exact restriction unless the person explicitly approves a connector review that changes it. An explicit DeviceOnly choice remains available. Health's trusted `wellbeing.derived` View admits only Derived categories; this default never admits raw Health data or bypasses the mandatory local transform, source-owned receipt, or HighlySensitive classification.

Pairing grants no connector Observe access. Source selection, system/provider access, standing Observe permission, processing policy and Act authority remain distinct. Access continues to validate exact Person, source, logical View, consumer, purpose, categories and current grant authority. Connections owns current source/resources/subject authority; Context retains exact observed physical resources, provenance, coverage and freshness. A source epoch change does not by itself mutate a standing grant.

A known source whose processing policy is insufficient produces an owner-derived SourceAccess review, equivalent to ReviewProcessingPolicy, on the owning connection. Review binds the exact compare-only source/grant/policy expectations. Model dispatch cannot widen a grant, reinterpret an old LocalOnly grant as GatewayAllowed, or authorize processing by choosing a different model.

The exact model-recipient consent vertical is retired, not cosmetically hidden. Consent-only requirements, lineage, storage, commands, model blockers and product DTOs are removed with their callers. Independently useful source or identity semantics remain at their real owners rather than preserving a duplicate authority.

### 3. Manager, shipped Experts and Learner share one reasoning selector

All three roles use canonical Inference with Gateway/server Primary and device-local LLM Fallback. Manager and delegated Experts normally use everyday_assistance; Learner uses deep_work. Their consumers, prompts, budgets and Task/job semantics remain role-owned.

| Planning or execution state | Required result |
| --- | --- |
| Usable Primary exists | Select Primary; do not consider local Fallback |
| No configured/admitted Gateway capability, or valid inventory declares the purpose unavailable | Consider local Fallback |
| Credential/identity admission failure, invalid inventory, observation or transport failure | Propagate failure; do not reinterpret it as absence |
| Source processing permission is insufficient | SourceAccess review; no local permission bypass |
| Selected Primary fails, times out, exhausts quota or is cancelled | Preserve the failure; no silent local retry |

Profile observation must preserve failure versus valid capability absence. Availability and execution use the same rules. A role-neutral provider composition replaces root-specific composition and the stale parallel router; built-in package-specific placement differences and Learner's local-only route do not remain as hidden policies.

A legitimate special-deployment or third-party execution constraint may still be represented by the generic contract. It is not a reason to split the shipped roles' default reasoning topology.

### 4. Health has a separate mandatory device-local privacy operation

The mandatory semantic-transform scope is Health/Wellbeing only. Transform is a small shared typed interface that concrete HealthTransform implements; a future EmailTransform may implement the same interface. This does not introduce a transform registry, discovery system or orchestration framework, and does not authorize transforms for other connectors. Apple is the implementation target; Android local-model implementation and build gates are not part of this cutover.

The user clarified and approved this boundary on 2026-10-03: domain transforms are independent of acquisition and model execution. FoundationModels is one peer implementation of a common backend-neutral DeviceModel contract. All admitted local execution consumers, including reasoning Fallback, use that same semantic contract across Swift and Rust bindings. DeviceModel owns bounded execution, capabilities and generic structured output; it does not inspect role/purpose strings to select Health, Learner or Agent business schemas. Domain owners supply explicit output contracts and validate domain meaning. Higher Engine/ModelPort/Inference authority, Gateway-primary selection, source permission, accounting and independent job/receipt lifetimes remain unchanged. The first implementation supplies FoundationModels only; an injectable boundary is not a claim that another local backend is implemented.

Device models are prioritized for transforms; Health specifically remains mandatory device-only with no remote or deterministic semantic fallback. Future transforms retain their own explicitly decided placement policy. Manager, Experts and Learner retain the common remote Primary/device Fallback rule in section 3; no historical Learner RemoteOnly exception or model-recipient background grant is restored.

```text
HealthKit
  -> bounded deterministic acquisition and aggregation/minimization
  -> mandatory device-local Health semantic privacy transform
  -> validated typed WellbeingView + source-owned transform evidence
  -> Context
  -> Gateway Primary or separate local reasoning Fallback
```

Keep deterministic acquisition safeguards, sample/query bounds, the bounded window, interval merging, numeric aggregation and metadata removal. The local model receives only a small Health-specific aggregate. Raw samples and the pre-transform aggregate never enter Agent Runtime, Conversation, Memory, Flutter product payloads or Gateway transport.

The Health-owned operation is neither an Agent Tool nor an Inference ModelPort call. It receives no Persona, Memory, Conversation, Run frame, Expert/Tool catalog, delegation capability or Agent model-step grammar. Its closed typed result contains capacity/recovery enums, not free-form authority-bearing text. The source host constructs handles, timestamps, confidence/provenance and policy/contract revision evidence; the model cannot assert them.

Local-transform unavailability, invalid output, timeout, cancellation or failure is fail-closed source/capability unavailability, not missing consent. Do not fall back to a remote sanitizer or to the old deterministic capacity/recovery classifier. Previously valid cached evidence keeps only its existing expiry and ordinary authority checks; a failed transform cannot refresh it.

The same physical Foundation model may perform the privacy operation and a reasoning fallback, but these are separate contracts and invocations. Health must pass the privacy operation before either local or Gateway reasoning can use it.

### 5. Sensitivity and processing eligibility remain independent

WellbeingView owns its HighlySensitive classification regardless of the consuming Expert. Model projection derives classes from admitted evidence; a caller may be stricter but cannot downgrade them. People and Attention remain Personal under their existing View contracts.

Transform success does not declassify Health and does not grant permission. Gateway use of Health requires all of:

- HighlySensitive classification retained;
- valid source-owned device-local transform evidence;
- current source/grant/provenance admission and GatewayAllowed policy for the categories used.

Missing, forged or stale required transform evidence fails closed. Credential and DeviceOnlyRaw remain forbidden from generic Agent reasoning. Baseline conversation/Persona/Memory processing does not recreate model-recipient consent; any additional stored-data processing restriction belongs to its data owner.

### 6. Genuine interactions resume durably without a routine Continue action

Conversation retains SourceAccess and ExpertBinding interactions. Actions review/execution remains Actions-owned. Source decisions still bind immutable reviewed state, persist decision intent before owner mutation, use CAS, and revalidate current owner evidence. Not now, denial, expiry or navigation do not grant authority.

When an origin's interaction group becomes terminal with at least one Resolved member, the need to resume must be durable. Prefer the existing deterministic resume command and unique per-origin slot, with idempotent recovery/reconciliation, over a second orchestration system. Crash after owner mutation or resolution, lost acknowledgement, duplicate Resolve and Resolve/Refresh races must converge on exactly one admitted linked child Run.

The child is fresh work with current context and live authority, not automatic takeover of the parent's validated batch and not another copy of the user's text. Existing exact-batch/environment checks still govern independent pending-work continuation.

Do not keep a live Run waiting for the person. Project the durable interaction separately from generated assistant output; do not invent a model answer or persist a synthetic model-consent limitation to explain an unissued generation. A normal resolved review must not require the person to press Continue. Internal resume machinery remains.

### 7. Grounding is evaluated against the intended Primary and Fallback roles

ADR 0033's factual-support rule, Run environment, context lifetimes and cache/authority separation remain in force. The old agent-execution-environment-grounding Checkpoint 07 Foundation evaluation remains historical evidence, including its hard-gate failure. It is not evidence that the intended Gateway Primary fails.

Do not start that plan's old CP08 or an immediate typed-grounding implementation. First complete the authority/privacy/reasoning cutover and run the existing 22-case corpus and rubric under the convergence plan's Checkpoint 11.

Primary uses the existing strict gate. Foundation Fallback uses a separately reported hard safety floor: unsupported current/private claims, stale or guessed observations, unavailable-as-all-clear, unobserved action success and explicit no-delegate violations remain failures. Excess delegation or conservative limitations are quality findings, not a requirement for Primary-level intelligence.

Primary strict PASS leaves typed grounding deferred unless another concrete need remains. Primary strict FAIL leads to a new post-cutover architecture task. Fallback-only safety failure marks that fallback not Ready and requires a bounded fallback mitigation; it neither makes unsafe fallback acceptable nor automatically forces a new grounding architecture on a passing Primary.

## Safety properties that remain mandatory

Preserve verified Gateway identity, source-owned processing authority, key identity, provenance, CAS, durable pre-dispatch intent, cancellation direction and uncertain external-write recovery. Removing recipient consent does not remove admission/handoff/post-I/O/release fences or allow a model, UI, cache or configuration snapshot to grant authority.

Observe and GatewayAllowed never authorize writes. Actions retains approval, exact target/provider preconditions, idempotency and lookup/reconciliation after uncertain effects. No automatic data reset, key replacement, account mutation or deployment is authorized by this decision.

## Consequences and alternatives

- Product decisions concern source use and processing boundaries rather than downstream model configuration.
- Health privacy must be established before Gateway-first caller migration; changing routing first would violate the required precondition or strand LocalOnly grants.
- Source review and durable automatic resume replace model review and manual recovery UX; hiding fields or deleting only the Continue button is insufficient.
- Gateway-primary reasoning is not outage failover. Errors stay observable instead of silently changing execution mode.
- Health-specific typed transformation is chosen over an Agent-driven sanitizer, remote sanitization, a deterministic semantic fallback or a speculative universal framework.
- Internal contracts are replaced directly with caller/test/doc deletion. No permanent old/new consent or routing compatibility path is introduced.

## Decision lineage

This ADR supersedes only the conflicting portions of the following decisions; their unrelated rationale and safety properties remain:

- [0011](0011-inference-performance-classes.md): external-transfer discovery/response consent facts and unspecified client fallback policy.
- [0015](0015-s4-privacy-aware-inference.md): per-domain recipient/transfer-consent authority and sensitive-data-driven default reasoning placement.
- [0024](0024-device-context-collection-and-convergence.md): remote-model consent as an additional recipient decision, and optional/deterministic-only Health processing before reasoning. Cross-device relay/sync and cross-Person sharing still require their own authority and are not implemented by this ADR.
- [0028](0028-pairing-integrated-authority-and-connection-permissions.md): a separate exact-model-recipient authority alongside source permissions.
- [0030](0030-durable-interaction-and-linked-resume.md): model-recipient interactions/lineage, synthetic model-blocked replies and best-effort/manual post-resolution resume. Durable review, no live user wait and fresh linked Runs remain.
- [0033](0033-run-scoped-agent-environment-and-grounded-manager.md): recipient-specific live authority and any immediate typed-grounding escalation based only on Foundation's pre-cutover evaluation.

Current implementation order belongs to the [active architecture plan](../plans/2026-10-02-architecture-refactor.md); the [original convergence plan](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/development/plans/reasoning-source-processing-convergence.md) retains its historical checkpoint evidence. Current runtime documentation must identify unimplemented parts as accepted targets until their corresponding code lands.
