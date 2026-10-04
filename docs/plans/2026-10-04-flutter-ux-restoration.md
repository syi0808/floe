# Restore the pre-refactor Flutter experience

## Scope and decisions

Requested on 2026-10-04 after the UI audit. Implementation is performed directly by the coordinating engineer; independent Opus adversarial review may be used. This plan is the execution authority for this restoration, not a new architecture specification.

- Visual and interaction baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`, immediately before the large architecture cutover. Implementation starts at `d2440730254309ca7f5fdb01f3328d30df673476`.
- Restore existing layouts, service identity, shared Floe controls, navigation, normal save/move flows and useful automatic refresh. Keep current semantic owners and verified contracts.
- Keep the previously accepted removal of the Read-only badge and original-calendar/source diagnostics from normal event details.
- Pairing retains administrator approval and code comparison in the dashboard. Remove the extra client `Codes match` action. The protocol's client key-possession proof remains mandatory and becomes an owner-driven step of the original explicit Start intent.
- No per-model invocation approval. Source permission reviews retain accurate current/requested data and processing scope.
- Preserve logs, paired state, private keys, unresolved commands and external-write evidence. No automatic reset, migration chain, legacy implementation, credential fallback or user-data deletion.
- Existing architecture refactor S3 permanent-suite reconstruction remains separate. Use bounded disposable behavioral qualification where needed; run gates at completed slices, not after every edit.

## Baseline evidence and limits

The audit compared all 30 changed presentation-path files and related application/owner call paths: 107 changed `apps/client/lib` files in the rename-aware diff. Shared design tokens, theme and core controls are unchanged. This is a source comparison; old and new full-app visual equivalence has not yet been demonstrated.

The desired result is not a wholesale checkout of old Flutter code. Several old screens performed policy, direct transport or lifecycle work that now belongs to Rust owners. Reuse their presentation and interaction structure, and bind that presentation to current typed gateways.

## Final ownership

- Connections: pairing progression, integration setup, exact source configuration, review intent and retained command outcomes.
- Access: actual source permission and immutable disclosed processing reviews.
- Day: calendar read/refresh admission, cache coverage and current query results.
- Actions: manual/Expert writes, durable dispatch, uncertainty and recovery.
- Experts: directory descriptions, assignment candidates and immutable binding reviews.
- Conversation: bounded historical message projection and session identity.
- Flutter: local presentation choices, input drafts, navigation and observing admitted work. Screen disposal never cancels owner work.
- Go Pairing/Trust: cryptographic client proof verification, administrator approval, immutable deadline and atomic enrollment.

## Ordered restoration

### R1 — Pairing ceremony and Remote server settings

Read before modification:
`crates/modules/connections/src/application/gateway_pairing.rs`, `ports/gateway_pairing.rs`, `application/product.rs`; `crates/adapters/vault/src/vault/gateway_pairing_store.rs`, `gateway_authority.rs`; `crates/adapters/providers/src/gateway/pairing.rs`; protocol/FFI Connections commands and snapshots; the Flutter Connections gateway, controller and Gateway panel; Go Pairing/Trust and dashboard handlers.

1. Distinguish machine key-possession confirmation from human comparison. The original Start command authorizes the former after the exact signed challenge is validated and durably accepted. The server administrator remains the human approval boundary.
2. Move progression through the existing durable confirmation transition into the Connections reconciliation driver. Derive a stable internal confirmation identity from the admitted operation and exact revision; preserve negative receipts and CAS. Never synthesize a user click in Flutter, and never confirm from an unrelated query.
3. Remove the product `connections.pairing.confirm` route, FFI dispatch, Dart gateway method, controller action and `Codes match` button. Keep `/pair/confirm` as the real cryptographic transport boundary. Its existence does not imply a human has compared codes.
4. Keep Cancel/Forget precedence at the repository, signer and activation transaction. A delayed challenge or confirmation response must not reactivate cancelled/forgotten authority.
5. Project the internal proof phase as connection preparation; normal UI shows the same code and waits for dashboard approval without another client action. Dashboard pending copy distinguishes proof preparation from approval readiness and never claims a human confirmed the code.
6. Keep the original five-minute signed deadline, same-operation replay, wrong-producer rejection and enrollment signature checks. Preserve current paired records without resetting data.
7. Restore the old Remote server card placement and saved-address display. Expose only the verified, non-secret configured address in the safe Gateway projection; do not invent a default for an already paired connection.

Verification: exact Start replay, approval before proof rejected, valid proof plus administrator approval succeeds, wrong issuer/producer fails, cancellation/Forget races, response loss and restart recovery, deadline unchanged on retries, no obsolete public Confirm route. Mac acceptance requires a separately authorized fresh pairing only if needed; do not remove the user's established pairing as setup.

### R2 — Connections list and service details

Files: `connector_screen.dart`, `integration_detail_panel.dart`, `connections_controller.dart`, Connections domain/application/infrastructure; baseline `calendar_panel.dart`, `personal_connection_cards.dart`, `server_connector_panel.dart` for presentation reference only.

1. Show the Remote server card in Settings only, not above the main Connections service grid.
2. Restore page-title typography, connected count, service-specific icon/copy, available/unavailable sections and meaningful loading/empty/error states. Do not infer availability from platform alone.
3. Restore service-specific detail layout and shared Floe selection controls. Preserve the current source/integration references; never derive authority from a display name.
4. A Connect action prepares its immutable setup review and proceeds through the reviewed setup flow. Do not expose `Review setup` plus `Continue setup` merely because there are two API calls. Retain exact command identity if preparation or start is uncertain.
5. Resource selection opens a dialog containing a prepared review and current selection. Save uses its exact revision and selected references; cancel closes the dialog without changing authority.
6. `Use with Floe` opens one concise permission decision with actual reviewed data and processing scope. Off pauses the existing grant. Keep explicit DeviceOnly choice, derived Health disclosure and source review on genuine expansion. Never use a switch to grant silently.
7. Restore disconnect confirmation and supported provider authorization presentation. Gateway-managed secrets remain on the Gateway; do not reintroduce Flutter-held provider credentials.
8. Scope controller review results and input drafts to their source/integration. Switching details while an asynchronous prepare completes cannot open or apply the old target's review.
9. Replace the unused `openDeviceCalendarDetail` flag with a typed source navigation target from Conversation to Connections. A missing target gets an honest unavailable state, not an unrelated source.

Decisions requiring further owner investigation: `All calendars including future calendars` is not equivalent to selecting today's inventory. Restore it only with an explicit persisted selection policy and matching authority semantics; do not silently widen grants. The old global Calendar preference must have a single defined role alongside multi-source Day and per-Expert bindings. If those decisions materially alter access, present the narrow recommendation before implementing that part.

Verification: responsive grid/detail layout; native Calendar/Contacts and hosted-service variants; denied permission; review expiry; dismiss/reopen; source switch during prepare; repeated Connect/Save; uncertain command replay; no duplicate Gateway card or raw enum labels.

### R3 — Calendar/Day normal interactions

Files: `calendar_context_rail.dart`, `calendar_agenda.dart`, `calendar_event_details.dart`, `personal_day_screen.dart`, its `personal_day/*` parts, `personal_day_controller.dart`, `calendar_action_proposal.dart`, Actions controller/gateway.

1. Replace the permanent large coverage diagnostic card with a compact truthful sync state and optional details. Preserve distinctions between complete-empty, unobserved, partial and stale data.
2. Restore drag-to-move as a direct manual Actions request using the exact event target, revision and duration. It must not open the editor solely to implement movement. Uncertain results stay recoverable; no second write on retry.
3. Close the event composer on confirmed success and show the existing success feedback. Keep pending/unknown/error states available rather than claiming success. A successful effect with failed display refresh must not be submitted again.
4. Restore automatic refresh triggers for first visible Day, date change, app resume and relevant connection/resource changes. Keep a bounded freshness interval equivalent to the old behavior. Coalesce refreshes, retain the original unresolved command, and suppress background timer work while the app is inactive. Invoke the Day refresh command; keep `loadDay` a pure query.
5. Preserve improved end-offset/DST handling, exact query-generation checks and source coverage fences. A result for an old date must never replace the current date.
6. Keep accepted simplified event details and ordinary shell, tasks and notes presentation unchanged.

Verification: drag success/failure/unknown, composer success and lost acknowledgement, date switch mid-refresh, repeated resume, permission revoke, partial source failure, empty successful calendar, no timer after disposal and no implicit cancellation.

### R4 — Experts, proposals and conversation

Files: `agent_registry_dialog.dart`, `agent_registry_controller.dart`, Experts directory domain/owner/FFI/DTO; `agent_proposal_card.dart`, `calendar_action_panel.dart`; Conversation session projection/service/FFI/DTO, `conversation_controller.dart`, `agent_panel.dart`, interaction cards.

1. Prepare the exact binding review when the user expands the relevant binding editor. Restore direct candidate editing without a second `Review sources` click. Keep revision, expiry, unavailable candidate and exact-replay handling.
2. Project the existing registry definition description through the safe directory contract and restore the useful descriptive text; version is secondary detail.
3. Restore compact proposal/action cards with progressive recovery details. Keep provenance and explicit effect intent. Resolve destination/timezone from validated proposal/source context when unambiguous; ask only when a real choice remains. Never invent a timezone or execute an artifact merely because it is shown.
4. Preserve chat bubbles, Markdown, composer and source-permission semantics. Repair source-review deep links in R2.
5. Restore access to retained older messages with a bounded Conversation-owned page query and stable message cursor. Authorize the same session/Person, project only safe existing message fields, deduplicate by identity and prevent an old page from replacing newer live state. Do not remove the payload bound or expose stored authority to recover history.
6. Memory and settings keep existing accepted controls; only restore presentation drift identified in the audit. Removed unused model files are not screens to resurrect.

Verification: expand/close/reopen binding editor, stale review, installation change while loading, duplicate save, descriptions, proposal success/unknown/denied, ambiguous destination, older-page boundaries, message append during pagination, foreign session rejection and viewport preservation.

## Deletion and residual gate

Search removed public confirmation routes, `Codes match` instructions, unused navigation flags, generic raw-enum UI, old direct-HTTP Flutter paths and obsolete duplicated detail components. Historical evidence may mention old behavior but current docs must describe the final path. Keep one runtime implementation for each owner concept.

## Qualification and publication

- Complete and review one coherent slice before formatting/compilation. No repeated broad intermediate builds.
- Run affected semantic probes with isolated synthetic data; preserve stronger repository final gates where applicable. Permanent S3 reconstruction remains deferred.
- Rust/FFI changes: final workspace gate, architecture boundary checker, FFI build and actual DTO/Dart boundary qualification. Go changes: race suite and vet. Flutter: analyze, applicable behavioral/widget checks and final macOS build from the same source snapshot.
- Inspect desktop and narrow-width screenshots for all changed screens; compare against baseline presentation using synthetic fixtures where possible. Actual provider/permission behavior not exercised remains explicitly unverified.
- Use Opus for adversarial review of pairing authority and other security-sensitive contract changes; root adjudicates and implements findings.
- Commit/push coherent completed work to authorized main with the user's Git identity, verify remote SHA, and report exact passed and unverified stages. Do not claim restoration complete until normal and interrupted flows are covered.

## Execution state

- Audit delivered; full restoration authorized.
- R1 initial snapshot `cdb202c7` passed the Rust workspace gate, FFI build, boundary checker and macOS Debug arm64/strict-codesign build. Five isolated owner probes passed. Flutter analysis contained informational baseline lint diagnostics only, with no errors or warnings.
- Independent Opus review of `cdb202c7` found no broken cryptographic/admin/CAS/cancellation invariant, but identified a new internal-receipt storage failure that could stop the pairing driver. The correction maps only that uncertain receipt storage outcome to the driver's existing retryable Indeterminate class. Before-commit rollback and after-commit response-loss probes now pass (six owner probes total). Authority failures retain their classification.
- The app's waiting label now directs comparison and approval in the dashboard, without claiming the server has accepted proof. Forget can still leave a remotely approvable request until expiry; local authority remains fenced and remote-revocation guidance remains visible. Cancel is the normal pending abort.
- R2 initial presentation cutover restores list/count, removes the duplicate Remote card, combines Connect preparation/start, moves resource/permission decisions into shared dialogs and carries exact source references through conversation navigation. Seven initial Flutter behavior/widget probes passed. A failed preparation cannot reuse an older review; success closes a dialog only from its own acknowledged command.
- R2 owner projection gaps (saved address, service identity/detail metadata and future-resource selection policy), further visual comparisons, R3 and R4 remain open. Full restoration is not complete.
- R2–R4 detailed call-site investigation remains part of their pre-edit gates; this plan does not claim every remaining contract decision is resolved.


### Subsequent implementation checkpoint

- `ac686050` is published to main after macOS Debug arm64 and strict codesign passed; Opus closed the scoped pairing receipt/copy findings. This is the initial Connections restoration, not full UI acceptance.
- R3 restores direct drag submission and closes the composer only on its own confirmed success. Current Day is read separately; uncertain results keep their original Actions identity. Five isolated freshness-controller probes and two composer outcome probes passed.
- Automatic Day freshness is presentation-driven on visible/resumed Day, with a three-minute interval and source/date invalidation. Hidden invalidation is retained, concurrent requests coalesce, and disposal stops observation/timers without cancelling owner work.
- R4 adds safe registry descriptions and prepares binding reviews on expansion. Session history uses the existing pure session query with a bounded stable-message cursor; three owner projection probes cover complete non-overlapping pages, append stability and missing cursors.
- Saved Gateway address is reconstructed from the canonical pairing setup at projection time, excluded from persisted summary serialization. Existing paired storage does not require resetting or adding a second address authority.
- Final combined compilation, cross-language qualification, rendered screens and macOS build for this subsequent checkpoint remain pending. Future-calendar selection policy, service-specific remaining metadata and proposal simplification still require completion; do not report the entire restoration complete.

- Proposal cards are compact until the user chooses Review. The explicit Add to calendar action remains; it is not disguised as a read-only inspection. The Actions owner supplies proposal-scoped destinations using the same authenticated receipt/contributor validation as submission. One eligible destination is selected; genuine ambiguity remains a choice.
- Proposal timezone input is removed end-to-end. The current verified draft contains absolute UTC millisecond instants and no recurrence; Actions preserves those instants with canonical UTC metadata. Existing stored effects retain their own schedules. This changes the local product request contract, not stored effects or unknown-write recovery evidence.

### Review correction checkpoint

- `dda7722f` is published to main; macOS Debug arm64 and strict codesign passed. Publication does not imply complete UI/UX acceptance. The user permits direct main/WIP checkpoints; review and platform qualification are reported independently.
- Source-only adversarial review identified four concrete issues. History reads now bypass the session-command replay lock; Day mirror reads wait for an already-observed acquisition rather than superseding it with stale data; proposal listing and submission share temporal/coverage and current source-fence predicates; synthetic history IDs include per-turn occurrence ordinals and ambiguous cursors are rejected.
- Initial historical collected Actions seed the UI observation baseline without triggering another mirror read. Initial Connections availability remains a freshness invalidation: suppressing it unconditionally could hide a source that became ready after the first Day acquisition.
- Five isolated history projection probes and the combined 22 client probes passed for these corrections. The final Rust workspace gate, FFI build and architecture boundaries passed; Flutter analysis reported informational diagnostics only (156), with no errors or warnings. The corrected snapshot's Mac build and review closure remain separate pending checks.
- Remaining proposal work includes a safe pre-submit effect preview and a read-only route to an already-admitted Action. Also qualify EventKit timezone normalization against immutable receipt/recovery semantics before claiming proposal runtime parity. Do not treat a successful build as this qualification.
- Remaining history UX includes explicit recovery when compaction invalidates a retained cursor. Service-specific details, future-calendar selection semantics, the global calendar preference and full visual/runtime comparison remain open.

### Follow-up review correction

- The first Day correction coupled independent mirror reads to acquisition failure and did not cover post-mutation reads. Replace that wait with completion-driven invalidation: a successful refresh superseded by another read re-queries the current mirror only when its query still matches. Other-date reads and failures remain independent; mutation receipt handling is unchanged. Eight freshness probes cover cross-date failure, task mutation during acquisition, normal mirror races, retained command replay and disposal.
- Proposal presentation now checks current grant coverage through the Actions repository's read-only transaction, reusing the exact dependency-coverage validator used by atomic admission. Source/temporal checks remain shared; this preview does not reserve admission or replace transactional revalidation.

- Native timezone investigation found a concrete alias mismatch: Foundation's TimeZone_GMT implementation parses UTC/fixed-offset aliases but exposes GMT-normalized identifiers, while EventKit write and recovery compared raw strings. The adapter now compares normalized identifiers and returns the admitted display spelling only after exact instants and canonical zone match. Actual native revision fingerprints remain unchanged. Source reference: https://github.com/swiftlang/swift-foundation/blob/main/Sources/FoundationEssentials/TimeZone/TimeZone_GMT.swift. macOS compilation and live EventKit receipt/recovery qualification remain distinct; no calendar writes or user-data reset were performed as part of this change.

- The scoped follow-up review accepted independent Day reads, read-only current grant coverage and timezone receipt translation without new regressions. Its additional post-mutation ordering case is now handled by always invalidating same-date display on acknowledged mutation, rather than suppressing that observation using a pre-command generation. A superseded refresh whose command succeeded but final display read failed also makes one pure re-query. Two additional probes cover these orderings. Timezone normalization is symmetric for observed and requested identifiers; native fingerprint/digest rules stay unchanged.

### Proposal presentation completion

Replace the destinations-only proposal query with one canonical pure proposal preview. The Actions owner returns either an authenticated unadmitted effect preview (title, absolute schedule and currently eligible destinations) or its existing admitted Action snapshot. Derive the latter from the same receipt/artifact identity used by admission, validate the stored record's actor and exact origin, and never replay a command to discover it. Current grant and source checks remain mandatory for a new proposal; an already-admitted recovery record remains inspectable when permissions later change. Flutter loads this preview only on Review, shows the effect before Add, and reopens an existing Action after card disposal without issuing another write. Remove the old proposal-destinations route and migrate all FFI/Dart callers together.

- The canonical proposal-preview cutover is implemented across Actions, AppHost, DTO/FFI and Flutter; the previous proposal-destinations route has no active callers. The owner shares the deterministic admitted-action identity and effect title with submission. A narrow rendered proposal now shows title, start/end and destination before Add. The synthetic widget flow verifies no query before Review, no write before Add, and reopening a disposed card observes its existing Action without another submit. Combined 27 client probes, full Rust workspace/FFI build and boundaries passed; analyzer reports 158 informational diagnostics with no errors/warnings. Actual native write/recovery and full-screen restoration remain unverified/open.

- Proposal-preview source review confirmed identity equivalence, foreign/fake receipt rejection, query purity, unchanged admission fences and lazy reopen behavior. Follow-up presentation hardening applies the same immutable identity/revision guards to Existing and refreshed snapshots, uses typed parser errors, neutralizes pre-review wording and removes the stale destination-only architecture description. A conditional admitted-Approved/no-job liveness path and gateway-lifetime retained-command acknowledgement still require a coordinated recovery solution; do not silently discard those pending identities or imply full recovery acceptance.


### Conversation startup diagnosis

User-reported Calendar/Gateway connections work, but opening Conversation failed. Filtered AppDiagnostics records at 16:19–16:20 UTC identify `conversation_session` / `internal`, before model execution. Root reproduced a concrete boundary error: Vault creates and persists a valid empty Session at revision 0, while ConversationSessionSnapshotDto rejected 0 and FFI mapped the rejection to Internal. The real Rust wire fixture also exposed the same wrong positive-only assumption in Dart AgentSession. Preserve the owner/storage revision contract and allow 0 consistently in the Rust snapshot validator and Dart decoder; keep the i64 upper bound, usage/message validation, actor checks and command CAS unchanged. Existing valid empty sessions need no reset or migration. A disposable actual-FFI probe failed before the fix with the same Internal error and passes afterward; overflow and invalid usage still fail. The resulting real Rust wire fixture is qualified through Flutter's gateway/controller start/resume flow. Actual model/provider execution remains a separate unverified stage.

- The focused startup review found the same incorrect positive-only restriction on `ConversationStartTurn.expected_revision`. The fix is scoped to this command's upper-bound check; recovery and every other owner's positive revision validator remain unchanged. An actual Flutter-generated first-turn request failed Rust request validation before the correction and passes afterward. Separate owner preparation probes accept initial 0, reject stale 0 after Session revision advances, and reject a foreign Person. This qualifies the initial wire and CAS path, not a real model response or full live admission/provider execution.

## Architecture and UX convergence audit — source snapshot `c19e9d7`

The user redirected work from live startup diagnosis to root-owned UI restoration and architecture assessment. Leave the running local development processes and diagnostics untouched; a Keychain prompt is a user hypothesis, not a verified cause. This section extends the existing restoration plan rather than creating a second migration plan.

### Scope and evidence limits

- Baseline presentation: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`; assessed implementation: `c19e9d7a79e3ac85658653a6e67b9886f2f54d72`.
- Inventory covers 730 tracked Rust/Dart/Go/Swift/shell files, 184,980 physical lines, including generated localizations and examples. These are inventory counts, not a claim of exhaustive semantic review of every line.
- Baseline-to-current Flutter `lib` diff spans 108 files, including ownership moves, removed adapters and generated localization changes. It is not 108 changed screens or a percentage of visual completion.
- Direct source review follows Connections, Day, Conversation, Actions, Experts, Knowledge, Inference, their relevant Vault/provider/native adapters and Flutter call chains. Large files were assessed for responsibilities and shared mutable state, not condemned by line count alone. Go provider-auth implementations were compared for repeated lifecycle mechanics and provider-specific differences.
- The static Rust boundary checker passes for 23 nodes and 126 allowed dependency edges. It explicitly excludes source-level semantics; passing it does not prove CQRS purity, UI parity or interface segregation.
- No compilation, app restart, live provider effects or permanent test-suite reconstruction is part of this audit. Full visual comparison and interrupted-flow qualification remain open.

### Assessment of the agreed architecture

1. **Modular monolith is present.** Business crates, contracts, generic runtime, adapters, composition and bindings are physically separated. Preserve this topology; another whole-system rewrite or microservice split is not justified by the findings.
2. **Ports/adapters are materially implemented.** Examples include ActionsRepository/ActionCalendarExecutor, KnowledgeRead/KnowledgeOwner, ModelPort and the provider/Vault implementations. However, several consumers depend on capabilities wider than they need, and the Flutter layers do not consistently follow the same dependency direction.
3. **Command/query separation exists at the Rust wire/owner boundary but is not consistent end-to-end.** Keep queries observational, command recovery explicit and durable work owned independently of whether a screen happens to be visible. CQRS does not require a second database or an event-sourcing conversion here.
4. **Model abstractions are present.** Swift Transform has generic Input/Output; HealthTransform consumes DeviceModel; FoundationModelsDeviceModel implements DeviceModel. Shared Inference selects Gateway first and permits Device fallback only after verified Primary absence. Do not replace these already-correct boundaries with new pattern scaffolding.

### Findings and target corrections

#### A. UI read models lost presentation information (confirmed, restore first)

- Old `calendar_panel.dart` grouped connected calendars by account and showed per-calendar last-success/failure details. The current `SourceConnectionPanel` renders a flat `ResourceSummary` list; `ResourceSummary` and `PermittedResource` carry only opaque identity, label and selection. The missing metadata cannot be accurately reconstructed by widgets.
- `ConnectorScreen._integrationCard` recognizes macOS Calendar using `category == calendar`, display name `Calendar` and target platform. This is a presentation workaround, not a stable service identity contract. Other services still receive generic category descriptions/icons.
- `IntegrationDetailPanel` is a generic Connect card. `SourceConnectionPanel` combines resource selection, processing disclosure and every source category in one presentation. This erases service-specific explanatory states, not merely obsolete internal APIs.
- Access disclosure still prints raw `viewId`/category identifiers, and operational failure strings are exposed by replacing underscores. These need typed, localized presentation mapping; do not remove the underlying error/recovery distinctions.

Target: Connections owns truthful service/source/readiness projections. Add bounded display metadata from the real catalog/adapter where evidence exists, separately from opaque permission references and authority digests. Flutter maps stable service kinds and projected states into the baseline card/detail/picker compositions. Labels must never determine source identity or authorization. Do not fabricate missing accounts or silently widen resources.

#### B. Connections is a concentration of unrelated workflows (confirmed)

`ConnectionsService` holds sources, Access, evidence, cleanup, pairing, gateway registry, remote integration, product repository, source catalog and native setup, plus native/job sets, cancellation maps and catalog state. `application/product.rs` is 3,167 lines and handles pairing, catalog reconciliation, native/OAuth setup, source configuration, Observe review/application, query projection, job spawning and activation recovery. `source_operation.rs` adds another 1,193 lines of operations to the same shared service.

Target: keep one Connections module and authoritative aggregates, but separate private workflow components for gateway pairing orchestration, integration setup, source configuration/Observe coordination and read projections. Give each only its required ports. A bounded owner-local job supervisor may own registration/drain mechanics; it must not become another authority or a generic workflow-policy engine. Move methods and dependencies together, not just `impl ConnectionsService` blocks into more files. Preserve durable intent, CAS, cancellation, reservation and pending recovery semantics at each cutover.

#### C. Query paths can still recover commands implicitly (confirmed)

`AppWireConversationGateway._session` is used by `resumeConversation` and `loadConversation`, yet first calls `_submit(pending)` whenever a retained session command exists. The recently separated `loadEarlierConversation` avoids that path, but ordinary session reads still replay a command and share its busy lock. Replaying the same command is idempotent; it is nevertheless a hidden write/recovery dependency of a query.

Target: a pure session query path and an explicit pending-session-command recovery operation, coordinated by the application controller before a user action that actually requires recovery. Reads must remain available independently when safe; an unresolved command must not be forgotten or assigned a new ID. Do not conflate technical transport receipt acknowledgement with a business command.

#### D. Accepted Action work and runtime jobs have a lifecycle gap (confirmed conditional path)

`ActionsService::submit`/`decide` durably admit an Approved record and then call `spawn`. `execution.rs::spawn` can fail on shutdown or the 64-job limit. `activate` explicitly does not dispatch Approved work; the recovery query selects Executing/Unknown/pending collection. A proposal preview correctly returns an existing Action without mutation, but the UI then hides Add. Thus durable acceptance is not itself a guarantee that the executor will pick up this work.

Target: define an Actions-owned durable runnable-work contract and its bounded scheduling/activation policy before changing code. New scheduling must revalidate expiry, current authority/source and the existing pre-dispatch CAS; Executing/Unknown work follows receipt reconciliation, never blind redispatch. Decide the explicit resume presentation for admitted-but-not-running work. Do not repair this by making preview queries spawn jobs or adding another Submit button with a new command identity.

`ActionCommandReplay` additionally keeps up to 128 pending identities in a static Expando keyed by gateway. Existing-action observation currently does not settle those correlations. Move lifecycle ownership to an explicitly injected app/feature command tracker and settle only against matching authoritative acknowledgement. Do not discard uncertainty on widget disposal or clear the whole map on refresh.

#### E. Flutter dependency direction and state ownership need convergence (confirmed)

- `CalendarActionFacade` merely constructs and forwards to `NativeCalendarActionGateway`, while depending on `AppRuntime`. It provides no independent policy or real external boundary.
- `AppWireDayGateway` accepts the whole AppRuntime although its wire operations need a narrow transport capability.
- Conversation application interfaces/controllers import concrete `AppWireConversationClient` and `NativeTransportException`; `NativeConversationRuntimeGateway` is implemented in the same application file as its interface.
- Actions controller imports AppRuntime for its error type. Connections command/review/pairing observation state lives together in a presentation-directory controller. Similar Vault generation, busy, pending identity and response validation mechanics recur in multiple features with different rules.

Target: composition constructs actual adapters and injects narrow feature ports. Remove the forwarding-only Actions facade. Put transport implementations/decoders in infrastructure, stable feature snapshots/errors at the port/domain boundary, and view/navigation state in controllers. Extract only proven common correlation/disposition mechanics; retain domain-specific admission/retry policy in each owner. Do not build a universal controller or error-swallowing retry wrapper.

#### F. Cross-module capabilities are broader than use (confirmed coupling, not a demonstrated permission bypass)

Context's `ExpertContextDependencies` receives the entire `DayRepository`, which includes mutation, Action collection and refresh repository capabilities. `task_context_view`/related projections need bounded reads. Writes still require DayWriteFence, so this observation alone is not an authority bypass.

Target: expose a Day-owned read port with the exact bounded selections Context needs; implement it in the same repository adapter. Keep storage transactions and write fences private to the write path. Apply this assessment to other broad dependencies case by case instead of introducing interfaces around every class.

#### G. Large storage/runtime files require selective decomposition, not blanket splitting

Vault `learning.rs` (2,113 lines), `agent_actions.rs` (1,942) and `conversations.rs` (1,747) combine SQL/row codecs/transaction orchestration and calls to owner-defined pure policy functions. The latter is often correct: decisions must be validated inside the same transaction. Split row codecs, bounded readers and transaction entry points by owner operation while retaining atomicity; do not move transaction invariants into asynchronous UI/application prechecks.

Agent `engine.rs` (1,621 lines) and wire/native validators are also large, but a shared role-neutral execution loop and independent validation at a real external boundary are legitimate responsibilities. Generated localization files are not god-object evidence. Preserve one Engine and one canonical DeviceModel contract; prioritize the demonstrably mixed Connections and Flutter state first.

Google/Microsoft/work OAuth runtimes repeat callback-server, credential lifecycle, locking and error mechanics. Provider identity verification, nonce, scopes, endpoints and revocation differences are real. A later extraction should share proven transport/lifecycle primitives only, with provider-specific policies retained and security review before cutover.

#### H. Bootstrap and diagnostics do not clearly separate waiting from failure (confirmed design limitation; live cause unverified)

Flutter `main.dart` awaits diagnostics initialization, runtime open and native acquisition registration before the first `runApp`. A delayed OS/native dependency can therefore leave no product loading/blocked state. `NativeTransport._open` waits on the worker ready port. Go startup maps different Node construction errors to one generic fatal message.

Target: an app-owned bootstrap state with a visible, safe phase indication and a single retained initialization attempt; attach the ready runtime only when admitted. Add privacy-safe stage/reason diagnostics. An observer deadline must not kill an in-progress Keychain/native operation, duplicate initialization, reset state or misreport a pending operation as denied. This is a lifecycle design task, not a conclusion that the current black window is caused by Keychain.

### Residual UX classification

- **Restore without changing product intent:** service-specific card/detail structure and display copy, account grouping where actual metadata exists, selection dialog layout, friendly state/error labels, stable navigation and progressive disclosure. Keep current owner commands and recovery facts.
- **Already restored in source, still needs visual/runtime qualification:** direct drag, composer close after acknowledged success, automatic Day freshness, Experts descriptions and expansion review, bounded earlier messages, proposal effect preview and existing Action reopen. Do not call these fully accepted based on source/build alone.
- **Requires explicit domain semantics before UI restoration:** all calendars including future calendars; the old global “Calendar used by Floe” choice in a multi-source Day/per-Expert world. The former is a dynamic resource policy, the latter is an assistant source preference, not an external-write default. Neither may be faked with labels or today's resource list.
- **Keep accepted differences:** removed read-only/source diagnostic details; no per-model Gateway approval; source-processing review remains; no extra client code-match confirmation; truthful uncertainty/recovery when commands may already have committed.
- **Additional interrupted-flow work:** compaction-invalidated history cursor must reset/rebase the historical window explicitly; initial resource selection must not reopen every time a zero-selection detail widget is recreated. Neither should be hidden by repeated generic Retry.

### Recommended execution order and completion gates

1. **Freeze screen intent and contracts before more implementation.** Compare every changed presentation entry against the baseline at desktop/narrow widths using synthetic fixtures. Record missing visual information and normal/empty/loading/permission/error states in this same plan. Separate accepted changes from regressions.
2. **Complete the Connections vertical slice first.** Define service/display/resource query metadata and private workflow responsibilities, then migrate its Rust projection → DTO → Dart port/controller → baseline widgets together. Retire display-name heuristics, generic-only detail and raw-enum presentation. Resolve future-resource/global-selection semantics separately before adding those controls.
3. **Converge command observation/recovery.** Fix the explicit session-query boundary and Actions durable runnable-work contract, then settle matching Flutter pending identities. Qualify lost acknowledgement, reopen/disposal, capacity, shutdown and cancellation without duplicate external effects.
4. **Narrow interfaces and remove redundant layers.** Cut Flutter AppRuntime dependencies/facade, Context's Day read capability and mixed controller state. Each change has one owner/path, migration list and residual search; do not create parallel old/new adapters.
5. **Finish screen families against the matrix.** Calendar, Experts, Conversation/proposals, Memory/Settings, then bootstrap waiting/recovery presentation. Keep owner semantics fixed except where the previous stages explicitly correct them.
6. **Selective internal cleanup after contracts stabilize.** Decompose storage readers/codecs/transaction implementations and proven OAuth mechanics. Preserve atomicity and provider-specific identity semantics. Use Opus for adversarial review only; root owns investigation, design and implementation.
7. **Qualification:** one coherent slice before formatting/compile gates; no repeated broad checks during edits. Use disposable synthetic behavior/contract fixtures for changed boundaries, source/visual parity checks, and the affected final Rust/Go/Flutter/Apple gates. Permanent S3 test reconstruction remains later. Live native permission/provider behavior is reported separately and never inferred from builds.

This is a source-grounded architecture assessment and ordered proposal, not a claim that all findings are fixed or that every repository file has received a complete semantic audit.
