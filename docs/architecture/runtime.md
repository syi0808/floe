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

`floe_app::open` opens an explicitly selected existing profile after verifying its local Person/device identity. It constructs the host runtime, product store, Day owner, native acquisition brokers, source-lease registry and initially locked `ProductGatewayLeaseRegistry`. `CallerContext` retains the verified identity and host runtime epoch. These host-lifetime resources exist independently of an encrypted ready generation or a Conversation Session.

The private Vault lifecycle queue serializes Create, Unlock and Lock, retaining receipts bound to the admitted caller and operation ID. Create/Unlock passes that identity and bounded cancellation scope to `ReadyGeneration::activate`. Activation checks the Vault Person, recovers Conversation and Task execution storage, constructs the source adapters and shared Inference service, and assembles the Connections, Experts, Knowledge, Actions and Conversation owners. Each owner performs its own activation before App publishes `ReadyOwners` and reports `VaultState::Ready`. A failed activation closes constructed owners, retires its exact Gateway generation and seals the Vault.

`ExpertsService.activate` owns shipped-bundle reconciliation and Directory publication. Existing binding and enable configuration survive reopen, including empty or unavailable selections; activation does not silently rebind them. An intentionally empty Directory remains valid. Conversation Start/Resume/Get/Recover own Session semantics, while each admitted Run retains one Experts-owned environment for discovery and delegation. The public `StartTurn` intent contains session/revision, text and optional continuation or retry references. App supplies the verified actor separately; the caller does not choose a model profile or credential source.

`SourceServices` constructs one `GatewayCredentialStore` over the encrypted Vault's trust reader for each ready generation. Shared clones feed pairing, integration and cleanup adapters, `CompositeModelProvider`, Inference admission and the product Gateway lease registry. The registry publishes that reader and the Vault authorization signer under an exact generation; each remote source operation prepares and retains its own verified credential binding. Readiness does not require a configured Gateway. Only verified credential absence produces `None`; locked, unreadable, malformed, staged or foreign identity remains a failure. Private credentials stay inside provider/storage adapters, and source catalogs are observed only by their owning operations.

App injects the same model, repository and source capabilities into the owners. Context implements Expert source/projection/candidate and Learner projection ports; builtin registrations supply pure Expert programs. App does not branch on package names, run model loops or publish source reviews. Lock and generation retirement first close owner admission and the exact Gateway lease generation, then drain owners and seal the Vault. Panic isolation lets the remaining owner retirements proceed when another owner fails; failure never becomes a usable ready generation.

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

Conversation publishes typed source-processing, source-navigation and Expert-binding reviews from authenticated execution evidence. A root projection binds its actual model operation; a delegated review binds the exact durable Task execution receipt and parent journal linkage. The encrypted publication transaction checks the owner-produced review reference and audit against the recorded Run and Task. Package artifact JSON, a capability name or a client-supplied target cannot authorize a review. Session transcripts carry opaque interaction identity and safe owner projections.

A SourceAccess decision records its immutable command, target digest and subordinate Connections command before applying the source review. Connections and Access own the actual mutation and committed receipt. Conversation resolves only against that exact receipt, verified again in the encrypted transaction. The resolution and durable resume request commit together. Activation recovers interrupted source decisions and pending resume requests; ordinary queries do not redrive them. A deterministic per-origin command and transactional child slot make duplicate wakeups and lost acknowledgements rejoin the same child Run. Resolved interactions do not expose a routine Continue button.

ExpertBinding uses an immutable Experts review reference. Settings save is a separate Experts command, and Allow cannot select or mutate bindings. Explicit Refresh has its own durable command and resolves only after finding the exact consumed review receipt, including its admitted device and validity interval. A changed current Registry binding alone is not approval. Source reviews originating in a Task retain its admitted selection; a later rebind or disable changes future admission, while source and grant fences remain live.

Linked resume admits a fresh Run with the recorded interaction lineage. Budget continuation instead replays the exact validated batch and cursor without model recall. Conversation reauthorizes its stored projection coverage before executing pending work and before releasing output. Task acknowledgements are recovered from the actual Task repository and re-journaled as immutable receipt references; the Session transcript shows an acknowledged Task once. Unknown coverage, an unresolved model attempt or altered evidence cannot be converted into successful empty work or a fresh external retry.

## Day and product Calendar reads

`DayService` owns manual records, bounded Day projection, durable refresh and Calendar cache reconciliation. It is a host-lifetime owner over `DayRepository`, `CalendarAcquisitionPort` and its clock. Context implements the acquisition and metadata-inspection port over current Connections metadata and real native/Gateway transport handles. Native product display is independent of the opened agent Vault generation; Gateway reads require the encrypted signer/pin and a current private credential lease.

Manual mutation and refresh share one Person/command UUID namespace. Intent includes the normalized requested date/offsets; manual intent also includes the complete mutation, including occurred_at and expected_revision. Display-only day.now and executor generation are excluded. A duplicate command rejoins its exact stored result, including the original safe snapshot, before new effects. A changed device, kind or intent conflicts. Manual mutation applies the Day-owned transition, local row changes, selected snapshot and immutable receipt in one transaction. The 4,096-command capacity rejects new admission without evicting replay evidence; manual input and stored receipt bytes are separately bounded.

`day.refresh` durably admits a Pending operation with the real operation ID and exact `MirrorExpectation::Absent` or `Present(revision)`, then starts bounded owned work. `day.refresh.get` only reads its closed state. Recovery interrupts unfinished work rather than issuing another provider read. Context resolves the complete configured source inventory; the product supplies a display date/offset range, never source selection or grant authority. Refresh accepts a nonnegative UTC span of at most 48 hours, 64 sources, 256 selected calendars, 10,000 records and 4 MiB normalized payload; remote pages are at most 128 records/1 MiB within the 60-second operation deadline. Overflow or unknown consumption fails explicitly.

Access issues a separate product Calendar permit bound to Person/device, the exact configured source/resources and current OS/provider permission, operation/read identity, range, limits, expiry and original cancellation/deadline. Native acquisition checks the real subject and permission around the read. Gateway acquisition retains one exact credential/signing generation and uses distinct `day_calendar_admission` / `day_calendar_release` proofs for `day_refresh` / `calendar.mirror`; release binds the staged page hash, and the adapter verifies the released raw bytes before decoding. These values cannot become an assistant grant, ContextDependency, model permission or Action approval. Assistant Calendar Tools continue through their own Observe grants and live dependency checks.

Day reconciles by source, provider, calendar and external event identity, with no first-source shortcut. Each resource retains its latest complete interval; complete empty replaces that resource’s cache, while a failed resource retains its prior complete interval and explicit failure metadata. The final short transaction compares the complete current inventory, successful-source fences, mirror revision and refresh operation. Any drift rejects the whole commit; stable partial provider failure may preserve failed resources while committing successful ones. Newly cached or reinserted events take the next durable mirror revision as their Day revision, avoiding EventId/revision reuse after interval eviction. External ProviderOpaque and ObservationFingerprint revisions keep their distinct meaning.

A fresh `day.read` calls metadata-only `inspect_sources`: Context reloads configured identity/resources and active fences and obtains actual available OS/provider permission evidence. Day compares that observation with the cached versions and rechecks the mirror after I/O. Changed, removed, fenced or unverified sources cannot appear Current, and a date outside the retained interval is stale/pending rather than complete-empty. This read does not prompt, acquire provider events, alter grants/identity, persist new cache state or implicitly refresh. Owner clock governs freshness. A new manual mutation receipt conservatively marks cache status stale because its transaction performs no metadata I/O; replay of any completed command returns the immutable historical result unchanged.

`DayReadQuery` selects only the requested display, Action window, open Tasks or current Notes before accumulating bounded results. The storage adapter uses conservative physical predicates and Day’s exact predicate, preserving truthful overdue counts without loading unrelated historical/deleted rows or truncating the selected result. Public Day values expose opaque source/resource/Event references and Day revisions; physical IDs, source authority and provider revisions remain private for owner revalidation.

Manual mutation and Action-result collection share Day’s admission/drain guard and durable active executor fence. Their nonserializable `DayWriteFence` retains the exact actor/generation and original owner/caller cancellation and deadline through the pre-commit check. Historical receipt replay is first. Collection atomically records the execution ID, exact effect-receipt digest and normalized collection intent with the mirror update; a retry cannot repeat an external effect or manufacture whole-calendar freshness. These local transactions contain no OS/provider I/O.

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
Flutter / native / local CLI caller
  -> protocol / FFI intent conversion
  -> AppHost request admission / verified CallerContext
  -> typed App service / composition
  -> owner service
  -> canonical internal runtime
  -> adapters
```

Local schema-2 commands and queries pass through `AppHost::request` and a `HostRequest` that owns its caller copy, service `Arc` and admission guard. Identity and runtime epoch come from verified `CallerContext`; product requests cannot supply Person/device authority. FFI validates and converts the safe DTO, calls the typed Conversation, Experts, Knowledge, Connections, Day or Actions owner, and serializes its projection. Vault lifecycle and native acquisition callbacks have their own narrow host APIs. Product open rejects unavailable identity before opening the database, and a locked ready-generation request fails explicitly.

Product commands carry owner intent and opaque references. They contain no model-profile choice, saved bearer, resolved route bundle, raw Task artifact authority or Access policy flags. Connections owns Gateway setup/pairing, integration, source configuration and Observe reviews through the same AppWire command/query surface. The caller supplies only the bounded setup address and explicit review or confirmation intent. Provider and Vault adapters verify and persist pairing credentials; tokens, signed pairing proofs and keychain writes are not a Flutter workflow. Responses expose safe owner snapshots and actions.

Owner services retain admitted background work and durable command identity. Reads and `events_v2` observe owner state; query timeout, screen disposal and native broker disposal do not cancel a Run or Task. A lost acknowledgement is recovered with the same command or operation identity through its owner API. Conversation owns Run event publication and Session projection. The Vault lifecycle queue retains only its lifecycle operations; there is no generic product worker action map or approve/execute/collect coordinator in App or FFI.

`floe_native_host_acquire` creates an independently owned callback lane under an admitted host request before serial product work can wait on native metadata. The lane retains the verified caller, `LocalContextHost` and shared admission token, with no `FloeHandle` pointer. Its command/query entry points accept only native registration, poll, completion, failure and disposal intents. Registrations are host-issued and bound to the exact Person/device/runtime epoch; completion must match that registration and an outstanding broker request. The lane can outlive the core handle, rejects calls after closure and disposes only its own exact registrations. Flutter can therefore service native callbacks while the product transport awaits their results.

`AppHost` precreates one retirement worker before exposing request admission. That worker owns the lifetime-root service `Arc`; the host keeps a weak lookup, and each admitted request releases its caller/service ownership before dropping the guard that announces completion. Closing admission first retires native callback registrations, then waits for admitted requests and invokes owner shutdown. Each close hook is isolated from another hook's panic, and terminal close state is recorded and signalled even on failure. The worker performs final service/runtime destruction without borrowing the opaque core handle.

The first close establishes one 175-second total caller budget, shared by all concurrent or repeated closes: 35 seconds for admitted requests, 60 for a lifecycle operation, 35 for generation retirement, 5 for the lifecycle runtime, 35 for Day retirement and 5 margin. Timeout records `Closed` with `HostError::Shutdown`; this permanently rejects new admission and does not assert a clean drain. The same retirement worker retains unfinished cleanup and resources until actual completion. Later close calls return the stored failure without starting a worker or renewing the deadline. Core and callback-lane free catch panics without unwinding through the C boundary.

The local C ABI consists of open, command/query/events v2, the independent native callback-lane acquire/command/query/free operations, protocol version and string/core free. Separate remote pairing/Access and old Day/Actions/LocalContext/AgentVault/fixture entry points are absent. App wire 2 and protocol 1 remain unchanged; callers and the bundled native library must come from the same source snapshot. Schema numbers do not imply compatibility with an obsolete local profile or binary.

## Local Go server

`server/internal/node` assembles the semantic owners, durable storage and connection-scoped provider runtimes. Trust issues immutable authenticated client principals and separate operator-session principals, owns producer and issuer identity, and atomically persists client issuance with enrollment activation. Its durable revocation tickets fence source use until Integrations completes the exact cleanup. Integrations owns source records, setup operations, scopes, provider identity, revisions, cached snapshots and cleanup. Views owns typed queries, normalized results and reader contracts. Authority combines Trust and Integrations fences with bounded signed admission, staging and one-use release. Day's private `calendar.mirror` acquisition has its own product proof domain; it carries no assistant grant or consumer.

Pairing owns immutable start/proof receipts, local confirmation, operator approval and credential delivery. Enrollment identity is the signed challenge UUID; the pairing UUID identifies the client and attempt. An interrupted activation has explicit operator Resume and Abort actions. Resume requires the original protected token, proof, still-valid challenge and Trust revision. Abort requires authoritative non-commit. Committed activation wins cancellation, and uncertain or missing inputs remain repair errors. Rust publishes Paired only after its exact local credential commit and readback, and eligible repair reconciliation remains fenced by the original Pending credential expectation.

Credentials owns context-aware exact Keychain slots. A serialized native worker and bounded waiting observers retain mutation ownership through late write/readback settlement. Three-second observation limits and noninteractive, device-only Security calls preserve missing, locked, unavailable and timed-out outcomes. Pairing requests have a ten-second observation limit. Inference owns durable operator configuration, purpose selection, account readiness, structured and Agent execution, and accounting; concrete model adapters live under `inference/providers` and `inference/codex`. Schema 2 product invocations pin a purpose capability revision and verified client principal. The operator-only `ProbeTarget` diagnostic accepts a registered target with fixed synthetic input, no tools and a bounded output; its total 40-second deadline includes readiness. Startup initializes local state without model dispatch.

`transport/http` owns strict framing, routes, headers, cookies and redacted response mapping, calling typed owners directly. Paired routes reject nonempty Origin; operator mutations require the current session, matching Origin and CSRF. No CORS, product model selection, recipient-consent fields or Go provider-write API is exposed. Rust Access owns client-side source-processing permission; the Gateway independently validates its producer, client, source and provider-account boundaries before dispatch and release.
