> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: conversation, interactions and recovery

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Full-source static extraction; no execution or removal. D = durable safety/property; P = product hypothesis; O = obsolete representation; H = harness. Mixed classification preserves safety meaning without freezing old shape. Entry headings provide source registration/span; the file hash binds all prose to exact baseline bytes.

## apps/client/test/features/conversation/agent_controller_test.dart

Full source read: lines1–139; SHA-256 `d4cba248734a2acec4ca6496395cdcf9d9bedfafd2d6fcfc5cf37d8e32a671bf`.

Current owner: AgentController recovery/presentation / Conversation owner policy. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'vault status failure does not disable a fresh start' (test; lines10–23; D/P)

When Vault status fails, load reports unavailable but keeps a new-conversation escape available. Remove the fake failure and explicitly load(newSession:true); a session becomes available. This is recovery UI availability, not permission to replace missing keys on error.

### 'session load timeout releases the new conversation escape' (test; lines25–44; D/P)

Make resume hang and set a10ms load timeout. Load returns idle with storage_unavailable/needsReload and allows an explicit new conversation. Unhang and start new successfully; observer timeout must not leave UI permanently busy.

### 'interrupted resume requires explicit recovery without model replay' (test; lines46–64; D)

Resume a saved session with abandoned active_turn. Loading requires recovery and disables send; a send attempt begins no model work. Explicit recover runs once, restores sending and still begins no turn. Recovery is not model replay.

### 'load errors do not manufacture retry intent across controller restart' (test; lines66–84; D)

Fail initial load, then recover and complete a turn with model_unavailable. Dispose/recreate controller against the same saved session and reload: canRetry is false. Load errors and restart must not fabricate a fresh retry intent.

### 'source-local review failure preserves the conversation session' (test; lines86–104; D)

A turn reports capability_unavailable/review_source while supplying a usable session. Preserve session, do not require whole-session reload and do not offer retry. Source-local review is separate from storage failure/model replay.

### 'non-read recovery action cannot retry a possibly effectful turn' (test; lines106–120; D)

A possibly effectful turn times out with recovery_action none and no completion session. canRetry remains false; generic deadline failure does not authorize redispatch.

### 'explicit safe read recovery is the only retryable action' (test; lines122–138; D)

A capability failure explicitly provides retry_read and a usable session. Only then enable retry; clear the fake failure and explicitly retry, yielding two total begins and no remaining failure. This contrasts with the two non-read recovery branches above.

## apps/client/test/features/conversation/agent_conversation_controller_test.dart

Full source read: lines1–527; SHA-256 `3d896a13fb54132d3de0fabaa361ad361ce113f50e6e5d17306c4878baf213aa`.

Current owner: AgentController/AgentPanel / ConversationRuntimeGateway. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'starts a conversation without a Calendar refresh prerequisite' (test; lines23–34; D/P)

With a ready conversation fake and no Calendar refresh, load/send one greeting. Exactly one turn starts and no reload is required; general conversation is not gated by unrelated Calendar refresh.

### 'general conversation accepts free-form messages' (testWidgets; lines36–87; P/D)

Render localized/themed AgentPanel, enter and submit Hello Floe. There is one compact composer (height≤50), Ask Floe tooltip, no Conversation title/stock FilledButton, and the exact text reaches the gateway. Render returned Markdown as selectable text, preserving lists/bold, using small block spacing; links have no tap handler, remote image builder is supplied and no Image widget appears. Inert remote content/privacy is durable; exact text/geometry/widget types are presentation hypotheses.

### 'product controller uses StartTurn and explicit Cancel only' (test; lines89–105; D)

Start a held runtime turn, await admission/start, explicitly stop and await completion. Exactly one StartTurn and one Cancel are recorded; the product controller uses explicit cancellation intent.

### 'product retry sends a new command with explicit Run lineage' (test; lines107–126; D)

First turn fails server_model_unavailable/retry_read. Enable an explicit retry, send a second command whose retryOf is the first Run ID, and clear failure on success. No implicit retry occurs.

### 'terminal runtime interruption does not clear the Vault session' (test; lines128–149; D)

A terminal interrupted runtime report without reload/seal flags leaves the session and ready Vault intact and needsReload false. Runtime interruption does not automatically mean storage corruption.

### 'disposing the view does not cancel the backend Run' (test; lines151–164; D)

Dispose the view while a backend Run is held. Cancellation count remains zero; finish the fake runtime separately and allow the pending observer to settle. Screen lifetime is independent of backend cancellation.

### 'owner report preserves reload and seal=$sealSession' (test; lines167–199; D)

Two registered loop expansions cover sealSession=false and true with reloadRequired=true. Both disable sending, preserve interrupted failure, perform no cancellation and exactly one turn. false retains session/messages and ready Vault; true clears session/messages and marks Vault unavailable. Explicit reload later restores session/removes needsReload without starting another turn. Reload and sealing are distinct owner decisions.

### 'local observation timeout requires reload without sealing or cancellation' (test; lines202–224; D)

An observation TimeoutException after one turn requires reload and disables send but preserves session/ready Vault and sends no cancellation. Calling retry cannot create a second turn; local observer timeout is not retry permission.

### 'oversized emoji input stays in the composer' (testWidgets; lines226–257; D/P)

Enter3000 ice-cube emoji, exceeding normalized UTF-8 admission size, and press Ask Floe. Composer retains the entire draft, controller reports invalid_input and gateway sees zero turns. Input rejection must not destroy user text; exact length is a fixture.

## apps/client/test/features/conversation/agent_delegation_fixture_test.dart

Full source read: lines1–32; SHA-256 `b9c33f9c14e0e1134185bb39f593dd46f02afc23c841ce5728b83e16140f8b38`.

Current owner: AgentMessage/AgentCapabilityMessage presentation decoder / shared ExpertReport-Task-Actions fixture. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'Rust delegation fixture projects generic Task result and Actions artifact' (test; lines8–31; D/O)

Read the tracked shared Rust-produced delegation JSON and parse as AgentCapabilityMessage. It projects One focus window from floe.builtin.schedule, is delegation and advertises both generic Schedule report and Actions Calendar-proposal media types. Raw bytes must omit grant_id/source_authority/consumer_policy. Preserve non-authoritative task/artifact presentation and privacy; exact artifact media versions may change. Fixture ownership is shared with Rust settlement, outside this deletion batch.

## apps/client/test/features/conversation/agent_interaction_card_test.dart

Full source read: lines1–216; SHA-256 `c00a1ebe401c2ad27075aeb9af0c830b392d028b7fdf3119854ceb31ab7d1ede`.

Current owner: AgentInteractionCard / AgentController owner-projected review presentation. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'card renders backend actions and decides on tap' (testWidgets; lines46–82; O/D/P)

With a pending processing-recipient card, render Model request, model.example, Waiting for review and exactly the backend-projected Allow/Deny/Not now controls; Continue is absent. Tap Allow, record approve, reload as Resolved, remove Allow and show Continue. This actual card kind/recipient consent vertical is obsolete under ADR0034; retain only the general invariant that UI cannot invent authorization actions and a decision is explicit. No Continue action is exercised by this widget test.

## apps/client/test/features/conversation/agent_interaction_test.dart

Full source read: lines1–606; SHA-256 `ebd5f1f588c643b56467a4f4f751fb3295d50c94aa96c5ccc26db19110a683a3`.

Current owner: AgentInteractionSnapshot/Result, FloeClient, NativeAgentInteractionGateway and AgentController. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'parses a consent card with scopes and backend actions' (test; lines54–71; O/D)

Parse the legacy pending processing-recipient consent fixture: preserve exact card/session identity,32-byte digest, personal input class, Calendar connection scope, allow/deny/dismiss backend actions and nonterminal state. Recipient/profile consent itself retires; strict binding to owner-projected review evidence remains relevant at source review.

### 'parses observe and navigation targets' (test; lines73–103; D/O)

Parse source_access/inline_observe with Personal Calendar member, then a navigation_only target to connection_settings. Each decodes its typed member/destination. This is parser evidence; no Observe permission is granted or navigation performed.

### 'parses navigation-only Expert binding without source authority' (test; lines105–132; D/P)

Parse Expert-binding navigation with assignment/package/version/requirement/capability and projected open-expert-settings,refresh,dismiss actions. Preserve package/requirement and absence of connector_id/grant_id: binding navigation cannot supply source authority.

### 'rejects unknown states, actions, targets and digests' (test; lines134–150; D/O)

Six malformed branches each throw FormatException: unknown state waiting_on_user, unsupported interaction_kind device_pairing, extra auto_approve action, admin_override target,3-byte digest and32 values of500. Strict bounded review data is durable; obsolete variant names need no compatibility layer.

### 'parses resolve results with linked children' (test; lines152–189; D/O)

Decode resolved operation with a linked accepted child Run, then stale operation carrying current revision2 and no child; reject unknown outcome maybe. Preserve owner outcome/child linkage instead of inventing a new Run from UI state.

### 'parses interaction message references' (test; lines191–209; O/D)

Decode an interaction message reference to the processing-recipient card, preserving ID; reject an admin_override kind. Old kind retires, while references must remain typed and cannot manufacture privileged variants.

### 'resolve sends only the reviewed binding' (test; lines213–238; D/O)

Prepare approve for card/session/expected revision/digest and submit. Wire command has exactly six reviewed-binding fields, no recipient or original_text. Resolution must bind to reviewed owner state rather than resend mutable descriptive content; current consent transport shape is legacy.

### 'resume omits caller text and observes the receipt' (test; lines240–262; D)

Prepare resume using session, origin Run and expected revision; command has four fields and no caller text, then decodes child receipt. Continuation must use durable stored intent, not replacement text supplied by the UI.

### 'get and list scope their snapshots' (test; lines264–287; D/O)

Get an existing card by ID, decode unknown_interaction as null for missing, and list one card scoped to session. These three positive branches do not inject foreign snapshots, so do not claim negative identity validation from this test alone.

### 'resubmits the identical decision after transport loss' (test; lines291–319; D)

Lose the first interaction decision transport response. Gateway resubmits the identical decision command ID, succeeds on second attempt and returns resolved. Stable identity supports idempotency rather than generating a new approval.

### 'decide observes the linked child and reloads the card' (test; lines323–349; D/O)

Load/refresh a card, explicitly approve, then observe its linked child-run and accept the resulting two-message session. Session replacement drops the cached card; explicit refresh retrieves resolved state. Observe the owner's linked child instead of starting a new turn; card semantics remain obsolete.

### 'stale review keeps the current card for a new tap' (test; lines351–367; D)

Apply a decision to a stale review and receive revision2. Keep the current card, record interaction_stale and do not demand session reload. A new explicit tap must review current state rather than silently retrying stale approval.

### 'continue claims the slot at the current revision' (test; lines369–387; D/O)

Given a resolved card at revision3, Continue passes the current session revision0 to resume and observes child-run. Card revision is not session CAS; UI must not confuse these owners. This tests explicit continuation, not automatic replay.

## apps/client/test/features/conversation/agent_panel_test.dart

Full source read: lines1–421; SHA-256 `b9ae19c9d660127567a4e95ba91dd59c0c5713e15bd3e1e72ae58874e265ecf0`.

Current owner: AgentPanel / PersonalDayScreen / AgentController presentation lifecycle. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'secure storage opens automatically at width $width and 200 percent text' (testWidgets; lines51–73; P)

Two loop registrations cover320 and390-pixel widths at200% text with real font loading. Missing storage opens automatically once, composer exists and canSend is true, obsolete setup/unlock/lock controls are absent and no layout exception occurs. Exact responsive presentation is a product hypothesis, not source authority.

### 'interrupted and failed sessions show explicit recovery and read-only reload' (testWidgets; lines76–99; D/P)

Start from failed session load; panel offers read-only Reload. Change saved session to interrupted, tap Reload then explicit Recover. One recovery and zero model begins occur; Ask Floe returns. Recovery controls must not conceal model replay.

### 'session load failure still allows a new conversation' (testWidgets; lines101–145; D/P)

An integrity session-open failure carrying start_new_session/export_diagnostics actions retains new-conversation escape, incident ID and owner domain while requiring reload. Render explanatory guidance, New conversation, incident and diagnostics action. Clear fake error, click New conversation and obtain session/no failure/no reload. Only the new-session path is activated here, not diagnostic export.

### '${failure.key} shows actionable recovery guidance' (testWidgets; lines152–168; P/D)

Three loop registrations show distinct guidance for access_review_required (Connections/Calendar review), server_model_timeout (server model timeout/dashboard route advice), and server_model_request_rejected (trace/model compatibility). No generic Try again button appears. Preserve no unapproved retry; historical wording and concrete routing advice must be revised for the target product.

### 'vault-backed model failure is not shown as a storage failure' (testWidgets; lines171–196; D/P)

Model invalid output with missing completion session shows model-validation guidance and Reload, never secure-storage failure text. Failure domain must stay honest instead of encouraging key replacement.

### 'stale context shows generic refresh guidance without reload' (testWidgets; lines198–220; P/D)

A stale_context/refresh_context load error renders generic Context refresh guidance, no Calendar-specific wording and no Reload control. Recovery presentation must follow the actual owner scope.

### 'source review guidance opens the explicit review surface' (testWidgets; lines222–247; P/D)

A capability_unavailable/review_source load error presents View Calendar source. Tapping invokes only the explicitly provided review-navigation callback; it does not grant source permission.

### 'Today opens one user-invoked assistant surface at width $width' (testWidgets; lines250–287; P)

Two widths1280/390 begin with no assistant panel. User clicks Floe is here to help and exactly one panel opens; only narrow width gets a BottomSheet. Explicit Close removes it with no layout exception. This is presentation/lifecycle evidence using product FakeDayGateway.

### 'Settings reloads action permissions after opening the agent vault' (testWidgets; lines290–339; D/P)

At desktop width, initial Day action-authority read occurs while Vault missing. Opening Settings auto-creates Vault once and reloads authority once more (two total reads); locked-storage guidance disappears and authority selector enables. UI may expose permission controls only after authoritative storage becomes available; this does not itself approve an Action.

### 'Today keeps conversation storage open across lifecycle changes' (testWidgets; lines341–385; P/D)

Open Today assistant against an already-created Vault, transition inactive→hidden→inactive→resumed, then close the surface. No locks or unlocks occur and no reload prompt appears. App lifecycle/view closure does not itself close shared conversation storage. _VaultBackedDayGateway delegates real presentation to production FakeDayGateway and action fake Executor, denying authority reads/writes unless fake Vault is ready.

## apps/client/test/features/conversation/conversation_runtime_gateway_test.dart

Full source read: lines1–388; SHA-256 `5af73499383bdea6a105d6118e770ab656b61c62c4995333c80f92be5178f6f6`.

Current owner: NativeConversationRuntimeGateway / FloeClient / AppReadModel. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'drives a product turn through command events and durable queries' (test; lines14–49; D/O)

Run a greeting through fake AppWire: one StartTurn, query-only get_run observations, at least three event reads, executing then finished callbacks, loaded session revision2 and sending reenabled. Explicit local-fast profile serialization is obsolete under accepted routing; event/durable-query convergence and owner observations remain durable.

### 'cancel before admission acknowledgement reuses one stable command' (test; lines51–93; D)

Block admission acknowledgment, request cancellation twice and lose the first accepted cancel reply. Release admission; both cancel callers settle and the Run becomes cancelled. There is one StartTurn and two transport Cancel attempts carrying one stable cancel command ID; no duplicate cancellation intent or read-model lockout remains.

### 'lost admission acknowledgement recovers the same command' (test; lines95–117; D)

Lose the accepted admission reply once. Gateway recovers through get_command as its first query, with exactly one StartTurn submission, and observes the same finished Run. Transport loss must not repeat admission/model work.

### 'continuation uses the durable source Run generation' (test; lines119–148; D)

For a session continuation at level1, read the durable source Run and submit continue with that Run ID, actual executor generation9 and next level2. Do not trust a stale caller-supplied generation.

### 'retry validates and serializes a durable terminal source Run' (test; lines150–178; D)

Explicit retry first reads the durable terminal source Run then submits retry_of referencing it. This positive fixture is partial/finished; it does not separately demonstrate rejection of foreign/nonterminal Runs despite the label's broad wording.

### 'unsupported host route fails before StartTurn admission' (test; lines180–202; D)

Let beforeStartTurn throw route unsupported. Synchronization may complete, but zero commands are admitted. Host route failure cannot slip into a model request or fallback admission; fake transport performs no provider I/O.

## apps/client/test/features/conversation/owner_failure_controller_test.dart

Full source read: lines1–108; SHA-256 `23502fe5422fa349c73a013d804e6dd7df322ce1cb3a1022233d5204ec24b903`.

Current owner: AgentController routing of Registry/Connections/Knowledge owner failures. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### '${operation.key} preserves owner reload/seal $flags' (test; lines20–68; D)

This nested registration expands to12 cases: registry, connections, memory and memory review each crossed with flags(false,false),(true,false),(true,true). Begin with a loaded session; each owner read throws one interrupted integrity failure containing reason, recovery, safe-actions, affected refs, incident and retry policy. Each case reads once and performs zero turn/cancel calls. false/false retains identical session, ready Vault, no global failure and sending enabled; true/false retains session/ready Vault but sets needsReload, copies all failure metadata and disables sending; true/true also clears session and marks Vault unavailable. Mutation methods deliberately throw UnsupportedError, preventing a hidden recovery mutation. No(false,true) row exists, so do not claim it was covered.

## Current owner, harness and target mapping

AgentController and AgentPanel own presented conversation/recovery state, never source grants, model routes or Action authority. NativeConversationRuntimeGateway drives stable owner commands plus event/durable-query observation through FloeClient and AppReadModel. Interaction gateway/controller translate owner-projected review and continuation; exact recipient consent/profile selection is superseded and must retire. Durable source review, CAS, command identity, linked-Run observation and no-replay/cancellation distinctions remain.

All local _ConversationGateway/_ImmediateConversationRuntime/_ControllerRuntime/_ConversationTransport/_CardGateway/_CardInteractions/_CardRuntime/_InteractionConversationGateway/_Gateway/_Runtime and session/snapshot/ID helpers are H, read in full. They synthesize known Runs/receipts, hold admission or completion with completers, supply failure metadata and deliberately throw on unsupported observation methods; none proves a real model/permission/native device call. The four-owner failure gateway rejects every unexpected mutation. Shared TestAgentGateway/TestVaultGateway behavior is recorded in the runtime ledger. AgentPanel's Executor import chain is recorded in the Actions ledger; fonts/localization/theme and product FakeDayGateway remain KEEP.
