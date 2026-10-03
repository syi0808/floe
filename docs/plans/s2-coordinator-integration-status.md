# S2 coordinator integration checkpoint

S2 is active and intentionally incomplete. G1 completed on the earlier first-vertical snapshot; no S2 formatter, compiler, build, checker, or tests have run. G2 remains after full structural closure.

Completed source transformations in this checkpoint:

- Conversation uses per-interaction origin/target links rather than parallel source-only arrays.
- Task origins carry exact immutable Task execution receipt references. Closed audit evidence distinguishes root projection, Task projection, source read, binding and navigation. Source review links retain their actual requirement; complete blocker groups remain in the audit for storage authentication.
- Domain validation checks publication/link identity, exact owner-review references, navigation meaning, duplicate review IDs and resolution-owner agreement.
- Conversation delegates journal ordering/accounting to the shared runtime and reexports canonical journal/unknown-attempt values. Continuation accounting aggregates distinct Task receipts rather than charging adopted execution twice.
- Fresh/finalization execution IDs are admitted explicitly; final payload validation preserves the exact owner-validated payload.
- Removed the public arbitrary Task interaction publisher and artifact-JSON-to-interaction authority path.

Still being integrated in the same S2 phase:

- Binding storage returned; native review is in progress. Task and source audit persistence now authenticate same-transaction evidence.
- Binding completion reconciliation now uses an explicit refresh cause and actual consumed-review receipt, never a fabricated approve decision. Safe DTO/client callers are still being cut over.
- App typed owner construction/lifecycle, remaining worker-bus removal, FFI/CLI/client cutover.
- Day, Actions, Experts and Knowledge owner outputs and storage/transport patches from their disjoint implementation scopes.

This checkpoint preserves work; it does not claim compilation, feature closure or behavioral validation. Old App callsites and some storage/DTO references are intentionally awaiting the same-phase owner cutover, not retained compatibility paths.

## Later S2 integration checkpoint

Task storage now reserves receipt capacity and authenticates journal heads. Narrow terminal Run reconciliation settles a previously admitted Task result before rebuilding continuation accounting; bounded continuation ancestry and immutable replay evidence authenticate adoption. A recovered Blocked Task remains out of model history and is re-published by the child Run before dispatch. Historical timeout state remains unchanged.

The Go source checkpoint, all builtin Expert Programs and the single encrypted Actions owner/native adapter are saved. App Actions, Experts and Knowledge forwarders are changing to typed owner calls with exact failures. Host construction now admits one Day lifetime before exposing AppHost. The old Vault/domain worker bus is being replaced with lifecycle-only scheduling. Context source policy extraction and Knowledge storage remain in progress. No S2 executable validation has run.

S3 diagnostic reconstruction item: local_model_smoke manager_guidance previously injected fabricated terminal Task receipts into five follow-up scenarios. S2 removes that misleading injection while preserving real first-response cases. Reconstruct these five follow-up cases through an actual isolated package/Task/common-Engine execution fixture in S3; do not treat their former shape assertions as verified behavior or silently claim coverage.

Task proposal display closure: durable Conversation Delegation messages now carry only the immutable Task execution receipt reference alongside the acknowledged Task snapshot. Safe TaskSummary exposes execution_receipt; unadmitted rejection has None. Artifact summaries expose ID/name/media types, never payload authority. Actions reloads and validates the actual Task receipt/artifact before accepting the safe ExpertProposal intent. The obsolete write-only agent_task_delegations table was removed from the new profile schema; continuation replay no longer collides on a redundant Task-ID backreference.

Additional S3 regression coverage must include zero-charge unknown token/cost accounting, same immutable Task adopted across multiple timed-out continuations, recovered blocked Task review publication, binding refresh reconciliation from actual consumed-review receipt, stale generation Drop after new unlock, panic/cancel during activation and shutdown, failed-resource latest-window cache preservation, uncovered-date coverage, lost acknowledgement replay after cache refresh, and command identity retention across observer release. These are source-derived targets, not executed validation.
