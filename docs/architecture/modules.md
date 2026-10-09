# Modules and ownership

This is the stable semantic ownership map for the Rust workspace. Package paths match the current workspace layout.

## Source processing and product-read ownership

[ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) defines Access-owned DeviceOnly/GatewayAllowed source processing. Context projects against the exact prepared model plan; neither the plan nor pairing grants source permission. Health requires an independently verified local privacy-transform receipt for Device and Gateway reasoning and remains HighlySensitive.

Direct Day display has a separate Access product-read admission over the configured source and actual OS/provider permission. It cannot become assistant Observe, model-processing or Action permission. The source and Day contracts below describe the current owner boundaries; build qualification is tracked separately from this ownership map.

| Layer | Package / path | Owns |
|---|---|---|
| Contract | `floe-kernel` — `crates/contracts/kernel` | IDs and small shared values |
| Contract | `floe-model-contract` — `crates/contracts/model` | backend-neutral bounded schema, output format and DeviceModel wire values; no domain or authority dependencies |
| Contract | `floe-context-contract` — `crates/contracts/context` | source views, grants, processing restrictions, provenance, coverage and Health-transform evidence values |
| Contract | `floe-agent-contract` — `crates/contracts/agent` | prepared-model, canonical journal, Tool/Delegate, immutable Task receipt, blockage and artifact contracts |
| Contract | `floe-conversation-contract` — `crates/contracts/conversation` | role-neutral agent/conversation identities, transcript/admission/checkpoint values and content-addressed evidence references; no persistence or authorization |
| Runtime | `floe-execution` — `crates/runtime/execution` | scoped cancellation, monotonic budget leases, dispatch facts and immutable per-attempt accounting |
| Runtime | `floe-agent-runtime` — `crates/runtime/agent` | common prepare/project/model/Tool/Delegate Engine, validated final payloads and canonical journal projection |
| Module | `floe-conversation-core` — `crates/modules/conversation-core` | neutral transcript append/replay, owner-driven Manager and TaskExecution recorder open/output/close/retirement, bounded transcript read and checkpoint transitions; semantic owners retain scheduling and command admission |
| Module | `floe-a2a` — `crates/modules/a2a` | bounded versioned transport-neutral exchange, peer ID mapping and host Task/peer exchange/cancellation observation ports; no HTTP binding |
| Module | `floe-access` — `crates/modules/access` | source grants and immutable reviews/receipts; operation authorization policy, immutable operation subject/review reference/decision receipt; source-processing/model dispatch fences and separate product Calendar read permits |
| Module | `floe-connections` — `crates/modules/connections` | Connection, OAuth and pairing lifecycle; durable Calendar and standing personal source identity, resources, local configuration CAS, `SourceAuthority` and trusted native subject identity |
| Module | `floe-inference` — `crates/modules/inference` | Gateway-primary preparation with verified-absence local fallback, owned prepared calls and Access-consumed attempt dispatch/accounting |
| Module | `floe-knowledge` — `crates/modules/knowledge` | Memory/Playbook semantics, safe memory review and user decisions, explicit learning discovery, claim journals and common-Engine Learner lifecycle |
| Module | `floe-day` — `crates/modules/day` | bounded Day selections and local manual commands, immutable command replay, durable refresh, multisource Calendar cache and mirror CAS, Action result collection, and a typed inward port for direct external Calendar commands |
| Module | `floe-context` — `crates/modules/context` | authorized source reads/projections, source review evidence, metadata-only cache inspection, provenance, coverage and freshness |
| Module | `floe-calendar-operations` — `crates/modules/calendar_operations` | immutable external-effect normalization and identity, source/event checks, Access authorization consumption, dispatch intent CAS, provider receipts, uncertainty and reconciliation |
| Module | `floe-experts` — `crates/modules/experts` | registry/directory, immutable binding review, Task admission/journal/receipt, typed blockage and common Engine endpoint |
| Module | `floe-conversation` — `crates/modules/conversation` | Session, root Run, transcript, exact validated-batch continuation and coverage reauthorization, finalization and durable user interactions (origin, reviewed target, lifecycle, decision intent, resume linkage) |
| Extension | `floe-experts-builtin` — `crates/experts/builtin` | pure package roles, source-tool schemas, domain judgment and final payload construction |
| Platform | `floe-diagnostics` — `crates/platform/diagnostics` | privacy-safe tracing, correlation and diagnostic export |
| Platform | `floe-native` — `crates/platform/native` | native host drivers and secure-key/platform access |
| Adapter | `floe-provider-adapters` — `crates/adapters/providers` | model/source/control transports and OS/HTTP adapter implementations |
| Adapter | `floe-vault` — `crates/adapters/vault` | encrypted storage engine and owner-specific repository adapters |
| Composition | `floe-app` — `crates/app` | concrete service construction and typed service handles |
| Binding | `floe-protocol` — `crates/bindings/protocol` | versioned product wire DTOs only |
| Binding | `floe-ffi` — `crates/bindings/ffi` | ABI, host lifetime and DTO conversion |

## Ownership rules

A stateful concept has one semantic owner. Storage location, call-site convenience or composition does not create a second owner.

Builtin Programs depend on owner contracts and pure domain helpers. Experts owns their shared Engine endpoint; packages have no separate model loop, source host or Conversation publisher. Context implements the Experts and Knowledge inverse projection/evidence ports using actual source owners. Vault implements owner-defined storage ports and applies owner-produced transitions in encrypted transactions. Repository location does not move admission, recovery or review policy into storage.

- **App** constructs immutable owner and adapter handles and supplies explicitly trusted shipped manifest/consumer pairs. `ContextTrustedConsumerCatalog` validates those pairs without changing their typed namespace; Access derives and enforces View categories, purposes, processing policy and the review policy digest. Source setup, configuration and Observe are Connections commands over owner-produced review references. Product input cannot supply a raw grant scope or replace reviewed authority. After `AppHost` admits a verified `HostRequest`, one stateless typed router dispatches product commands, queries and bounded observations. Direct external Calendar commands enter through Day product routes; Day's inward port is implemented by Calendar Operations and held weakly to avoid a runtime `Arc` cycle. Assistant feature settings, proposal submission, operation decisions and calendar-policy settings use the Conversation product projection; Access remains the semantic policy owner. Product command/query groups are Conversation, Connections, Day and Memory. No Actions, Access, Experts or top-level Calendar Operations product namespace is exposed.
- **Protocol/FFI** validate and convert product DTOs to the App API and convert results back. FFI performs no product owner dispatch; the product API uses domain types rather than serialized DTOs and has no protocol or FFI dependency. Runtime control and NativeHost callbacks remain separate lanes.
- **Adapters** implement owner-defined ports. A provider may hold credentials or perform transport without inheriting Access or Context policy.
- **Vault** may physically store multiple owners' records. Shared storage does not grant cross-module table or policy ownership. A private static layout declaration drives creation and existing-open inspection. All current encrypted-layout-3 tables and seed rows are created in one transaction; Registry and interaction absence is represented by rows, not optional schemas. Existing open inspects without creating or repairing DDL. A durable identity-bound creation marker precedes key insertion and is retired only after validation/checkpoint/fsync; interrupted creation remains unavailable with explicit `IncompleteCreation`, preserving its key and artifacts. Unknown I/O failures never imply reset or replacement-key permission. Gateway secrets share the existing encrypted Vault while their workflow remains Connections-owned.
- **Conversation Core custody** is a neutral transcript recorder, not a scheduler. Conversation owns Manager admission, Session CAS, Run lifecycle, interaction handling, cancellation and recovery. Vault binds each Manager New, Continue and linked Resume Run to its exact Core input and opens a fresh HostRun recorder in the owner transaction. New appends one User entry; Continue and Resume reuse the original User entry and create fresh Run/recorder identities. Continue carries an exact pending batch, cursor and model pin through empty or batch-only lineage until a child claims that cursor. Linked Resume retains its origin and input but starts fresh model work with an interaction marker. Exact command, admission, output and close retries return immutable receipts without dispatch. Core has no queue, scheduler or effect authority.

  Experts owns TaskExecution lifecycle while Core records one transcript segment per admitted Task execution. Vault maps the complete Task execution key to exactly one Core Run and appends the delegated input when the existing Submitted-to-Working CAS opens its TaskExecution recorder. A later Task for the same verified Person, registry instance, installation, assignment, package/version and definition continues that assignment transcript from its exact pinned head. Core has no Task journal, receipt, dispatch or recovery authority.

  The default Manager identity is stable per Person and pins definition `manager-role` at the prompt policy revision (currently 8). Run/Task IDs and the Expert registry environment revision are not part of that identity. The exact Session-to-Conversation/branch binding is persisted; a changed definition uses a new identity-bound conversation. A bounded Session metadata/CAS shell contains no growing message-vector history. Typed User, Assistant, Capability, Delegation and textless Interaction entries are normalized and linked to Core. Stable public cursor aliases resolve through indexed exact/reverse reads; missing, ambiguous and stale aliases fail explicitly, and byte limits include hydrated evidence.

  Output, ToolResult, DelegationResult and Interaction evidence commit with their owner journal/evidence and typed Core entry. Delegation checks the actual Task snapshot, receipt and producer Run journal. A narrowly scoped Core recovery contribution permits exact authenticated late TaskResult evidence under an open recorder when a terminal or pending-terminal Run still has unresolved model accounting. Vault proves the receipt in the same transaction; recovery does not fabricate Working state or erase uncertainty. Current-generation settlement and stale-generation retirement remain distinct fenced transitions.
- **Conversation Core storage** supports Core revision 3 and output extension revision 2. Normalized rows retain immutable input/output receipts, per-entry Task evidence, owner links and the chained transcript commitment. Owner and transcript revisions remain separate, and owner/Core writes compose inside one bounded Vault transaction without external I/O. Stored revision-2 Core or revision-1 output markers fail closed; prior bytes remain untouched. Startup validates stored receipts and commitments. Head, exact and reverse-page reads are bounded. Fresh development profiles are the supported scope: there is no old Session import, migration chain, dual read/write, reset or database/key deletion. An incompatible profile remains untouched and unavailable.
- **Manager archive custody** commits an exact completed Core prefix, summary, coverage, archive manifest and Session CAS together. Protected active work, pending batches, open interactions, Continue origins, pending Resume slots, uncertain effects and archive references block unsafe compaction. Archive reads reconstruct typed records from normalized Core. Payload pruning is not part of this milestone; Unknown coverage stays Unknown and source revocation continues to apply.
- **Inference and Connections** are separate domains. Model routing does not infer source authorization, and a Connection does not authorize model processing.
- **Standing source truth is Connections-owned.** A `SourceConnection` binds Person, connector, stable connection and execution owner to lifecycle, current physical resources, local configuration revision, `SourceAuthority` and native subject identity. Native setup first persists a Pending source; an OS permission acknowledgement does not grant Observe. Reviewed configuration binds actual selected resources and subject evidence. Authority changes commit Access invalidation under a durable source reservation before publishing the exact source successor. A label-only successor with unchanged resource/account identities, mode, lifecycle, native subject and SourceAuthority instead settles its local presentation command and source CAS atomically, without a durable reservation fence, Access receipt or grant mutation. Native observation precedes that transaction; positive and negative replay use the same journal. Local CAS revision and provider revision remain distinct. Source identity/resource/subject changes stale earlier evidence; a label is display metadata, not permission.
- **Calendar mirrors are Day-owned observations.** Each source/resource retains its latest complete acquired interval; a complete empty batch replaces that interval, while failure preserves the prior complete interval with failure metadata. Context resolves all configured Calendar sources and performs Access-admitted acquisition outside storage transactions. Day reconciles full source/provider/calendar/external-event origins and atomically compares the complete inventory, successful-source fences, mirror expectation and refresh record. Drift rejects the whole commit. New or reinserted events receive the next mirror revision as their Day revision, preventing stale EventId/revision reuse after cache eviction; external revisions remain exact tagged evidence.
- **Day commands are immutable and bounded.** Manual mutation and refresh share a Person/command UUID namespace and a 4,096-record capacity. Changed device, command kind or normalized intent conflicts; replay returns the original result before new effects. Manual mutation applies Day’s pure transition, changed rows, bounded safe snapshot and receipt in one transaction. `DayReadQuery` bounds the selected display/Action/local-source result without loading unrelated history. Day uses its admitted executor generation, shared admission/drain guard and nonserializable `DayWriteFence` for new local mutations and collection commits.
- **Context reads Day evidence through separate bounded read-only budgets.** Context owns the Task/Note evidence shapes and serialized `NativeContextView` projection limit. The App adapter maps the distinct Day acquisition budget—at most 128 selected rows, at most 4 MiB of stored JSON payload before serde decoding, and Day's existing 4 MiB projected-item allowance—to `DayReadQuery`. The Vault query applies exact Person/OpenTasks/CurrentNotes predicates, bounds rows with an overflow sentinel, and rejects count/byte overflow before decoding the extra payload. Context receives no Day repository or mutation capability for these projections.
- **Fresh Day status is a read-only observation.** `CalendarAcquisitionPort::inspect_sources` is implemented by Context over actual configured-source and native/Gateway metadata. A fresh snapshot compares cached versions/resources, active fences and available permission evidence; changed, absent or unverified sources cannot appear Current. An uncovered date stays stale/pending. This query does not prompt, acquire provider events, grant permission, persist new status or implicitly refresh. Immutable command replay returns its historical snapshot unchanged.
- **Product Calendar authority is separate.** Access binds a product permit to verified Person/device, the configured exact source/resources, current permission/identity, refresh/read operation, range, limits, expiry and original cancellation. Context releases only matching bounded results. Native product refresh can operate with the agent Vault locked under current OS permission; Gateway reads require the ready-generation signer, pin and private credential lease, and return VaultLocked while those capabilities are unavailable. The private `day_calendar_admission` / `day_calendar_release` wire for `day_refresh` / `calendar.mirror` cannot be replayed as an assistant grant or model permission.
- **Context and Access** are complementary: Context owns evidence, acquisition and projection semantics; Access owns the exact source/processing admission and dispatch/release policy. `ContextSourceReview` and `ContextDependencyResolver` use required repository/native/Gateway capabilities; App has no source-policy workflow. Context implements Day, Experts and Knowledge inverse ports without an optional whole-service lookup or a reverse owner dependency.
- **Native acquisition completion is validated at its source boundary.** App verifies native-host registration, runtime epoch, caller Person/device and host epoch, then binds that caller identity into the completion. The native Attention and Personal acquisition exchanges validate request correlation, mode/domain shape, typed projection schema and freshness, native-subject identity and catalog bounds before returning an accepted result. The Personal provider consumes the exact Health transform receipt bound to the outstanding request, process/device, source subject, output digest and expiry. Context retains its source-read contract checks and coverage semantics; App host registration does not own source-result policy. People and Attention project Personal evidence; Wellbeing remains HighlySensitive after its mandatory local transform. Context unions actual evidence classes with admitted role classes. Gateway processing additionally requires the grant’s allowed categories and current exact Gateway admission. A transform, a manifest or local model selection cannot lower sensitivity or grant permission.
- **Standing Observe grants name stable sources.** `GrantSourceBinding` identifies Person, Connection, Connector and execution owner, and review cannot replace it. Grants name logical connection/View resources; physical Contacts handles and Calendar IDs remain source configuration and observed provenance. `GrantAuthority` is the permission/state/scope epoch, while Connections owns `SourceAuthority`. Authority-changing configuration uses an Access invalidation receipt and requires review before renewed Observe; it never silently expands an active grant. Native/provider drift independently stales old dependencies, without changing Expert bindings.
- **Calendar grant review classification is Access-owned.** Access matches grants against the exact `GrantSourceBinding`, ignores revoked grants, classifies absent or paused grants as requiring Observe, and preserves the exact observed grant identity and authority for paused or active grants. Context acquires current connection evidence and projects Access's classification into source requirements; credential expiry still projects as Reconnect.
- **Standing consumers reload Connections.** Context acquisition, candidate discovery, source review evidence and dependency reauthorization, plus Calendar Operations proposal/dispatch checks, use the exact current `SourceConnection` and source-operation fence. Native Calendar and Contacts reads bind every selected physical resource; Attention and Wellbeing bind their native singleton subject. Hosted assistant reads use signed View preview/admission/read/release and retain one exact grant/source dependency per contributor. Observe and product Calendar reads have separate authority.
- **Permission and provenance resources differ.** `ContextDependency.resources` names grant permission scope; `source_resources` names the exact observed physical set, with separate grant/source epochs. Access reloads immutable source reviews and current grant expectations at apply. Context rechecks current source and native/provider identity outside the grant transaction; Vault performs owner-defined grant/receipt CAS and invalidation inside it. Health transform evidence remains source-specific. No model-recipient consent record or digest replaces these checks.
- **Logical multi-source views preserve physical authority.** Context may deterministically merge a bounded set of source payloads, but the authorized read retains one dependency/scope binding per contributing source and every dependency is recorded and reauthorized independently.
- **Registry binding is not source permission.** Experts owns selected source references and binding revisions; Context owns candidate identities and exact-target acquisition; Connections owns source configuration; Access owns Observe and processing permission. Trusted consumer registrations come from explicitly supplied shipped manifests, independently of assignments. A Task retains its admitted selection while later source/grant/provider/OS checks remain live. Binding or enable changes affect future admission and do not upgrade, reroute or authorize the pinned source.
- **Conversation** owns Session creation admission, Run execution and executor-driven recovery. Session Start returns a typed positive receipt or structural refusal; storage failures retain admission uncertainty. Resume/Get/history are pure reads. The Flutter gateway explicitly settles an uncertain Start before a fresh user intent, while controller observation epochs prevent stale adoption without cancelling owner work. There is no second Session Recover command; see [authority and recovery](authority-recovery.md#conversation-session-admission-and-observation).
- **Conversation path:** the Manager production path uses normalized typed history and Core custody through the owner transaction. Expert production execution uses bounded history pinned by Task admission and isolated by verified assignment and definition; Experts remains the genuine Task lifecycle owner and Manager evidence source. The `floe-conversation -> floe-experts` relation remains for real delegation, and Core remains neutral. See [ADR 0035](../decisions/0035-conversation-core-and-a2a-boundaries.md).
- **A2A contract status:** `floe-a2a` carries external peer IDs and observations through explicit mapping and host-owned ports. It does not own authoritative Task state, authenticate its own transport, implement HTTP, or establish A2A standard conformance. Directory discovery and host Task lifecycle stay with their existing owners.
- **Experts** owns Task admission, pinned selection, canonical Task journals and immutable execution receipts. It also derives the isolated Expert conversation identity from the verified Person and admitted registry installation/assignment/package/version/definition, pins that conversation's exact head in canonical host Task input, and maps each Task execution to one fresh Core Run segment. The existing Working CAS appends the exact delegated input and opens the TaskExecution recorder; terminal Task custody settles or remains fenced with the Task owner. Its common endpoint runs a retained pure Program through the role-neutral Engine with bounded history from only that pinned assignment. Conversation records the settled Task receipt once as a DelegationResult; it does not duplicate Task ModelResults or charge them twice. Context supplies actual source reads and prepared-plan projections through Experts-owned ports. Assistant feature settings and Task-origin reviews share the same durable review/command policy.
- **Knowledge** owns explicit learning eligibility, immutable claim inputs, candidate staging, user review and confirmed-memory projection. Learner is a bounded role of the common Engine with its own claim journal and authenticated head. Recovery retains known output without inference replay and preserves unknown charges. Knowledge owns background scheduling, foreground preemption and bounded shutdown. Its public review/decision projections omit storage hashes and raw evidence references.
- **Calendar Operations** owns Calendar Create/Update/Delete, immutable effect identity, source/event fences, pre-dispatch intent, native outcome evidence, explicit reconciliation and Day collection tickets through the existing encrypted repository. Access owns operation policy, operation authorization subjects, review references and decision receipts. Day defines the direct-manual inward port implemented by Calendar Operations; direct Day instructions omit the independent Expert-policy revision while retaining actor, source and OS checks. Expert proposals remain inert until the exact Access review is consumed. Native adapters implement single-use execution and causal receipt lookup. Conversation publishes the immutable approval target and resumes the linked Run against the existing operation after resolution.

## Dependency enforcement

The allowed compile-time DAG is machine-readable in
[`tools/architecture/module-dependencies.json`](../../tools/architecture/module-dependencies.json).
Run:

```sh
python3 tools/architecture/check_boundaries.py
```

The checker reads the actual Cargo manifests. Do not add a second hand-maintained "current dependency graph" to documentation.

## Gateway storage boundary

Go Node composition admits one process-locked encrypted profile before constructing
Trust, Integrations, Inference or connector runtimes. The compiled credentials
provider owns the purpose-separated root-key slot: production OS keyring, explicit
development private-file custody. A create-only identity/attempt marker and
authenticated root seal distinguish fresh creation from an existing unavailable
profile; missing keys and old plaintext never authorize replacement or migration.
The owner-scoped root layout is explicitly versioned. Ready is published only after
first Trust persistence succeeds; interruption before that retains initializing
evidence and reports creation_incomplete, never regenerating identity or keys.

`server/internal/adapters/storage` implements the Trust, Integrations, and Inference repository
ports over scoped authenticated file capabilities. It owns `trust.json`,
`producer-identity.json`, `admin-token`, `integrations.json`, and `inference.json` file selection,
strict JSON encoding/decoding, and encrypted atomic replacement. The Trust port
reports presence and write disposition for its complete state, producer identity,
and operator credential; Trust decides fresh initialization, rejects partial or
damaged bootstrap, validates semantic state, and latches unavailable authority on
integrity or indeterminate writes. The Integrations port reads and replaces one
complete typed snapshot; Integrations validates its records, advances revisions,
and adopts only a confirmed commit. The separate credential adapter accepts only
an owner-defined connection binding for writes and deletion; runtime credential
reads are scoped to the exact binding. No secret token is included in the Trust
repository read snapshot. Explicit local token retrieval remains a separate
read-only adapter operation.

Inference owns its typed complete-snapshot contract, configuration validation,
provider-target preparation, and engine adoption after a confirmed commit. The
storage adapter retains the existing bounded JSON shape and encrypted atomic
replacement. The credential adapter accepts a separate `FLOE_KEY_*` reference
for Inference provider-key reads and writes; runtime connection credential reads
remain scoped to the exact binding. Provider-key writes still precede the
Inference snapshot commit, while target/provider removal leaves its old
credential slot. That cross-store recovery lifecycle remains a bounded follow-up
for parent design review. No secret values enter owner snapshots.

Logical file purpose and root identity are AEAD-bound. Trust, Integrations and
Inference retain their schema, validation, transitions and indeterminate-write
fences behind typed repositories. Trust state/producer identity/admin token,
integration journals, inference configuration and Gmail index are encrypted.
Node releases its profile lease after owner shutdown and after the storage
lifetime guard drains in-flight file I/O. Closed capabilities reject later
writes. Explicit local administrator token retrieval opens an existing root
read-only without starting owners or creating credentials. Public
profile/identity markers, diagnostic logs and operator-supplied environment
input remain outside the encrypted payload store.

Each Gateway semantic owner receives a distinct storage scope; connector factories
receive only the connectors subtree. Per-file write locks serialize replacement,
while a shared lifetime guard fences Close. Indeterminate replacement makes the root unavailable to new Node admission and
owner readiness checks. Integrity failures propagate to the semantic owner, which
retains its fail-closed state.
Determinate pre-replacement filesystem errors remain retryable. Trust may create its
identity only in a newly admitted root; loss of all Trust files is not fresh creation.

Connections resource grouping is presentation metadata retained from the source
catalog. Native Calendar provides explicit account identity and label, separately
from calendar title. Product projections derive opaque Person/Connection-scoped
group references. Groups never grant authority or replace exact resource selections.
Day projections without a group header use the account-qualified resource label,
within their existing display bounds; group-aware views retain the separate title.
The overview has no aggregate revision: each item retains its own CAS revision, and
Flutter orders concurrent overview responses by request generation.

The macOS Calendar System access card observes only OS authorization through a
narrow Flutter application port, implemented at the native MethodChannel boundary.
AppRuntime injects that port; the card owns read generations, resume observation and
explicit settings-navigation feedback. Neither inspection nor opening System Settings
enumerates calendars, requests permission, configures a source or changes Observe.
Source setup still owns permission requests through its admitted native broker path.
This preserves the baseline macOS card without adding it to iOS or treating stored
source readiness as evidence of current OS permission.

### Calendar collection display correlation

Context contracts define person-scoped opaque source/resource display references
shared by Connections, Day and Conversation navigation. They preserve Connections'
existing reference bytes and carry no access or command authority. Day alone owns
collection timestamps/failures and cache-source inspection. Flutter composition can
pass that immutable Day coverage into Connections detail; exact identity joins show
historical collection facts, never infer permission or current synchronization from
labels. Connections does not acquire events or maintain a second collection clock.

Configure finalization is a private Connections workflow with only the source
repository, source evidence and shared owner lifetime capabilities. It does not
receive pairing, Gateway, integration or product-store dependencies. The broader
Connections service still coordinates admission/replay; this bounded extraction is
not a claim that all source workflows have been decomposed.
