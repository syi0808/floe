# S2 static adversarial review adjudication

Review input: public `6d435397f8192e98c6e6fd0a7dad7efc3ab448c4`, identical tree after author rewrite at `2e27b48c80e2a5bdcf23f2b086d9da3ac99b7d17`. The requested local Claude Opus 5.5 High invocation completed with exit 0. This is a source review, not executable validation. Findings below are adjudicated against later implementation and do not inherit the review model's confidence labels automatically.

| Claim | Current disposition | Evidence / closure |
| --- | --- | --- |
| F1: activation interrupts parent before Task receipt recovery | Fixed in source at `982c6f87`, unverified | Task activation runs first. Conversation advances its executor fence and starts the retained paged recovery driver; it authenticates exact Task receipts before completing pending terminal intent. |
| F2: cancellation/deadline loses late Task accounting | Fixed in source at `982c6f87`, unverified | An immutable pending-terminal intent retains the active Session until delegated evidence settles. Receipt attachment, authenticated accounting and finalization share one transaction. Late terminal evidence preserves outcome and updates Session usage only under the exact current revision/no-active-turn CAS. |
| F3: admission can outlive a dropped delegate future without a driver | Fixed in source at `199d2fa2`, unverified | The original admission gap is removed: the owner now registers an exact active reservation and retained owner driver before storage mutation, with authoritative readback for uncertain acknowledgements and no endpoint retry. |
| F4: pre-invocation cancellation becomes permanent Unknown | Fixed in source, unverified | `81e47512` binds known non-invocation cancellation/timeout to positive NotApplied proofs; `742eb1bf` adds the final shared close/cancellation/deadline fence after asynchronous source checks. Actual/native-scheduled uncertainty remains Unknown. See `s2-actions-f4-adjudication.md` for race limits. |
| H1: source review blocker may escape called Tool requirement | Refuted for the production path | `ContextExpertSources::read` authenticates manifest/revision, matches tool ID to the selected declared requirement, checks capability/version/cardinality and selected refs, then validates each blocker’s exact consumer, Assistant purpose, capability-derived source and selected logical resource. Access review preparation requires that one matching resource. Package/model JSON does not supply blocker authority. |
| H2: Day positional zip can validate different source lists | Refuted | `repositories/day_refresh.rs` compares `current_versions == acquisition.inventory`; `CalendarAcquisition::validate_identity` requires equal lengths, unique ascending connection IDs and exact full source-version equality per outcome. `RefreshCommit::validate` revalidates and recomputes the mirror before the first write. Reordered, short or foreign lists fail closed. |
| H3: cancellation can discard acknowledged endpoint Output | Fixed in source at `199d2fa2`, unverified | A validated Completed/Blocked endpoint acknowledgement is now preserved through fenced terminal settlement; an ambiguous Output-append acknowledgement is a distinct boundary and is not claimed as recovered private settlement. |

An independent native audit also confirmed that the global 64-row pending-resume/resolving-interaction scans can reject valid cross-Session/device backlogs before actor filtering. Source checkpoint `982c6f87` replaces those scans with actor-scoped bounded pages, exact-origin lookup and a retained owner recovery driver. No silent cross-Session cancellation or query-triggered recovery is allowed.

G2 remains unopened. No formatter, compiler, build, analyzer or test ran for this review or its current fixes.


## Closure qualification and S3 evidence

All four findings and the completion-order hypothesis have source corrections; both source-boundary/order hypotheses were refuted. These are static dispositions. G2 production builds and S3 behavior tests remain required. The ambiguous Engine Output-append acknowledgement is not recast as acknowledged private-state settlement, and no endpoint/provider work is retried to manufacture a receipt.

Required reconstructed cases include cancellation after Task admission or Working CAS but before an observer acknowledgement, exact duplicate joins, uncertain storage acknowledgements, late valid endpoint completion, parent terminal intent with live Tasks, restart with terminal Task receipts, multiple late receipts, older-Session accounting, same-generation orphan proof, and more than 64 recoverable rows across Sessions/devices. Each must establish at-most-once dispatch, immutable terminal meaning, conservative accounting and bounded recovery without query-triggered work.
