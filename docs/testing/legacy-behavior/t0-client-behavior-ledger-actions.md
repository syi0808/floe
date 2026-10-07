> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: Actions, review and external-write recovery

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Full-source static extraction; no execution or removal. D = durable safety/property; P = product hypothesis; O = obsolete representation; H = harness. Mixed classification preserves safety meaning without freezing old shape. Entry headings provide source registration/span; the file hash binds all prose to exact baseline bytes.

## apps/client/test/features/actions/agent_proposal_card_test.dart

Full source read: lines1–254; SHA-256 `aa421011cce3148340987271547f08cf405eba0be57941cc0f5141fd95590a68`.

Current owner: AgentProposalCard / PersonalDayScreen existing-Action navigation. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'proposal card is explicit, read-only and usable at width $width' (testWidgets; lines57–108; D/P)

Two loop registrations at520px normal and320px200% text: opening View source shows suggestion but does not inspect; explicit Check saved action reads durable status without model work; only explicit Open Calendar action invokes the recorded ID callback. Change result to explicit absence and show Nothing was created/remove navigation; change to storage failure and show uncertainty, never absence. Closing storage removes the card. Exact labels/layout are hypotheses; read/decision separation and uncertainty honesty are durable.

### 'recorded $status is displayed without approval or retry controls' (testWidgets; lines120–143; D/P)

Seven registered statuses pending,approved,rejected,executing,blocked,unknown,succeeded each render recorded status after one inspection but expose no Approve,Retry or Open Calendar action when no callback is supplied. No model begins. A status card cannot invent execution controls from text or status.

### 'Today opens the existing S3 action without deciding at width $width' (testWidgets; lines147–195; D/P)

At1280 and390px, navigate Today→assistant→source→saved action→existing review dialog. Dialog is bound to proposalCall and shows Focus time, Day actions have reloaded at least twice, one inspection occurred and zero decisions occurred. Opening an existing Action is not approval. ProposalDayGateway throws on any attempt to decide and on unexpected mutations; it relies on product FakeDayGateway for read-only Day content.

## apps/client/test/features/actions/agent_proposal_test.dart

Full source read: lines1–248; SHA-256 `a287f3e87b74d0730dafabdc3df4723f7aeaadbad32d890f5a38039c2874d202`.

Current owner: NativeProposalGateway / AgentController / Actions-owned proposal inspection. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'only an Actions artifact marks a delegation as reviewable' (test; lines18–45; D/O)

Parse a delegation containing an Actions Calendar-proposal artifact and mark it reviewable. Replace artifacts with an Expert Schedule report whose data says execute:true; it must remain inert/nonreviewable. Model/package text cannot mint Action authority; exact media version is representational.

### 'native inspection distinguishes explicit absence, malformed response and failures' (test; lines47–152; D/O)

Inspect through query-only actions.proposal.inspect/read_result. Eight positive result branches distinguish explicit action absence(null) from pending/approved/rejected/executing/blocked/unknown/succeeded. Seven corrupted branches reject foreign person, session, invocation, schema2, malformed session ID, mismatching action ID and unknown executed status. Missing action field and absent proposal envelope also fail; explicit not_found failure becomes typed AgentVaultException rather than absence. No branch submits/executes an Action.

### 'controller reads only durable scoped proposals and serializes inspection with chat' (test; lines154–203; D)

Loaded durable delegation can be inspected, but a forged call-ID message produces no request. Hold the valid inspection: controller is busy, sending/duplicate inspection/new-session transition do not overlap and exactly one scoped person/session/invocation request occurs. Completing it caches execution ID without model begin or message replacement. Later explicit absence remains an action-null snapshot; storage failure clears stale proposal but retains session and exposes failure; foreign-session response is rejected; switching sessions clears a valid cached proposal.

### 'lock hides proposals immediately, drains owned inspection and ignores late results' (test; lines205–229; D/O)

Load/cache a personal proposal, then hold a second inspection and closeView. Messages/proposal disappear immediately, inspection disables and Vault lock waits until owned read drains. Release the read: exactly one lock, locked state, no late proposal reappearance. Preserve sealing/draining; do not derive backend cancellation permission from view closure.

### 'key loss removes the conversation and all proposal metadata' (test; lines231–247; D)

After a proposal is visible, inspection fails vault_unavailable. Clear conversation messages, proposal and proposal-failure metadata and mark storage unavailable. Key loss cannot leave derived plaintext visible or trigger replacement storage.

## apps/client/test/features/actions/calendar_action_execution_test.dart

Full source read: lines1–362; SHA-256 `196933ea6dde056c7951551abb56f8fcf298159eddf9372255109905889d133c`.

Current owner: CalendarActionController presentation/command sequencing / Actions execution owner. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'unresolved writes block their own target, not every calendar drag' (test; lines101–121; D)

Load an unknown direct mutation targeting blocked: canDirectFor(blocked) is false but another event remains allowed. Replace it with unknown untargeted direct create: generic canDirect is false while another-event remains allowed. Uncertainty blocks the affected operation/target without globally freezing unrelated edits.

### 'manual actions stay out of Review after reload regardless of automation policy' (test; lines122–150; D/P)

For every ActionAuthorityMode value(ask,allow,deny), explicitly submit a manual direct event, then reload. It is never a Review item, no proposal/decision occurs, exactly one execution occurs and persisted action.direct remains true. User-direct intent and automated authority are distinct; all three enum branches must be reassessed against target Action semantics rather than copied blindly.

### 'uncertain manual work remains Activity-only and prevents another write' (test; lines152–177; D)

Make direct execution outcome unknown. After explicit direct submit and reload, action is Activity-only, unknown and generic direct creation disabled; only one create happened. Ambiguous external write cannot become a replacement proposal or retry.

### 'calendar create authority allows automatic execution or denies intent' (test; lines179–211; D/P)

With automated Calendar create allow policy, proposing a focus event explicitly creates a proposal, records one approval and executes once to succeeded. Change authority to deny and canPropose becomes false. Preserve owner policy distinction; client-driven autoexecution composition must move to canonical owner, not be retained as duplicate authority.

### 'explicit approval executes once; failed read retries only collection' (test; lines213–243; D)

Explicitly approve a pending action and execute once. First post-write collection read fails; action remains succeeded, collection=failed and no global reload requirement. retryRead reruns only collection, making it collected with two reads but still one create. A read failure cannot repeat a write.

### 'relaunch and lookup never create; unresolved actions prevent replacement proposals' (test; lines245–273; D)

Approve/execute and obtain unknown, dispose/recreate controller and load. New proposals are blocked; ordinary run must not create again. Explicit recover uses one lookup, leaves total creates1 and reaches succeeded. Relaunch and lookup preserve external-write uncertainty without replay.

### 'old approval and disabled builds never auto-execute' (test; lines275–295; D)

Loading an old approved record does not execute. Then disable writes, load pending and explicitly approve: approval persists but execution count remains0. Approval/restart/build availability cannot cause surprise writes.

### 'segmented event editor validates and saves without creating a review' (testWidgets; lines297–361; D/P)

At390px render segmented event composer with one title input/two time pickers, no timezone text. Empty Create event causes no proposal; enter title and create: exactly one direct submission/execution, zero proposals/decisions, local non-UTC start and UTC±HH:MM timezone, no Review dialog/Approve & create control and no exception. Explicit manual editing should not create a second approval workflow; exact fields/format are presentation hypotheses.

## apps/client/test/features/actions/calendar_action_gateway_test.dart

Full source read: lines1–90; SHA-256 `f755dd7ed157c2355f5b281ec2ae5538571da0cdc2081153118c629397d5eb1d`.

Current owner: NativeCalendarActionGateway / Actions authority owner jobs. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'action policy mutation uses the authoritative vault job' (test; lines9–51; D)

Set Calendar create authority to deny via actions.calendar owner command without caller person_id. First reply is unfinished; subsequent read_result completes authoritative deny state and releases it (one mutation plus two result reads). Policy changes travel through the owner job, not client state alone.

### 'vault action failure is released and retains recovery metadata' (test; lines53–88; D)

Reading Action authority returns a completed vault_unavailable/reopen_vault failure. Raise typed recovery metadata and release the result via actions.read_result; do not silently treat locked authority as default permission.

## apps/client/test/features/actions/calendar_action_ui_test.dart

Full source read: lines1–650; SHA-256 `c747b226240aa8150140d4304c46279e85cce3826c1a572f8141b5e72d80d5c1`.

Current owner: CalendarAction decoder / CalendarActionController / ReviewRequestPanel and ActivityPanel. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'Agent action attribution is optional, scoped and versioned' (test; lines139–155; D/O)

Agent attribution is optional; valid schema1 origin retains floe.schedule and saved session without changing pending status. Three corruptions throw: schema2, invocation ID unequal to Action ID, and highly_sensitive class. Preserve scoped provenance; current schema/data-class set is legacy and cannot weaken target HighlySensitive handling.

### 'approval guards stale, expired, disconnected and foreign proposals' (test; lines156–221; D/P)

A current pending local proposal can approve only with serving error-free matching connection revision and current time in bounds. Reject missing connection, not-serving connection, revision4 for ordinary proposal, denied connection, exact expiration boundary and time before creation. Agent-origin proposal with revision4 is allowed by this legacy client branch and requires owner-level target reassessment. Loading a foreign-person action marks failed/needsReload rather than adopting it.

### 'duplicate decisions are suppressed and response loss requires lookup' (test; lines223–260; D)

Hold first approval; a second approval is suppressed(count1). Lose response after fake save: mark needsReload and suppress even an attempted rejection. Read-only reload finds approved state and clears reload, with no second decision. UI ambiguity cannot authorize competing decisions.

### 'a decision can finish after its owner is disposed' (test; lines262–278; D)

Begin a rejection, dispose its controller, then complete the owner result. The pending future settles without exception. View disposal must not destroy the in-flight owner decision.

### 'review, close and approve without creating at $width' (testWidgets; lines320–385; D/P)

Two registrations at390/1200px open review details without deciding. Show destination/When; hide technical IDs/timezone by default, and for same-civil-day start/end show localized combined range (conditional branch only). Expand Technical details to reveal starts/IDs while still hiding UTC/Asia-Seoul strings. Close leaves decisions0; reopen and Save approval only records one approval, removes Review control and explicitly says no event created. Exact strings/geometry are hypotheses; approval is not claimed execution.

### 'Expert action attribution stays readable at 320 pixels and large text' (testWidgets; lines388–426; P/D)

At320px with200% text, attributed future Action shows Suggested by Floe but hides saved session. Technical details reveal Schedule planning, saved conversation and Expert call ID without overflow. Escape exits with zero decisions. Attribution rendering must not become authorization.

### 'activity reload is positioned in the page header' (testWidgets; lines428–457; P)

Render succeeded Activity and compare geometry: Reload activity aligns vertically within8px of title, lies right of item center and above the item. This is layout evidence only.

### 'overnight review shows both local dates and a plain-language block reason' (testWidgets; lines459–505; P/D)

A blocked23:45–00:30 action spans two local dates. Detail view shows both localized full dates/times and plain schedule-conflict explanation saying nothing created, not raw code or approval button. Honest blocked outcome is durable; exact text is product choice.

### 'expired review explains why approval is unavailable' (testWidgets; lines507–531; D/P)

A proposal expired one second ago renders expired/nothing-created explanation, still permits technical-detail inspection, hides raw execution ID initially and makes zero decisions. Expiration must not silently create permission.

### 'closing during a decision preserves its single in-flight request' (testWidgets; lines533–563; D/P)

Begin Decline then close/reopen dialog while owner result is held. Decision count stays1. Completing rejected result displays declined/no-event-created and removes approval controls. Dialog lifecycle cannot duplicate the decision.

### 'failed decision keeps review visible and disables decisions until reload' (testWidgets; lines565–600; D/P)

Approval is saved by fake owner but its response is lost. Keep review visible, mark needsReload and disable decision button. Clear fake transport failure and choose Reload reviews: recover approved state with decisions still1. No blind retry is offered for ambiguous mutation.

### 'Escape closes without a decision; terminal states never offer create' (testWidgets; lines602–649; D/P)

Escape from pending review closes without decision. For each terminal rejected,blocked,succeeded record, Review control stays absent and read-only Activity details have no Save approval only/Decline/visible execution-ID until expansion. Across all branches decisions remain0. Terminal display is not execution permission.

## apps/client/test/features/actions/calendar_direct_interaction_test.dart

Full source read: lines1–200; SHA-256 `8b8613d3f140f926bd657ced1709707867461384678d4ad5a367dd1b5a3642f0`.

Current owner: CalendarDateTimeField / CalendarAgenda user-intent callbacks. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'time field uses the wheel picker and commits on Done' (testWidgets; lines22–55; P)

Open iOS-style time wheel for September8 2026 09:30. Changing wheel to14:45 does not commit immediately; Done applies14:45 while preserving original date, form validates and no exception occurs. This is draft editing, not an external write.

### 'drag snaps, cancels with Escape/outside, and menu edits/deletes' (testWidgets; lines57–199; D/P)

For a modifiable09:00–10:00 event in900×760 viewport, mouse drag37px shows preview and snaps requested move to09:30; holding a mouse does not open touch context menu. Escape during a second drag and releasing outside during a third produce no extra move. Secondary click/Edit and explicit Event actions/Delete each invoke their distinct callback once. Drag near lower edge scrolls but Escape prevents commit. Shift-F10 opens keyboard context menu; Escape closes. Re-render with modification denied: Edit entry is disabled and no stock PopupMenuItem is used; touch long-press still opens the menu. Preserve explicit-action/cancel/disabled guards; snap amount, widget shape and geometry are product hypotheses. Callbacks only are exercised, no provider mutation.

## Dependency and target disposition

Current owners under test: proposal inspection translates Actions-owned durable records; CalendarActionController maintains client presentation and command sequencing; NativeCalendarActionGateway translates owner jobs; CalendarAgenda/date-time fields only emit user callbacks. Target durable properties must be re-proven through the canonical encrypted Actions repository/owner, preserving locked-state unavailability and external-write uncertainty. No client fake/controller becomes a competing permission owner.

`calendar_action_ui_test.dart` defines shared Gateway/action/connection fixtures. Its Gateway saves an approval/rejection before a possible lost reply and supports a held result. `calendar_action_execution_test.dart` imports those symbols and defines Executor, a fake with direct-submission/execution/lookup/proposal counters, policy state and no real provider I/O. Executor is also imported by conversation/agent_panel_test.dart and settings/settings_screen_test.dart. Delete this fixture chain only as a covered aggregate batch. Widget host/tap/mount/load helpers, font loaders and fixture origin maps are H; production fonts/theme/localization/FakeDayGateway remain product assets/code. Test timing uses fake frames/minimum loading duration except DateTime.now-based proposal fixtures; the conditional same-day assertion is host/time dependent and cannot be claimed universally executed.
