# Authority and recovery invariants

These invariants survive refactoring. They are architectural safety properties, not a progress checklist.

## Authority is explicit and owner-scoped

- A Connection describes source/account/execution-owner lifecycle; it does not itself authorize AI use.
- Access owns grants, exact recipients, purposes, processing restrictions, revocation and admission/release fences.
- Context may acquire/project only evidence authorized for the current Person, source, scope and freshness requirements.
- Inference may choose an approved model route, but route selection cannot enlarge data authority.
- Saved provider credentials remain private to credential/provider boundaries and are not product-wire or Conversation inputs. The narrow exception is newly issued approved-pairing output for secure persistence, never a subsequent request input; token-bearing Debug/diagnostics are redacted.
- Root, built-in Expert and Schedule composition share one host-scoped current-connection store. Provider adapters bind loaded credentials to the verified person/device; exact-recipient authority reloads current state at admission, handoff and post-response revalidation. A prepared transport or availability observation never substitutes for those checks.
- Pairing setup accepts only a bounded loopback endpoint and pairing evidence. Person/device come from AppHost's verified `CallerContext`, never a product route bundle; Connections/key-holder validation binds the exact pending pairing ID, signed challenge and owner issuer. Remote authority and Calendar/View grant services prepare transports from the same current store through provider-owned exact person/device admission. Access still validates producer/source/revision/provider/recipient/grant evidence; model consent and source catalogs are not pairing/grant request fields.
- Revocation prevents later admission or release. It cannot retroactively recall data already transmitted or a provider effect already accepted.

## Provenance and coverage travel with evidence

Source-backed model/tool inputs retain enough identity to determine:

- Person and source/producer;
- authority/grant dependency;
- observation/projection revision;
- coverage and freshness;
- consumer/purpose/data class;
- exact processing recipient where required.

A broader follow-up request cannot silently reuse evidence whose coverage is too narrow. Unknown or unavailable evidence is not represented as an empty successful observation.

## Validated pending work is durable work

The Agent Runtime validates a complete model-produced batch before executing side effects. Once a validated batch is durable, crash recovery must continue that batch rather than ask the model for a different plan.

Stable identities bind:

- execution;
- validated batch;
- step ordinal;
- Tool intent/result;
- Delegation Task intent/result;
- preamble where journal ordering requires it.

Journal corruption or mismatched durable identity fails closed as storage/recovery failure rather than being corrected by a model.

### Cross-run continuation

A continuation child does not supersede a parent's pending batch merely by existing. The child must durably claim the **exact same validated batch and starting cursor** before takeover.

This invariant was proven by the completed/frozen
[2-B.1 Cross-run Resume Lineage](../refactoring/stage-2/2-b1.md) work and remains here as stable architecture rather than being copied into another status document.

## Model attempts and budgets

- One attempt identity is preserved through Engine, Inference and transport.
- Inference is the model usage reserve/handoff/settlement owner on the canonical path.
- Preflight failure before dispatch must not be charged as dispatched work.
- A dispatched failure's durable usage survives restart.
- Recovery must not resurrect budget, double-settle an attempt or double-charge finalization.

## External writes and uncertain outcomes

Consequential external effects require:

1. validated proposal and approval;
2. exact target authority and provider preconditions;
3. durable pre-dispatch execution intent;
4. idempotency/stable execution identity;
5. reconciliation when the provider may have succeeded but the response is lost.

An unknown outcome is never retried as a blind create/write. Recovery performs a bounded lookup/reconciliation path and preserves uncertainty when identity cannot be proven.

## Cancellation

Cancellation flows from the owning Run/Task/execution scope. Query, preview, observer timeout, route refresh or screen disposal are not implicit cancellation of durable work.

Do not hold a global Vault transaction across model or provider I/O.

## Related decisions

- [ADR 0016 — Native Agent model protocol](../decisions/0016-native-agent-model-protocol.md)
- [ADR 0018 — Manager–Expert A2A delegation](../decisions/0018-manager-expert-a2a-delegation.md)
- [ADR 0024 — Device context collection and convergence](../decisions/0024-device-context-collection-and-convergence.md)
- [ADR 0025 — Person-owned connections](../decisions/0025-person-owned-connections.md)
- [ADR 0027 — Connection authority and observation](../decisions/0027-connection-authority-and-observation.md) (proposed; use current Access/Context ownership above for implemented architecture)
