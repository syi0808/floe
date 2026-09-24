# 05-B — source outcomes and trusted publication

- **Status:** planned; depends on 05-A.
- **Baseline windows:** those in the 05 index, plus re-find named symbols before editing.
- **Exit:** direct tool and delegated source blockers produce usable completed turns with durable refs and no fabricated evidence.

## 1. Cutover map

| Path | Work |
|---|---|
| `crates/modules/context/src/application/tools.rs:124-310` | ContextToolService must preserve typed source outcome instead of only raising AgentFailure |
| `crates/modules/context/src/application/source_view.rs`, `ports/source_reader.rs`, `application/service.rs` | Thread typed recoverable acquisition outcome without losing AuthorizedSourceBinding |
| `crates/modules/context/src/application/remote_sources.rs:223-327`, `remote_views.rs` | Preserve per-source identity for blockers and all CP4 merge/provenance guarantees |
| `crates/app/src/vault_host/conversation_turn/expert_host.rs` | Construct concrete source requirements at current owner boundary; do not decode failure text |
| `crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:95-230` | Bind interaction publication to validated Session/Run/Task/device and common endpoint |
| `crates/experts/builtin/src/shared.rs:24-39`, `host.rs`, `schedule/dispatch.rs`, all domain dispatch callers | Remove dropped requirements, preserve mandatory/optional semantics, carry host-validated refs |
| `crates/runtime/agent/src/engine.rs` | Journal blocked observations and refs once; retain Manager loop and usage/cursor semantics |
| `crates/modules/experts/src/dispatch.rs`, `task.rs` | Successful blocked-domain report is Completed, actual execution failure remains Failed |
| `crates/adapters/providers/src/models/wire.rs`, `crates/modules/conversation/src/application/recovery.rs` | Model-safe observation rendering and replay validation |

## 2. Source classifier belongs with current source facts

Expected cases: paused/missing grant, source selection needed, OS denial, OAuth reconnect and reviewable source drift. Hard cases: foreign Person/consumer, invalid signature, duplicate exact authority, corrupt grant/policy, invalid input, hard budget/cancellation/deadline. Do not classify solely by AgentFailure enum.

Extend Context/Access source adapters with typed outcomes at the point that knows current connection/resource/device identity. Direct tools currently using `.await?` need the same semantics as Calendar. Cover Calendar, direct people/attention/feasibility/wellbeing and remote mail/work/logistics callers; do not limit the fix to Schedule.

Unknown/unconfigured target gives a navigation-only requirement for its source category. It does not invent a connection id, resource, fingerprint or consumer alias. A policy-denied consumer not in the default bundle cannot be made admissible by toggling that bundle; return a truthful unavailable/policy state or correct the proven owner-policy mismatch before offering repair.

## 3. Trusted capture, not raw artifacts as authority

Context returns owner-produced outcomes. A trusted App/Conversation integration publishes them under the admitted origin before model/UI exposure. A narrow injected publication port or App ToolPort adapter is justified by origin validation, durable publication and safe projection; it must not be a compatibility facade.

Keep Context and Experts free of Conversation persistence dependencies. For direct tools, migrate the sole product ToolPort boundary to convert typed Context results into settled ToolResult + safe UserInteractionRef. For delegated built-ins, use the common endpoint and explicitly invocation-scoped host state; no Schedule-only publisher and no unscoped global staging map.

If host capture is separate from the final Task result, the durable origin receipt links them and replay reuses it. Merely accumulating refs in memory until return is not crash-safe.

The old Schedule raw SourceAccessRequirement artifact is internal proposal material only. Validate it against the trusted host outcome, consume it at that boundary and publish only the safe ref. External/model-produced JSON with the same media type must not create an interaction.

## 4. Optional, mandatory and multi-source reads

Mandatory source: domain judgment reports needs_user_action with no source conclusion. A deterministic truthful blocked result is Task Completed. An iterative Expert receives a failed/blocked capability observation and may explain it without fabricating a successful read.

Optional source: preserve the requirement/ref while reasoning over other authorized evidence. `optional_calendar_views` must not silently discard the requirement. Never relabel the entire output independent if other sources contributed.

Remote acquisition must inspect enough owner connection state to distinguish no configured source from a paused selected source; grant enumeration alone cannot explain missing grants. This inspection is read-only and does not restore Registry preflight authority.

Preserve the existing bounded multi-source acquisition semantics. When a source fails, do not return a truncated aggregate as a complete Ready result. Return concrete blocker(s), with no payload/dependency from failed acquisition leaking into metadata. Multiple blocked accounts remain distinct; if the single-requirement contract is insufficient, replace it with a bounded nonempty requirement collection and migrate callers directly. Do not invent one aggregate grant or fake source id. Keep successful independent calls' dependencies intact.

## 5. Engine, Manager and replay

ToolResult indicates blocked operation, carries safe ref(s), and includes no successful source evidence for that blocked call. Use existing issue/coverage validation honestly. Engine journals the result/ref with stable call identity before proceeding.

Provider wire exposes bounded semantic fields such as status, generic source class, reason and opaque interaction id. No native fingerprint, owner CAS tuple, selected-resource names, raw signed preview, credential or UI route is sent to the model.

Locate `manager_role.txt` via repository search and add only a generic instruction: explain host-provided blocked state, do not claim approval/data, and continue with useful admitted evidence. Do not encode permission workflows or Calendar navigation in prompts.

Keep deterministic batch/cursor recovery. A validated pending batch is not discarded just because a source step blocked. Settle that step, execute/settle the remaining valid work under existing semantics and finish normally. Actual hard failure behavior stays unchanged.

05-D supplies the typed outcome for model admission blockage. Until then, do not catch generic PolicyDenied and pretend a safe model response was generated.

## 6. Tests and deletion

Required fixtures:

- direct Contacts/Attention tool blocked → one durable ref → second Manager iteration explains → Run Completed;
- delegated Schedule blocked → Task Completed/no conclusion → Manager answers → original lease released;
- optional Calendar blocked while other evidence succeeds → interaction retained and real coverage retained;
- all eight built-ins remain registered and source-independent;
- remote source A active/B paused → honest source-specific blocker, not empty/partial-complete evidence;
- same requirement replay → same id/message; different account → separate id;
- forged requirement/ref, bad signature or corrupt policy → hard failure/no approval card;
- crash at capture/result/message boundaries recovers without repeating completed effects.

Search every `NeedsUserAction(_)`, `.await?` source admission caller, `SourceAccessRequirement`, `USER_INTERACTION_MEDIA_TYPE`, and raw requirement artifact producer. Every remaining discard must be removed or proven a non-product caller with an explicit safe disposition. Preserve `/focus` evidence and stateful settlement tests.

Run Context/Access/built-in Experts/Agent Runtime/Conversation/Vault focused tests, library check and architecture check. Exit requires real app-composed tests, not only synthetic UserInteractionRef fixtures.
