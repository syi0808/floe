# Canonical inference, product and dependency contracts

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

**Decision revision, 2026-10-02 13:46 UTC:** the three product/data choices are accepted; see [current accepted decisions](2026-10-02-architecture-refactor.md#101-accepted-user-decisions-and-clean-profile-cutover). Existing Floe development data may be discarded for a clean profile/schema without migration. No actual reset/deletion or implementation is authorized by this documentation update; new-system recovery safety remains mandatory.

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Proposed execution specification, not implementation or permission to implement. The main plan owns P0/T0/S1/S2/S3 order. This appendix owns the exact cross-language and public-owner contracts for review findings R1/R4/R7; other appendices link here rather than choosing alternate signatures. No production file, manifest or test changes and no executable validation are part of P0.

## 1. Rust planning and prepared-call contract

Place the following in `crates/modules/inference/src/ports/model_provider.rs`, exporting the public types from Inference. `ModelPlanRequest`, `PreparedModelPlan`, `ProcessingBoundary`, `ModelCapabilities` and `ModelBindingDigest` are the one agent-contract definitions in `crates/contracts/agent/src/model_plan.rs`; Inference reexports shared values rather than defining a second capability set. `BoxFuture` is the execution-owned generic alias reexported by agent-contract (§4.1). No duplicate request DTO belongs to Inference.

```rust
pub trait ModelProvider: Send + Sync {
    type Prepared: PreparedModelTransport + Send + Sync + 'static;
    fn observe_primary<'a>(&'a self, request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<PrimaryObservation<Self::Prepared>, ModelObservationError>>;
    fn observe_local_fallback<'a>(&'a self, request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<LocalObservation<Self::Prepared>, ModelObservationError>>;
}
pub enum PrimaryObservation<P> {
    Available(PreparedModelProfile<P>),
    Absent(PrimaryAbsence),
}
pub enum PrimaryAbsence { NoGatewayConfigured, PurposeNotConfigured, PurposeDisabled }
pub enum LocalObservation<P> {
    Available(PreparedModelProfile<P>),
    Unavailable(LocalAvailabilityReason),
}
pub enum LocalAvailabilityReason { Unsupported, Disabled, NotReady }
pub enum ModelObservationError {
    InvalidIdentity, InvalidInventory, CredentialRejected, PermissionDenied,
    Timeout, TransportUnavailable, Cancelled, StorageUnavailable, QuotaExceeded,
}
pub struct PreparedModelProfile<P> {
    pub capability: ObservedModelCapability,
    pub transport: P,
}
pub struct ObservedModelCapability {
    pub purpose: ModelPurpose,
    pub consumer: ModelConsumer,
    pub capabilities: ModelCapabilities,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
}
pub trait PreparedModelTransport: Send + Sync {
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget;
    fn generate<'a>(&'a self, request: CanonicalModelRequest,
        target: AdmittedDispatchTarget)
        -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>>;
}
pub struct ProviderUsageObservation {
    pub tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}
pub struct CanonicalModelResponse {
    pub output: Result<Vec<ModelStep>, AgentFailure>,
    pub usage: ProviderUsageObservation,
}
```

`PreparedModelProfile` is retained as a transport pairing type, not a product profile selection. Its old `profile: ModelProfile` field is removed; `ObservedModelCapability` has no profile ID, endpoint, provider, placement or recipient. `ModelCapabilities` is a set of closed agent capabilities, currently exactly `chat`. `ModelBindingDigest` is 32 opaque bytes, serialized only for non-secret journal correlation. `AdmittedDispatchTarget` privately binds the same selected Device/Gateway identity and generation; replace `matches(profile_id, recipient)` / `recipient()` with `matches(binding_digest, boundary) -> bool`. Construction remains possible only from consumed Access admission; a digest supplied by product callers is never authority. The prepared transport supplies its owner-private expected dispatch target through `dispatch_target()`, including the non-secret verified Gateway binding when applicable. This missing adapter-to-Access seam is clarified during S1 implementation: it is not serialized into the agent plan and carries no bearer/endpoint. Inference checks its digest/boundary against the observed capability; Access independently admits, consumes and revalidates the exact current binding. Returning the expected target alone grants no authority.

`ModelPlanRequest` carries `principal: String`, `device_id: String`, `purpose: String`, `consumer: String`, `required_capabilities: ModelCapabilities`; all are admitted/validated before observation. Purpose is one of `quick_response`, `everyday_assistance`, `deep_work`. No prompt or source content is needed to observe. `PreparedModelPlan` carries `operation_id: Uuid`, the exact admitted principal/device/purpose/consumer, capabilities, boundary and binding digest. The private prepared object owns the actual Gateway capability and checks current binding generation immediately before handoff and again before result release. Gateway identity drift is failure; downstream operator configuration is private, with the non-secret capability revision fence below.

The agent boundary is exactly:

```rust
pub trait ModelPort: Sync {
    fn prepare<'a>(&'a self, request: ModelPlanRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<Box<dyn PreparedModelCall>, AgentFailure>>;
}
pub trait PreparedModelCall: Send + Sync {
    fn plan(&self) -> &PreparedModelPlan;
    fn generate<'a>(&'a self, request: ModelRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ModelResponse, AgentFailure>>;
}
```

Selection order: observe Primary once; `Available` selects it; `Absent` alone allows one local capability observation; `Err` returns failure. Local `Unavailable` becomes model unavailable, never an empty successful observation. There is no `Failed` arm, `PurposeUnavailable`, `Vec::new()` recovery, transport-failure loop, role override, or second selector. Availability calls the same planner. Local fallback still needs current source permission and mandatory Health transformation. Projection happens against `prepared.plan()` before allocating ModelIntent/attempt. Generate never observes/selects again. Correction attempts keep the selected object and use fresh attempt/projection identities and source fences. Reopen never reconstructs and resends an unsettled model attempt.

Observation failure projection: InvalidIdentity→PolicyDenied; InvalidInventory→ServerModelInvalidOutput; CredentialRejected→CredentialExpired; PermissionDenied→PolicyDenied; Timeout→DeadlineExceeded; TransportUnavailable→ServerModelUnavailable; Cancelled→Cancelled; StorageUnavailable→StorageUnavailable; QuotaExceeded→QuotaExceeded. These are failure mappings, never absence mappings.

## 2. Exact paired Rust/Go inference wire

### 2.1 Routes, framing and version

A deliberate same-snapshot schema break uses `schema_version: 2` on the inference routes only. AppWire remains schema 2 / protocol 1 and unrelated signed source/provider namespaces do not change. No v1 decoder, alias route or dual backend is retained. Rust adapter private DTOs live at `crates/adapters/providers/src/gateway/inference_wire.rs`; transport at `gateway/inference.rs`. Go transport DTOs live at `server/internal/transport/http/inference_dto.go`; HTTP at `inference.go`.

| Method + route | Authority | Exact result |
|---|---|---|
| `GET /v1/inference-purposes` | Current paired bearer authenticated by Trust | `InventoryResponseDTO` (200) or typed error; no body/query parameters |
| `POST /v1/agent` | Same paired bearer; current Person/device/issuer binding | `AgentResponseDTO` (200) or typed error |
| `POST /v1/generate` | Gateway operator/admin session only; never a Rust product model path | `StructuredResponseDTO` (200) or typed error |
| `GET /v1/traces`, `GET /v1/traces/{trace_id}` | Operator/admin session only | Existing bounded privacy-safe diagnostic snapshots; paired product transport never calls these |

Successful authentication supplies an immutable `trust.Principal`; inference does not maintain `tokenHash` or compare bearer bytes. HTTP retains method/path/content-type/Origin/CSRF rules. Paired routes reject nonempty Origin and never enable CORS. Operator routes use the same admitted admin session and CSRF policy as management. All responses use `Content-Type: application/json`, `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`. Redirects/proxies are disabled in Rust. Non-JSON, invalid UTF-8, duplicate object members, trailing JSON, unknown keys at any fixed DTO level, null where not explicitly allowed, non-integral/out-of-range numbers, nonfinite numbers and schema other than 2 fail closed. Enforce duplicate-key rejection before Go typed decoding; `DisallowUnknownFields` alone does not reject duplicates. JSON Schema and model argument objects are the only intentionally open objects; their enclosing shapes and byte/depth bounds remain strict.

Limits: request body ≤98,304 bytes; response/error/inventory body ≤65,536 bytes. Read incrementally and stop before buffer growth beyond the bound. UTF-8 byte counts, not character counts, apply to string/content bounds. Integers are decimal JSON integers in `0..=9007199254740991` unless a narrower bound is listed. UUIDs use lowercase hyphenated non-nil canonical form; digests use lowercase hex. Missing optional fields and null are not interchangeable: below, `?` means omitted when absent; `T|null` is a required key with either value.

### 2.2 Inventory (complete strict object)

`InventoryResponseDTO = {schema_version: 2, purposes: {quick_response: PurposeCapabilityDTO, everyday_assistance: PurposeCapabilityDTO, deep_work: PurposeCapabilityDTO}}`. All three purpose keys are required; missing/extra keys are InvalidInventory, not absence. `PurposeCapabilityDTO` is a discriminated union:

- `{status:"available", capability_revision:Hex64, capabilities:["chat"]}`
- `{status:"not_configured"}`
- `{status:"disabled"}`

No `available` bool, recipient, `requires_external_consent`, placement, provider/model/effort or free-text reason is present. No extra fields are allowed on an absent entry. `available` requires a configured enabled target with usable provider/account capability at observation. A configured route whose account/readiness lookup fails returns an error, not `not_configured` or `disabled`; known temporary model/provider unreadiness is 503. `disabled` is explicit operator configuration, not an operational failure. `not_configured` means no route entry for that purpose.

`capability_revision` is a private-service HMAC-derived opaque digest of purpose, immutable configured target identity, route/config generation, credential/account generation and relevant model/effort configuration; the HMAC secret stays Go-private. Same facts produce the same revision; any bound change produces a different revision. It is not a bearer or permission. Rust pins it in prepared adapter state, checks the exact echo, and never displays it as route information. Inventory is authenticated; only 200+strict `not_configured` / `disabled` yields corresponding `PrimaryAbsence`. `NoGatewayConfigured` comes only from a successful exact local credential-slot read proving there is no committed binding. Locked store, malformed credential, unverified staged binding, wrong Person/device/producer, timeout, 401/403/404, incompatible schema and invalid JSON are errors.

### 2.3 Agent request

`AgentRequestDTO` has exactly these required keys:

| Key | JSON type/constraint | Rust origin → Go domain |
|---|---|---|
| `schema_version` | integer 2 | constant → decoder only |
| `purpose` | closed purpose string | prepared capability purpose → `Purpose` |
| `capability_revision` | Hex64 | pinned inventory revision → `AgentInvocation.CapabilityRevision` |
| `attempt_id` | UUID | `CanonicalModelRequest.attempt_id` → `AgentInvocation.AttemptID` |
| `data_classes` | sorted unique nonempty array, ≤3; `synthetic|personal|highly_sensitive` | admitted projection classes → `DataClasses` |
| `instructions` | nonblank UTF-8, ≤9,216 bytes | rendered immutable stable instructions → `Instructions` |
| `input` | strict `AgentInputDTO`, encoded ≤32,768 bytes | bounded canonical frames/catalog → `Input` |
| `max_output_bytes` | integer 1..16,384 | min(owner bound,16,384) → `MaxOutputBytes` |

`TemporaryAiContext`, `DeviceOnlyRaw`, `Credential` cannot be sent as reasoning classes; reject before network, never relabel them `personal`. Mandatory transformed Health stays `highly_sensitive`. Source processing admission already occurred in Rust; sending these declarations never confers permission in Go.

`AgentInputDTO = {messages:[MessageDTO], tools:[ToolDTO]}`. messages length 1..256; tools length 0..64, unique function names. Fixed variants:

- User: `{role:"user", content:String}`.
- Assistant text: `{role:"assistant", content:String}`.
- Assistant calls: `{role:"assistant", tool_calls:[ToolCallDTO], content?:String}`; 1..8 calls.
- Tool result: `{role:"tool", tool_call_id:CallID, content:String}`.
- `ToolCallDTO = {id:CallID,type:"function",function:{name:Alias,arguments:JsonObjectString}}`.
- `ToolDTO = {type:"function",function:{name:Alias,description:String,parameters:JsonSchemaObject,strict:false}}`.

`CallID` is 1..128 non-control UTF-8 bytes; Alias matches `^[A-Za-z0-9_-]{1,64}$`. Arguments are a JSON-encoded object string, not null/array; duplicate JSON keys are rejected. JSON schema parameters must be an object with `type:"object"`; accept the bounded provider-neutral schema vocabulary already emitted by the validated catalog, with total input bound and maximum nesting depth 32. An invalid catalog schema is InvalidInput, never substituted by `{type:"object",properties:{}}`. All text/schema/argument values fit within the enclosing 32,768-byte input. Enforce unique call IDs across messages; each pending call has exactly one following tool result before the next user/assistant message; no orphan result or unresolved final call. A historical call name need not exist in the *current* catalog, but new generated calls must resolve the current immutable catalog. Tool result content is the canonical serialized success/error object, preserving bounded failure and authorized content, not raw provider data.

Keep the existing stable alias `floe_{FNV1a64(tool_id):016x}`; reject collisions across tools and the delegation capability before I/O. The delegation alias derives from `floe.a2a.delegate`; its strict arguments are `{agent_id:String,message:String,context_refs?:[String]}`. Resolve agent ID, definition revision and context-ref bounds against the immutable current AllowedCatalog. `definition_revision` is taken from that catalog, never invented by Go/model output.

### 2.4 Agent response, usage and replay

`AgentResponseDTO = {schema_version:2,purpose:Purpose,capability_revision:Hex64,attempt_id:Uuid,trace_id:Hex32,output:[StepDTO],call_ids:[CallID],usage:UsageDTO}`. Every key is required. `output` length 1..16; encoded canonical mapped steps fit the requested `max_output_bytes` and the whole response bound. Step union:

- `{kind:"preamble",text:NonblankString}`
- `{kind:"answer",text:NonblankString}`
- `{kind:"call",capability_id:Alias,input:JsonObjectString}`

Go normalizes a delegation as the ordinary hashed delegation call. Delete the unused direct `delegate` wire variant; Rust maps that call into canonical `ModelStep::Delegate` through the current catalog. Other aliases map `CallTool`; preamble and answer map their same canonical kinds (answer artifacts empty). Engine retains whole-batch grammar and catalog validation. `call_ids` contains one unique provider-normalized CallID for each call in output order, otherwise exactly `[]`; these are validation/correlation data only. Engine allocates its own durable invocation identities at validated-batch admission; provider call IDs never authorize replay/effects.

`UsageDTO = {tokens:UInt|null,cost_micros:UInt|null}`. Provider-reported zero is valid; missing accounting is null, not fabricated 1/4096 tokens or zero cost. Map exactly to `ProviderUsageObservation`. Remove `used_tokens` from nested output and `CanonicalModelResponse`; no string-encoded output JSON or route object remains. Inference owns settlement once: add `BudgetAttempt::settle_observed(tokens:Option<u64>, cost_micros:Option<u64>)` in `runtime/execution/src/budget.rs`; Some dimensions settle reported amounts and None dimensions settle the existing conservative unknown reservation/estimate with the unknown marker retained. Existing `settle(u64,u64)` delegates Some/Some. Persist observed usage plus effective ledger charge in the sole agent `JournalEvent::ModelResult.accounting` described below, not the deleted Inference ModelAttemptRecord and not a pretend provider bill. A valid usage envelope is accounted before catalog/output-policy rejection; bad usage/body after network handoff keeps unknown charge. Do not lose usage when output is rejected or withheld at the post-I/O authority fence. `ModelResponse.usage` remains the charged numeric budget projection; certainty stays in the same acknowledged ModelResult journal event. No provider request retry occurs just to recover usage.

Paired wire intentionally carries **no provider-native replay**: delete input `replay_source`, assistant `provider_items`, response `routing.replay_source`, output `replay`, request `replay_of`, and related public transport decoder branches. Baseline Rust never forwarded/persisted the decoded provider replay, so this closes an unused ambiguous seam instead of promising provider replay recovery. Go provider adapters may retain in-call native reasoning data privately, but do not export it. Normal canonical tool history remains available through the bounded messages above. Keep Engine's validated-batch `ReplayReceipt` and durable Tool/Task idempotency unchanged; they are independent of provider replay. `attempt_id` is correlation, not a server idempotency promise. No automatic resend after a lost response or server restart, no cross-account native replay and no compatibility decoder.

Every success must echo exact purpose/attempt/capability revision and a valid trace ID. A mismatch is ServerModelInvalidOutput/PolicyDenied (binding mismatch uses PolicyDenied), never fallback or a second provider invocation. The server compares pinned capability revision against fresh current configuration/account identity before provider dispatch and before release. Configuration/account drift after dispatch withholds output and preserves unknown/reported accounting locally; it never silently returns another provider's result.

#### Single acknowledged attempt/accounting record

Delete Inference `ModelAttemptRecord`, `ModelAttemptState`, `AttemptLifecycle`, `AttemptJournal`, `UsageLedger` and `AttemptUpdate` as specified in the Rust appendix. The only durable attempt identity/result is `crates/contracts/agent/src/ports.rs::JournalEvent::{ModelIntent,ModelResult}` through `ExecutionJournal`. Add generic immutable `crates/runtime/execution/src/budget.rs::ModelAccounting {observed_tokens:Option<u64>,observed_cost_micros:Option<u64>,unknown_tokens:bool,unknown_cost:bool}` and `ModelAttemptReceipt {attempt_id:Uuid,charged_tokens:u64,charged_cost_micros:u64,accounting:ModelAccounting,dispatched:bool}`. The existing numeric `ModelUsage` remains the charged projection, not provider certainty.

Add `BudgetLease::begin_model_attempt(attempt_id:Uuid,tokens:&mut u64,cost_micros:&mut u64)->Result<BudgetAttempt,AgentFailure>`; duplicate IDs conflict instead of allocating again. `BudgetAttempt::settle_observed(tokens:Option<u64>,cost_micros:Option<u64>)->Result<ModelAttemptReceipt,AgentFailure>` settles each known dimension and retains the existing conservative estimate/reservation and unknown flag for each null dimension. `mark_dispatched` records an irreversible dispatch fact in the pending attempt; it cannot finalize provider accounting before a response exists. Successful settlement, failed-over-budget settlement or drop publishes exactly one terminal immutable receipt for that scope-owned admitted ID. No provisional snapshot is exposed as a `ModelAttemptReceipt`; Engine reads the terminal receipt after generation returns or its guard drops. This 2026-10-02 implementation clarification makes the intended immutable accounting lifecycle explicit. `BudgetLedger::model_attempt_receipt(attempt_id:Uuid)->Option<ModelAttemptReceipt>` reads it; no global map, service lookup or second durable journal. The bounded scope releases receipt payload memory after its owner acknowledges the durable ModelResult, retaining a bounded admitted-ID tombstone until scope end so an acknowledged attempt ID cannot be reused. Outstanding receipts survive only within that active scope.

`JournalEvent::ModelResult` becomes `{attempt_id,usage:agent_contract::ModelUsage,accounting:execution::ModelAccounting}`; `ModelIntent` additionally persists the non-secret PreparedModelPlan with projection ref. `ModelResponse` adds `accounting:ModelAccounting`. `runtime/agent/src/engine.rs::ActiveDrive::record_model_result` records the exact receipt after success/error; `failed_attempt_usage` no longer erases known-versus-estimated information. Vault Conversation journal adapters serialize the added fields in the same S1 snapshot. A missing receipt before handoff is no-charge; after acknowledged intent/possible handoff it is unresolved/unknown and never reissued. A failed result acknowledgement retains the intent and unknown charge for recovery, not a refund.

Adapter decoding validates framing, exact metadata/binding and usage before decoding output; a well-formed envelope with trustworthy usage but invalid steps returns `CanonicalModelResponse {usage,output:Err(...)}`. Inference first settles the receipt, then propagates that output failure. Valid mapped steps use `output:Ok(steps)`. A malformed envelope/untrusted usage cannot use this path and leaves unknown charge. Thus Result failure of the transport and Result failure of its output have deliberately different accounting semantics; no optional parallel attempt recorder is reintroduced.

### 2.5 Structured operator request and response

`StructuredRequestDTO = {schema_version:2,purpose:Purpose,capability_revision:Hex64,attempt_id:Uuid,data_classes:[Class],instructions:String,input:JsonObject,output_schema:JsonSchemaObject,max_output_bytes:UInt}`. Same validation as above, except instructions ≤8,192 bytes; input and output_schema each ≤32,768 bytes; object schema has `type:"object"`; max output 1..32,768 bytes. No recipient/transfer flags, provider identity, route/model selection or `replay_of`. `StructuredResponseDTO` has the same metadata/usage as AgentResponse, `output:JsonObject`, and no `call_ids`. Strict output validation against the requested schema occurs inside the Go provider adapter/service before release. Production synthetic/operator probe callers move to this typed route and retrieve capability revision from the owner, not a product bearer bypass. Exact input/output JSON payload objects are intentionally open within their declared schema/bounds; outer DTOs are strict.

Remove baseline `ReplayOf` trace-comparison branch and `requestDigest` dependence on it. Diagnostic rerun is a new explicitly initiated operator attempt with a new UUID; trace lookup never performs generation. Audit retains private provider/account identity digest and request/output hashes for continuity, no prompts/source text/bearer/provider secret, no second public replay endpoint.

### 2.6 Error/status mapping

`InferenceErrorDTO = {schema_version:2,error:{code:FailureCode},trace_id:Hex32|null}`. All keys required; no message, upstream body or secret. Error/status pair must match this table; unknown code/status/malformed body fails closed. A trace is required after provider handoff, null is permitted before handoff. 200 is the only success. No success from 201/204/redirects.

| HTTP | code(s) | Observation error | Generation AgentFailure |
|---|---|---|---|
| 400 | `validation`, `unsupported_schema` | InvalidInventory | ServerModelRequestRejected |
| 401 | `unauthorized` | CredentialRejected | CredentialExpired |
| 403 | `permission_denied` | PermissionDenied | PolicyDenied |
| 403 | `identity_mismatch` | InvalidIdentity | PolicyDenied |
| 404 | `not_found` | InvalidInventory | ServerModelRequestRejected |
| 405 | `method_not_allowed` | InvalidInventory | ServerModelRequestRejected |
| 409 | `capability_changed`, `purpose_not_configured`, `purpose_disabled` | InvalidInventory | PolicyDenied |
| 413 | `body_too_large` | InvalidInventory | BudgetExceeded |
| 415 | `content_type_unsupported` | InvalidInventory | ServerModelRequestRejected |
| 429 | `model_busy`, `quota_exceeded` | QuotaExceeded | QuotaExceeded |
| 502 | `invalid_output` | InvalidInventory | ServerModelInvalidOutput |
| 502 | `request_rejected` | TransportUnavailable | ServerModelRequestRejected |
| 503 | `provider_credentials_unavailable`, `model_unavailable` | TransportUnavailable | ServerModelUnavailable |
| 504 | `model_timeout` | Timeout | ServerModelTimeout |

Cancelled request contexts generally cannot return a usable HTTP response; caller cancellation maps directly to Cancelled, timeout to DeadlineExceeded before handoff / ServerModelTimeout after handoff. Network/DNS/connect/read failure maps TransportUnavailable or ServerModelUnavailable; malformed response maps InvalidInventory/ServerModelInvalidOutput. A 401 reflects paired client authentication, not downstream provider OAuth; a provider account problem is 503. Old 403→ConsentRequired mapping is deleted. Old `invalid_agent_*` strings normalize to `validation`; old `invalid_proposal`→`invalid_output`, `credential_expired` (provider)→`provider_credentials_unavailable`, `route_unavailable` at dispatch→`purpose_not_configured`. Absence only comes from strict successful inventory, never any error code including purpose_not_configured.

### 2.7 Go owner/adapter/transport symbol closure

- `internal/trust/operator.go`: `OperatorPrincipal` has private session ID/generation and expiry, no Person/device authority; zero value invalid. `Service.AuthenticateOperatorSession(ctx, sessionCookie string, csrf string, mutation bool) (OperatorPrincipal,error)` verifies current session and, for mutation, exact CSRF. `WithCurrentOperator(principal OperatorPrincipal, consume func() error) error` fences current operator session without impersonating a paired app. HTTP passes this capability to InvokeStructured; private synthetic startup probes are node-owned explicit self-checks and do not fabricate a Principal.
- `internal/inference/contracts.go`: closed `Purpose`, `PurposeStatus`, `PurposeCapability`, `PurposeInventory`, `AgentInvocation`, `StructuredInvocation`, `AgentResult`, `StructuredResult`, `UsageObservation`, `FailureCode`, `Failure`; all are typed domain values, no HTTP request/writer or bearer.
- `internal/inference/service.go`: `Service.ObservePurposes(ctx, trust.Principal) (PurposeInventory,error)`, `InvokeAgent(ctx, trust.Principal, AgentInvocation) (AgentResult,error)`, `InvokeStructured(ctx, trust.OperatorPrincipal, StructuredInvocation) (StructuredResult,error)`.
- `internal/inference/ports.go`: `PurposeCatalog.Snapshot(ctx) (PurposeInventory,error)`; `ModelExecutor.InvokeAgent(ctx, ResolvedModelTarget, AgentInvocation) (AgentResult,error)` and `.InvokeStructured(...)`; `ModelAccount.Ready(ctx) error` and `.ReplayIdentity() string`. ResolvedModelTarget is private immutable target identity+generation+effort, not transport credentials.
- `internal/inference/config.go`: `InferenceConfig{Routes map[Purpose]PurposeRoute}` and `PurposeRoute{TargetID string,ReasoningEffort string,Enabled bool}`; validated once. Remove `classForPurpose` from runtime. Existing operator fast/balanced/high_effort configuration maps to purposes exactly once in operator config loading; canonical stored config uses purpose keys, and old input keys are not a permanent parallel config schema.
- `internal/inference/providers/config.go`: `ProviderTarget{Provider,BaseURL,Model,APIKeyEnv}`. All concrete provider HTTP, OAuth credentials, endpoint validation and native provider DTO normalization remain here. `providers/codex.go::CodexClient` is private adapter integration implementing ModelExecutor/ModelAccount; owner does not import codexauth.
- `internal/inference/validation.go`: `ValidateAgentInvocation`, `ValidateStructuredInvocation` own semantic bounds/grammar. HTTP `decodeAgentRequest`, `decodeStructuredRequest` own strict JSON and convert fields, not purpose routing.
- `internal/transport/http/inference.go`: `InferenceHandler.ServeHTTP`, authentication handoff, routes/header/status, `writeInferenceError`; `inference_dto.go` owns the DTO names above. Gateway `ServeHTTP` splits explicitly into these methods and Service, never two owners for one branch. `node` constructs all ports, no owner imports node. `cmd/floe-server/main.go::legacyGateway` is deleted.

Rust `models/server.rs::{PurposeInventory,ObservedPurpose,GenerateResponse,RoutingResponse,AgentOutput,WireStep}` are replaced by the strict private DTOs above. `fetch_canonical_model_purposes`→`gateway/inference.rs::observe_purposes`; `canonical_server_profile_for`→`prepare_gateway_capability`; `valid_external_recipient` deleted; `ServerModelProvider`→`GatewayModelProvider`; `PreparedServerTransport`→`PreparedGatewayTransport`. `observe_profiles`→the exact Result observation methods in §1; no swallowed error. `canonical_model_input`→`encode_agent_input`, `map_canonical_step`→`decode_agent_step` preserve bounded alias/catalog semantics; `gateway_failure` becomes exhaustive status+code mapping; `decode_output` decodes the typed output directly, no nested JSON string; `authenticated_json` becomes strict bounded request/response helper. `models/root.rs` becomes role-neutral `gateway/model_provider.rs::CompositeModelProvider` that returns these observations without selecting; InferenceService is the sole selector. Every old field/branch above is explicitly retained, replaced or deleted in S1.4 with Go callers and production probes in the same snapshot.

## 3. Product command/query/owner mapping

### 3.1 Shared values, replay and failure

Product AppWire schema remains 2 / protocol 1. `request_id` is transient transport correlation; a command's `command_id` is durable replay identity; `operation_ref` is observation identity; `expected_revision` is aggregate CAS. Never exchange them. All `*Ref` identifiers are typed non-nil UUID wrappers, encoded as canonical strings, except `ReviewRefDto = {id:Uuid,revision:UInt,digest:Hex64}`. Review subtype is fixed by the command/response type and owner store, never guessed from an ID. Every mutable aggregate has revision ≥1. `Revision` is owner-local, never a remote provider revision. Strings have existing protocol bounds; new display labels ≤256 bytes, display code ≤32 ASCII bytes, input address ≤2,048 bytes; no control characters. timestamps are RFC3339 UTC strings, owner-supplied; observation delay is optional UInt milliseconds ≤60,000.

All public service methods below take `actor: &OwnerActor` and `scope: &ExecutionScope`; synchronous read-only methods may still return the same boxed ready future. `OwnerActor {person_id:PersonId,device_id:String,runtime_epoch:u64}` is a new non-serialized pure value in `crates/contracts/kernel/src/actor.rs`. App `CallerContext` constructs it only after HostRequest admission; it is not an AppWire field. Owners check person/object ownership and current generation, so it is not an opaque authorization capability. All listed methods return `BoxFuture<'a,Result<ResultType,OwnerError>>` (the named owner's error), not `serde_json::Value`. Commands additionally persist `(person_id,command_id,canonical_intent_digest)` with their receipt/result. Identical replay rejoins; same ID/different digest is Conflict. Expected revision is part of the digest. Pure inspect/get/list methods do not allocate reviews, reconcile identity, commit credentials, dispatch or resume.

Each owner returns its own typed snapshot and `OwnerFailure {domain,category,reason,incident_id,correlation_id,reload_required,seal_session,recovery,safe_actions}` using closed domain enums. Protocol DTOs copy these fields. `safe_actions` are owner-produced display affordances, revalidated on mutation. Binding code does not compute retry/expiry/approval from strings. Malformed/mismatched replies are transport ContractFailure with no enabled mutation. Shared enum domains/categories remain the existing kernel failure-envelope domains/categories; add explicit Connections/Access/Experts/Knowledge/Actions owner-domain variants where current App/Source labels would conceal the owner. `recovery` is closed `none|reobserve|reconcile|unlock|reopen|new_review`, never an executable instruction string. No provider error body enters failure text. Specific owner error reasons remain in the domain appendices; missing authority, wrong actor, stale generation and review mismatch are not transport retries.

### 3.2 Connections commands and queries

Names in this table are exact. Dart uses camelCase arguments; AppWire uses snake_case. Omitted `scope`/`actor` in the method column are implicit as above. Native open-URL actions are explicit user effects; constructing a launch action is not opening it.

| Dart method and explicit fields | AppWire tag; kind | ConnectionsService method → result |
|---|---|---|
| `prepareGatewaySetup(commandId,addressText)` | `connections.gateway.prepare_setup`; command | `prepare_gateway_setup(command_id,address_text)` → GatewaySetup |
| `overview()` | `connections.overview`; query | `overview()` → ConnectionsOverview |
| `startPairing(commandId,gatewayTargetRef)` | `connections.pairing.start`; command | `start_pairing(command_id,target_ref)` → PairingSnapshot |
| `confirmPairing(commandId,operationRef,expectedRevision)` | `connections.pairing.confirm`; command | `confirm_pairing(command_id,operation_ref,expected_revision)` → PairingSnapshot |
| `observePairing(operationRef)` | `connections.pairing.get`; query | `get_pairing(operation_ref)` → PairingSnapshot |
| `cancelPairing(commandId,operationRef,expectedRevision)` | `connections.pairing.cancel`; command | `cancel_pairing(command_id,operation_ref,expected_revision)` → PairingSnapshot |
| `forgetGateway(commandId,gatewayRef,expectedRevision)` | `connections.gateway.forget`; command | `forget_gateway(command_id,gateway_ref,expected_revision)` → GatewaySummary |
| `getGateway(gatewayRef)` | `connections.gateway.get`; query | `get_gateway(gateway_ref)` → GatewaySummary |
| `prepareIntegrationReview(commandId,integrationRef,expectedRevision)` | `connections.integration.prepare_review`; command | `prepare_integration_review(command_id,integration_ref,expected_revision)` → IntegrationReview |
| `inspectIntegrationReview(reviewRef)` | `connections.integration.inspect_review`; query | `inspect_integration_review(review_ref)` → IntegrationReview |
| `startIntegration(commandId,integrationRef,reviewedSelectionRef,expectedRevision)` | `connections.integration.start`; command | `start_integration(command_id,integration_ref,review_ref,expected_revision)` → ConnectionOperationSnapshot |
| `observeOperation(operationRef)` | `connections.operation.get`; query | `get_operation(operation_ref)` → ConnectionOperationSnapshot |
| `cancelOperation(commandId,operationRef,expectedRevision)` | `connections.operation.cancel`; command | `cancel_operation(command_id,operation_ref,expected_revision)` → ConnectionOperationSnapshot |
| `prepareSourceReview(commandId,sourceRef,expectedRevision)` | `connections.source.prepare_review`; command | `prepare_source_review(command_id,source_ref,expected_revision)` → SourceReview |
| `inspectSourceReview(reviewRef)` | `connections.source.inspect_review`; query | `inspect_source_review(review_ref)` → SourceReview |
| `configureSource(commandId,sourceRef,reviewRef,selectedResourceRefs,expectedRevision)` | `connections.source.configure`; command | `configure_source(command_id,source_ref,review_ref,selected_resource_refs,expected_revision)` → SourceSummary |
| `disconnectSource(commandId,sourceRef,expectedRevision)` | `connections.disconnect`; command | `disconnect(command_id,source_ref,expected_revision)` → ConnectionOperationSnapshot |
| `prepareObserveReview(commandId,sourceRef,expectedRevision,requestedProcessing)` | `connections.observe.prepare_review`; command | `prepare_observe_review(command_id,source_ref,expected_revision,requested_processing)` → ObserveReview |
| `inspectObserveReview(reviewRef)` | `connections.observe.inspect_review`; query | `inspect_observe_review(review_ref)` delegates Access `inspect_review` → ObserveReview |
| `setObserve(commandId,sourceRef,true,reviewRef,expectedRevision)` | `connections.observe.set`; command, tagged enable branch | `apply_observe(command_id,review_ref,Allow)` → SourceSummary; exact source/revision must match stored review |
| `setObserve(commandId,sourceRef,false,null,expectedRevision)` | same command, tagged pause branch | `pause_observe(command_id,source_ref,expected_revision)` → SourceSummary |
| `requestManagementLaunch(commandId,gatewayRef,expectedRevision)` | `connections.gateway.management_launch`; command | `request_management_launch(command_id,gateway_ref,expected_revision)` → LaunchAction |

Protocol encodes Observe as `{kind:"enable",source_ref,review_ref,expected_revision}` or `{kind:"pause",source_ref,expected_revision}` under the command envelope; do not implement invalid combinations of boolean+optional ref in Rust. Dart convenience bool is encoded into the tagged union by its adapter. `requestedProcessing` is `device_only|gateway_allowed`; the persisted Access descriptor additionally binds exact categories/consumers/purpose and current reviewed absence/grants. The product never supplies those authority fields.

The Connections repository stores Gateway setup receipts, pairing operations, IntegrationReview and SourceReview. Access's encrypted repository stores ObserveReview/ConnectionReview and resolves its refs. Experts stores BindingReview, separately. `IntegrationReview` pins connector/catalog revision, supported setup/initial selection, Gateway generation and expiry; its reviewedSelectionRef is specifically `IntegrationReviewRef`, not an Experts selection or grant. `SourceReview` pins source identity/revision, private account/subject/resource-catalog evidence and permitted choices; it is source configuration evidence, not a processing grant. A source configure that invalidates a grant yields the Access-owned required review/invalidation outcome; it never expands Observe implicitly. No FFI/client reconstructed authority or duplicate review store is allowed.

`prepareGatewaySetup` records a Connections command receipt and bounded expiring setup descriptor with `target_ref`. Validate loopback as in main §3.2; replay returns the same descriptor even if now expired, never allocates a fresh target under the same command. Starting a new expired target requires a new explicit prepare command. Endpoint and poll proof stay in adapter-private state; safe display address is returned only by GatewaySetup, not general GatewaySummary.

The full RemoteIntegrationPort at `connections/src/ports/remote_integration.rs` is (`OperationScope` is the Connections public alias of the existing `floe_execution::ExecutionScope`, not a second cancellation/budget implementation):

```rust
pub trait RemoteIntegrationPort: Send + Sync {
    fn list<'a>(&'a self, query: IntegrationCatalogQuery, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationCatalog, IntegrationError>>;
    fn begin<'a>(&'a self, command: BeginIntegration, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>>;
    fn observe<'a>(&'a self, operation: &'a IntegrationOperationRef, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>>;
    fn cancel<'a>(&'a self, command: CancelIntegration, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>>;
    fn configure<'a>(&'a self, command: ConfigureIntegration, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>>;
    fn disconnect<'a>(&'a self, command: DisconnectIntegration, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>>;
    fn management_launch<'a>(&'a self, request: ManagementLaunchRequest, scope: &'a OperationScope)
        -> BoxFuture<'a, Result<ValidatedManagementLaunch, IntegrationError>>;
}
```

`CancelIntegration {operation_id,remote_operation_ref,expected_remote_revision}` is built by Connections from its persisted exact operation after local CAS; client cannot supply remote identity. Cancel before committed authorization prevents publication. If remote completion won, return the completed state and require separate explicit disconnect, never pretend cancellation rolled it back. A lost acknowledgement rejoins the same cancel operation; no new begin call. Connections' own source-review-apply cancellation uses the Vault appendix's durable abort/receipt protocol, not a remote integration cancel as a shortcut. Observer disposal/timeout never invokes cancel.

`ManagementLaunchRequest {operation_id,gateway_binding_ref,expected_binding_generation,purpose:ManageGateway}` is host-bound. The adapter constructs an address from its pinned private Gateway and the literal `/manage` route, verifies loopback/origin/path/expiry, and returns `ValidatedManagementLaunch {action_ref,purpose,validated_url,expires_at}`. No bearer, OAuth state, one-time login token, userinfo or credential-bearing query/fragment is placed in the URL. Browser authentication remains a Go management responsibility. The command persists the exact launch receipt/expiry; replay does not mint or open another launch.

### 3.3 Exact safe DTOs and state serialization

Fields marked `?` are omitted, not null. Required arrays may be empty. `failure` is the owner failure projection. Protocol DTO suffix is `Dto`; Dart uses the same names with camelCase. These are safe projections, not raw owner record serialization.

- `GatewaySetup {target_ref,display_address,expires_at}`.
- `GatewaySummary {gateway_ref,revision,display_name,state,allowed_actions,failure?,remote_revocation_pending:bool}`; state `unpaired|paired|repair_required|forgotten`; no endpoint/bearer/key envelope.
- `ConnectionsOverview {revision,gateways:[GatewaySummary],integrations:[IntegrationSummary],sources:[SourceSummary]}`.
- `PairingSnapshot {operation_ref,revision,state,display_code?,expires_at?,gateway?,allowed_actions,failure?,next_observation_after_ms?}`.
- `IntegrationSummary {integration_ref,revision,display_name,category,state,capabilities,source?}`; category is a supported owner connector category, capabilities are closed product affordances, not OAuth scopes.
- `ConnectionOperationSnapshot {operation_ref,revision,state,launch_action?,display_code?,source?,allowed_actions,failure?,next_observation_after_ms?}`; state `pending|running|awaiting_user|completed|failed|cancelled|repair_required`. Owner typed state carries only its valid payload: completed has source or explicit no-source completion kind; failed/repair_required require failure; cancelled is not a timeout.
- `SourceSummary {source_ref,revision,display_labels,availability,last_observed_at?,selected_resources:[{resource_ref,label}],observe_state,allowed_actions}`. availability `available|unavailable|permission_required|identity_changed|disconnected`; observe state `disabled|enabled|paused|review_required`. Private source authority/native subject/remote revision/resource IDs do not leak.
- `SourceReview {review_ref,source_ref,source_revision,labels,permitted_choices:[{resource_ref,label,selected}],processing_disclosure,expires_at,allowed_actions}`.
- `IntegrationReview {review_ref,integration_ref,catalog_revision,gateway_ref,gateway_revision,display_name,setup_kind,expires_at,allowed_actions}`; setup_kind `browser_authorization|device_code|gateway_managed_secret|native_permission`; no secret input form.
- `ObserveReview {review_ref,source_ref,source_revision,display_members,processing_disclosure,expires_at,allowed_actions}`. `processing_disclosure {current:device_only|gateway_allowed,requested:device_only|gateway_allowed,categories:[personal|highly_sensitive],scope_labels:[String]}` is derived from the exact stored descriptor. Synthetic is not a personal source category.
- `LaunchAction {action_ref,purpose,validated_url,expires_at}`; purpose `manage_gateway|authorize_integration`. Authorization launch URLs are the pinned Go-hosted `/manage/setup/{operation_ref}` route with a non-secret canonical operation UUID, no query/fragment. Go privately generates and handles the provider redirect/OAuth state after management authentication. No raw OAuth state, provider URL containing state, provider secret or saved bearer is returned in LaunchAction. The Gateway management action uses `/manage` as above.

Pairing domain→product mapping is total: `Pending`→`starting`; `AwaitingLocalConfirmation`→`awaiting_local_confirmation`; `AwaitingApproval`→`awaiting_gateway_approval`; `Verifying`→`verifying`; `Committing`→`committing`; `Paired`→`connected`; `Rejected`→`rejected`; `Expired`→`expired`; `Cancelled`→`cancelled`; `RepairRequired`→`repair_required`. A connected result requires exact secure commit and readback. Native transport status is not a second state machine. The UI cannot infer connected from remote approval or display-code confirmation.

### 3.3.1 Exact AppWire result wrappers

The outer `AppResponse` remains `{schema_version:2,request_id,status:"ok",result}` or the existing typed error envelope. Connections results use one explicit typed payload, never the old optional worker-result matrix: `connections.gateway_setup` + `setup`; `connections.gateway` + `gateway`; `connections.pairing` + `pairing`; `connections.overview` + `overview`; `connections.integration_review` + `review`; `connections.operation` + `operation`; `connections.source` + `source`; `connections.source_review` + `review`; `connections.observe_review` + `review`; `connections.launch` + `launch_action`. Here each prefix is the exact `result.kind`, and the following name is its sole snapshot payload key. Commands and queries returning the same snapshot use the same wrapper. The tagged Observe enable/pause value is carried in the `mutation` field of `connections.observe.set`. These S1 transport-shape clarifications do not change owner semantics.

### 3.4 Experts binding and remaining owner intents

`experts/src/api.rs` methods:

- `directory(actor,scope) -> ExpertDirectorySnapshot` is pure.
- `set_installation_enabled(actor,command_id,installation_id,expected_revision,enabled,scope) -> ExpertDirectorySnapshot` atomically changes installation/assignment visibility.
- `inspect_binding(actor,assignment_id,requirement_key,scope) -> BindingInspection` is a pure live catalog view; it does not persist a review or grant permission.
- `prepare_binding_review(actor,command_id,assignment_id,requirement_key,expected_binding_revision,scope) -> BindingReview` persists an immutable Experts descriptor.
- `inspect_binding_review(actor,review_ref,scope) -> BindingReview` is pure stored-review inspection.
- `replace_binding(actor,command_id,review_ref,expected_binding_revision,candidate_ids,scope) -> ExpertDirectorySnapshot` resolves the descriptor, obtains fresh catalog evidence and CASes the binding; no caller package/definition fields.

`BindingReview {review_ref,assignment_ref,requirement_ref,binding_revision,candidate_refs_and_labels:[{candidate_ref,label,availability,selected}],expires_at,allowed_actions}`. Experts persists Person/device, assignment, package/definition revision, requirement key, expected binding revision, exact candidate IDs→source mapping, catalog revision/digest and expiry in `BindingReviewDescriptor` via `BindingReviewRepository` in `experts/src/ports/binding_review.rs`. Vault implements it at `repositories/expert_binding_reviews.rs`. A candidate ref is meaningful only within that exact review; stale/new candidates require a new review. Replace rechecks package/definition/source continuity, rejects duplicate/unknown candidates and changed command payload, preserves missing selected candidates as unavailable in inspection, commits once and then republishes Directory. Binding changes are configuration, never source-grant mutations.

| Dart method | AppWire tag/kind | Public owner method |
|---|---|---|
| `directory()` | `experts.directory`; query | Experts `directory` |
| `setInstallationEnabled(commandId,installationRef,enabled,expectedRevision)` | `experts.installation.set_enabled`; command | Experts `set_installation_enabled` |
| `prepareBindingReview(commandId,assignmentRef,requirementRef,expectedRevision)` | `experts.binding.prepare_review`; command | Experts `prepare_binding_review` |
| `inspectBindingReview(reviewRef)` | `experts.binding.inspect_review`; query | Experts `inspect_binding_review` |
| `replaceBinding(commandId,reviewRef,selectedCandidateRefs,expectedRevision)` | `experts.binding.replace`; command | Experts `replace_binding` |
| `refreshDay(commandId,dayQuery)` | `day.refresh`; command | Day `refresh_day(actor,command_id,DayQuery,scope)` |
| `ActionsGateway.listActions(cursor?,limit)` | `actions.list`; query | Actions `list(actor,cursor,limit,scope)` |
| `inspect(actionRef)` | `actions.inspect`; query | Actions `inspect(actor,action_ref,scope)` |
| `inspectAuthority()` | `actions.authority.get`; query | Actions `inspect_authority(actor,scope)` |
| `setCalendarCreateAuthority(commandId,mode,expectedRevision)` | `actions.authority.set_calendar_create`; command | Actions `set_calendar_create_authority(actor,command_id,mode,expected_revision,scope)` |
| `submit(commandId,ActionIntent)` | `actions.submit`; command | Actions `submit(actor,command_id,intent,scope)` |
| `decide(commandId,actionRef,reviewRef,decision,expectedRevision)` | `actions.decide`; command | Actions `decide(actor,command_id,action_ref,review_ref,decision,expected_revision,scope)` |
| `reconcile(commandId,actionRef,expectedRevision)` | `actions.reconcile`; command | Actions `reconcile(actor,command_id,action_ref,expected_revision,scope)` |

Actions mode is the existing closed `allow|ask|deny` for Calendar Create only; changing it is an explicit owner CAS command, never an implication of Observe or Expert binding. Actions result/storage/recovery is exactly App appendix §8's user-accepted one-encrypted-store model, including external Action unavailability while Vault is locked (accepted 2026-10-02 13:46 UTC). Knowledge read/review/decide APIs and DTOs remain Knowledge-owned and map mechanically at the binding; no widget-interface import inward. Conversation commands/query/result fields remain App §3.1 plus Vault's persisted blocked/resume contract, with this exact addition: `RunState::Blocked`→`AppRunStateDto::Blocked`→`state:"blocked"`; `AppTurnExecutionDto::Blocked`→`execution:"blocked"`; `AppReplyStatusDto::NotProduced`→`reply:"not_produced"`. No Finished alias hides Blocked, no generated answer/attempt is fabricated, and no routine resolved-card Continue command remains after durable auto-resume lands.

### 3.5 Review application is two-store reconciliation

Use the Vault appendix's exact names: Connections `ConnectionReviewApply` carried in `SourceOperationRecord` through `SourceOperationRepository`, and Access `ConnectionReview`, `GrantCommit`, `GrantCommitReceipt`, `GrantOperationReceipt::{Committed,Aborted}` with `GrantAbortReceipt`. Source metadata stays in Turso; Access review/grants/receipts stay encrypted. Connections reserves/fences exact source generations before Access CAS; Access atomically commits its grant bundle plus receipt or durable abort tombstone; Connections reconciles the authoritative receipt and releases/finalizes its reservation. Never describe this as atomic source+grant commit. Source mutation/cancel/crash/locked-Vault behavior is the state table in the Vault appendix; product observation is read-only, while a named owner recovery command may drive receipt reconciliation. No global transaction spans either store plus remote/provider/model I/O.

S1 implementation clarification: the canonical no-review pause command uses explicit `SourceOperationKind::ConnectionObservePause` and `GrantCommitKind::PauseObserve`, through the same exact source reservation, snapshot/CAS and receipt protocol. Only Active grants become Paused with authority advancement and required invalidation/cleanup; Paused and Revoked entries remain unchanged. Pause never means disconnect, credential removal or revocation. Same-command replay rejoins the persisted receipt without advancing authority again. This permission-reduction case was omitted from the earlier operation-kind enumeration.

## 4. Manifest-level graph and inverse ports

### 4.1 Exact new ports and constructor wiring

No new Rust workspace crate is proposed: `gateway/*`, new `ports/*` and new application modules are directories in their existing packages. Root `Cargo.toml` workspace membership remains unchanged. Port visibility is public through its owner crate's `lib.rs`; private repositories remain owner capabilities rather than shared mutable tables. Add a role-neutral `BoxFuture<'a,T>` alias in `crates/runtime/execution/src/lib.rs`; the existing agent-contract alias becomes a re-export of that exact alias. Connections/Day/Access can use boxed futures without importing agent-contract just to name a future.

| Port and owning path | Exact signature / values | Concrete implementation and construction |
|---|---|---|
| `day/src/ports/calendar_acquisition.rs::CalendarAcquisitionPort` | `acquire<'a>(&'a self,request:CalendarRefreshRequest,scope:&'a ExecutionScope)->BoxFuture<'a,Result<CalendarAcquisition,CalendarRefreshError>>`; request `{actor:OwnerActor,query:DayQuery,expected_mirror_revision:Revision}` | `context/src/application/calendar_acquisition.rs::ContextCalendarAcquisition` resolves current configured/admitted sources, bounded range/resources and returns source identities/revisions, coverage, per-resource batches/failures, observation times. `DayService::new(repository,Arc<dyn CalendarAcquisitionPort>,clock)`. App passes Context adapter. Day never imports Context/Connections/Access. |
| `knowledge/src/ports/learner_projection.rs::LearnerProjectionPort` | `project<'a>(&'a self,request:LearnerProjectionRequest,scope:&'a ExecutionScope)->BoxFuture<'a,Result<ModelProjectionOutcome,AgentFailure>>`; request `{actor,job_id,plan:PreparedModelPlan,evidence_refs,bounds,expires_at}` | `context/src/application/learner_projection.rs::ContextLearnerProjection`; validates Knowledge-selected evidence and current source permission/processing using the plan. `KnowledgeService::new(repository,review_repository,learner_jobs,Arc<dyn ModelPort>,Arc<dyn LearnerProjectionPort>,clock)`. Knowledge uses common Engine with empty tools/catalog and bounded candidate-only output; no Context dependency. |
| `experts/src/ports/candidate_catalog.rs::CandidateCatalog` | `inspect<'a>(&'a self,query:CandidateQuery,scope:&'a ExecutionScope)->BoxFuture<'a,Result<CandidateSnapshot,ExpertError>>`; query `{actor,package_ref,definition_revision,requirement_key,current_candidate_refs}`; result `{revision,digest,candidates:[Candidate],source_expectations}` | `context/src/application/candidate_catalog.rs::ContextCandidateCatalog` reads safe current Connections/Context source facts. `ExpertsService::new(registry,tasks,binding_reviews,Arc<dyn CandidateCatalog>,Arc<dyn ModelPort>,source_ports,clock)`. Candidate listing never creates grants or removes unavailable configured candidates. Context imports Experts port; Experts never imports Context/Connections. |
| `access/src/ports/trusted_consumer_catalog.rs::TrustedConsumerCatalog` | `registrations(&self)->&[TrustedConsumerRegistration]`; pure registration `{package_identity:String,manifest_revision:UInt,declared_view_capabilities:[TrustedViewCapability],consumer_identity:GrantConsumer}` | `app/src/composition/trusted_consumers.rs::StaticTrustedConsumerCatalog` copies validated shipped registration values into Access-owned pure DTOs at construction. `AccessService::new(grant_repository,review_repository,Arc<dyn TrustedConsumerCatalog>,clock)`. No callback into Experts/Connections/builtin, no registry read at grant time and no policy in adapter. Access derives consumer policy/digests. |
| `experts/src/ports/binding_review.rs::BindingReviewRepository` | `prepare(PrepareBindingReview)->Result<BindingReviewDescriptor,ExpertStoreError>`; `get(actor,review_ref)->Result<BindingReviewDescriptor,...>`; `commit_replacement(ReviewedBindingReplacement)->Result<BindingCommit,...>`; boxed futures + scope, short repository CAS | Vault `repositories/expert_binding_reviews.rs`; commit binds review consumed state, command receipt and binding CAS under the same encrypted repository transaction. Experts computes policy outside storage, Vault enforces passed exact expectations. App supplies implementation. |
| `access/src/ports/grant_repository.rs::GrantRepository` and `access/src/domain/connection_review.rs::SourceReservationEvidence` | `receipt(GrantReceiptQuery)->Result<Option<GrantOperationReceipt>,AgentFailure>`; `abort(GrantAbort)->Result<GrantAbortOutcome,AgentFailure>` plus `commit(GrantCommit)->Result<GrantCommitReceipt,AgentFailure>`; exact fields/states in Vault appendix | Connections passes immutable Access-owned source reservation evidence into Access API. Vault implements grant receipts/CAS; Turso implements Connections `SourceOperationRepository`. Access imports neither Connections nor its repository; Connections performs reservation and recovery orchestration. |

`CalendarRefreshRequest` source set is owner-resolved; the product DayQuery supplies requested display date/timezone, not source authority. Replace App appendix's older shorthand “Person/source reference” with the exact request above. Knowledge evidence refs are internal Knowledge selections; product cannot submit trusted raw context. A prepared model plan is an immutable value, not permission to read sources. Experts normalized `Candidate` includes safe label/availability plus private owner source identity/expectation; product sees only review-bound opaque ref.

Conversation uses **direct public-owner edges**, not reverse implemented workflow ports: add `ConversationService::new(repository,events,Arc<AccessService>,Arc<ConnectionsService>,Arc<ExpertsService>,Arc<ActionsService>,model_port,projection_port,clock)` (concrete service generics may be type-erased by their owner public traits, but dependencies and method semantics do not change). `conversation/application/interaction_resolution.rs` dispatches only its typed interaction target: source review→Connections `apply_observe`/Access inspection; binding review→Experts `replace_binding`; Action review→Actions `decide`. The owner returns its typed receipt; Conversation stores the decision/result and claims the fresh child. Access never imports Conversation or implements a Conversation workflow trait. Source projection ports remain the agent contract and Context implementations. Constructor code in App builds adapters first, then Access/Connections/Inference, then Day/Knowledge/Experts with Context port handles, then Context (see deferred-free wiring below), then Conversation and FFI service handles.

Avoid a runtime construction cycle: `ContextCalendarAcquisition`, `ContextLearnerProjection` and `ContextCandidateCatalog` are small independently constructed Context-owned adapters over shared immutable `ContextCore` handles (source/read repositories, Access, Connections, native/Gateway readers and clock), not callbacks to an optional whole `ContextService`. Build `ContextCore` after Access/Connections; construct these adapter objects; build Day, Knowledge and Experts; finally build `ContextService::new(core,day_public_reader,knowledge_public_reader,experts_candidate_values)`. The last parameter is an immutable candidate value view if required, never a callback needed to construct the earlier adapters. No `Option` late initialization, mutable ServiceLocator, Arc cycle or App-owned source policy. Inference is independent and constructed with its own provider/Access capability ports before model consumers.

### 4.2 Exact Cargo and policy deltas

Apply only during the owning authorized implementation stage, not P0. Paths below are relative to the changed manifest. Keep existing external library dependencies unless actual moved symbols eliminate their last production use; T0 removes separately proven test-only dependencies. `Cargo.lock` changes only from authorized manifest resolution at a completed boundary; no manual lock edits or unrelated upgrades.

| Manifest | Production dependency change | Stage / `module-dependencies.json` change |
|---|---|---|
| `crates/runtime/execution/Cargo.toml` | ADD external `uuid.workspace=true` for exact admitted model-attempt UUID accounting; no new internal owner edge | S1.1/S1.4; policy unchanged |
| `crates/modules/day/Cargo.toml` | ADD `floe-execution={path="../../runtime/execution"}` | S2.2; policy already permits `execution`, retain |
| `crates/modules/context/Cargo.toml` | ADD `floe-experts={path="../experts"}`; MOVE `floe-kernel={path="../../contracts/kernel"}` from dev to production for OwnerActor/IDs | S1.1/S2.2; context allowed list ADD `experts`,`kernel` |
| `crates/modules/knowledge/Cargo.toml` | ADD `floe-agent-runtime={path="../../runtime/agent"}`; no Context/Inference/Access import needed: ModelPort and projection port are injected | S2.4; knowledge policy ADD `agent_runtime`, REMOVE unused permitted `inference`,`access` |
| `crates/modules/experts/Cargo.toml` | MOVE `floe-agent-runtime={path="../../runtime/agent"}` from dev to production; ADD `floe-kernel={path="../../contracts/kernel"}`; REMOVE `floe-inference` after common model port replaces old ExpertModel/A2A context | ADD kernel at S1.1 for review/service actor contracts; promote runtime and remove inference at S2.3 common Engine cutover; policy adds the matching edges at those stages |
| `crates/modules/conversation/Cargo.toml` | ADD `floe-access={path="../access"}`, `floe-connections={path="../connections"}`, `floe-actions={path="../actions"}`; MOVE `floe-context-contract` dev→production for explicit persisted coverage/source values | S1.1/S1.5; conversation policy ADD `access`,`connections`,`context_contract`; `actions` already allowed |
| `crates/adapters/providers/Cargo.toml` | MOVE `floe-kernel={path="../../contracts/kernel"}` dev→production for explicit OwnerActor/ID port signatures; existing inward owner/native dependencies retained | S1.1; providers policy ADD `kernel` |
| `crates/bindings/ffi/Cargo.toml` | ADD `floe-conversation`, `floe-connections`, `floe-access`, `floe-context`, `floe-day`, `floe-actions`, `floe-experts`, `floe-knowledge`, each `{path="../../modules/<owner>"}`; ADD `floe-kernel={path="../../contracts/kernel"}`, `floe-agent-contract={path="../../contracts/agent"}`, `floe-context-contract={path="../../contracts/context"}` | S1.6/S2 owner cutovers; ffi allowed list becomes exact `app,protocol,conversation,connections,access,context,day,actions,experts,knowledge,kernel,agent_contract,context_contract` |
| `crates/bindings/protocol/Cargo.toml` | NO new owner/service/adapters dependencies; keep pure kernel/agent/context contracts and serialization | S1/S2; policy unchanged; add pure DTO source modules only |
| `crates/modules/access/Cargo.toml` | NO new internal edge; trusted catalog uses Access-owned pure values | S1.1/S1.3; policy remains kernel/context_contract/execution |
| `crates/modules/inference/Cargo.toml` | NO Connections/Context/Experts/builtin edge; existing Access/agent_contract/context_contract/execution remain | S1.1/S1.4; policy description changes to purpose/common selector, not model profiles |
| `crates/app/Cargo.toml` | Keep assembly's existing owner/adapter/builtin edges; no new gateway crate; add no semantic dependency to make a façade compile | S1/S2; policy remains composition allowlist |
| `crates/adapters/vault/Cargo.toml` | Existing owner edges implement new repository ports; no new ownership path needed; Inference dev-only dependency disappears with T0 old tests, never promoted for a second journal | S1/S2; remove unused permitted `inference` from vault policy after old journal deletion |
| root `Cargo.toml` | NO workspace member addition/removal for these new modules; preserve package names; only T0's independently proven test-target/dev-support adjustments | P0 read only; implementation requires no root `gateway` member |

FFI gets owner public APIs directly and never Vault/providers/native credential driver imports. It obtains an admitted actor and execution scope from AppHost and invokes typed service handles. No transitive `floe_app::*` reexport can hide an otherwise missing owner dependency. Protocol remains pure DTO. Native callback completion remains internal `native_host.*`, bound to host-issued registration/outstanding operation; these dependencies do not authorize product trusted-view publication.

### 4.3 Complete proposed internal dependency DAG

This table is the final production graph for the changed owner/adapter/binding closure (external libraries omitted). Entries are direct dependencies, not merely allowable future edges. Contracts and unchanged leaf edges are included so path/cycle checking can be done statically during P0. Dependencies used only by deleted tests are excluded.

| Package ID | Direct internal dependency IDs |
|---|---|
| kernel | none |
| context_contract | kernel |
| execution | kernel |
| agent_contract | kernel,context_contract,execution |
| diagnostics | kernel |
| agent_runtime | agent_contract,execution |
| access | kernel,context_contract,execution |
| connections | access,kernel,context_contract,execution |
| inference | access,agent_contract,context_contract,execution |
| day | kernel,context_contract,execution |
| knowledge | kernel,agent_contract,context_contract,execution,agent_runtime |
| experts | kernel,agent_contract,context_contract,execution,agent_runtime |
| context | kernel,access,connections,day,knowledge,experts,agent_contract,context_contract,execution |
| actions | kernel,agent_contract,context_contract,connections,day,execution |
| conversation | kernel,access,connections,actions,context,experts,inference,knowledge,agent_contract,context_contract,execution,agent_runtime |
| builtin | agent_contract,actions,context_contract,day,execution,experts |
| native | kernel,context_contract,execution |
| providers | kernel,access,actions,agent_contract,connections,context,context_contract,day,execution,inference,native |
| vault | kernel,access,actions,agent_contract,connections,context,context_contract,conversation,day,execution,experts,knowledge |
| app | kernel,access,actions,agent_contract,connections,context,context_contract,conversation,day,diagnostics,execution,experts,builtin,inference,knowledge,providers,vault |
| protocol | kernel,agent_contract,context_contract |
| ffi | app,protocol,conversation,connections,access,context,day,actions,experts,knowledge,kernel,agent_contract,context_contract |

Actions currently calls Connections/Day and owns its permissions/repository; no Access production edge is introduced by R8 unless a documented public Access value is actually needed. Delete unused allowed edges only at S2 final graph reconciliation, not speculatively during P0. The native row matches its frozen manifest; its platform-gated OS libraries are external, not business edges. No dev-only owner dependency is promoted implicitly.

A valid topological layering is kernel → context_contract/execution/diagnostics → agent_contract/access/day/native → agent_runtime/connections/inference → knowledge/experts/actions → context/builtin → conversation → providers/vault → app/protocol → ffi (independent nodes may be earlier). Critically, Inference's reachable owner set ends at Access and contracts/runtime; Connections' reachable owner set also ends at Access and contracts/runtime. Neither reaches the other, even indirectly through trusted consumers. Context→Day/Knowledge/Experts all point inward; none of those owners points back. Conversation→Access/Connections is one-way. Adapters and FFI are leaves above owners, never imported inward. This is a manual graph argument, not a claim that the architecture checker was executed.

## 5. Stage/caller closure

- P0: only these specifications, baseline reads and static reconciliation. No source/manifests/test deletion, formatter/compiler/analyzer/build/test/checker run or remote write.
- T0 after approval: complete per-behavior documentation, shared-consumer audit, then exact old test deletion and test-target references. No preserving obsolete semantics because a test asserted them.
- S1.1: only the shared pure types/ports/DTOs/manifests and constructor dependencies needed by the complete S1 vertical. Day CalendarAcquisitionPort/Day→execution and Knowledge LearnerProjectionPort/Knowledge→agent_runtime land together at S2.2/S2.4; Experts actor/review types + kernel edge land S1.1, common Engine runtime edge/removal of old Inference edge land S2.3. A contract is never compiled before its required manifest edge, and no S2-only declaration is left half-wired at G1. S1.2: Trust-integrated pairing and private credential commit/readback. S1.3: source review, durable two-store fencing/recovery and mandatory local Health transform. S1.4: schema2 inference wire/common Primary selector for Manager/Expert/Learner. S1.5: Conversation/journal/blocked review and exactly-once fresh resume. S1.6: FFI/C/Dart/CLI/native callers switch together. S1.7: obsolete-path deletion/static reconciliation. Only now G1 formatting and production compilation, no tests and no full package/release build.
- S2: finish all owner extraction and dependency narrowing, including Day/Knowledge/Experts inverse adapters and Actions recommendation if accepted. G2 formats once, compiles/builds the completed full production structure, including real affected platform targets where available; unavailable prerequisites are explicit blockers, not a pass.
- S3 only after G2 structural closure: write fresh target-architecture tests, run final behavioral qualification and required final compilation/build. No tiny-step format/compile loops and no new tests during S1/S2.

The client has no independent numbered route-switch order: use these exact central stages. The first Conversation vertical cannot switch before trust/pairing/private credentials, source processing and mandatory Health-local prerequisites are in place. The complete first slice is the milestone; an intermediate contract file compiling is not it.

## S1 callback and refresh transport clarification

Native completion is restricted to direct `native_host.{calendar,attention,personal}.{register,poll,complete,fail,dispose}` command/query tags. Registration takes no Person/device/epoch from product input and returns `{kind:"registered",registration:{registration_id,host_epoch,runtime_epoch}}` bound by App to the verified caller and host family. Every later operation echoes that registration; complete carries `result`, fail carries `request_id,failure`, poll/dispose carry only registration. Poll returns the corresponding `{kind:"calendar_acquisitions"|"attention_acquisitions"|"personal_acquisitions",acquisitions:[...]}`; mutation acknowledgment is `{kind:"acknowledged"}` directly under AppResponse.result. Inner completion host_epoch must equal the issued registration. Health completion carries only an optional transform_operation_id; a Wellbeing payload requires a real independently held native success receipt. No public publish/read/revoke route remains.

The complete S2 Day refresh contract (prepared during S1, not connected until its owner/acquisition implementation is complete) uses command `{kind:"day.refresh",day:DayQuery}`, pure query `{kind:"day.refresh.get",operation_ref}`, and result `{kind:"day.refresh",refresh:DayRefreshSnapshot}`. Pending/running snapshots contain operation_ref,revision,state; completed additionally requires day:DaySnapshot; failed/interrupted additionally require an owner failure and contain no day. State payloads form a closed union. Same-command replay rejoins persisted operation before new source observation, changed input conflicts, and recovery does not automatically repeat unfinished acquisition. Source inventory is owner-resolved; no Day payload grants source authority.

S1 native setup clarification: IntegrationReview has a closed `target` union: `{kind:"device",device_id}` for `native_permission`, or `{kind:"gateway",gateway_ref,gateway_revision}` for remote setup kinds. No native review fabricates a Gateway binding. The exact target is included in the immutable reviewed digest and owner replay identity.

S1 implementation dependency clarification: FFI also declares `floe-execution` for the exact `ExecutionScope` parameter named by typed owner public ports and the host-native lane. This is a pure runtime contract edge, not source/model orchestration. Context's new object-safe EvidenceReader directly names kernel AgentFailure, so its retained T0 kernel dev edge is promoted now. Unused Knowledge/Experts/Vault Inference policy edges are removed alongside the old concrete model callers; later S2 Engine port promotions remain staged with their implementations.
