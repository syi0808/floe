# Actions F4 adjudication

Static review date: 2026-10-03. Basis: current source after f94f9735 and 81e47512; the original Claude review examined 6d435397.

## Conclusion

F4's missing final lifecycle check remained confirmed. `ActionsService::drive` checked cancellation before the final asynchronous source, Day event and inherited dependency reads, then could proceed to `prepare_dispatch` without checking owner shutdown again. The correction rechecks the actual shared `closed` flag, the retained job cancellation and its deadline after those awaits, before classifying a source mismatch or admitting durable execution.

F4's permanent Unknown outcome for a provably uninvoked live preparation is stale after 81e47512. The native Rust capability now returns bound `NotApplied(cancelled)` or `NotApplied(timeout)` before native work is scheduled. A failed source recheck uses its precise prewrite reason. Each proof retains the exact execution identity, host epoch and preparation ID; it is not reconstructed after restart.

## Source evidence

- `crates/modules/actions/src/application/execution.rs`, `drive`, lines 49–65: asynchronous source/event/dependency checks are followed by the new final closed/cancellation/deadline check, then pure expectation/expiry checks and `prepare_dispatch`.
- `crates/modules/actions/src/application/owner.rs`, `shutdown` and `shutdown_and_drain`, lines 37–47: the real owner sets the shared closed flag, cancels every registered job and waits for the owned job leases to drain within the shutdown budget.
- `crates/adapters/providers/src/sources/native_calendar.rs`, `NativePreparedCalendarEffect::dispatch`, lines 627–656: pre-invocation cancellation, expiry and failed metadata checks return positive non-invocation proofs. Once native work may have been scheduled, cancellation/timeout retains Unknown semantics.
- `crates/adapters/vault/src/vault/agent_actions.rs`, `actions_prepare_dispatch` and `finish_actions_transaction`: immutable Executing and exact Task/grant/authority validation share the encrypted transaction; Vault access is checked before commit. Matching execution replay never returns a fresh dispatch capability.
- `crates/app/src/ready_generation.rs`, `shutdown`, lines 122–136: close admission and generation retirement precede bounded owner drains; the unconditional retirement seal also covers timeout, cancellation and panic paths.

## Ordering and limits

If closure or cancellation is observed at the new final check, the owner records a pre-dispatch stop and does not request Executing. Cancellation can still race after that in-memory check while encrypted SQL is in progress. If Executing commits, the live capability performs another cancellation/source check and can persist a truthful non-invocation proof. If the process exits before that proof is durably stored, recovery retains Unknown; the system does not invent proof that the provider was never invoked.

The in-memory lifecycle flag, source metadata reservation store, encrypted Vault and native SDK are separate boundaries. These changes do not establish cross-store atomicity or erase the documented final-check-to-SDK race after durable Executing. After native invocation becomes possible, only a causal acknowledgement or the existing narrowly defined Create recovery evidence can settle success; there is no blind retry.

No compiler, formatter, tests or runtime Calendar/credential operations were run. This adjudication establishes source-level ordering only; coordinated G2 verification remains outstanding.
