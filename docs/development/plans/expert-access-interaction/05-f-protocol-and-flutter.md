# 05-F — protocol, snapshots and Flutter interaction UI

- **Status:** complete at `27299093263fb4998e59f800f663c5e604f22ac3`.
- **Exit:** the real App/protocol/FFI/Flutter path renders, resolves and resumes without frontend authority.

## 1. Existing integration points

- `crates/bindings/protocol/src/dto/commands.rs`, `queries.rs`, Conversation DTO/conversion modules: inspect exhaustive command/query dispatch.
- `crates/app/` admitted Conversation services and read model; FFI owner conversions and native bundled ABI.
- `apps/client/lib/app/runtime/floe_client.dart`, `app_read_model.dart`: prepared command identity, receipts, events and resync.
- `apps/client/lib/features/conversation/domain/agent_session.dart`: new typed Interaction message parsing.
- `apps/client/lib/features/conversation/application/conversation_runtime_gateway.dart:77-270`: existing Run observation/admission retry.
- `agent_controller.dart`, `agent_conversation_gateway.dart`: Session/controller state and explicit commands.
- `apps/client/lib/features/conversation/presentation/agent_panel.dart`, existing proposal card and connection router: safe inline presentation/recovery.

Use the current design system (including squircle primitives) and owner-route conventions. Do not recreate AgentCalendarExpertController or a second connection grant editor.

## 2. Owner wire

Expose a minimal Conversation interaction API with semantics:

```text
get/list interaction snapshots (Session-scoped, bounded, read-only)
resolve(id, expected_revision, decision, stable command_id)
refresh(id, expected_revision, stable command_id)
resume(id/group reference, expected Session revision, stable command_id)
```

Exact operation spelling follows current protocol conventions. Resume generally follows backend admission returned by resolution; an explicit resume action is needed for deferred/newer-Session cases and response-loss reconciliation, not for Flutter to create another StartTurn.

The request must not accept source/connection replacement, raw resource list, grant scope, consumers, purpose, native fingerprint, recipient string, model endpoint, original text or arbitrary parent Run. Those are loaded from the stored reviewed descriptor. Existing owner connection screens keep their own properly reviewed contracts; they do not write Conversation storage.

Decision responses carry the current interaction snapshot and, when admitted, the standard linked Run/command receipt. Errors distinguish stale review, expired/non-actionable, owner recovery pending and hard failure. No automatic frontend retry with newly inspected authority values.

## 3. Snapshot/event semantics after Run completion

Original Run polling still stops when finished. Pending cards must remain inspectable and update after restart without holding that poller open.

Use immutable Interaction message refs plus owner snapshots. A bounded InteractionUpdated invalidation/revision event may reuse the existing App event stream; snapshots remain authoritative. If no push event is needed, explicit result refresh and Session-focus/resume synchronization must still recover all pending/resolved cards.

Update bootstrap/resync to load interaction refs for the current Session and any linked Run receipts. Handle missed/duplicate/out-of-order events by revision, not destructive message edits. Do not infer resolved state from a previously serialized status in a message.

Get/list/inspect do not reconcile mutations or admit Runs. A recovery command is explicit. Screen focus may inspect, but cannot approve.

## 4. Card actions and safe presentation

Render an inline generic card following the assistant explanation. States include Pending, Resolving, Resolved, Denied, Superseded and Expired; linked continuation status is separate from authority state.

Actions come only from the trusted backend projection:

- Allow / Not now for an unchanged inline-eligible scope;
- Review changed source / manage resources;
- supported native permission request or system settings;
- reconnect/open owning connection;
- review exact recipient;
- explicit Continue original request where auto-resume was withheld.

Display safe user-facing account/source/resource or recipient details necessary for informed review, but do not display raw fingerprints, policy epochs or protocol error blobs. Escape provider labels. Resolve navigation via an allowlisted app route and current owner id; never open a URL from model/tool text.

For native recovery, dialog invocation is not approval evidence. After actual native/settings return, refresh current source and explicitly reconcile the interaction. Wrong-device/system capability must be non-actionable.

For a multi-view connection, show the actual affected capabilities; for multi-source blockers, show separate accounts/cards. Recipient consent copy must state this request's recipient and data scope and must not imply LocalOnly or Act is being granted.

## 5. Controller and admission recovery

Use per-card busy state; do not unnecessarily block the composer while a completed Run awaits a decision. Retain prepared command ids across retries. If resolve response is lost, query/rejoin that decision or current snapshot/linked receipt; do not generate a new grant command or a new StartTurn.

When backend returns a linked receipt, observe that Run with the normal event/read-model machinery. Refactor the existing run observer for already-admitted Runs rather than duplicating a second runtime poller. Do not call `prepareStartTurn` with invented `continue` text.

An interaction-producing Run is completed. Do not restore its original composer text as failed input or set generic controller.failure merely because access was unavailable. Keep genuine top-level failures and model/provider integrity errors visible.

Disposing a card/screen stops observation only. It does not cancel an owner operation or the child Run. Explicit cancel targets the admitted child, not the completed origin.

## 6. Tests and deletion gates

Protocol/FFI:

- valid source/recipient/ref snapshots round-trip;
- unknown variants/extra authority fields/forged foreign id rejected;
- same command id/different decision digest conflicts;
- response loss returns/rejoins same linked receipt;
- old Calendar-Expert wires stay rejected.

Flutter:

- parse every state; reject malformed id/revision/action;
- assistant + card, optional-source data + card and no-model deterministic explanation + card;
- double click/busy state, retry same command, stale review requires new user click;
- native denied/granted/settings return and reconnect navigation;
- restart/resync after origin Run has finished;
- event gap/duplicate/out-of-order updates;
- group with multiple cards and later user turn;
- one linked Run/no duplicate User text;
- proposal card coexistence; Allow source access never triggers Action approval;
- widget disposal is not cancellation.

Search footer mappings and failure-string-derived recovery buttons. Remove obsolete expected-blocker-to-global-error paths only after trusted replacement tests pass. Do not remove hard error reporting wholesale.

Run protocol/FFI tests, `flutter analyze`, targeted conversation/controller/runtime/connection/widget tests, full Flutter suite and macOS build from the same source snapshot as the dylib. Record platform skips honestly.
