# Runtime architecture

This document describes the canonical runtime and current product boundaries.

## Shared reasoning and source processing

Manager, delegated Experts and Learner use the same role-neutral Engine and purpose-only model preparation path. Inference selects Gateway Primary and may select the local model only when planning positively establishes valid Primary absence. A failed Primary, missing source permission, unavailable credential expectation or dispatch uncertainty cannot trigger fallback. [ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) defines this source-processing boundary.

Access owns source-grant DeviceOnly/GatewayAllowed processing permission and live dispatch fences. Health has a separate mandatory local privacy transformation before either reasoning placement; its output retains HighlySensitive classification and transform provenance. Model selection, pairing and transformation confer no source or Action permission.

## General Conversation

Conversation owns the durable Session/root-Run lifecycle and projects the state required by the role-neutral Agent Runtime.

### Model path

Canonical ownership:

```text
Conversation, Experts Task or Knowledge claim
  -> role-neutral Agent Runtime Engine
  -> ModelPort::prepare(ModelPlanRequest)
  -> owned PreparedModelCall + immutable PreparedModelPlan
  -> Context projection against that exact plan
  -> NeedsSourceReview | AuthorizedModelProjection
  -> canonical owner journal ModelIntent
  -> Inference attempt reservation and Access admission/consumption
  -> prepared transport handoff and response revalidation
  -> immutable attempt receipt -> canonical journal ModelResult
```

Responsibilities do not collapse across this chain:

- Conversation owns Run/transcript/continuation and durable user interactions (origin, reviewed target, lifecycle, decision intent, resume linkage).
- Context owns source-backed projection, coverage, provenance and freshness.
- Inference owns prepared model selection, attempt reservation, dispatch accounting and bounded transport handoff.
- Access owns source processing permission and admission, consumption, revalidation and release fences.
- Provider adapters resolve private credentials and execute transport.

The Agent contract's `PromptAssembly` owns the stable-instruction limit: 9,216 UTF-8 bytes including rendered separators, accommodating the revision-7 Manager policy with a maximum-size Persona, with each component and Persona still bounded to 4,096 bytes. Rust Foundation and server transports use `MAX_STABLE_INSTRUCTIONS_BYTES`; Swift `LocalModelInput` and Go `/v1/agent` enforce the same byte boundary before model I/O. This limit does not enlarge token budgets, response limits, deadlines, Foundation context reservation or transport-body limits.

### Model-input lifetimes and content identity

`ContextEnvelope` schema 2 directly separates stable program, `RunInstructions`, `DiscoveryContext`, contextual evidence, causal Conversation, `AttemptContext` and a derived safe manifest. Context derives discovery only from the executable `AllowedCatalog`: capability IDs and Expert definitions sort by stable ID, retaining the exact catalog revision, including zero. Discovery and evidence remain data, not higher-precedence instructions.

Conversation constructs the Manager `PromptAssembly` once when its Run-bound projector is created. Retries clone that assembly; finalization clones a separately stored assembly whose Role contains only finalization prose. Role output contracts remain in Run instructions. Conversation binds the same Experts-owned `RunExpertEnvironmentIdentity` into the projector and durable `TurnRequest`; its neutral manifest projection is diagnostic metadata, not another authority.

The Agent contract owns canonical Run-frame JSON (`run_instructions`, `discovery`) and Attempt-frame JSON (`contextual_data`, `attempt`, `manifest`), with fixed field ordering. Shared provider framing preserves Run frame → retained history → Attempt frame → current turn, without sorting causal messages. Server sends those exact frames as user messages with separate stable `instructions`; Foundation embeds them in `run_frame`, `history`, `attempt_context`, `current_turn`. The shared macOS/iOS Swift local-model consumer reads only this shape. Go's outer `/v1/agent` remains schema 1 and validates native transcript grammar without decoding a second envelope contract.

Validated manifest hashes are SHA-256 of exact component content UTF-8, rendered stable instructions, canonical Agent-card JSON, and canonical Run-frame JSON. Card definition revisions and optional environment revision/digest are recorded separately, alongside evidence/memory identities. Evidence and correction changes affect only the Attempt frame; discovery changes do not affect stable program identity. These safe identities support diagnostics/cache optimization, never freshness, permission or authority. No provider cache metrics are synthesized when the provider does not report them.

Context returns `NeedsSourceReview` before the Engine allocates an attempt or acknowledges a ModelIntent. The review binds the actual immutable plan, projection operation and source blockers. A root model projection block is a typed Conversation outcome; a delegated block is stored in an immutable Task execution receipt. Neither creates a model attempt, synthetic answer or model-recipient consent. Hard planning, authority and transport failures remain typed failures. A validation correction may use the same owned prepared call and plan; a dispatch-uncertain failure is never reissued to recover an answer.

Product Run snapshots project model-attempt and delegated-Task references from Conversation's
durable intent journal. The refs remain derived read data: FFI does not manufacture them, and
Inference and Experts retain ownership of attempt and Task semantics.

`InferenceService : ModelPort` serves all three reasoning roles. Its public plan carries bounded non-secret identity and capabilities. Only the consumed Access fence yields the transport's admitted dispatch target; private credentials and endpoint binding remain provider-owned. The preparation's cancellation and deadline remain effective when the owned prepared call is retained, so a new caller scope cannot revive a closed generation.

### Host composition

Vault Create/Unlock owns root Agent-environment readiness, independently of Conversation Session lifetime. The admitted local Vault operation supplies the verified device identity, trusted operation ID and cancellation to App-private `OpenVault::activate`. Activation constructs and recovers the Conversation and Task repositories, reconciles the shipped Expert bundle, applies initial default bindings only if the Registry was absent before reconciliation, and publishes the Expert Directory before exposing `VaultState::Ready`. Preparation failures leave no usable OpenVault. An absent Registry is a readiness failure, not an empty catalog; an existing Registry with every assignment disabled is Ready with an intentionally empty Directory.

First-install binding is a bounded convenience mutation, not a publication owner or startup repair. Existing binding and enable configuration survive reopen, including empty or unavailable selections; later sources and bundle reconciliation do not auto-rebind. Root activation publishes after initial binding completes, while explicit Experts settings mutations retain their own Directory publication. Conversation Start/Resume/Get/Recover own only Session semantics; turn and resume-turn execution consume the prepared Directory without installing, refreshing, binding or republishing the root environment. Each Run samples one Experts-owned immutable environment for both Manager discovery and delegation.

`ConversationTurnRequest` carries only session/revision, text, profile intent, continuation/retry intent and the verified device identity. It has no credential source or lookup semantics.

The Vault worker owns one shared `CurrentSavedConnectionStore`, cloned into the open Vault and its root, built-in Expert and Schedule compositions. Production binds the host keychain; tests inject a fixed or mutable store at worker construction. Provider-owned `RootModelProvider::from_current_connection[_scoped]` and `ServerSourceClient::from_current_connection` load and admit the current connection against the verified person/device, returning opaque capabilities. App neither materializes saved credentials nor selects model placement. Recipient authority composes the Access-owned consent store with pairing admission over this store plus a clock, reloaded at every fence rather than trusting the prepared transport's snapshot. Saved pairing credentials contain no recipient approval.

`InferenceAvailability` observes execution classes for a purpose/consumer through the same candidate rules used for execution. Experts owns card eligibility against that non-secret observation. Availability is not dispatch authorization: actual execution still runs the canonical Inference/Access checks. Remote source capability is observed independently of model availability; source catalogs remain lazy and never participate in profile discovery or selection.

Delegated Expert model dispatch uses ordinary Inference/Access admission, consumption immediately before provider handoff, and post-response revalidation. Registry configuration is not a model authority fence; exact-recipient consent, source/grant provenance and current provider admission remain live.

### Root Manager capabilities and generic Tools

The root Manager catalogue contains active Expert cards with their actual definition revisions and no current Tool descriptors. Its private `NoManagerTools` port denies unexpected invocation with `CapabilityDenied` without I/O or mutation. It may answer from sufficient already-admitted Conversation, Persona, Memory and product context; fresh source-backed domain acquisition and domain judgment use the Experts-owned Task path. Disabled, unavailable or unbound Experts cannot trigger direct-source fallback or hidden context prefetch.

Generic `ToolDescriptor`, `ToolPort`, `ModelStep::CallTool`, provider Tool wire support and stable Tool journals remain role-neutral. An unregistered Tool request can produce a durable correction intent/result without dispatch. Expert capabilities and future non-domain presentation/navigation capabilities may use these contracts; no presentation capability is implemented here.

Manager orchestration guidance is owned by Conversation and remains independent of the installed Expert roster. Agent Cards describe purpose and capabilities; they do not grant source access or prescribe routing policy. Selection and evidence sufficiency remain model judgments, while existing host contracts continue to enforce identity, authorization, output shape, budgets and external effects.

The revision-7 Manager policy permits factual claims about private, current or changing external state only with relevant user-supplied information, admitted current Context or a settled Expert result covering the claim's scope and time. When required support is missing, the Manager delegates to a suitable advertised Expert if allowed, otherwise states the limitation and answers only the supported remainder. Cards are discovery metadata, not observations or authority. Failed, blocked, unavailable or partial observations cannot be silently widened into success or full coverage; external-change success requires an observed successful result. This is model-visible production policy: `ManagerPayloadValidator` still validates structure, not arbitrary natural-language truth. The pre-cutover Foundation evaluation remains historical evidence, not qualification of the intended Gateway Primary; ADR 0034 defines the separate Primary/Fallback evaluation and defers typed-grounding follow-up until that evaluation.

Provider adapters preserve the supplied instructions and catalog. They may describe native call mechanics, but do not append a separate delegation preference or infer tool visibility from user-language substrings. The assembled stable-instruction limit is 9216 UTF-8 bytes across Rust provider preparation, the native input contract and the server Agent endpoint; individual prompt-component, Persona and total transport limits remain separate.

Trusted endpoint blockage is a closed typed result: model projection review, source-read review or missing declared binding requirements. Experts settles it under the admitted Task and its canonical journal. Conversation authenticates the exact `TaskExecutionReceiptRef` and parent journal linkage before publishing source or binding reviews. Package artifact JSON cannot authorize a review, and Experts has no Conversation publication callback.

### Expert delegation

Canonical ownership:

```text
Conversation Run admission
  -> TaskCoordinator::environment(principal)
  -> immutable Directory snapshot / RunExpertEnvironment
  -> Manager catalog and DelegationPort
  -> validated Delegate batch with exact projection coverage
  -> DelegationIntent
  -> TaskCoordinator admission and Task-owned execution journal
  -> EngineExpertEndpoint + retained pure ExpertProgram
  -> Context source/projection ports + common Engine/ModelPort
  -> atomic Task terminal state, journal head and execution receipt
  -> parent DelegationResult references the exact receipt
  -> Manager synthesis or typed Conversation blockage
```

Experts owns Task identity, admission, lifecycle, persistence and cancellation. `RunExpertEnvironment` retains the principal and exact Directory snapshot for a Run. The Engine binds `DelegationContextInput` to the actual authorized projection, producing `DelegationExecutionContext` with inherited coverage. The canonical request digest covers principal, original parent, selected definition, context references, session/device, output bound and projection coverage; Task and invocation IDs are checked separately, and Task admission pins the exact assignment and source selection. `EndpointInvocation` carries the real Task execution key and its sole canonical journal. The common `EngineExpertEndpoint` runs under the Task's bounded child scope and uses the same prepared ModelPort as the Manager.

Directory publication replaces one owner's complete endpoint set under one write lock, preserving unrelated registrations and rejecting cross-owner collisions before mutation. Run admission samples the exact Directory revision, sorted eligible definitions, admission identities, execution selections and endpoints under one read lock. The environment's SHA-256 digest binds configuration only, never endpoint pointers or live authority. Its catalog preserves revision zero for an intentionally empty initial environment. Delegation resolves only this snapshot, including before Task admission; a later publication cannot reroute the active Run, and the next Run samples the changed Directory. Task replay must match the pinned admission and selection.

Every Conversation Run durably stores the required `RunExpertEnvironmentIdentity` revision/digest as execution admission, separate from canonical user intent and request digest. A duplicate/lost-ack command or linked-resume winner returns the stored identity without redriving against current configuration. Every validated model batch, including finalization and resumed re-records, must retain the admitted Run's catalog revision. Continue starts a new Run; takeover of pending validated work requires the source and destination environment identities to match exactly before execution. Without pending work, the new Run may use a new environment. Crash/reopen interrupts Working Runs and preserves their original identity rather than restarting them under current Directory state. Registry rebind and assignment/installation disable affect future Runs only; they cannot cancel, reroute or suppress an admitted Task.

Task admission persists the exact selection, execution/generation, output bound and allowance. Each append atomically advances an authenticated journal revision/digest. Terminal settlement validates the canonical journal, exact final payload and coverage, then stores one immutable `TaskExecutionReceipt`; a stateful package's private-state CAS occurs in the same transaction. Settlement preserves newer binding/enable configuration and conflicts on an incompatible same-assignment state transition. Successful non-stateful Tasks retain the same live dependency checks. A missing journal tail is corruption, not an empty execution.

Expert settings project persisted selected refs independently of live remote candidate discovery. An unavailable catalog leaves saved refs visible and removable under binding CAS; nonempty replacement still resolves current candidate IDs. Hosted Calendar candidates use the Calendar product connection and the current paired server's pinned producer for execution-owner identity; candidate identity depends on the connection/View, not selected leaf resources. Native and hosted Calendar each have one stable connection/View candidate and binding across resource edits. The next read acquires the current Calendar set and records that exact set in dependency provenance.

The common endpoint resolves each Tool call to a declared requirement in the admitted manifest/selection and calls the Experts-owned `ExpertSourcePort`. Context implements that inverse port and `ExpertProjectionPort`: it bounds the query, reauthorizes inherited coverage, captures actual source outcomes and projects them against the prepared plan. Source dependencies and native retention capabilities remain attached through Task settlement. A never-called source cannot be reported as Unavailable; a later actual Ready observation clears that requirement's earlier Unavailable warning.

The builtin crate supplies `ExpertRegistration<Arc<dyn ExpertProgram>>`. Each pure Program declares its role, output contract and source-tool schemas, then validates/finalizes a response over actual captured observations. It neither calls a model nor drives a private loop. `ExpertsService.activate` reconciles the supplied manifest bundle through its RegistryRepository and publishes compiled common endpoints. Package selection is resolved once from the admitted registration; App does not branch on package names or construct domain hosts. The delegated model consumer is `experts.delegated`; source reads retain the exact admitted package consumer.

Package finalization returns `ValidatedFinalPayload { text, artifacts }` before the Engine acknowledges `ValidatedBatch` or `Output`. The Engine checks final byte/artifact bounds and coverage against the actual projection. Task settlement requires exact equality with the journaled output; replay consumes this stored payload without rerunning package transformation. Package result artifacts preserve full judgment coverage, while an inert Schedule proposal identifies its actual captured Calendar contributor. The Task execution receipt retains the full terminal coverage for later Actions admission. A generic text result or artifact schema never substitutes for that receipt.

Context's declared-source resolver accepts a key only when it occurs in the admitted manifest and Task selection, bounds the query and returns typed `SourceReadOutcome<ExpertSourceRead>` with every successful contributor dependency. Pure package finalizers decode their domain views from the captured observations; model and settlement control stay in the common endpoint. Candidate discovery is a read-only settings operation over current connection metadata and canonical Context source identities; it never reads payload or creates a grant. Product sees opaque candidate IDs and safe labels, not `SourceSelectionReference`, and save resolves IDs against a fresh catalog. Expert reads receive only the Task's exact selected references. Remote acquisition checks the pinned producer and selected connector, connection and logical View resource before exact grant/scope and signed-preview checks; unrelated grants are not candidates or blockers. Native and hosted Calendar Expert selections each name one `calendar.timeline:<connection>` View. Native Context reloads Connections and acquires every current Calendar resource at read time. Hosted Context reads through the generic remote View path; its signed source preview names the logical resource and exact current server `calendar_ids`, and its dependency records the logical permission in `resources` and physical IDs in `source_resources`. Remote reauthorization compares both the signed source authority and exact physical set. The server uses one generic `PreviewView -> AdmitView -> ReadView -> Release` authority path and a connection-level bounded multi-Calendar runtime. Attention, Contacts and Wellbeing receive exact native references; Tasks and Confirmed Memory validate intrinsic local Context projection targets without Access grants. ConfirmedInteractions currently has no authorized candidate and reports an optional limitation rather than successful empty evidence. Context retains exact dependency reauthorization for later model/history use.

Personal standing candidates exist only for serving Contacts, Attention and Wellbeing `SourceConnection`s, never as synthetic grant-derived fallbacks. The personal native acquisition broker carries only People and Wellbeing; Attention retains its separate trusted observation broker. Apple mobile inventory contains Contacts, Health and the Screen Time capability gate. Their IDs and saved Expert references bind stable connection/View identity rather than current source resources or subject, so source edits preserve binding revisions. Expert personal reads reload the selected Connections source, use its native subject and physical resources for acquisition, then recheck source and grant continuity after device I/O.

Experts owns per-assignment bindings and enable state as configuration. An empty required binding is detected after real Task admission and produces a typed Blocked receipt with declared requirement keys and an empty model journal. Settings and Task-origin review use the same immutable `BindingReviewRef`, candidate snapshot and replacement policy. Public reviews expose opaque candidate references and safe labels; replacement resolves only that exact stored review against current source expectations and binding/Registry CAS. Durable command and consumed-review receipts support exact lost-ack replay and Conversation refresh. Activation preserves saved empty or unavailable selections and never silently rebinds. Binding grants no source, model-processing or Action permission. Source/grant owners still admit each acquisition and release.

Schedule owns bounded Calendar/time-planning judgment. Its pure request plan fixes the requested range; the common Engine invokes the declared Calendar tool and the finalizer validates complete bounded pagination over actual observations. The finalizer prepares the assignment-local state transition and asks Actions' pure sealer to construct an inert proposal from the exact captured Calendar dependency and admitted selection. Task storage commits the state transition and final payload together. The proposal itself grants no execution authority.

### Knowledge and Learner

`KnowledgeService` owns confirmed-memory reads, safe review projections, user decisions and the Learner lifecycle. Context receives the narrow `KnowledgeRead` capability. Product review returns `MemoryReviewDisplay` with candidate text, kind, confidence, source count, validity and owner-derived actions; decision acknowledgement exposes actual command/candidate/decision/time and resulting target/revision. Storage hashes, idempotency keys, source references and raw candidate envelopes remain internal. Only a user decision activates a staged memory.

Knowledge discovers explicit learning signals from stored Conversation evidence, including the original User message identity of a resumed Assistant answer. It admits only eligible completed independent evidence and records an immutable job input. A claimed job pins Person/device, budget and claim generation. `ContextLearnerProjection` re-reads that actual claim and evidence and projects against the Engine's prepared plan and exact correction operation. The Learner has an empty Tool/delegation catalog and uses the same Engine, ModelPort and canonical JournalEvent sequence as other roles.

Each claim has one durable journal and authenticated head. Unknown or unresolved attempts remain conservatively charged and cannot be retried automatically. Recovery of an empty journal or a known acknowledged Output retains the same claim; acknowledged Output is parsed/staged without another model call. Candidate staging is Pending-only and idempotent under an immutable full-request stage receipt. Same candidate-key reuse with changed payload, revision, time or claim proof conflicts.

Knowledge owns background discovery and execution scheduling. Conversation holds `KnowledgeForegroundLease` through its admitted driver and finalization; that owner lease preempts active background learning. Closing the owner stops new admissions and cancels its generation. Shutdown drains admitted memory operations and the retained Learner/background driver under one bounded deadline, reporting timeout without claiming unfinished work has stopped or erasing durable journal evidence.

### Durable interactions

The following is the pre-cutover interaction implementation. ADR 0034 retires model-recipient interactions and synthetic model-blocked replies, and requires durable automatic resume for the remaining SourceAccess/ExpertBinding reviews. The existing unique slot prevents duplicate child admission, but the best-effort trigger after resolution still has a crash gap and Resolved still projects a routine Continue action. Those are replacement targets, not completed recovery guarantees.

Conversation durably records recoverable owner requirements as interactions: the journal-verified origin (Tool call, Delegation Task or Model attempt), the owner-produced requirement, and the immutable reviewed target (exact connection/device/source, resources, capability bundle, consumer/purpose, one bundle source revision, per-member grant expectation including expected absence, and compare-only App-produced `policy_digest`). Publication identity derives deterministically from origin plus canonical digests, so crash replay settles the same row; decisions bind the reviewed digest through compare-and-swap with identical-command rejoin. Lifecycle is Pending to Resolving to Resolved, with Denied, Cancelled, Superseded and Expired as the other terminal states; the original Run completes with its limitation instead of waiting. Session messages, transcripts and model-safe artifacts carry only the opaque interaction reference. Authority stays with Access, Connections and the provider/native owners, which re-verify current state and prospective policy when a decision resolves; a linked resume is a fresh Run, not budget continuation. It admits through one atomically bound per-origin slot, dispatches under origin-carried lineage so origin-reviewed grants still scope it, and re-scopes any fresh blockage review to the attempting Run. Flutter decides through versioned commands carrying only the decision and the reviewed digest; snapshots carry safe review identity plus backend-projected actions, never authority. See [ADR 0030](../decisions/0030-durable-interaction-and-linked-resume.md).

Missing Expert configuration uses the same durable interaction system with an `ExpertBinding` target bound to the Task's assignment, package, requirement, binding revision and selection digest. Its safe actions navigate to generic Expert settings, refresh or dismiss; Allow cannot choose or mutate a binding. Settings save is a separate Experts command. Explicit Refresh re-reads current binding and only then resolves the interaction, allowing the existing linked-resume path to admit a fresh Task. An Expert-origin SourceAccess review binds the originating durable Task selection, not current Registry configuration. A target mismatch or live source/grant drift still fails closed; pure rebind/disable does not supersede the review.

Budget Continue instead replays the exact validated batch and cursor without model recall. The batch's stored `projection_coverage` is its source provenance: Conversation reauthorizes every recorded dependency against the current resolver before a pending step executes and again before terminal output release. Stale or Unknown coverage blocks stored steps and answers; message shape, Expert identity and capability names never substitute for provenance.

## Consequential actions

`ActionsService` owns Calendar Create, Update and Delete, with manual instructions and Expert proposals admitted through one encrypted `ActionsRepository`. App constructs the owner and forwards typed commands; it does not choose between repositories or run an approve/execute/collect workflow. A locked or unavailable Vault cannot fall back to plaintext state. Model or Expert output may produce an inert proposal artifact; a stored Task receipt proves its provenance but does not authorize an external effect.

The product submits safe intent: Create names an opaque owner-issued destination choice; Update/Delete name a Day Event and expected Day revision. Actions privately resolves the current source, native target and exact preconditions. An admitted manual instruction, immutable review decision or current standing policy authorizes the resulting effect. Before native dispatch, the encrypted transaction commits the exact `Executing` intent. Only that first commit may consume the single-use prepared native capability.

Actions owns bounded background execution and shutdown/drain. A receipt settles success or a proven prewrite rejection; insufficient evidence settles `Unknown`. Startup marks retained `Executing` intents as response-lost uncertainty without native I/O or redispatch. Read-only inspection never drives work. Explicit reconciliation reads native evidence, and successful effects retain a durable collection ticket until Day acknowledges the exact execution and receipt digest. Day collection is idempotent and cannot turn a succeeded provider effect back into failure.

See [Authority and recovery](authority-recovery.md).

## Product boundary

The macOS debug CLI is another caller of the same admitted `AppHost` services.
It reuses an explicitly selected client profile, existing identity, Keychain
credentials and Expert/source configuration, and never owns model/source policy.
It bundles the existing Apple model and EventKit drivers without launching Flutter.
It does not supply Flutter-only live personal/Attention host publication or execute
Actions. See [Debug conversation CLI](../development/debug-cli.md).

The final outer path is:

```text
Flutter / native / server caller
  -> protocol / FFI intent conversion
  -> AppHost request admission / verified CallerContext
  -> typed App service / composition
  -> owner service
  -> canonical internal runtime
  -> adapters
```

The pre-cutover product command still carries explicit model-profile intent. ADR 0034 rejects that as a final product contract; its removal belongs to the product/AppWire cutover, not a claim that the field is already absent. Outer callers must not carry saved bearer tokens, arbitrary model endpoints, resolved internal route bundles or Access policy flags. Before a connection is saved, pairing alone may supply a bounded loopback setup endpoint and signed challenge/polling evidence. Only an approved pairing result may return the newly issued token for secure persistence; its Debug/diagnostic representation is redacted.

Local AppWire commands and queries pass through `AppHost::request`, `HostRequest` and typed owner services for Vault lifecycle, Conversation sessions/turns, Experts, Access, Knowledge, Connections, Day, Actions and Context. Identity and runtime epoch come from `CallerContext`; neither requests nor nested native completion values can supply person/device authority. Product open requires the verified local profile and fails before database side effects when identity is unavailable. There is no unverified host fallback, `FloeHandle::services()`, `legacy_services()` or public concrete AppComposition getter.

The existing schema-2 `command_v2/query_v2` carry owner-prefixed intents, not worker action maps. `events_v2` remains Conversation event delivery with principal filtering. FFI strictly validates/converts DTOs, admits the host request, calls a typed service and serializes values; it does not load credentials or decide authorization, routing, eligibility or Action policy. App's private worker/job machinery remains where owners require non-blocking work.

Async local results bind the exact intent and originating Person/device/runtime epoch/owner. Observation and accepted-decode-before-release retain the same operation identity across uncertain acknowledgements, and do not imply cancellation. Flutter has separate owner gateways and mechanical correlation only, not a generic request bus. Day preserves Person ownership and repository CAS. Actions exposes safe owner snapshots and retains a single encrypted effect record, immutable command receipts and explicit uncertain-write reconciliation; App and Flutter perform no execution or recovery policy. Context injects admitted identity while retaining native request/epoch/fingerprint evidence; broker disposal is not Run cancellation.

App publishes a terminal Conversation Run event only after the owning turn job is marked complete.
The read-only Conversation Session get operation may also overlap the narrow interval after the
Run and Session commit but before that job returns; mutating Session operations remain serialized.

Pairing and remote Access have separate owner-oriented protocol envelopes and typed `RemotePairingCommands` / `RemoteAccessCommands` services. Their FFI entry points admit each request through `AppHost::request(request_id)`; FFI validates/converts structure and never supplies caller identity or decides authority. Pairing derives Person/device from `CallerContext` and client identity from the exact pending pairing ID. Connections and the key holder validate challenge, issuer and report identity.

After pairing, producer inspection and enrollment remain remote-specific, while standing Calendar and View grants use the common connection Observe intent. Provider-owned `RemoteAuthorityEndpoint::from_current_connection` and `ServerSourceClient::from_current_connection` reload the shared current store and admit the verified Person/device before preparing transport. App sees non-secret admitted client identity, not raw saved credentials. Access retains producer pinning and exact source/revision/provider/recipient/grant checks; source catalogs stay lazy and separate from model discovery.

Remote operations use bounded worker jobs. Their result observations and completed-job release are bound to the originating Person, device, runtime epoch and owner domain; they do not cancel a Run or Task. There is no generic AgentVault product polling/stop/release path. While a job is retained, a changed command cannot reuse its operation ID. Approved credentials are held only in bounded results until release/eviction and secure persistence by the caller.

Flutter binds the two remote owner entry points through the app-lifetime `NativeTransport`, sharing schema-2 envelope correlation with Conversation. Its separate pairing and Access gateways centralize bounded submit/read/release without generic worker polling or implicit cancellation. Pairing alone supplies a setup target; Access requests contain no saved endpoint or bearer. Approved results remain retained until the client writes and re-reads the exact connection through the `app.floe.local-server` / `connection-v1` Keychain bridge. Uncertain acknowledgements retain the same operation ID rather than reissuing mutations. Native code only stores bytes and opens the loopback management page; it does not decide Access or model policy.

The local C ABI consists only of open, command/query/events v2, remote pairing/access v2, protocol version and string/handle free. Old Day/Actions/LocalContext/AgentVault/fixture entry points and their wire envelopes are deleted, not aliased. Conversation sessions use their canonical admitted owner service. Internal `WorkerAction::ConversationTurn` and other private owner machinery remain.

ConversationService, Engine, Inference, prepared provider transports and Session storage form one runtime path. Live shared Expert host composition is named `expert_host`. Flutter tests use pure product-interface fakes; native smoke uses canonical Inference. App wire 2 and protocol 1 remain unchanged. Conversation storage schema 9 directly requires the Run environment identity, with no old-row decoder or default migration; old local profiles/binaries are not implied compatible.

## Local Go server

`server/internal/application` composes the local Console, durable disk/Vault adapters and concrete connector runtimes. Authorization owns the authority Engine, producer signing/validation, source admission/read/release semantics and bounded admission records. Its source operations receive current-source fencing and provider-identity preflight capabilities from Application. Connections owns domain records, scope validation, listing and detached registry snapshots; Application owns durable connector cleanup and concrete runtime construction. Pairing owns the pending operation, proof/identity checks, confirmation and approval sequencing, calling Application only for durable client/issuer settlement.

`transport/http` owns routing, strict JSON decoding, headers, management sessions and response mapping. Application injects typed management, connector, pairing and source capabilities; HTTP neither reaches Console state nor decides authority. The transport-independent `operation` result values carry only outcomes and failure categories. No owner imports Application, and no Console/state alias crosses this boundary. Synthetic management model probes also run through Inference's existing exact-recipient fence; they do not exempt requests from consent. Rust Access remains the local authority owner—the Go server is its remote producer, not a replacement authority policy engine for the client.
