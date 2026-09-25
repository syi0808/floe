# Authority and recovery invariants

These are durable architectural safety properties, not a progress checklist.

## Authority is explicit and owner-scoped

- A Connection describes source/account/execution-owner lifecycle; it does not itself authorize AI use.
- Access owns grants, exact recipients, purposes, processing restrictions, revocation and admission/release fences, including the contextual recipient-consent store: an approval grants one exact reviewed dispatch, never a standing recipient allow.
- Context may acquire/project only evidence authorized for the current Person, source, scope and freshness requirements.
- Inference may choose an approved model route, but route selection cannot enlarge data authority.
- Saved provider credentials remain private to credential/provider boundaries and are not product-wire or Conversation inputs. The narrow exception is newly issued approved-pairing output for secure persistence, never a subsequent request input; token-bearing Debug/diagnostics are redacted.
- Root, built-in Expert and Schedule composition share one host-scoped current-connection store. Provider adapters bind loaded credentials to the verified person/device; the saved pairing contains no recipient approval. Exact-recipient authority reloads current state at admission, handoff and post-response revalidation. Only a consumed Access fence produces the prepared transport target. External server requests then carry request-scoped `allow_external=true` and the exact `expected_recipient`; local requests carry `allow_external=false` and no external recipient. A prepared transport or availability observation never substitutes for those checks.
- Pairing setup accepts only a bounded loopback endpoint and pairing evidence. Person/device come from AppHost's verified `CallerContext`, never a product route bundle; Connections/key-holder validation binds the exact pending pairing ID, signed challenge and owner issuer. Remote authority and Calendar/View grant services prepare transports from the same current store through provider-owned exact person/device admission. Access still validates producer/source/revision/provider/recipient/grant evidence; model consent and source catalogs are not pairing/grant request fields.
- Revocation prevents later admission or release. It cannot retroactively recall data already transmitted or a provider effect already accepted.
- Source revocation, pause and drift affect Access authority and later dependency admission only. They never mutate Expert Registry state; a Registry revision change is never required to block, and never sufficient to admit, a source read.
- Connection-level **Use with Floe** is a read-only projection plus explicit App intent, never persisted authorization state. Explicit connection completion may review the App-derived first-party Observe bundle; startup inspection may not. Off pauses Observe, disconnect revokes Observe before source deletion, and neither operation changes Act or model-recipient authority.

## Provenance and coverage travel with evidence

Source-backed model/tool inputs retain enough identity to determine:

- Person and source/producer;
- authority/grant dependency;
- observation/projection revision;
- coverage and freshness;
- consumer/purpose/data class;
- exact processing recipient where required.

A broader follow-up request cannot silently reuse evidence whose coverage is too narrow. Unknown or unavailable evidence is not represented as an empty successful observation.

When one logical view reads multiple connected sources, each source retains its own grant, grant authority, consumer-policy authority, source authority and `ContextDependency`. Context may merge bounded payloads, but it cannot manufacture an aggregate grant or dependency or drop a contributing dependency from model coverage.

## Validated pending work is durable work

The Agent Runtime validates a complete model-produced batch before executing side effects. Once a validated batch is durable, crash recovery must continue that batch rather than ask the model for a different plan.

`ValidatedModelBatch.projection_coverage` records the exact source provenance of pending work. Conversation reauthorizes that stored coverage through the current `DependencyResolver` immediately before pending execution and again before terminal output release. Stale or Unknown dependencies suppress the stored step or answer; neither message shape nor Expert/capability identity can reconstruct missing provenance.

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

This invariant is part of the stable architecture: a child may take over only after durably binding the exact validated batch and starting cursor.

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
