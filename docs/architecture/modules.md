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
| Runtime | `floe-execution` — `crates/runtime/execution` | scoped cancellation, monotonic budget leases, dispatch facts and immutable per-attempt accounting |
| Runtime | `floe-agent-runtime` — `crates/runtime/agent` | common prepare/project/model/Tool/Delegate Engine, validated final payloads and canonical journal projection |
| Module | `floe-access` — `crates/modules/access` | source grants, immutable reviews and receipts, source-processing/model dispatch fences, separate product Calendar read permits |
| Module | `floe-connections` — `crates/modules/connections` | Connection, OAuth and pairing lifecycle; durable Calendar and standing personal source identity, resources, local configuration CAS, `SourceAuthority` and trusted native subject identity |
| Module | `floe-inference` — `crates/modules/inference` | Gateway-primary preparation with verified-absence local fallback, owned prepared calls and Access-consumed attempt dispatch/accounting |
| Module | `floe-knowledge` — `crates/modules/knowledge` | Memory/Playbook semantics, safe memory review and user decisions, explicit learning discovery, claim journals and common-Engine Learner lifecycle |
| Module | `floe-day` — `crates/modules/day` | bounded Day selections and manual commands, immutable command replay, durable refresh, multisource Calendar cache and mirror CAS, Action result collection |
| Module | `floe-context` — `crates/modules/context` | authorized source reads/projections, source review evidence, metadata-only cache inspection, provenance, coverage and freshness |
| Module | `floe-actions` — `crates/modules/actions` | proposals, review/approval, idempotent external actions and outcome reconciliation |
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

- **App** constructs immutable owner and adapter handles and supplies explicitly trusted shipped manifest/consumer pairs. `ContextTrustedConsumerCatalog` validates those pairs without changing their typed namespace; Access derives and enforces View categories, purposes, processing policy and the review policy digest. Source setup, configuration and Observe are Connections commands over owner-produced review references. Product input cannot supply a raw grant scope or replace reviewed authority.
- **Protocol/FFI** convert product intent and results. They do not decide execution topology.
- **Adapters** implement owner-defined ports. A provider may hold credentials or perform transport without inheriting Access or Context policy.
- **Vault** may physically store multiple owners' records. Shared storage does not grant cross-module table or policy ownership. A private static layout declaration drives creation and existing-open inspection. All current encrypted-layout-3 tables and seed rows are created in one transaction; Registry and interaction absence is represented by rows, not optional schemas. Existing open inspects without creating or repairing DDL. A durable identity-bound creation marker precedes key insertion and is retired only after validation/checkpoint/fsync; interrupted creation remains unavailable with explicit `IncompleteCreation`, preserving its key and artifacts. Unknown I/O failures never imply reset or replacement-key permission. Gateway secrets share the existing encrypted Vault while their workflow remains Connections-owned.
- **Inference and Connections** are separate domains. Model routing does not infer source authorization, and a Connection does not authorize model processing.
- **Standing source truth is Connections-owned.** A `SourceConnection` binds Person, connector, stable connection and execution owner to lifecycle, current physical resources, local configuration revision, `SourceAuthority` and native subject identity. Native setup first persists a Pending source; an OS permission acknowledgement does not grant Observe. Reviewed configuration binds actual selected resources and subject evidence, commits Access invalidation under a durable source reservation, then publishes the exact source successor. Local CAS revision and provider revision remain distinct. Source identity/resource/subject changes stale earlier evidence; a label is display metadata, not permission.
- **Calendar mirrors are Day-owned observations.** Each source/resource retains its latest complete acquired interval; a complete empty batch replaces that interval, while failure preserves the prior complete interval with failure metadata. Context resolves all configured Calendar sources and performs Access-admitted acquisition outside storage transactions. Day reconciles full source/provider/calendar/external-event origins and atomically compares the complete inventory, successful-source fences, mirror expectation and refresh record. Drift rejects the whole commit. New or reinserted events receive the next mirror revision as their Day revision, preventing stale EventId/revision reuse after cache eviction; external revisions remain exact tagged evidence.
- **Day commands are immutable and bounded.** Manual mutation and refresh share a Person/command UUID namespace and a 4,096-record capacity. Changed device, command kind or normalized intent conflicts; replay returns the original result before new effects. Manual mutation applies Day’s pure transition, changed rows, bounded safe snapshot and receipt in one transaction. `DayReadQuery` bounds the selected display/Action/local-source result without loading unrelated history. Day uses its admitted executor generation, shared admission/drain guard and nonserializable `DayWriteFence` for new local mutations and collection commits.
- **Fresh Day status is a read-only observation.** `CalendarAcquisitionPort::inspect_sources` is implemented by Context over actual configured-source and native/Gateway metadata. A fresh snapshot compares cached versions/resources, active fences and available permission evidence; changed, absent or unverified sources cannot appear Current. An uncovered date stays stale/pending. This query does not prompt, acquire provider events, grant permission, persist new status or implicitly refresh. Immutable command replay returns its historical snapshot unchanged.
- **Product Calendar authority is separate.** Access binds a product permit to verified Person/device, the configured exact source/resources, current permission/identity, refresh/read operation, range, limits, expiry and original cancellation. Context releases only matching bounded results. Native product refresh can operate with the agent Vault locked under current OS permission; Gateway reads require the ready-generation signer, pin and private credential lease, and return VaultLocked while those capabilities are unavailable. The private `day_calendar_admission` / `day_calendar_release` wire for `day_refresh` / `calendar.mirror` cannot be replayed as an assistant grant or model permission.
- **Context and Access** are complementary: Context owns evidence, acquisition and projection semantics; Access owns the exact source/processing admission and dispatch/release policy. `ContextSourceReview` and `ContextDependencyResolver` use required repository/native/Gateway capabilities; App has no source-policy workflow. Context implements Day, Experts and Knowledge inverse ports without an optional whole-service lookup or a reverse owner dependency.
- **View sensitivity is type-owned.** People and Attention project Personal evidence; Wellbeing remains HighlySensitive after its mandatory local transform. The host verifies the transform operation independently against the outstanding request, process/device/source identity, expiry and output digest before trusted evidence is produced. Context preserves those facts in coverage and unions actual evidence classes with admitted role classes. Gateway processing additionally requires the grant’s allowed categories and current exact Gateway admission. A transform, a manifest or local model selection cannot lower sensitivity or grant permission.
- **Standing Observe grants name stable sources.** `GrantSourceBinding` identifies Person, Connection, Connector and execution owner, and review cannot replace it. Grants name logical connection/View resources; physical Contacts handles and Calendar IDs remain source configuration and observed provenance. `GrantAuthority` is the permission/state/scope epoch, while Connections owns `SourceAuthority`. Explicit configuration uses an Access invalidation receipt and requires review before renewed Observe; it never silently expands an active grant. Native/provider drift independently stales old dependencies, without changing Expert bindings.
- **Standing consumers reload Connections.** Context acquisition, candidate discovery, source review evidence and dependency reauthorization, plus Actions proposal/dispatch checks, use the exact current `SourceConnection` and source-operation fence. Native Calendar and Contacts reads bind every selected physical resource; Attention and Wellbeing bind their native singleton subject. Hosted assistant reads use signed View preview/admission/read/release and retain one exact grant/source dependency per contributor. Observe and product Calendar reads have separate authority.
- **Permission and provenance resources differ.** `ContextDependency.resources` names grant permission scope; `source_resources` names the exact observed physical set, with separate grant/source epochs. Access reloads immutable source reviews and current grant expectations at apply. Context rechecks current source and native/provider identity outside the grant transaction; Vault performs owner-defined grant/receipt CAS and invalidation inside it. Health transform evidence remains source-specific. No model-recipient consent record or digest replaces these checks.
- **Logical multi-source views preserve physical authority.** Context may deterministically merge a bounded set of source payloads, but the authorized read retains one dependency/scope binding per contributing source and every dependency is recorded and reauthorized independently.
- **Registry binding is not source permission.** Experts owns selected source references and binding revisions; Context owns candidate identities and exact-target acquisition; Connections owns source configuration; Access owns Observe and processing permission. Trusted consumer registrations come from explicitly supplied shipped manifests, independently of assignments. A Task retains its admitted selection while later source/grant/provider/OS checks remain live. Binding or enable changes affect future admission and do not upgrade, reroute or authorize the pinned source.
- **Experts** owns Task admission, pinned selection, canonical Task journals and immutable execution receipts. Its common endpoint runs a retained pure Program through the role-neutral Engine. Conversation records the settled Task receipt once as a DelegationResult; it does not duplicate Task ModelResults or charge them twice. Context supplies actual source reads and prepared-plan projections through Experts-owned ports. Binding settings and Task-origin reviews share the same durable review/command policy.
- **Knowledge** owns explicit learning eligibility, immutable claim inputs, candidate staging, user review and confirmed-memory projection. Learner is a bounded role of the common Engine with its own claim journal and authenticated head. Recovery retains known output without inference replay and preserves unknown charges. Knowledge owns background scheduling, foreground preemption and bounded shutdown. Its public review/decision projections omit storage hashes and raw evidence references.
- **Actions** own Calendar Create/Update/Delete, manual and Expert-origin admission, immutable review/approval, pre-dispatch intent, native outcome evidence, explicit reconciliation and Day collection tickets through one encrypted repository. Product inputs use opaque destination choices or Day Event references; raw targets and source preconditions are resolved inside the owner. Experts prove historical Task/artifact provenance through an inward port implemented by Vault; Actions owns effect policy, without an Actions-to-Experts dependency. Native adapters implement single-use execution and causal receipt lookup. App constructs and forwards owner handles, and intelligence may propose inert artifacts without directly mutating providers.

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

The storage adapter provides scoped authenticated file capabilities. Logical file
purpose and root identity are AEAD-bound. Semantic owners retain their schema,
validation, transitions and indeterminate-write fences; they no longer concatenate
raw private payload paths. Trust state/producer identity/admin token, integration
journals, inference configuration and Gmail index are encrypted. Node releases its
profile lease after owner shutdown and after the storage lifetime guard drains in-flight file I/O. Closed capabilities reject later writes. Explicit local administrator
token retrieval opens an existing root read-only without starting owners or creating
credentials. Public profile/identity markers, diagnostic logs and operator-supplied
environment input remain outside the encrypted payload store.

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
