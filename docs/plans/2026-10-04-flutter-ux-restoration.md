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
- R1 pairing ceremony implementation in progress: owner-driven durable proof progression and obsolete product Confirm removal. Qualification and independent review pending.
- R2–R4 detailed call-site investigation remains part of their pre-edit gates; this plan does not claim every remaining contract decision is resolved.
