# Restore the pre-refactor Flutter experience

## Current completion overview — 2026-10-05

This overview distinguishes source implementation from complete visual/runtime
acceptance. The execution checkpoints below are chronological evidence, not a claim
that every earlier open item is still open or that the entire restoration is done.

- **Development storage prerequisite:** build-selected non-Keychain development
  custody and isolated encrypted client/server profiles are implemented. Production
  retains OS keyring and encryption. Synthetic refusal/restart gates pass; production
  Keychain ACL after changed signing identity and full live startup remain separate.
- **R1:** restored Remote server/pairing product flow is implemented and synthetic
  encrypted client/server pairing/restart is qualified; full current app acceptance
  is not inferred from those probes.
- **R2:** service cards/count, stable identity/copy, account grouping, selected-resource
  and permission dialogs are implemented. The macOS OS-access card is now implemented
  through a status-only native port; live OS verification remains separate. Day-owned
  freshness detail, future-calendar policy/global assistant preference and full
  source/workflow dependency decomposition remain open.
- **R3:** drag-to-move, acknowledged composer close and automatic Day freshness are
  source-implemented. Full screen-state comparison and live EventKit write/recovery
  acceptance remain open.
- **R4:** Experts descriptions/expansion review, bounded older messages and safe
  proposal preview/existing Action reopen are source-implemented. History cursor
  recovery, retained-command lifecycle and full visual/runtime acceptance remain open.
- **Architecture convergence:** initial modular-monolith topology is already present;
  the newer cleanup is still in the Connections/read-contract/recovery stage.
  Session query purity, Actions Approved-work recovery, narrower Flutter/AppRuntime
  and Context/Day ports, real workflow dependency separation and selective storage
  decomposition remain outstanding. Moving configuration methods to a private file
  is not completion of the dependency split.
- **Global gate:** Memory/Settings/detail/error-state parity, full active-Observe/live
  Calendar flows and the whole-screen matrix are not complete. Permanent S3 suite
  reconstruction remains deferred. There is no evidenced whole-project percentage.

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

Target: keep one Connections module and authoritative aggregates. First fix demonstrated association/read-contract/recovery defects; then perform a behavior-preserving separation of private workflow components for gateway pairing orchestration, integration setup, source configuration/Observe coordination and read projections. Give each only its required ports. A passive owner-local job registry may own registration/drain mechanics; workflow-owned job loops register with it, and it holds no workflow handles; it must not become another authority or a generic workflow-policy engine. Move methods and dependencies together, not just `impl ConnectionsService` blocks into more files. Preserve durable intent, CAS, cancellation, reservation and pending recovery semantics at each cutover.

#### C. Query paths can still recover commands implicitly (confirmed)

`AppWireConversationGateway._session` is used by `resumeConversation` and `loadConversation`, yet first calls `_submit(pending)` whenever a retained session command exists. The recently separated `loadEarlierConversation` avoids that path, but ordinary session reads still replay a command and share its busy lock. Replaying the same command is idempotent; it is nevertheless a hidden write/recovery dependency of a query.

Target: a pure session query path and an explicit replay-retained-session-command operation (distinct from conversation.session.recover), coordinated by the application controller before a user action that actually requires recovery. Reads must remain available independently when safe, with revision-monotonic observation guards; an unresolved command must not be forgotten or assigned a new ID. An absent resume result while Start is uncertain cannot authorize another Start or Start-turn. Do not conflate technical transport receipt acknowledgement with a business command.

#### D. Accepted Action work and runtime jobs have a lifecycle gap (confirmed conditional path)

`ActionsService::submit`/`decide` durably admit an Approved record and then call `spawn`. `execution.rs::spawn` can fail on shutdown or the 64-job limit. `activate` explicitly does not dispatch Approved work; the recovery query selects Executing/Unknown/pending collection. A proposal preview correctly returns an existing Action without mutation, but the UI then hides Add. Thus durable acceptance is not itself a guarantee that the executor will pick up this work.

Target: close the existing Actions admission/execution lifecycle before inventing a new scheduler abstraction. Once committed, return the admitted snapshot or an explicit Admitted outcome when spawning fails. Extend the existing owner activation/dispatch path with a deliberate Approved-work policy; do not depend on a volatile Flutter replay map. New scheduling must revalidate expiry, current authority/source and the existing pre-dispatch CAS; Executing/Unknown work follows receipt reconciliation, never blind redispatch. Decide the explicit resume presentation for admitted-but-not-running work. Do not repair this by making preview queries spawn jobs or adding another Submit button with a new command identity.

`ActionCommandReplay` additionally keeps up to 128 pending identities in a static Expando keyed by gateway. Existing-action observation currently does not settle those correlations. Move lifecycle ownership to an explicitly injected app/feature command tracker before removing the facade, and settle only against the exact command-kind acknowledgement predicate. Existing proposal identity proves an Action exists, not that a particular new command was accepted. Do not discard uncertainty on widget disposal or clear the whole map on refresh.

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
3. **Converge command observation/recovery.** Before broad mechanical service splitting, fix the explicit session-query boundary and Actions durable runnable-work contract, then settle matching Flutter pending identities. Qualify lost acknowledgement, reopen/disposal, capacity, shutdown and cancellation without duplicate external effects.
4. **Narrow interfaces and remove redundant layers.** Cut Flutter AppRuntime dependencies/facade, Context's Day read capability and mixed controller state. Each change has one owner/path, migration list and residual search; do not create parallel old/new adapters.
5. **Finish screen families against the matrix.** Calendar, Experts, Conversation/proposals, Memory/Settings, then bootstrap waiting/recovery presentation. Keep owner semantics fixed except where the previous stages explicitly correct them.
6. **Selective internal cleanup after contracts stabilize.** Decompose storage readers/codecs/transaction implementations and proven OAuth mechanics. Preserve atomicity and provider-specific identity semantics. Use Opus for adversarial review only; root owns investigation, design and implementation.
7. **Qualification:** one coherent slice before formatting/compile gates; no repeated broad checks during edits. Use disposable synthetic behavior/contract fixtures for changed boundaries, source/visual parity checks, and the affected final Rust/Go/Flutter/Apple gates. Permanent S3 test reconstruction remains later. Live native permission/provider behavior is reported separately and never inferred from builds.

This is a source-grounded architecture assessment and ordered proposal, not a claim that all findings are fixed or that every repository file has received a complete semantic audit.

### Screen-by-screen residual matrix (source comparison, not rendered acceptance)

| Surface / entry | Baseline behavior or presentation | Current source evidence | Next correction / gate |
|---|---|---|---|
| Connections catalog — `connector_screen.dart` | Service-specific card identity, description, grouped availability | Grid/count restored; generic category fallback and display-name-based macOS Calendar recognition remain | Stable service presentation kind from Connections, exact service card fixtures; no identity decisions from names |
| Native Calendar detail — old `calendar_panel.dart`, current `source_connection_panel.dart` | Account-grouped calendars, selected scope, system permission and acquisition information | Flat selected-resource list, generic availability badge, no account structure in DTO | Calendar-specific detail composed from safe observed metadata; permission vs Observe vs freshness remain distinct |
| Calendar resource picker | Account/calendar choices plus explicit all-current-and-future policy | Flat reviewed checkboxes, nonempty explicit selection only | Restore hierarchy/layout first; implement future-resource policy only after authority semantics are specified |
| Contacts detail/picker | Native Contacts-specific access and resource controls | Shares generic resource panel with Calendar | Keep reviewed opaque resources; restore Contacts-specific wording/empty/denied states, never expose raw contacts unnecessarily |
| Health/Attention detail | Source-specific privacy/system-access explanation | Generic selected-resource presentation can describe a singleton as a resource list | Separate singleton source presentation using actual capabilities; preserve local Health transform and unsupported Attention state |
| Hosted-service detail | Provider-specific authorization steps, source scope fields and Update scope | Generic Connect/Use with Floe; product source review/configuration currently rejects non-native sources | Specify owner-mediated scope configuration; do not restore direct Flutter HTTP or credential handling as a visual shortcut |
| Remote server settings | Address, pairing/status, dashboard, cancel/check/forget | Core composition largely restored; raw failure words and recovery controls still need normal/error-state comparison | Keep no extra client confirmation; compare paired, waiting, uncertain, repair and forgotten fixtures |
| Source-processing review | Source permission action, understandable disclosure | Prints raw View IDs/categories for non-Health sources | Localized presentation mapping over unchanged reviewed scope; no per-model approval |
| Calendar Day/agenda | Existing timeline, navigation, drag, edit/create dialog and concise feedback | Direct drag and acknowledged composer close restored; new coverage/freshness states added | Desktop/narrow layout plus stale/partial/empty/error states; retain accepted event-detail simplification |
| Experts | Description and source-binding editor | Description and on-expansion preparation restored; changed installation/review containers and pending-state controls | Compare disabled/loading/stale review/reopen; preserve exact binding review and current-source checks |
| Conversation | Existing bubbles/composer/history and interaction navigation | Startup contract fixed; earlier-message pagination added; compaction-invalid cursor rebase remains open | Pure reads, explicit recovery, stable viewport and ordinary/interrupted fixtures |
| Proposal/Action cards | Compact proposal, inspect details and explicit calendar effect | Scoped preview/reopen restored; substantial widget-local loading/cache/replay state remains | Feature-owned proposal state, explicit admitted-action progress, no hidden submission on inspection |
| Memory/Privacy | Existing list/review cards and navigation | Core structure similar; new command acknowledgement, retry and raw operation label | Keep uncertainty recoverable but present it progressively and consistently; do not blindly revert receipt handling |
| Action permissions | Ask/allow/deny Calendar creation control | Labels narrowed to actual Expert Calendar-create policy | Keep truthful scope; compare loading/error/control layout without restoring misleading “all supported actions” wording |
| App shell/bootstrap | Existing shell/navigation; no profile-selection product step | Shell injection changed, no profile chooser; first frame waits for native initialization | Preserve shell design; explicit safe loading/blocked bootstrap phase in a separate lifecycle slice |

Two additional Connections contract gaps were found while tracing the matrix:

- `integration_summary` selects the first non-disconnected SourceConnection matching only connector ID. Integration records distinguish target device/Gateway but the source join does not use that relation. This is a confirmed under-specified association; a wrong-source display in a multi-instance history remains a conditional risk, not a demonstrated authorization bypass. The read model must derive an exact current source relation before adding account/service metadata.
- `RemoteIntegrationPort::configure` and `ConfigureIntegration` exist in the provider boundary, but no current Connections application/FFI caller uses them. `prepare_source_review` and `configure_source` reject non-native sources. Old hosted-service Update scope therefore needs a real admitted owner workflow, not just a restored button. Credential entry remains a separately scoped Gateway management concern.

### Connections cutover map

Implement within the existing module; do not introduce extra crates, parallel routes or forwarding wrappers merely to reduce file size.

| Existing responsibilities | Final private unit / state owner | Required capabilities | Removal and invariant gate |
|---|---|---|---|
| `product.rs` overview/get/review inspection/source/integration projections | `application/queries.rs` (`ConnectionsQueries`) | Read-only product/source/Gateway/Access projections plus runtime observation | No persistence, spawn or permission prompt on inspection; exact integration-source association |
| Catalog refresh and native integration metadata | `application/catalog.rs` (`IntegrationCatalogWorkflow`) | Current verified Gateway, catalog adapter, product CAS, source read capability | One catalog publisher; invalidation on producer/credential change; labels never identify authority |
| Integration review/start/cancel/drive/native setup | `application/integration_setup.rs` (`IntegrationSetupWorkflow`) | Product command journal, remote integration/native setup, source admission coordinator | Preserve persisted intent before external dispatch, same operation identity, cancellation precedence and uncertain recovery |
| Source review/configuration and resource policy | `application/source_configuration.rs` (`SourceConfigurationWorkflow`) | Catalog/evidence, source read capability, product journal, existing source mutation coordinator | Keep exact reviewed resources/catalog digest and source reservation → Access invalidation → source successor sequence |
| Observe prepare/apply/pause | `application/source_observe.rs` (`SourceObserveWorkflow`) | Access, source/evidence readers, source mutation coordinator | Source-scoped authority only; no model recipient approval; no implicit grant expansion |
| Source reservation/receipt/commit/reconciliation currently in `source_operation.rs` | Retained, narrowed `SourceMutationCoordinator` | Source operation repository, Access and cleanup ports | One coordinator for Access-coupled transitions; configuration and Observe use it. Initial setup source creation remains explicitly owned by setup, with its separate admission protocol |
| Spawn registration, cancellation, drain, activation dispatch | `application/runtime.rs` (`ConnectionsJobRegistry`) | Job identity/cancellation/drain state only, no workflow handles | Passive registration; workflow loops remain with their owners; preserve exact operation replay and pending recovery |
| Public `ConnectionsService` entry point | Admission/lifecycle composition of the above | Narrow handles, not the old entire dependency bag | Migrate FFI/App callers together where necessary; remove old monolithic method bodies after cutover |

The names specify the target roles; stateless projection helpers should remain functions rather than acquire invented mutable objects. Shared runtime state is limited to liveness/observation. Durable aggregate state, reviews and command outcomes remain in their existing semantic owners/repositories.

Ordered implementation inside this slice:

1. Pin exact integration/source identity and display-read contracts. Read all affected record/projection/adapter/DTO/widget callers before editing; distinguish physical resource identity from display metadata. Include reopened existing connections, not only fresh setup.
2. Move pure projections and narrow read capabilities first, with no command behavior change. Migrate wire/Dart models and baseline card/detail presentation in the same cutover; remove display-name heuristics and generic-only rendering.
3. Separate catalog/setup/configuration/Observe workflows around the existing source mutation coordinator. Move dependency/state ownership with methods. Preserve disposition classification (`NotAdmitted`, `NotApplied`, `Admitted`, `Indeterminate`) and exact pending command identity.
4. Consolidate owner-local job registration/drain and catalog invalidation only after workflows have narrow entry points. Qualify registration vs shutdown and restart recovery; no screen may become responsible for starting forgotten durable work.
5. Restore hosted scope editing through an owner command if selected for this slice; otherwise keep the missing capability explicitly open. Do not expose a UI control whose underlying owner path does not exist.
6. Delete replaced method bodies/facades/dead exports and search for old names, broad dependency fields, direct Flutter HTTP, display-name identity tests and implicit query-command replay. The unused `FloeCore::source_service`/forwarding `SourceConnectionService` is a separate small removal candidate after verifying all callers again; it is not a second active write path.
7. Run one slice-level contract/behavior gate and the required final platform checks. Record screenshot fixtures separately from actual Keychain/native/provider qualification. Do not call this slice complete while its screen matrix still contains unqualified changed states.

Additional read-contract constraints from tracing the producer paths:

- `GatewayIntegrationAdapter::list` validates remote `connecting`/`error`/`connected`/`disconnected`/`unavailable` but reduces them to `available` and `connected` booleans in `IntegrationDescriptor`. `integration_summary` can consequently project only Available/Connected/Unavailable. Restore truthful connection-progress/error states at the owner projection boundary; styling cannot recover discarded status.
- macOS `subjectEvidence` currently concatenates account/source title and calendar title into one label. Do not split that text on a delimiter to invent grouping. Carry bounded separate display fields from the real native catalog if account grouping is restored, without changing the physical identity/fingerprint policy merely for display.
- Day already owns per-source/resource last-success and coverage projections. Connections sets `SourceSummary.last_observed_at` to None; that field is not an alternative acquisition-history authority. A composite detail must use Day's real read projection for freshness.
- Day `display_reference` and Connections `source_ref`/`resource_ref` currently use different derivation namespaces. They are not interchangeable just because both are UUID-shaped. Define a deliberate read-model correlation contract (or explicit owner-supplied mapping) before joining these projections; never correlate by label or cast one opaque identifier into another. Do not move Day acquisition state into Connections to simplify the screen.

## Development storage profile — newly requested prerequisite

The user approved the convergence sequence and additionally requested development-mode execution without OS keyring/Keychain, while production must retain keyring and encryption. Treat this as an explicit build/profile boundary, not recovery from a Keychain error. It is a prerequisite for repeatable UI qualification; the Connections and subsequent convergence order otherwise remains unchanged.

### Target and safety invariants

- Keep the encrypted Vault engine, schema, repositories and owner behavior identical in development. Replace only key custody with a development-only file-backed VaultKeyProvider; restarting a development app can reopen its same isolated data.
- Production uses the OS-backed provider and fails closed when it is unavailable. A runtime environment variable or Keychain failure cannot select the development provider. Release/profile packaging must reject a development-provider build rather than silently include it.
- Development keys are not as protected as OS keyring keys. Store them only in a separate, clearly marked development profile with private directories/files, create-only keys and bounded reads; never copy existing Keychain material, import an existing production profile, or overwrite a missing/mismatched key. Use synthetic data by default.
- Client data, Gateway data, credential slots and pairing identity must be development-specific. A development binary cannot adopt the production root merely because an arbitrary path/environment override points there. Existing data and logs remain untouched.
- Cover both Rust client Vault keys and Go Gateway credentials.Store; changing only the Rust provider would leave server Keychain dependencies. Native provider permissions such as Calendar access are separate and remain real permissions.
- Normal development startup performs one explicitly selected flow. No plaintext fallback, mock encryption, hard-coded universal key, automatic credential migration, silent reset or blanket permission bypass.

### Existing seams and required cutover

1. `VaultKeyProvider` already defines load/insert/preflight inspection. Move its neutral read-failure classification out of the keyring implementation module so alternate custody does not depend on a concrete OS adapter.
2. Add mutually exclusive build-selected production/development key providers. Route both `AppComposition::open_default` preflight and `vault_lifecycle::execute` through the same profile-selected provider; the former currently hard-codes KeyringVaultKeys and must not be missed.
3. Add explicit profile admission before installation discovery/open. Development uses a separate marker and root, rejects adoption of unrelated/existing production files, and does not inherit automatic reset-on-error behavior as a substitute for correct custody. Explicit-path CLI/FFI entry points must enforce the same mode contract.
4. Go Node construction currently instantiates credentials.Keychain directly. Select credentials.Store at composition with a development-only build selection and isolated server root; provider runtimes continue consuming Store. Do not add separate provider logic for development.
5. Wire macOS/iOS Rust embedding and run-local together so Flutter, native library and server agree on profile. Release/profile cannot bundle development custody. Make the active development mode visible without changing the normal production UI.
6. Qualify encrypted creation/reopen, distinct random keys, missing/malformed-key refusal, exact profile isolation, key-file permissions/symlink rejection, no OS-keyring calls in development, and production build exclusion. Synthetic fixtures exercise the same persistence/owner path. Never count that as real Keychain qualification.

### Production encryption coverage is not yet complete

Source inspection found that EncryptedAgentVault uses AES-256-GCM, but `crates/adapters/vault/src/engine.rs::TursoStore` deliberately opens a plain SQLite store and validates a plaintext SQLite header. Day items/mirrors and host source state therefore must not be described as covered by Vault encryption. Go private JSON/identity files also use private-file permissions, not general at-rest encryption.

Before claiming the user's production requirement is satisfied, inventory which persisted payloads contain personal data or secret material and define their encryption/key ownership. Preserve the existing distinction between host-lifetime product reads and the agent Vault's lock/admission state; do not force every Day read through an unrelated Conversation/Vault permission just to reuse an encryption helper. A separate storage-key boundary can protect host data while retaining that lifecycle distinction. Production encryption closure and development Keychain independence are separate acceptance gates.

### Root adjudication of the source-only Opus design review

One Opus review of `ccb0a667` challenged the plan; root checked the relevant implementation before accepting its recommendations. No implementation or investigation was delegated to the relay.

- **Accepted:** the Action gap includes returning a bare spawn error after durable admission. Correct the outcome contract and existing activation path before proposing a new runnable-work framework. Preserve expiry/current-source/authority/pre-dispatch CAS; choose and document restart policy explicitly.
- **Accepted and confirmed:** product repository `list` orders all records by record_ref and applies `LIMIT 1024` without continuation. Overview, forgotten-Gateway lookup and restart recovery filter that truncated prefix. Replace with typed selective/paginated read contracts; do not drop old recovery records or pretend one capped page is complete. This correctness gate precedes a large Connections split.
- **Narrowed:** integration/source identity must be exact, but account grouping does not by itself prove 1:N SourceConnections. A native connection already has many calendars across accounts; current Go storage enforces a connector/person ownership key. Preserve the cardinality actually supported, distinguish resources/accounts from SourceConnections, and represent multiple source instances only where the owner really permits them. Do not invent a new relation persisted separately from source admission.
- **Disproved for current query routing:** Flutter observePairing/observeOperation map through FFI to get_pairing/get_operation, not reconcile methods. Those observations do not acquire OS permission. Reconcile, activation/recovery, forget, management launch and start/cancel are command/workflow behavior and must stay outside the pure projection unit. Automatic post-frame review preparation is still a durable command and should move to explicit user-intent flow rather than repeat on widget recreation.
- **Accepted:** a dormant remote Configure port cannot implement scope change with its current limited result. The full reserve/invalidate/external-write/uncertain-recovery/successor evidence sequence must be specified; otherwise remove the unused surface at the cutover. Do not expose a decorative Update scope button.
- **Accepted:** preserve the different native-drive exclusion and job-drain lifetimes. Use a passive job registry to avoid a supervisor/workflow dependency cycle. Keep catalog invalidation on Forget explicit. Shared product command-journal/disposition helpers belong to one private command-admission component. The source mutation coordinator covers Access-coupled transitions, not every initial source creation.
- **Accepted:** removing CalendarActionFacade must follow migration of its Expando-keyed tracker. A generic existing proposal snapshot cannot acknowledge a different command. New session creation remains blocked while its prior Start is uncertain even after read paths become pure.
- **Accepted:** prefer the actual bounded connector identifier already owned by Connections over inventing a duplicate presentation taxonomy. Flutter presentation maps that identity to copy/icons without letting labels act as identity.
- **Priority refinement:** data/read-contract correctness and recovery completeness precede broad file/object moves. Large-file size indicates review cost; it is not proof that every large service needs another object. Preserve the approved product/owner topology and target concrete coupling.

The latest development-storage request and additional producer-path findings were outside that review's input. They require their own implementation evidence and security-focused review; the completed review does not qualify them.

### Development custody implementation checkpoint

The first implementation keeps the encrypted Agent Vault engine and changes build-selected custody only. Rust uses mutually exclusive os-keyring/development-storage features; Release/Profile rejects development custody. FFI exposes a build-profile code and Dart checks it before native open. Composition admits marker-bound development-storage/client data, uses one selected provider for preflight and activation, and refuses implicit reset/adoption. Native installation recovery is an explicit composition policy. Go development builds exclude the Keychain implementation, use a private file Store and distinct FloeServerDevelopment/18431 defaults; run-local chooses the same mode as Flutter. iOS Debug Contacts handle identity also has separate file custody; its Release/Profile path retains Keychain.

Qualification completed on the root's isolated execution environment:

- Four disposable Rust development probes: isolated installation/create/reopen including actual asynchronous Vault activation; encrypted Session CAS/persistence; missing-key refusal; exact-slot no-overwrite, private modes and symlink rejection. Existing sentinel data was unchanged.
- Real development FFI host: storage-profile code 2, isolated open, Vault create/unlock and Session start/resume across host restart. No model/provider call.
- Default full Rust workspace gate including doctests and production-custody FFI build passed. The default-custody probe rejects a marked development profile before keyring access. Apple-target normal dependency projection for the development FFI has no keyring dependency.
- Both Go build modes passed race tests and vet, including disposable private-file custody and profile-isolation probes.
- Flutter analysis: 157 informational diagnostics, no errors/warnings. Combined external restoration/boundary fixtures: 32 passed, including production-library rejection before any Debug data open and matching development native transport open/close.
- Real Release and mixed-feature compile attempts rejected development custody with their intended guards.

Disposable probe sources/logs are retained outside the repository; permanent S3 reconstruction remains deferred. macOS/iOS compilation, source-only adversarial review of the new custody implementation, live Apple startup and full production encryption coverage are still distinct pending gates. Earlier architecture review did not cover this code. Existing user's app/server/data/logs were not touched by these probes.

Root's post-checkpoint coexistence probe found that a development-only auxiliary directory made the unchanged production fresh-root validator reject a first production installation. The correction reserves only the known development auxiliary directory names alongside diagnostics/recovery; it does not read/adopt their contents, follow symlinks or move them during recovery. The same-base development-first/production-first sequences are now explicit qualification cases, separate from rejecting a production binary pointed directly at a marked development root.

### Development custody review closure (2026-10-04)

The a08919b Opus review confirmed no runtime fallback into development custody.
Root reproduced and fixed development-first/production-first coexistence (d601b5c).
The next correction removes development-only whole-host Vault preflight: the
canonical VaultBridge owns typed presence/open failures while Day remains usable.
Explicit-path hosts pass the canonical database path to that same VaultBridge.
Both directions of Swift/Rust iOS Debug profile selection are now guarded.
CLI default custody matches Flutter Debug, with an explicit Release/OS-keyring
`--production` mode. The reset helper defaults to development only, refuses running
hosts and symlink paths, moves known artifacts to Trash, and never deletes Keychain
entries or diagnostics. Partial marker/key initialization remains fail-closed;
inspect retained evidence before explicitly moving the isolated development
profile aside. Do not infer that missing or invalid keys authorize regeneration.

Remaining qualification/design limits: storage namespaces do not forbid explicit
cross-profile Gateway pairing; the existing signed identity and human approval
remain required, but a signed build-profile admission policy is not yet present.
Custom server address settings can differ from the client default and must be
configured explicitly. Private file custody does not protect against a malicious
same-user process or all ancestor-directory races. Production Day/Gateway payload
encryption remains a separate open implementation requirement. No live user data
or Keychain reset was performed to qualify these changes.

Qualification for this closure: real development FFI accepted an existing relative
path and unlocked its Vault; after moving a synthetic key aside, the independent
host reopened and Vault unlock returned typed `vault_unavailable`, without a new
key. Both profile creation orders passed. Synthetic reset fixtures passed both
profile scopes plus busy/unknown process and symlink refusal, preserving diagnostics.
Default Rust workspace/doctests and both Go race/vet modes passed. The separate
exact-a08919b Mac Debug arm64 build, strict signature check, bundled profile getter
(2), iOS arm64 Debug simulator build and both Go builds passed; no app was launched.
The later correction still requires its own Apple build qualification.

### Connections presentation contract cutover, first bounded change

Replace descriptor `available`/`connected` booleans with one canonical
IntegrationState so Gateway Connecting/Error survive adapter → owner → Flutter.
Only Available admits a fresh integration review; a connecting operation is not
silently represented as available. Add a computed safe service-kind projection
from the owner-controlled target/connector identity; Flutter must not identify
Apple Calendar by its mutable display name. No credential or source authority is
added to the DTO. Existing stored descriptor meaning changes directly under the
pre-stable policy, with no legacy decoder; old development fixtures may need an
explicit fresh profile, never automatic reset. Exact source association, bounded
journal reads, resource grouping and the remaining screen matrix stay open.

The first presentation-contract change passed the default Rust workspace/doctest
gate, 34 Flutter behavior fixtures plus one actual Rust-FFI-to-Dart overview decode,
and Flutter analysis with the same 157 informational baseline findings (no errors
or warnings). It does not yet qualify remote-provider live states or full rendered
parity. Exact f95ad601 Apple/CLI/Go build qualification also passed separately;
no runtime launch or Keychain access occurred.

### Custody closure follow-through

The focused f95ad601 Opus review verified that removing development preflight
cannot recreate a partial Vault or replace keys. Root accepts the remaining
explicit-open path mismatch: canonicalize once before validation, installation
lock, identity lookup, store open and Vault construction, including the short
`people/<person>/floe.db` form. Instead of adding a third debug-custody ABI mode,
remove automatic archive/reset from every build. This eliminates the unshipped
OS-keyring-debug path that could archive production data while reporting the same
custody code. Remove its recovery enum, reset method, archive/resume code and
unused reset-evidence preflight module. Existing incomplete reset markers remain
fail-closed and preserved. The explicit operator reset script remains separate;
it is not invoked by startup. This is a lifecycle simplification, not a weaker
schema/key/identity check. Exact f95ad601 Apple builds passed before this follow-up.

Root qualification for canonical admission: actual FFI opens/unlocks the short
relative path from the installation root; an alias to a database already held by
another host fails on the same installation lease. Both default OS-keyring-debug
and development profiles preserve byte-identical synthetic damaged databases and
installation files after failed admission, with no archive directory or reset.
Missing-key typed Vault failure and both coexistence orders still pass. Default
workspace/doctests, both FFI builds and architecture boundary checks passed. The
removed preflight/reset API has no remaining crate caller or export.

### Connections journal enumeration contract

Before extracting read projections, replace the repository's silently truncated
`list(person, 1024)` with explicit ascending record-ID pages. The storage adapter
owns bounded decoding and continuation; a private owner-side scan checks scope,
Person and monotonic progress while retaining only one 64-record page. Overview,
forgotten-Gateway lookup and startup recovery consume to terminal continuation.
This is an observation scan, not an atomic multi-owner snapshot: concurrent later
commands remain owned by their admission/spawn path and a later overview can
observe them. No scan may classify truncated history as complete. Cancellation or
deadline fails explicitly, and pending recovery does not delete or duplicate
external work. SourceRepository/pairing enumeration limits remain separate open
contracts; do not claim this closes all recovery capacity gaps.

The same Connections read-contract cutover now uses a catalog source identity
(connection ID, execution owner and incarnation/epoch) instead of selecting the
first local source with a matching connector name. Native entries obtain it from
the exact native connection; the paired Gateway catalog already publishes these
fields, which the adapter now preserves and validates. A projection attaches only
an already-admitted matching local source. This metadata is not a grant or signed
source-admission substitute. Cached Gateway entries also require the exact current
paired binding, not just a cached Gateway UUID; stale entries cannot borrow a
new enrollment's source display. No label-based or cross-Gateway join remains.

Focused disposable fixtures passed: 1,030 encrypted product records survived
checkpoint/reopen and returned in 17 ordered 64-record pages with no missing tail
or duplicates; foreign Person, zero limit and terminal empty-page cases rejected
or completed as specified. Pure source-correlation cases reject another device,
another connection and another incarnation/epoch despite the same connector label.
Gateway catalog parsing rejects partial/invalid identity and does not correlate an
identity-unverified source. These fixtures remain external qualification evidence,
not the deferred permanent S3 suite.

The bounded enumeration/correlation checkpoint passed default workspace/doctests,
development FFI build, actual Connections overview through that FFI and the static
23-node/126-edge dependency check. No whole-screen or live-provider qualification
is implied. Source/pairing operation enumeration and acquisition presentation remain
open; this checkpoint fixes only the product-journal prefix and exact catalog join.

## Production at-rest coverage closure — design before implementation

The user's production condition is not satisfied by OS custody for Agent Vault
alone. Source inspection confirms the host product database contains Calendar,
Day, Tasks/Notes and source/lifecycle records in plaintext; Gateway trust, producer
private identity, integrations, inference configuration and Gmail metadata index
also have private-file rather than encrypted-at-rest storage. Development must
keep the same payload protection paths while substituting isolated file custody.

### Client target

- Keep Day/product availability independent from Agent Vault lifecycle; do not move
  Day into the Agent Vault or make a UI query create/unlock that Vault.
- The product database gets its own purpose-separated root key, bound to the
  admitted Person/device installation. Production uses a distinct OS-keyring
  service; development uses an exact private file under the isolated profile.
  Shared OS/private-file key mechanics have one adapter implementation; Agent
  Vault and product-store key identities stay distinct.
- Installation admission owns Fresh versus Existing. Only Fresh may generate and
  insert a key. Existing loads its exact key; missing, malformed, locked or changed
  keys fail without regeneration, plaintext fallback or reset. The existing
  installation creation marker/lease precedes key publication and remains through
  database identity/schema creation, checkpoint and durable Ready publication.
- TursoStore uses the same pinned AES-256-GCM engine already used by Agent Vault,
  including WAL. Existing-only pinned-file IO remains in place; remove the
  plaintext SQLite-header assumption. Validate an encrypted product-identity row
  against the caller's admitted Person/device before owner activation. Rename
  physical layout terminology from Plain to Product so encryption and semantic
  table ownership are not conflated.
- Product key loading occurs at host admission. It does not add repeated Keychain
  prompts to every Day read; database/key lifetime belongs to the retained host.
  The Agent Vault's existing key/session fences remain unchanged.

### Gateway target

- Node composition admits one process-locked storage root before publishing Trust
  or provider owners. A non-secret create-only storage identity chooses an exact
  purpose-separated root-key slot in the selected credential backend. No root key
  is generated for a preexisting unmarked data directory or a missing existing key.
- Introduce one authenticated bounded-file codec using standard-library AEAD and
  the existing durable private write mechanics. Associated data binds format,
  storage identity and logical file purpose. Owners retain schema, CAS, uncertain
  write classification and business recovery; the codec never interprets records.
- Inject scoped file storage into Trust, Integrations, Inference configuration and
  Gmail index. Remove competing raw JSON/metadata writes from those paths. Existing
  provider credentials continue through the selected credential Store.
- Producer private keys and persistent admin bootstrap credentials are sensitive
  storage too. Production must not leave an unencrypted admin-token alongside an
  encrypted database. Define an explicit local operator retrieval path before
  changing the dashboard token workflow; keep development setup convenient.
- Non-secret identity/format markers and lock files may remain readable. Diagnostic
  logs must not receive payloads/keys as an encryption side effect. At-rest AEAD
  does not claim protection against an already-authorized malicious same-user
  process or rollback of an entire valid encrypted snapshot.

### Sequence and qualification

1. Shared custody mechanics and encrypted client product-store admission, migrating
   the three actual TursoStore creation/open callers in App together. No old-format
   decoder, implicit migration or new profile-selector UI.
2. Disposable end-to-end fixtures: fresh/reopen, wrong/missing key, altered identity,
   partial key/schema creation, both build profiles, main-file/WAL plaintext-sentinel
   absence, canonical lock exclusion, preserved diagnostics and no auto-reset.
3. Gateway storage identity/lease and authenticated-file seam; migrate all listed
   owners and Gmail cache together, including admin-token retrieval contract.
4. Wrong-purpose/cross-root ciphertext, truncated file, uncertain rename/fsync,
   concurrent open, missing-key and post-restart tests. No real user data reset.
5. Build-profile exclusion and Apple Debug/Release qualification. Only then mark
   production payload encryption covered; no completed build alone proves live
   Keychain permission or provider behavior.

This section is a target/sequence, not an implementation claim. The Connections
screen/capability restoration continues after this newly requested storage
prerequisite; its remaining matrix and command-lifecycle issues remain open.

### Pagination/correlation review adjudication

Opus found no pagination-core defect. Root accepts the unsigned catalog correlation
risk: Gateway source identity must match the pinned producer's execution owner,
instance/fingerprint/audience, and may never attach a device-native source. Native
state/identity and setup now share the existing canonical Apple-owner derivation.
Reject disconnected/unavailable-with-identity and connected-without-identity
catalog combinations; allow Connecting with an existing source because the actual
Go catalog can emit that while a new attempt is pending.

Overview is a current-state projection, not an unbounded history browser. Gateway
registry reads the exact current Forgotten receipt via its durable expectation;
overview no longer accumulates every historical Forgotten summary or old Gateway
integration. Exact historical command/Gateway lookup remains available for replay.

The suggestion to run all recovery branches after corruption is not applied
blindly: ReadyGeneration fences and seals the failed generation on activation
failure, so spawning more independent branches before returning the same error
would not establish recovery. Preserve fail-closed corruption semantics; a bounded
independent recovery/availability design belongs to the explicit command-lifecycle
stage. Source epoch-change presentation remains a known missing UI state, not a
reason to attach stale authority. Snapshot revision and the other repository
limits are pre-existing open contracts, not claimed fixed here.

### Client product encryption implementation checkpoint

Implemented a distinct product-store key identity bound to Person/device, with
shared secret-byte/private-file/OS-custody mechanics and unchanged Agent Vault key
namespace. The three App creation/open callers pass admitted installation identity.
Fresh product creation reserves the file, creates its exact key, and seeds encrypted
identity/schema in one transaction before checkpoint/fsync and installation Ready.
Existing open retains pinned-file IO, decrypts read-only first and checks the stored
identity before writable owner activation. The former Plain layout is now named
Product; there is no plaintext decoder or missing-key fallback. Existing older local
profiles need an explicit fresh development profile; they are preserved, not migrated
or reset automatically. The dormant Android source receives only the mechanical key
type rename; no Android functionality or qualification is added.

Actual development FFI qualified Day note creation/read/restart with a synthetic
sentinel absent from the main file and WAL. Wrong/missing product key failed without
replacement; restoring the exact original key reopened the note. Separate direct
store fixtures verified copied key material cannot adopt another Person/device,
preexisting/partial files cannot create another key, and the product file is0600.
These are client storage results, not Gateway encryption completion or live Keychain
permission qualification. Apple479b Debug/Release builds passed before this encryption
change; newer Apple qualification is still required. The earlier Release app shell
was universal while bundled Floe dylibs were arm64, so only arm64 bundle compatibility
was established, not Intel support.

The combined client encryption/correlation working tree passed default Rust workspace
and doctests, both FFI feature builds, 35 Flutter behavior/boundary fixtures, actual
Day encrypted restart and canonical path/lock probes. A real disposable Go development
Gateway plus Rust FFI also completed initial pairing, dashboard administrator approval,
local Forget and encrypted-client restart; overview retained the exact current Forgotten
state and removed stale Hosted cards. No Mac user data or real provider was involved.

That end-to-end exercise exposed a separate reproducible re-pairing defect: after
Forget, the second administrator approval returns `pairing_conflict`. Metadata-only
inspection of the synthetic state proved expected/current Trust revision both2,
no pending cleanup and no duplicate new client, but the requested issuer key already
exists. Agent Vault retains one `remote_authority_owner` key across pairing attempts;
Go ActivatePairing rejects any issuer key already enrolled or revoked. This is an
existing lifecycle contract mismatch, not a reason to weaken revocation, erase state
or silently generate replacement keys. It is a next correctness design/qualification
blocker. The full two-cycle test is FAILED; only its first cycle and restart passed.


### Product encryption review closure and remaining bootstrap boundary

Exact e084 Apple Debug/Release builds, signature checks and custody getters passed;
iOS arm64 Debug and dev CLI compiled, without launching a host or accessing Keychain.
Opus found no authority/fallback/reset defect. Root accepts classification gaps:
carry exact Missing/Malformed/Unavailable product-key outcomes through an App-owned
storage failure enum and existing FFI error metadata; distinguish unsupported prior
plaintext format before requesting a key. An identity-bound unfinished creation
admission can classify schema/partial-data failure as IncompleteCreation without
creating or replacing any key. Unknown I/O remains unavailable, never reset evidence.

Product encryption necessarily makes its key a host-admission dependency. Release
waiting/Keychain prompting still needs the planned visible bootstrap lifecycle;
there is no unencrypted Day fallback. iOS product custody retains the existing
WhenUnlockedThisDeviceOnly policy, rather than silently broadening background key
access. Turso requires an owned hex String for encryption options; Floe hands that
buffer directly to the API and cannot guarantee zeroization of its internal copies.
Adding a transient Zeroizing copy would not erase the API-owned copy; no complete
memory-erasure claim is made. Failed generation activation fences/seals but does not
synchronously drain already-spawned work; that distinction is retained in lifecycle
follow-up rather than claiming stronger shutdown semantics.


Bootstrap's first visible surface is now installed before awaiting native admission,
so a pending Release key operation need not leave an unpainted window. The shared
waiting/error view is passive and cannot create a second open or reset data. It is
not a complete cancellable/retryable bootstrap redesign, nor proof of live Keychain
prompt presentation. Existing ready-shell UI and the no-profile-picker policy stay
unchanged. Typed FFI probes distinguish all five new failure cases and preserve
files/keys; the old plaintext format is identified without asking for a product key.

This diagnostics/startup checkpoint passed default Rust workspace/doctests, both FFI
feature builds, all five typed-error probes, the encrypted Day restart/absence probes,
and 37 Flutter behavior/boundary fixtures including passive waiting→failure rendering.
Flutter analysis retained its informational baseline with no new errors/warnings.
The new waiting surface has not been observed against a live Apple Keychain prompt.

## Re-pairing issuer lifecycle — corrective design

The reproduced conflict must not be fixed by allowing a revoked issuer, clearing
Trust state or resetting the user's Vault. Server enrollment binds an issuer key
to one client and retains revocation fences. The client currently supplies one
Vault-wide key to every pairing, contradicting that lifecycle. The target is a
fresh enrollment-scoped signing key for each newly admitted pairing operation,
while an exact retry/restart of that operation retains the same key.

### Final ownership and cutover

- Connections owns the pairing operation and exact Start command. Its atomic first
  admission stores the operation, polling proof, a fresh wrapped Ed25519 enrollment
  key, credential expectation and command receipt together. Generating a key is not
  a side effect of a public-key query or transport call.
- The encrypted Vault adapter owns private key generation/wrapping/readback. Bind
  wrapping AAD to Person/device and operation identity. Keep historical operation
  keys/receipts; Forget invalidates live signing authority, not recovery evidence.
- PairingPrivateSnapshot projects the exact operation's admitted issuer alongside
  its existing private transport material. Remove EnrollmentSigner.public_key;
  public-key lookup cannot independently select another operation. Enrollment
  signatures validate the command against that key.
  Ordinary authorization reads only the currently committed enrollment key and
  retains all existing credential, pin, runtime-generation and source-grant fences.
- Producer pin lookup remains its own read boundary. It must not depend on a global
  owner key existing before a new pairing; the pin remains validated and preserved.
- Remove the Vault-wide remote_authority_owner table/seed and singleton validation.
  The existing encrypted physical schema is replaced directly under the pre-stable
  policy, without a legacy decoder or automatic profile reset. Unpaired fresh Vaults
  remain valid for local features, without pre-generating a remote signing identity.
- Server unique/revoked issuer rules and human approval stay intact. No server
  credential is silently revived and no old pending external request gets a new key.

Files/callers to migrate together: schema/gateway.rs, vault/schema_lifecycle.rs,
vault/authority_keys.rs, gateway_pairing_store.rs Start admission/private-row reads,
gateway_authority.rs enrollment and ordinary signing, Connections EnrollmentSigner
port and GatewayPairingAdapter.begin, and Vault startup validation. Audit every
remote_owner_public_key/load_owner_key/validate_owner_key caller by phase; pre-pair
pin reads, exact enrollment and committed authorization cannot share an implicit
current-key lookup.

### Required qualification

- Two actual isolated pairing/approval/Forget/restart cycles succeed with different
  issuer keys; same Start command retry and pending restart preserve one exact key.
- Cancel/reject/expiry followed by a newly admitted Start uses a new key while old
  receipts stay readable. Wrong operation/device/AAD or revoked/current-generation
  mismatch cannot sign or publish credentials.
- Simulate lost acknowledgement and stale response without duplicate admission,
  replaced key or pin. Preserve atomic Start and Paired commits.
- Verify an unpaired fresh Vault opens for local work. Run coherent Rust/FFI/Go/
  Flutter gates and Apple build qualification after the cutover. The current two-
  cycle test remains failed until this implementation is actually qualified.

This observed user-flow correctness issue is addressed before extending Gateway
file encryption; the latter remains a required unfinished production gate.

### Corrective design review decisions

The single Opus review of 7c3c88fe supports operation-scoped keys. Accepted
corrections: sign enrollment and AssistantView inside the same Immediate transaction
that checks live authority and writes the receipt, as product signing already does.
No transaction spans provider/model I/O. Keep a post-commit expected-issuer check
for ordinary authorization as an additional release fence; it is not a replacement
for atomic signing admission. Bind accepted Start responses, activation, credential
readback and signing to the exact operation's admitted public key.

Unpaired means no live signing authority, not an empty key table. Cancel/reject/expiry
can restore Unpaired or an earlier Forgotten expectation while historical keys
remain. Startup validates only the current referenced Pending/Committed key;
Forgotten receipt validation and historical recovery do not decrypt retired keys.
Private unwrap is limited to current-key validation and live signing. Malformed
authoritative public metadata still fails closed. AAD uses a fresh context and length-prefixed Person/device/op/issuer
fields. Private-key ID and public-key columns are unique. Generate only after valid
fresh Start admission inside its transaction; no key is generated on receipt replay
or a rejected command. Generation/wrapping are local CPU/random operations.

The old physical Vault schema is incompatible. Existing files and keys stay
untouched and old Vaults return UnsupportedVersion; they are not silently adopted,
reset or migrated. The earlier R1 preservation promise covered that checkpoint's
ceremony-only change, not this later stored-key meaning change. Continuing against
the new schema requires an explicitly selected fresh local test profile. Preserve
old profiles and unresolved external-operation evidence before any user-selected
reset. No real Mac profile reset is part of qualification.

Local Forget does not revoke the remote client. Two-cycle success is not unbounded
re-pairing: Trust retains client/issuer and receipt caps. Operator revocation also
creates person-wide cleanup and advances Trust revision; do not automatically revoke
an old client while new enrollment is pending. Remote cleanup UX remains separate.

The Access AuthorizationSigner.public_key method has no runtime callers (repository
residual search); remove it instead of introducing a second public phase lookup.
Internal committed-issuer resolution returns PolicyDenied for Pending/Unpaired/
Forgotten, never absence or a pending key. Keep provider/liveness fallback semantics.

Qualification additionally covers staged Cancel then reopen, Cancel B restoring
Forgotten A then reopen, rejected concurrent Start with no extra key, wrong issuer
at response/activation, and preservation of old-format profiles.

### Enrollment-scoped cutover implementation evidence

The singleton table, seed, loader/validator and both unused public-key port methods
are removed. Pairing private readback projects the admitted issuer; new Start
stores unique key/public identity and authenticated wrapped bytes atomically.
Accepted responses, activation, private enrollment reads and committed authority
verify the same operation-scoped issuer. Enrollment and AssistantView signatures
now execute inside their receipt/authority transaction. Product signing resolves
its key inside that transaction as well; ordinary signatures additionally compare
the exact expected committed issuer after commit.

Disposable owner probes passed exact Start replay, pending restart, rejection with
no extra key row, staged Cancel/reopen, cancellation restoring an earlier Forgotten
expectation/reopen, fresh issuer per Start, wrong Person/device and cross-operation
ciphertext rejection, wrong admitted issuer rejection, pre-confirm denial, exact
signature replay, denial after Forget with historical receipt retained, and late
old approval preserving a newer Pending operation. A prior singleton-shaped schema
returns UnsupportedVersion with identical database bytes and key files. The synthetic
proof verifier in the owner probe does not qualify strict wire decoding; that is
covered separately by the actual Go/Rust flow.

Two real isolated Go development server plus Rust FFI cycles now pass pairing,
explicit administrator approval, catalog observation, Forget and encrypted-client
restart. The second server issuer is genuinely new rather than a relaxed Trust
check. The first trial reached second approval but encountered a contended Forget
acknowledgement; the fixture was corrected to retain and replay an identical command
envelope on an indeterminate/admitted response. A complete subsequent run passed.
No real user profile, Keychain or live Mac runtime was touched. Full workspace and
Apple qualification of this cutover are pending at this checkpoint.

The completed enrollment cutover also passed the default Rust workspace final gate
(including doctests), default OS-keyring FFI build, development FFI build and the
architecture policy check (23 nodes, 126 allowed edges). Native same-snapshot
builds and independent implementation review are the remaining qualification for
this checkpoint; no production-wide storage completion is claimed.

### Gateway payload encryption — concrete cutover

Node composition owns one process-locked root admission and purpose-specific root
key slot selected through the compiled credentials provider. A create-only public
identity/initializing marker precedes first key creation; key readback and an
AEAD-authenticated root seal precede Ready. Existing incomplete, unmarked populated,
wrong-key and missing-key roots fail closed without adoption or replacement. The
normal server owns the lease until owner shutdown; a narrow explicit local
--print-admin-token command reads an existing Ready root without starting owners,
creating directories/keys or acquiring the writer lease.

The storage adapter exposes one scoped encrypted Files implementation (bounded
Read/Write/Exists/Scope). AAD binds format, root ID and canonical logical file path;
random AES-GCM nonces and existing atomic fsync/rename durability are retained.
Owners keep their schema/validation/policy, receiving a storage capability instead
of concatenating paths. Trust, Integrations, Inference and Gmail all migrate in one
change. Gmail's duplicated file helpers retire; its reset writes an empty encrypted
index durably, and uncertain writes poison subsequent reads/writes until reopen.
History checkpoint reads propagate unavailability rather than fabricating absence.
Raw private-file utilities remain only for public root/profile metadata and the
explicit development credential backend. Logs and operator-provided env input are
not silently encrypted/migrated or deleted by this change.

Qualification uses fresh synthetic roots and credentials only: ciphertext absence
of sentinel payload, restart, absent/corrupt/wrong key, wrong-purpose/root swaps,
truncation, incomplete creation, lease exclusion, readonly token retrieval without
initialization, and actual pairing after server/client restart. Old plaintext test
profiles remain untouched and require an explicit fresh profile. No live provider
credentials, mail, Mac app, or keychain prompt are part of these probes.

Independent Opus implementation review of f0236f54 found no high-severity defect
and verified the operation/issuer/transaction/revival fences. Follow-up hardening
uses public issuer metadata (no private unwrap) for response/activation/transport
readback, rechecks enrollment expiry inside its signing transaction, and classifies
Turso Busy/BusySnapshot query failures as Conflict rather than a whole-Vault integrity
latch. Commit/rollback uncertainty still latches. Malformed authoritative metadata
remains fail-closed. Agent Vault old-format errors map through UnsupportedVersion
at the existing FFI boundary; product-store UnsupportedSchema is a different code.
Exact f023 Apple Debug/Release and arm64 iOS simulator builds, strict Mac signatures
and profile getters passed; no live app was launched. Intel packaging remains
unqualified because embedded libraries are arm64 only.

Gateway encryption implementation now uses one Node root-admission lease and scoped
Files capability throughout Trust, Integrations, Inference and Gmail. The previous
Gmail source-handle-as-filename coupling is removed: its connection-scoped storage
uses index.json, while public source handles keep their existing semantics. Gmail
checkpoint/read/cleanup paths fence a failed write; Reset persists an empty encrypted
index instead of deleting an uncertainty-bearing file. The normal server never
prints tokens. Explicit --print-admin-token is read-only against an existing Ready
profile, and run-local prints the matching binary invocation, not the credential.

Disposable Go race probes passed encrypted payload roundtrip/absence of plaintext,
bounds, cross-purpose/root/scope and wrong-key/truncated-file rejection, corrupt-slot
write refusal, path/symlink/read-only guards, scoped closure, live read-only token
retrieval, writer exclusion, restart/token stability, missing/malformed/wrong root
key refusal with no regeneration, incomplete admission preservation, and old
plaintext-profile refusal. Gmail roundtrip/reopen/empty reset and failure fencing
also passed. One fixture exposed an invalid source-handle filename at the new storage
boundary; the implementation now uses the canonical scoped index path. An actual
isolated Go+Rust flow passed two pairing/approval/Forget cycles with both client and
server restart, encrypted server files, stable administrator token and fresh issuer
keys. No live credentials/provider or Mac runtime was used. Default and floe_dev Go
race/vet gates passed with the disposable fixtures; S3 reconstruction remains deferred.

Final combined source passed Rust workspace/doctests, both FFI profiles, owner review
closure probes, encrypted product/typed-error probes, Go default/development race
and vet, architecture boundaries and shell syntax. The latest encrypted client/server
pairing sequence passed both cycles and server restart. A preceding trial returned a
durable NotApplied/Conflict on Forget; the fixture checks exact negative replay and
permits a new explicit intent only after refreshing the same still-paired target.
Unknown/admitted responses never get a new identity. The intermittent Conflict's
operational UX remains part of command-observation convergence, not silently counted
as uninterrupted success. This final run completed without that conflict. Independent
server-storage review and same-snapshot Apple/server-native builds remain pending.

### Connections presentation metadata and observation correction

Next root-owned slice restores account grouping from actual EventKit metadata. The
native catalog must expose calendar title and an explicit source/account identifier
and label; never split a concatenated display label. Connections retains optional
resource-group presentation alongside its selected resource labels (ungrouped
resources are a real state for other connectors). Product projections derive an
opaque Person/Connection-scoped group reference, never exposing native identifiers
or using a display group for read/processing/write authority. Reviewed choices and
selected summaries carry the same safe group shape. Calendar detail restores baseline account columns/groups. The baseline selector
uses flat checkbox rows, so preserve that layout and compose its visible account/title
label from explicit metadata, with exact reviewed resource identities;
freshness remains Day-owned and is not invented from account metadata.

Also remove the fabricated overview-wide revision end-to-end. It is only the maximum
of unrelated record/source revisions, not a monotonic aggregate version, and Flutter
currently drops valid observations if that maximum falls. Keep per-item revision/CAS
and client request-generation ordering. Initial empty Calendar/Contacts selection is
a controller-lifetime presentation offer keyed by exact source identity, so widget
recreation does not repeatedly reopen it. Neither change mutates grants or starts
owner commands from a pure overview query. Future-calendar selection policy and the
old global assistant Calendar preference remain separate unresolved semantics.

### Storage review adjudication and corrective boundary

Opus found no production fallback/key regeneration/purpose-binding defect. Its
conditional Rust corruption-downgrade concern is not present: the actual pre-existing
storage() maps every non-Busy database error to StorageUnavailable and the finisher
latches both StorageUnavailable and VaultUnavailable. Duplicate/incomplete Busy
handling and the resulting command semantics are real issues: finish_product_command
can record a durable negative for a contention-originated Conflict. Replace that
conflation with typed StorageBusy across the Gateway repository/Pairing boundary.
Busy preserves the exact pending command without writing a rejection receipt or
retiring the Vault. Semantic Conflict and commit/rollback uncertainty stay distinct.

Gmail must retain its integrity/indeterminate-write fence, while a determinate
filesystem failure such as ENOSPC must remain retryable rather than permanently
blocking later cleanup. The storage codec distinguishes integrity failures. Separate
per-logical-file write locks from the shared root lifetime lock; advertise root
unavailability at Node admission and current Trust/configuration checks. Give Trust,
Integrations, Inference and connector factories distinct root scopes. Trust creation
requires the newly admitted root, so losing all Trust files cannot regenerate an
identity inside an existing root. Read-only token retrieval enters only Trust scope.

Production login-Keychain ACL behavior across changed Go binaries remains unqualified
and requires an explicit Mac runtime ceremony; no signing purchase, weakened ACL,
Data Protection switch or fallback is inferred. Preserve typed non-secret startup
categories for locked/denied, busy/timeout, absent/malformed key and root integrity.
Move the initial attempt marker to immediately before first Put, after successful
read-only slot preflight: a locked/busy Get with no attempted mutation need not strand
a new root. An uncertain Put still preserves its initializing marker and exact slot.
The authenticated-file threat model excludes per-file as well as whole-profile
rollback and same-user compromise. Existing marker/lock metadata is not claimed
byte-identical, while user payloads and keys remain preserved on failed admission.

Exact c5f8295c macOS Debug/Release, arm64 iOS simulator and both Darwin Go compilation
gates passed. Pure profile getter/signature checks passed, with no execution of Go
binaries, app/core, simulator or Keychain access. These builds predate the corrective
changes above; native requalification follows the next coherent checkpoint.

Corrective checkpoint verification: the full Rust workspace/default examples/doctest
command, both production and development FFI builds, Go default and floe_dev race/vet
commands, architecture boundary checker and diff checks passed. Existing repository
suites remain removed under T0; these gates do not imply S3 reconstruction. Disposable
owner probes verified Busy cannot create a negative receipt, a held same-process
Vault writer permits exact-command retry after release, resource grouping preserves
source authority, and native Calendar rejects incomplete account metadata. External
Go probes verified scoped storage, missing Trust refusal, read-only token retrieval,
non-stranding read-only custody failures, and retry/cleanup after a real pre-rename
EFBIG without corrupting the prior ciphertext. Temporary probe code was preserved
outside the repository and removed before final gates.

Actual isolated Go/Rust encrypted pairing completed two enrollment/approval/Forget
cycles with client and server restart, distinct issuer keys and stable administrator
token. Naturally occurring Forget contention retried the identical envelope once
and succeeded without a new command ID. Product encryption, missing/wrong key refusal
and five typed startup diagnostics passed. An attempted separate-process Turso writer
probe was rejected by the engine's process lock; it was not bypassed or counted as a
pass. Contention injection instead used the same-process owner probe described above.

All 58 external Flutter fixtures passed after retiring obsolete fixture expectations
for local code-confirmation and generating the pairing projection from the current
Rust/Go flow. Analyzer retains 157 pre-existing informational findings, no errors or
warnings. Calendar groups were rendered and inspected at 390 and 1024 logical pixels
with actual fonts/icons; the baseline flat selection dialog still saves exact
resource references. This is partial R2 restoration, not full UI parity. Native Apple
builds for this corrective checkpoint and production Keychain rebuild/restart runtime
qualification remain outstanding. No live Mac app/server or user profile was touched.

### Service detail identity presentation

Restore the baseline icon/name/description header in both connected and disconnected
details from the same pure presentation mapping as the service grid. Apple Calendar
copy uses the validated service kind and actual platform, never display-name matching;
hosted Calendar stays hosted. This component owns no permission/readiness workflow.
Keep connected status visible on its own wrapping-safe row at narrow widths. The
removed read-only pill stays removed. System-access observation and Day freshness
must not be inferred from source availability; those still need owner-qualified
projections before their baseline sections can return.

Header checkpoint: the existing 58 external fixtures and three additional Apple
platform/hosted-identity/narrow-header probes pass. Analyzer remains at 157 existing
informational findings, no errors/warnings. The 390/1024 screenshots were re-rendered
and inspected with the actual fonts/icons. No Rust/Go/native inputs changed after
the preceding full gate; Apple application rebuild remains pending for this UI-only
checkpoint. No system permission claim or old read-only pill was reintroduced.

### Second storage review: root adjudication and next corrective order

The exact 38e review confirms the Gateway/Pairing Busy fix and Go lock/scope/write
corrections, but identifies adjacent gaps that root verified in source:

1. Root Ready is published before first Trust persistence. Delay publication until
   Trust bootstrap succeeds, preserving initializing evidence on failure; distinguish
   the owner-scoped layout explicitly and reject older layout before making scopes.
2. Access and other Vault SQL boundaries still conflate Busy with semantic Conflict.
   Audit the real owner paths rather than claim crate-wide closure from Gateway-only
   tests. Assistant signing also rejects its own previously committed operation after
   a lost post-commit acknowledgement; exact replay needs current-authority validation
   and immutable request matching, not a new request identity.
3. Calendar title-only metadata reaches Day's calendar_name projections outside the
   group-aware Connections views. Preserve account-qualified presentation there too.
   The pure SourceConnection transition advances source authority only for resource
   handle/policy/fingerprint/state changes. However, the application still routes any
   changed metadata through ConnectionConfigure and invalidate_source; this broader
   workflow remains open and the domain-only probe does not prove Observe retention.
4. Native Calendar must explicitly handle an absent EventKit source without a crash.
   Missing/ambiguous account metadata stays unavailable, never assigned to a fabricated
   account. Catalog failure must not silently shrink the allowed set.

Production Keychain runtime remains unqualified. The credentials adapter does impose
its own deadline, which must be verified before accepting the review's conditional
unbounded-wait allegation. Preserve raw evidence and do not broaden runtime QA.

Verified additional review details: Actions' live EventKit destination projection
already composes account and calendar titles; the confirmed missing qualification
is in the two Day calendar-name projections. The Keychain observer has a three-second
deadline and retains its native mutation lane until readback, so an unbounded caller
wait is not present. Native ACL behavior itself remains unqualified.

### Incremental macOS signing dependency correction

Exact 0386 UI-only builds compiled, and each nested App.framework verified alone,
but the enclosing app seal still referenced the preceding framework. Both build
logs contain Flutter framework signing and no Runner CodeSign task; exact 38e native
change logs contain the enclosing CodeSign task and passed strict verification.
Declare the actual embedded App.framework and FlutterMacOS.framework outputs on
the existing Flutter embed phase so Xcode can order and invalidate enclosing signing
from the build graph. Preserve Xcode's configured identity/entitlements and normal
signing owner; no post-hoc deep re-sign or cache clean. This hypothesis requires
strict verification after both the initial build and a second Dart-only incremental
build with unchanged Swift/Rust/project inputs.

Reference: Apple's Run Script guidance requires declared input/output dependencies:
https://developer.apple.com/documentation/xcode/running-custom-scripts-during-a-build

Bootstrap/label checkpoint: isolated probes passed for interrupted-before-Trust
initializing state, typed old-layout refusal before scope creation, no key replacement,
read-only token/restart, and cached Trust becoming unavailable after detected ciphertext
corruption. Account-qualified labels preserve punctuation and the existing 256-byte
Day bound; overlong combined labels are rejected, never truncated into ambiguity.
Rust workspace/doctests, both FFI builds, Go default/dev race/vet, architecture/diff
gates and the two-cycle actual encrypted pairing/restart + product-encryption/startup
diagnostics passed. Temporary probes were archived outside the repository before
final gates. Apple source guards and the incremental signing dependency change still
require the next native qualification; broader Busy/assistant replay and metadata-only
configuration workflow findings remain open.

### Storage contention convergence and live assistant receipt replay

Move the typed Turso-to-AgentFailure mapper from the Gateway-specific adapter to
the Agent Vault boundary. Migrate SQL begin/query/execute/next operations across
Access, Context cleanup/dependencies, Session/Conversation, Registry/Learning/Tasks
and pairing/authorization repositories. Keep value decoding separate and preserve
commit/rollback uncertainty in transaction finishers. Actions' storage port and the
Connections source-operation port gain explicit StorageBusy rather than Conflict;
source settlement and the physical source negative journal both refuse to create
a negative receipt from Busy. Failed source rollback is now reported as unavailable,
not silently discarded. Startup product-store schema diagnostics retain their own
admission contract; this is not a claim about every Day runtime error projection.

Gateway credential admission preserves transient unavailability/timeouts/cancellation
instead of converting every read failure to PolicyDenied and terminal repair.
Assistant signing replays the same receipt only when all stored intent fields match;
it still verifies current enrollment, grant, producer proof, release lineage and
expiry before deterministic signing. A stored receipt never restores old authority.

Disposable probes cover a held same-process writer against Pairing, Access cleanup
and Actions authority change, followed by exact Pairing/Actions replay after release;
source DB contention and the absence of a Busy negative receipt; and an actual SQL
assistant receipt's exact/mismatched field matrix. The latter qualifies receipt
matching only, not a complete live Assistant authorization workflow. Full final
gates and independent review follow this checkpoint; metadata-only configure still
needs a distinct display-metadata path rather than invalidating Observe.

Convergence checkpoint qualification: the three temporary owner/SQL probes passed and
were archived/removed before the final workspace/default-example/doctest gate. That
gate and both FFI profiles passed. Actual encrypted pairing/approval/Forget/restart,
product encryption/key refusal, typed startup failures and session reopen passed;
one natural Forget contention retried the exact command and completed. All 61
external Flutter fixtures passed against the current Rust pairing projection. Dart,
Go and native source inputs are unchanged from the preceding qualified snapshot;
required Apple rebuilds for the new Rust artifact and focused independent closure
review remain pending. The product connection-opening path also preserves the typed
Busy code before projecting it to the source repository, rather than laundering it
through generic storage or semantic Conflict.

Exact a770 native qualification separately passed macOS Debug/Release strict signing,
arm64 iOS simulator and Darwin Go production/dev compilation. Its synthetic Dart-only
incremental builds and restored-original builds both triggered Runner CodeSign and
passed strict whole-app verification without re-signing workarounds or cache cleanup.

### Third review adjudication: uncertainty admission and display bounds

Root confirmed the Source DB commit-uncertainty gap. Add a physical store unavailable
latch and a source-journal writer guard held through commit or latch publication. A
queued negative writer must not enter the gap between SQLite releasing its lock and
the caller observing a failed COMMIT. Failed commit/rollback closes new product-store
connections until an explicit reopen; all five source-journal mutation paths use the
same guard and finisher. No negative receipt can be written through the closed store.
A deterministic failed-COMMIT probe checks admission closure and unchanged data after
reopen; it does not assume unverified Turso fsync or drop semantics.

The combined account/title limit was a Day presentation constraint, not stored source
integrity. Remove that constraint from ConnectionResource validation. Day's own
bounded calendar-name allowance becomes 1024 bytes to carry both independently
bounded 256-byte labels intact; identifiers and overall acquisition/snapshot budgets
are unchanged. Long labels remain display metadata, never a resource identity.

The remaining Registry query/next and startup SQL begin/read mappings now use the
typed database failure mapper. AccessService's reviewed snapshot/receipt/commit paths
were inspected and propagate repository errors; their Conflict conversions apply to
pure grant transitions, not caught storage errors. The concrete Gateway verifier
derives expectation, purpose and key identity from the signed canonical challenge;
consumer kind additionally remains bound to the currently admitted grant. Server
StageResult consumes an assistant admission once. Mirror that single-release rule in
the client signer: exact receipt replay remains allowed, but a second release
operation/challenge for one admission conflicts.

The review's request to commit permanent regression suites and grep gates is deferred
under the user's explicit T0/S3 sequence and repository guidance. Preserve the natural
language cases and disposable evidence; do not override that instruction. Plaintext
startup diagnostic wording was corrected to profile_invalid, distinct from encrypted
old-layout unsupported_layout. Whole runtime QA, metadata-only configure separation
and production Keychain ACL qualification remain open.

The source writer guard is armed before BEGIN and settles only after the SQL finisher
returns. Dropping an armed future latches the store before the mutex releases, closing
the cancellation gap as well as the explicit COMMIT-error gap. A determinate Busy at
BEGIN disarms without retiring the store. Probes passed for forced COMMIT failure,
armed-future drop, no resulting negative receipt, exact data/key reopen, long Unicode
account-qualified labels, the Day display limit, and single assistant release with
exact-receipt matching. Permanent suites remain S3 work; probe code is archived
outside the repository before final gates. Exact 378 Apple builds/signatures and
profile getters separately passed; the new guard/label/release checkpoint needs its
own native artifact qualification.

Uncertainty checkpoint final gate: workspace/default examples/doctests, production
and development FFI, architecture/diff checks passed after probe removal. The actual
Go/Rust encrypted two-cycle pairing/restart flow passed with one exact Forget retry
in each cycle, as did product encryption, key/error preservation and session reopen.
All 61 external Flutter fixtures passed. Go executable inputs and Dart inputs are
unchanged; their previously passed gates apply. Native Rust artifact requalification
and a focused review of the cancellation/commit fence remain pending.

### Product journal analogue and bounded recovery

The focused review verified the Source guard, then identified the analogous product
journal gap. Root traced its complete positive/negative write set: product records,
Access review admission, and the pairing Forget path, plus the related Gateway
authority/receipt writers. These 16 Immediate transaction paths now share the Vault
journal mutex and an armed drop fence through their existing SQL finisher. Deferred
read transactions are unchanged. The physical guard is factored once for the two
encrypted stores; it contains no authorization or command policy. This does not
claim every unrelated Vault mutation now uses that journal gate.

Root also replaced the one-shot SourceConfiguration/recovery handling of StorageBusy
with bounded backoff. Every retry uses the same operation and reloads the durable
record; already-completed SourceMutation records terminate without new work. Only
determinate Busy retries, and the existing owner deadline/cancellation remains the
boundary. Database corruption now retains VaultUnavailable through the common typed
mapper rather than losing its integrity category.

Disposable probes passed for both stores' uncertain commit/drop fences and absence
of a manufactured negative receipt after reopen, corruption versus Busy categories,
three independently contended recovery jobs, no retry of semantic denial, and the
retry deadline. The recovery-helper probe is not full live multi-source activation
QA. Temporary probe modules were archived/removed before final gates.

Shared-journal checkpoint final qualification: after removing the temporary probes,
workspace/default examples/doctests and both FFI profiles passed; architecture and
diff checks passed. Actual encrypted pairing/approval/Forget/restart completed twice
with an exact-command contention retry, and product encryption/key refusal/startup
classification/session reopen passed. All 61 external Flutter fixtures passed. New
Apple Rust artifacts still require the exact-snapshot build/signature gate. No live
Mac application/server or production Keychain runtime was exercised.

Exact 478182e Apple qualification passed: macOS Debug/Release builds, strict deep
signature verification and storage profile getters (2/1), plus unsigned arm64 iOS
simulator build. The isolated checkout stayed clean with unchanged lockfiles.
Release Runner is universal but its Floe libraries remain arm64-only; Intel runtime
is not qualified. These builds do not establish actual app/provider behavior or
production Keychain ACL behavior.

The focused single Opus review found no blocking defect in the supplied shared
journal fence, its 16 writer paths, Source guard extraction or bounded Busy retry.
Root confirmed the documentation understated the retry scope: reconciliation,
including ReviewApply, also retries Busy using the same reservation and live checks.
The current architecture now states that scope and the conservative non-Busy BEGIN
retirement behavior. Possible foreground signing contention, native permission
outcome recovery, read-only integrity/latch differences and explicit in-process
reopen UX remain separate availability work; this checkpoint does not claim them
closed. No additional production code or permanent test suite was introduced by
this qualification/documentation closure.

### Next bounded cutover: source presentation without grant invalidation

Root traced the rename path through SourceConnection, reviewed configuration,
SourceOperationChange, the SQL successor/operation CAS, Context standing-source
checks, Day inventory digests, and Actions/native final fences. SourceAuthority
already distinguishes authority changes from label changes; the application loses
that distinction by treating every changed successor as InvalidateSource.

Chosen boundary: retain one Connections source record and its local CAS revision.
Do not introduce a competing presentation database or bypass the durable command
journal. Add an explicitly local presentation operation to the existing source
operation owner. Its terminal proof is an owner-validated presentation successor,
not a fabricated Access receipt. It may change display labels/group labels only;
identity, selected handles, group identity, mode, native subject, lifecycle and
SourceAuthority remain equal. A local revision still advances. Exact reviewed
snapshots and in-flight Actions may therefore require fresh observation when their
shown data changed, while standing Observe grants must remain untouched.

Ordered file cutover:
1. source.rs: expose a pure validated presentation-successor predicate so the
   application and transaction validator use the same semantic classification.
2. domain/source_operation.rs: represent the local presentation operation and
   terminal phase/proof explicitly; validate its Reserved-to-terminal CAS and
   reject grant proofs, authority changes, unrelated source or missing successor.
   Local cancellation may terminate Reserved without an Access call.
3. application/product.rs and source_operation.rs: classify the reviewed successor,
   reserve the original immutable command, recheck native subject before local
   completion, atomically apply successor plus terminal operation through the
   existing repository, and settle the original product record. Recovery reloads
   the same operation; cancellation must not invent grant commits or aborts.
4. Keep source SQL writes on the existing shared uncertain-write guard/finisher.
   No new storage table, external route, FFI command, grant scope or permission UI.
5. Audit all source-operation phase/kind/proof matches and downstream label users.
   Preserve Day calendar labels and exact resource/account identity. Document the
   local revision versus SourceAuthority semantics at the current owner boundary.

Disposable verification must cover preserved active Observe, unchanged source
identity/authority, rejected resource/mode/account/subject expansion, exact replay,
source/operation atomic commit, interruption/reopen, cancellation and conflict.
The existing actual encrypted flow, Flutter fixtures and coherent Rust/Apple gates
follow once this slice is structurally complete; permanent suites remain S3.

The bounded implementation now extracts source successor execution/recovery from
product.rs into private application/source_configuration.rs. Shared classification
validates the complete successor and exact group identity; local presentation
proofs cannot use grant phases or mutate authority. Existing SQL operation/source
CAS supplies the atomic write and uncertainty fencing without a new table or public
wire route. Two disposable probes passed for local commit/replay/reopen/cancellation
and rejection of resource, mode, account and subject changes. These are owner/store
probes, not yet a full active-Observe product workflow or live Calendar QA.

Presentation slice local final gates passed after temporary probe removal:
workspace/default examples/doctests, production and development FFI, architecture
23-node/126-edge policy and diff checks, two actual encrypted Go/Rust pairing and
restart cycles (one exact immutable Forget retry per cycle), product encryption and
key-preserving startup diagnostics, session reopen, and all 61 external Flutter
fixtures. The deliberate wrong-key probe emits an AES authentication failure before
passing its refusal/preservation assertions; this is expected negative evidence.
Apple exact-artifact qualification and the independent targeted review follow this
commit. The active-Observe full product scenario and live Calendar remain unproved.

#### Review correction: remove the presentation reservation window

Exact 5663561 Apple artifacts passed, but the independent review confirmed that a
presentation Reserved fence could survive a failed native observation, and that
aborted/superseded product commands could remain pending. Supersede the two-step
presentation plan above: observe outside SQL, then atomically settle the original
presentation command and source successor in the source journal. Presentation must
never publish a durable Reserved row. Positive and negative outcomes share the same
writer admission so a concurrent rejection cannot overtake a committed effect.
The Access-coordinated authority workflow keeps its reservation protocol.

Delete the local Presentation proof/Reserved-to-terminal CAS path. Introduce one
owner-defined settle_presentation repository command: exact replay first; reject
while another source operation is fenced; otherwise validate current source CAS
and write either PresentationCommitted with its successor or Aborted with no source
change in the same transaction. Native observation cannot hold a source fence.
A terminal product rejection projects NotApplied; a terminal source commit settles
from its historical recorded successor even if a newer source mutation has occurred.
Foreground and recovery races reload the winning immutable product result. Start a
leased recovery job for every admitted source configuration, before foreground drive.
Group identity joins the same scope identity comparison used for SourceAuthority.
No compatibility path for old in-flight development records or automatic reset.

The atomic local path replaces the presentation proof/CAS route, rather than keeping
both. Three disposable probes passed: atomic apply/reopen and exact replay after a
newer rename; reject/apply races returning the first settled outcome; no source fence
on local completion/rejection; authority-resource/account/subject/mode changes
rejected; and an active authority reservation returning Busy without adding a row
or changing its watermark. Group-only identity changes now advance SourceAuthority.
Temporary probes were archived and removed. Generic authority completion must still
prove Completed, not RepairRequired, before historical product settlement. Full
active-Observe product workflow/live Calendar behavior remain outside these probes.

A fourth disposable check passed for rejecting foreign/absent source identity even
on the negative settlement path, preventing a false journal watermark for another
source. The current source identity must exist and match before either local
outcome is written; revision drift may produce a known same-source rejection.

Atomic-presentation local final qualification passed after all temporary probes were
removed: workspace/default examples/doctests, production/development FFI, architecture
23-node/126-edge policy, diff check, actual encrypted two-cycle pairing/Forget and
server/client restart, product encryption and key refusal diagnostics, session reopen,
and all 61 external Flutter fixtures. Exact Apple artifacts and a focused closure
review of the one-transaction settlement are still required for this new snapshot.

#### Driver ownership and authority-settlement follow-through

The 13b0dab review closed the atomic presentation path but found that unconditional
background spawn made the older authority driver's race frequent. Replace parallel
foreground/background execution with the same per-command job lease: a foreground
owner drives once, another observer only reads, and an interrupted/erroring foreground
releases its lease before handing unfinished work to recovery. A dropped foreground
future uses the same handoff; shutdown prevents new handoffs. Recheck owner admission
before local settlement/product CAS. Completed authority operations skip further
Access invalidation and settle their historical result.

A durable source fence is semantic, not SQL Busy. Inspect it cheaply before native
observation and return a non-retried conflict if it appears at the transaction edge;
only actual storage contention retains the rapid Busy retry. Pre-reservation authority
drift settles through the existing source-journal negative command receipt, which
atomically excludes a concurrent/prior positive reservation, then the same terminal
product NotApplied result. Never infer that rejection solely from a prior read of
operation absence. Existing genuine authority operations retain their receipt protocol.

Five disposable source/store probes passed, including semantic-fence refusal without
a new row and a definitive negative authority admission excluding a future reserve.
One lease primitive probe passed for foreground/recovery exclusivity and release;
this is not a full fake-provider cancellation workflow. Temporary probe modules were
archived/removed. Root additionally traced replay after a negative receipt succeeds
but product settlement fails: the driver consults the existing negative receipt
before mutable evidence, so recovered evidence cannot resurrect the command.

Driver-ownership follow-through final local gate passed after removing probes:
workspace/default examples/doctests, both FFI profiles, architecture/diff checks,
actual encrypted two-cycle pairing/Forget/restart, product encryption/key refusal
and session reopen, and all 61 external Flutter fixtures. Exact Apple artifact
qualification and targeted N1–N5 closure review remain pending for this snapshot.

#### Narrow closure corrections

The 0d97ac35 source review closed driver exclusivity, semantic-fence handling,
completed-authority no-reinvalidation and the atomic presentation proof. Root
confirmed its two remaining edge cases: native drift is surfaced as an error rather
than a mismatched successful observation, and owner admission must be rechecked
after native I/O. Treat only AccessReviewRequired as a definitive native-drift
rejection before reservation; other failures remain pending. Re-admit before reserve
and Access invalidation. Register the existing directional child cancellation scope
for foreground ownership, so owner shutdown cannot cancel a caller/Run upward;
parent cancellation still reaches the child. Observer polling becomes 100 ms.

The older post-invalidation authority-repair/no-product-exit path is not fixed by
this bounded presentation change and remains explicit follow-up in the source
configuration/Observe workflow cutover. Do not claim all Connections recovery or
full live active-Observe qualification is closed. Old 566-era in-flight presentation
profiles are not migrated or reset automatically.

Two disposable probes passed for definitive-versus-transient drift classification
and actual ExecutionScope child cancellation direction. Probe modules were archived
and removed before the final gate. These are focused primitive checks, not a full
shutdown-mid-native-I/O or live-provider scenario.

Narrow admission/cancellation local gates passed: workspace/default examples/doctests,
both FFI profiles, architecture/diff checks, encrypted pairing/restart, product
encryption/key refusal and session reopen. Dart/Go inputs and wire shapes did not
change, so their previously passed gates (including 61 external Flutter fixtures)
are reused rather than rerun. Exact new Apple artifacts and focused delta review
remain pending.

Exact 92ab5393 Apple qualification passed: Debug/Release builds, strict deep
signatures, profile getters 2/1 and unsigned arm64 iOS simulator build; isolated
checkout/lockfiles stayed clean. The narrow Opus delta review closed the primary
R1–R3 findings and found no serious delta regression. Its cancellation-boundary
recommendation is a defensive post-observation scope check; retain it for the next
Connections change rather than treating the review as full runtime acceptance.

Root inspected the omitted concrete paths: run_bounded prioritizes cancellation;
Calendar/Attention brokers recheck after receive; Calendar metadata validates
fingerprint syntax as PolicyDenied; Personal acquisition may classify invalid or
changed binding as AccessReviewRequired. Thus that error means renewed source review
is required, not proof of the physical cause being exclusively fingerprint drift.
No finalization_scope calls exist in the inspected Access/source-evidence paths.
These source findings do not replace live cancellation/permission tests. The older
post-invalidation authority repair/no-exit issue remains explicitly open.

### Next R2 presentation investigation

The baseline System access card was a read-only OS-permission observation, distinct
from Floe feature permission. Its old implementation inferred permission by listing
calendars with requestAccess=false; do not revive that private-data read just to show
an access badge. Current Apple bridges expose only readAcquisition, and the current
SourceConnectionPanel has no truthful standalone OS access observation. Calendar
source readiness cannot fill that gap. Investigate a narrow OS-status-only boundary,
its macOS/iOS implementation, composition injection and baseline card/resume states;
keep actual permission requests in the admitted native setup flow. No such new port
or UI implementation has landed yet. Day freshness is a separate Day-owned join,
not something to manufacture from this permission badge or source local revision.

#### System access card cutover

Restore the baseline macOS-only Calendar System access card in both connected and
unconnected native Calendar detail. Introduce one narrow Flutter application port
for OS authorization observation and explicit settings navigation; its native
MethodChannel adapter returns only an authorization enum, never calendar names,
events, Floe grant state or a permission request. AppRuntime constructs the adapter
and injects the required capability into the presentation controller; the card owns
only mounted/resume/read-generation and navigation feedback. Do not add Rust grant
or query commands, infer permission from SourceReady, or restore old direct calendar
listing. Actual permission requests remain the existing admitted setup path.

Files: a Connections application port, a native infrastructure adapter, a dedicated
CalendarSystemAccessCard, the two detail widgets, ConnectionsController constructor,
AppRuntime composition, and macOS CalendarBridge method dispatch. Keep iOS's existing
layout unchanged (the baseline card is macOS-only), and do not extend Android. Strict
native status decoding, missing plugin/unavailable, stale response, resume/disposal,
settings click/no automatic grant, hosted-service exclusion and narrow/wide rendering
are the disposable qualification matrix. Preserve existing logs and accepted removed
Read-only/source diagnostic UI. Also apply the pending defensive post-observation
scope check before classifying a source rejection in this Connections slice.

System-access slice local qualification: all 71 external Flutter fixtures pass,
including ten new status/resume/disposal/settings/target/render cases. Analyzer has
the same 157 informational baseline findings and no errors/warnings. Narrow (390)
and desktop (1024) standalone and connected-detail renders were inspected with real
fonts; no overflow or unintended grant action occurred. The first extra rendering
fixture stalled on font I/O inside fake-async; font loading was moved to setUpAll
and the final full run passed. No production workaround was used for that fixture.

Rust workspace/default examples/doctests, both FFI profiles, architecture/diff checks
passed for the defensive post-observation scope check. No permanent tests or data
reset were added. Native settings navigation and actual OS status on the user's
running app remain unverified; exact Apple compilation/signature qualification is
next. iOS layout remains unchanged, and hosted Calendar never gets the macOS card.

#### System access review closure

Independent Opus source review of 033f312 found no blocking or serious delta defect.
Root accepted the bounded presentation findings: re-observe OS metadata when an
admitted owner command settles, keep the last observed badge visible during refresh,
stop an invalidated loading indicator on deactivation, and clear Settings-navigation
failure feedback when permission no longer needs recovery (including late failure).
The native status method now maps one authorization snapshot rather than reading it
twice. No permission, grant, source or persistence owner changed.

Root verified the omitted service discriminator: product.rs derives AppleCalendar
only from Device + calendar.event_kit; Gateway bindings are Hosted. The Dart parser
preserves that enum and detail lookup joins integration.source.sourceRef, not labels.
An unlinked source intentionally lacks a fabricated integration identity/card; full
unavailable-catalog/orphan-source UX remains part of the broader screen-state matrix.
Actual Settings pane navigation and OS lifecycle remain live-runtime evidence gaps.

All 74 disposable Flutter fixtures pass after the review fixes, including command
settlement without focus change, retained badge across interrupted reads, and late
Settings failure after permission recovery. Analyzer remains 157 baseline infos,
zero errors/warnings. Rust inputs are unchanged from the prior qualified slice.
Apple qualification of 033f312 runs separately; these later Dart/Swift edits require
qualification of their own final source snapshot before claiming native acceptance.
