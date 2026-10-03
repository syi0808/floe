# Server ownership cutover: execution-ready proposal

**Execution authorization update (2026-10-02 13:49 UTC): the user authorized the full plan in its stated order. T0 behavior-ledger completion must still precede each recoverable test deletion; completed-slice and final-structure verification gates are unchanged. This preparation revision changes documents only; assigned implementation/research tasks start through the coordinated handoff. No push/PR/merge or actual development-data reset is performed here. Permanent data deletion and credential/security actions retain their exact-target/action-time approval requirements. Earlier plan-only/future-authorization language records the preparation phase; its execution-approval condition is now satisfied, without waiving other prerequisites.**

**Decision revision, 2026-10-02 13:46 UTC:** the three product/data choices are accepted; see [current accepted decisions](2026-10-02-architecture-refactor.md#101-accepted-user-decisions-and-clean-profile-cutover). Existing Floe development data may be discarded for a clean profile/schema without migration. No actual reset/deletion or implementation is authorized by this documentation update; new-system recovery safety remains mandatory.

Status: **PLAN ONLY, awaiting review**. No product files, tests, fixtures, manifests or build configuration have been modified. No test, build, compiler, formatter, Go command, push or commit has been run.

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`.

This appendix is subordinate to [the repository refactor plan](2026-10-02-architecture-refactor.md). The proposed module names match that plan. It supplies exact Go contracts and ordered changes, not a claim that the current implementation already satisfies them.

## 1. Complete inspection and bounded scope

- Frozen inventory: `git ls-files server`, **171 files, 1,073,037 bytes, 27,692 lines**.
- Every frozen file was read in full as UTF-8, including `.env.example`, `go.mod`, all embedded HTML/CSS/JavaScript, all JSON fixtures, macOS cgo source, and all tests. No binary/image file exists in this scope.
- [Per-file read ledger](file-read-ledger/server.json) records size, SHA-256, method, completion and meaningful observations. Truncated batch spans were reread explicitly; no truncated span counts as complete without a full reread.
- [Exhaustive baseline symbol map](2026-10-02-server-symbol-map.md) and its [machine-readable form](file-read-ledger/server-symbol-map.json) cover every frozen file and **1,609 named top-level Go declarations**, including test helpers. Every entry has a proposed KEEP/MOVE/SPLIT/REWRITE/DELETE disposition and target. Anonymous function-local helpers follow their containing function. Struct fields move with the explicitly mapped type except the split fields described below.
- Test inventory: **54 `*_test.go` files, 9,972 lines, 266 top-level `Test…` functions**. There are 12 explicit `t.Run`/`test.Run` call sites; that is not the behavior count. Unnamed table loops, maps, mutation loops, sequential phases and shared-fixture negative cases add more behaviors.
- There are no `Benchmark`, `Example` or `Fuzz` entry functions in server. No additional executable test-only source was found outside `*_test.go`; live product connection-check code is explicitly preserved below.

All baseline tests are **LEGACY ASSERTION evidence, not target requirements**. Full behavior extraction and deletion are later gated steps, not part of this proposal.

## 2. Final ownership and dependency graph

| Owner | Owns | Must not own |
|---|---|---|
| `node` | Composition, provider factories, required/optional initialization, startup recovery, shutdown, HTTP wiring | Bearer verification, grants, source policy, mutable integration/issuer maps, provider selection |
| `trust` | Verified client principal; operator sessions; producer identity; active issuer bindings; revocation tombstones; exact-byte signature primitives; atomic paired credential plus issuer activation; durable revocation handoff journal | OAuth token refresh, provider selection, View query interpretation, source scope mutation |
| `pairing` | Bounded pairing/enrollment attempts, local proof of possession, exact fingerprint/admin confirmation, pending/poll/cancel/expiry | A second issuer registry or durable credential activation path |
| `integrations` | Person/connection source lifecycle, credential-slot identity, resource configuration/CAS/epoch, durable OAuth attempt and cleanup records, normalized catalog/snapshots, exact Reader selection, provider identity preflight/fence | Runtime grant authority, model recipient policy, concrete provider wire calls |
| `views` | Typed normalized query/payload/descriptor contracts, validation and bounded encoding, Reader contract, shared normalized calendar paging/merge algorithms | Credentials, trust decisions, provider routing, bearer/grant parsing |
| `authority` | Signed preview → admission claim → bounded read → staged one-use release; live trust/source rechecks; runtime quotas, stage lifecycle | Concrete connector imports, provider or View-ID switch, account credentials, an issuer cache, source lifecycle |
| `inference` | Operator configuration, internal purpose→provider/model selection, bounded native/structured invocation, replay identity and content-free audit, provider OAuth boundary | App-owned grant/source policy; execution of model-proposed tools |
| `transport/http` | Bounded strict decoding, routes, status mapping, cookies/headers, redacted presentation | Constructing an authenticated principal, business callbacks, provider reads, owner mutable state |
| `credentials` | Keychain Store and Person/connection namespace derivation | Source ownership or consent |
| `storage` | Private atomic-file write/read mechanics and indeterminate-write classification | Business state or transaction policy |

`authority` imports only the inward trust/views contracts plus neutral operation/error and standard-library facilities. Its `SourceResolver`/`SourceFence` interfaces are implemented by integrations. Integrations may import authority solely to name the resolved-source DTO/port and satisfy that interface; authority must never import integrations, so there is no cycle. An alternative duplicate shared port package is rejected.

`views` imports neither trust nor integrations nor concrete connectors. Integration principal-aware resolution happens before the Reader receives an immutable admitted source/query. Concrete `connectors/*` import `views` and the narrow integration setup/snapshot ports. Node imports concrete constructors and injects factories; integrations does not import provider implementations.

OAuth implementations move under `connectors/googleauth`, `connectors/microsoftauth`, and `connectors/workoauth`. Keeping the last package is justified by two actual implemented provider profiles, not speculative extensibility. `codexauth` moves to `inference/codex`; its tokens are model-provider credentials, not source authority. No forwarding aliases or duplicate old/new runtime paths remain at a completed checkpoint.

## 3. Exact contracts

These are proposed signatures, not files added by this planning task. All returned slices/bytes/maps are owned copies; no caller receives a mutable owner map. Typed IDs below are validated string wrappers; authentication can only be created by trust, never JSON decoding.

### 3.1 Trust

`trust.Principal` carries client, Person and device IDs plus an unexported trust-generation stamp. It has read-only accessors `ClientID()`, `PersonID()`, `DeviceID()`. Remove the exported `Authenticated bool`; a caller-supplied boolean cannot constitute authority. The zero value is invalid.

`trust.Service` exposes:

```go
AuthenticateBearer(ctx context.Context, bearer string) (Principal, error)
WithCurrentPrincipal(principal Principal, consume func(PrincipalSnapshot) error) error
ActiveIssuer(principal Principal) (IssuerSnapshot, error)
WithActiveIssuer(principal Principal, keyID string, consume func(IssuerSnapshot) error) error
ProducerMetadata() (ProducerMetadata, error)
SignProducerChallenge(challenge []byte) ([]byte, error)
ActivatePairing(ctx context.Context, activation PairingActivation) (PairingCommit, error)
RevokeClient(ctx context.Context, clientID string) (RevocationReceipt, error)
RevokeIssuer(ctx context.Context, keyID string) error
PendingCleanup() ([]CleanupTicket, error)
AcknowledgeCleanup(ctx context.Context, ticket CleanupTicket, receipt CleanupReceipt) error
RequiredSecurityError() error
```

- `PrincipalSnapshot`: exact principal IDs, trust generation, active flag and cleanup-blocked flag. The short callback executes under the trust read fence. It must not perform network or vault I/O or take a lifecycle-operation mutex.
- `IssuerSnapshot`: validated exact principal, issuer key ID, copied Ed25519 public key, activation generation. Revocation and activation use the trust transition lock; `ActiveIssuer` selects the sole active issuer only for challenge construction; `WithActiveIssuer` verifies the exact key and principal stamp before proof acceptance. Every principal/issuer activation or revocation advances the shared monotonic trust generation. Final source consumption validates that same generation under the single trust fence, never relies on an issuer map in authority.
- `ProducerMetadata`: schema, instance ID, execution owner ID, signed routing audience, key ID, public key, fingerprint. No private key; a failed reopen never creates a replacement identity.
- `PairingActivation`: immutable pairing ID, exact Person/device, expected producer identity/fingerprint/audience, issuer public key/key ID/fingerprint, locally confirmed proof receipt, exact admin confirmation, expected trust revision and token hash. `PairingCommit` is a receipt/generation only. Pairing alone temporarily holds the plaintext app token and releases it only after successful durable commit. The trust call does not accept arbitrary persistence callbacks.
- `RevokeClient` atomically removes the paired bearer and related issuer keys, retains issuer tombstones, advances trust generation, and writes a durable cleanup ticket. It returns only after fsync/rename/directory-fsync. A postrename durability uncertainty latches deny and is never described as success or rolled back to a trusted in-memory state.
- `CleanupTicket`: ticket UUID, revision, Person, initiating/revoked client, kind (`client_attempts` or `person_all_sources`), and trust revocation generation. It contains no tokens, provider bodies or mutable integration records. Trust blocks source use/re-pairing until integration completion for that exact ticket. A remaining same-Person client retains unrelated completed sources and other clients' attempts; only the revoked client's attempt ownership is removed unless the revoked client was last.
- `CleanupReceipt`: ticket identity/generation and completed integration durable revision. Trust validates exact ticket and generation. Old/replayed completion cannot clear a later cleanup blocker.
- Errors: typed `operation.Error` categories `Invalid`, `Unauthenticated`, `Denied`, `Conflict`, `Limited`, `Unavailable`, `Internal`; stable public code only. Detailed OS/provider text is not returned to clients.

Operator session authentication also moves from `transport/http/session.go` into trust. Transport retains cookie/header extraction and mapping; trust owns token hash, session expiry/cap, login rate limit and CSRF state. Sessions remain process-local and cannot be used as app principals.

The shared proof codec in `trust/proof.go` owns canonical base64/UUID/duplicate-free bounded JSON primitives, `Proof`, and exact-byte signature verification. It does not interpret grants or source queries. Pairing encodes/validates enrollment operations; authority encodes/validates admission and release operations. Preserve current external signature domains unless an actual protocol meaning change is deliberately coordinated with the Rust owner; moving packages alone must not rename them.

### 3.2 Integration setup and identity ports

Replace `Action(ctx,string) (any,error)`, optional interface assertions, mutable singleton rebinding and contextless reads with explicit provider-side ports:

```go
type Setup interface {
    Begin(context.Context, CredentialBinding) (AuthorizationProgress, error)
    Poll(context.Context, AttemptRef) (AuthorizationProgress, error)
    Cancel(context.Context, AttemptRef) error
    Disconnect(context.Context, CredentialBinding) error
    CachedStatus(CredentialBinding) CredentialStatus
}
type IdentityVerifier interface {
    Preflight(context.Context, CredentialBinding) (ProviderIdentity, error)
    WithVerified(CredentialBinding, ProviderIdentity, func() error) error
}
type RuntimeFactory interface {
    Open(context.Context, RuntimeConfig) (Runtime, error)
}
```

- `CredentialBinding`: opaque slot name derived from immutable namespace + Person + connection UUID, connection incarnation and binding generation. Only integrations and real credential/provider adapters see it. Never put it in a View, proof query, user-facing snapshot, inference context or trace.
- `AuthorizationProgress`: attempt reference, typed state (`pending`, `connected`, `disconnected`, `failed`), optional HTTPS authorization URL, optional bounded device user code, redacted error code. No verifier, access/refresh/ID token, client secret or raw identity response.
- `AttemptRef`: immutable attempt UUID and connection/binding generation. Provider completion must check the same generation; a canceled/revoked/rebound attempt cannot publish later.
- `CredentialStatus`: cached readiness, cached verified identity availability and bounded reason code. No vault/provider I/O under an owner lock.
- `ProviderIdentity`: provider namespace and verified immutable account subject, verification state and generation. Google Calendar uses verified UserInfo subject; Microsoft Calendar uses validated tenant+subject OIDC evidence. Email, display name, endpoint host and merely possessing an access token are not identities.
- `RuntimeConfig`: exact connection/Person IDs, canonical typed selected resources, credential binding and immutable provider profile; node-injected factory has real adapter dependencies. Do not make integrations reconstruct provider clients by importing concrete packages.
- `Runtime`: typed `Snapshot(context.Context) (IntegrationSnapshot,error)` for normalized cached state, `Setup`, `IdentityVerifier` when supported, plus a map of registered `views.ID` to `views.Reader`. Unsupported identity support is explicit; missing identity remains denied, never fabricated by fallback.
- A factory opens a connection-scoped runtime. No process-global provider singleton is rebound while an earlier logout/token refresh is active. Per-connection lifecycle serialization remains; different sources do not contend on a global owner mutex.

For the present single-operator product retain one Person per node and one connection per connector/Person until a separately approved multi-account requirement exists. Remove the accidental cross-provider Calendar exclusivity check: each explicit Google/Microsoft connection can coexist, but a read remains pinned to the admitted exact connection and has no cross-provider fallback. This aligns the general source identity model and does not add write capabilities.

`integrations.Service` has typed `Catalog`, `List`, `Start`, `Poll`, `Cancel`, `UpdateScope`, `Disconnect`, `ApplyRevocation`, `ResumeCleanup` operations. Every mutating method takes a trust-issued principal (except trusted revocation replay), validates it again at commit, and uses connection ID+revision CAS. Reordering canonical resource sets is a no-op; adding/removing resources advances revision and source epoch exactly once. It never advances Rust GrantAuthority or silently re-reviews a grant.

### 3.3 SourceResolver, SourceFence and Reader

```go
// Defined by authority; implemented by integrations.
type SourceResolver interface {
    ResolveSource(context.Context, trust.Principal, views.SourceTarget) (ResolvedSource, error)
    PreflightSource(context.Context, trust.Principal, views.SourceSnapshot) error
}
type SourceFence interface {
    WithCurrentSource(trust.Principal, views.SourceSnapshot,
        func(views.SourceSnapshot) error) error
}
type ResolvedSource struct {
    Snapshot views.SourceSnapshot
    Reader   views.Reader
    Limits   views.Bounds
}
// Defined by views; implemented by concrete source runtimes.
type Reader interface {
    Read(context.Context, ReadRequest) (Result, error)
}
type ReadRequest struct {
    Source SourceSnapshot
    Query  Query
    Bounds Bounds
}
```

- `views.SourceTarget`: exact View ID, connector ID, connection UUID, requested connection revision (zero permitted only for preview lookup), and logical resource ID `view_id:connection_id`. Principal/Person is supplied separately by trust, never trusted from a target/query.
- `views.SourceReference`: connector ID, connection ID, execution owner ID, source incarnation, source epoch.
- `views.SourceSnapshot`: SourceReference + authoritative connection revision, Person ID, optional bound device ID, verified provider identity and its generation, canonical physical resource IDs, source active flag, schema/descriptor limits, and copied normalized source descriptor. It carries **no credential key or token**. Provider-native resource IDs are owner/adapter routing data; they never enter normalized result/Agent context.
- Resolver loads the current owned record, checks Person/device/current revision/source epoch/identity/cleanup, selects exactly the registered Reader for that View, and returns deep copies. It cannot try another source on permission, identity or provider failure. It also cannot trigger a source read just to discover descriptor metadata.
- `PreflightSource` may refresh/reverify provider identity with a bounded caller context, outside all owner locks; afterwards it compares the complete expected source revision/incarnation/epoch/resources/provider identity/generation. A changed identity is quarantined. It never silently rewrites the source into a newly trusted account.
- `WithCurrentSource` checks the full snapshot rather than only SourceReference. It invokes a short, non-I/O callback only while the trust current-principal generation fence, integration current-record fence and provider cached-identity fence remain valid. Proof verification has already captured the exact active issuer under the same principal generation; every issuer mutation invalidates that generation. Do not recursively acquire the trust RWMutex from an already fenced callback. The Reader itself must not run inside this callback.
- `views.Bounds`: maximum items and serialized bytes, bounded by descriptor and runtime stage caps; effective limit is the minimum of caller grant, View contract and server cap. No adapter may increase it.
- `views.Query`: closed tagged union with exactly one of CalendarQuery, MailQuery, WorkQuery, LogisticsQuery. `views.ParseQuery(ID, raw)` strictly validates and returns canonical bytes+digest alongside the typed value. Canonicalization is coordinated with the Rust caller; the signed digest must always bind the exact agreed bytes, not re-encoded ambiguous input.
- CalendarQuery: nonnegative start/end Unix milliseconds, increasing range ≤32 days, canonical composite cursor ≤2,048 bytes, limit 1..128. MailQuery: bounded search ≤512 bytes without CR/LF/NUL, integer cursor ≥0 (canonical cap 10,000), limit 1..100. Work/LogisticsQuery: schema version only; configured source resources are never query-controlled. A provider may impose a smaller bound.
- `views.Result`: closed typed union of CalendarView, CommunicationView, WorkContextView, LogisticsView, with one valid payload. Envelope includes version/View ID, opaque source handle, observation/expiry, coverage completeness, typed cursor, and typed items. Mail bodies are outside the currently exposed four-View contract; preserve internal explicit body-read authority and do not expose a new body route incidentally.
- `views.EncodeBounded(Result, Bounds)` validates view ID, item fields, evidence handles, observation/expiry consistency, cursor, source linkage and duplicates, recomputes item count, and returns canonical JSON bytes+count. Result JSON is not an unconstrained `any`; a provider cannot claim a lower count than its payload. Use actual UTF-8 byte counts.
- Reader receives no grant object, principal token, app bearer, model provider or arbitrary URL. Concrete adapter retains token source internally; source mutation remains an integrations operation. Reader errors distinguish invalid query, credential expired, permission denied, rate limited, unavailable, invalid provider response, cancellation/deadline. No raw upstream body escapes. Authority maps them to stable public error codes and discards any accompanying payload.

## 4. Runtime authority algorithm and lock discipline

1. Authenticate bearer through trust. Decode the request strictly; reject duplicates, aliases, unknown fields, trailing bytes, malformed proof/base64/UUID and excess nesting before an owner call.
2. Preview resolves an owned exact source snapshot and signs that current snapshot, physical resource set, logical resource, provider identity, producer routing audience and expiry. The audience is transport routing evidence, never model-recipient permission.
3. Admission resolves the same current connection/revision, validates typed query and effective budgets, runs identity preflight outside locks, then issues one-use signed challenge binding principal, issuer, source reference, **independent GrantAuthority**, resource, query digest, purpose, consumer and budgets. Source edits cannot mutate the grant epoch.
4. Read claims the admission once, verifies exact challenge bytes and current active issuer, preflights provider identity, then checks the full source snapshot immediately before dispatch. Release owner locks and invoke the exact Reader with caller context and immutable admitted query. No automatic source/provider fallback.
5. Validate the normalized payload and budgets. Recheck full source/credential identity continuity after I/O; if revision/incarnation/epoch/provider identity/generation changed, drop the payload. Stage a copied bounded result and a separate one-use release challenge binding result digest and admission ID. Read returns only the release proof challenge, never source contents.
6. Release verifies the exact issuer proof, then validates the captured trust generation and current source/identity under short fences, consumes the stage exactly once, and returns copied result bytes. Concurrent releases have one winner. Cancellation, timeout, failed callbacks or lost continuity drop unusable stage/checking state; they never reset a challenge to pending.
7. Revocation, sticky uncertainty and cleanup blocks deny both newly fetched service references and cached authority objects. Do not rely on swapping a pointer to nil.

Lock order: lifecycle mutex (only for setup/mutation, never held by read fence) → one trust current-principal generation read fence → integrations record read fence → provider cached identity read fence → authority stage mutex. `WithActiveIssuer` proof verification completes before requesting SourceFence; it is not held while SourceFence obtains trust. Every key/principal mutation increments trust generation, so the final check closes the intervening race without recursive RWMutex acquisition. Authority releases its mutex before requesting any outer fence. Cleanup provider/vault I/O executes with only a per-connection operation reservation, outside trust/integrations map locks. Never hold a global Vault transaction across model/provider I/O. Observer cancellation cancels the observation/read only; it does not manufacture a Run cancellation.

Trust revocation does not wait for a provider logout to return. The durable ticket denies use immediately; cleanup may finish or retry later. A late callback must compare initiating client and binding generation before publication and otherwise discard/clean its credential under the journaled intent.

## 5. Durable state split and ordered execution

The central plan’s T0/S1/S2 ordering is authoritative. The subsections below map Go work onto that sequence and are not independent checkpoints or validation gates. S1.2 must include trust-coupled integration records/cleanup ownership, not a temporary bridge to Console; if that subset cannot be separated safely, move the entire integration lifecycle into S1.2. Reader normalization, remaining provider factories and generic source HTTP cutover remain S2.1.

No migration chain, compatibility decoder, `next` API or new schema version solely to retain old local state. Use a clean new Floe development profile for the final stored format. The user permits discarding existing Floe development data; do not require preservation, migration or an old decoder/runtime for compatibility. A later reset has exact Floe-only targets, action-time confirmation and a narrow external-effect hazard check; provider/calendar/mail data is not a reset target. A read/open/key error never authorizes deleting or replacing it.

### Central T0: review and extraction gate

Approve the target contracts and owner map. Freeze source hash and test inventory. Complete natural-language per-behavior ledger before removing any test file. Keep this plan and historical ledger separate from product/architecture truth.

### Central S1.1: establish inward contracts only

1. Move normalized payloads/queries from `connectors/common`, Gmail and Microsoft Mail to views; integration metadata to integrations. Remove duplicate aliases/types in the same change.
2. Add credentials.Store and pure storage private-file primitives. Preserve exact file mode, file sync, rename and directory sync semantics and indeterminate result.
3. Define the exact trust/integration/authority ports above. Concrete connector/Reader normalization, OAuth package movement and all associated caller/import updates belong to central S2.1, except dependencies required to make S1.2 trust/lifecycle ownership real. Do not broaden S1.1 into full provider migration.
4. Keep concrete provider filtering/HTTP/ID hashing in connectors. Record the bounded timestamp/profile-binding defects as S2.1 adapter corrections; do not encode them as requirements.

### Central S1.2: trust, pairing and coupled durable integration lifecycle

1. Move producer identity, bearer principal verification, issuer/tombstone records, security latches and operator session state into trust. Establish a single durable trust store and its exact activation transaction.
2. Move pairing/enrollment temporary state into pairing; replace Host callback bag with narrow trust port. Remove legacy start/approve/CommitLegacy paths after fixture callers are documented and removed/migrated.
3. Authority stops loading/owning active issuers. Replace every issuer lookup with trust fences. No change to an external signature domain as a side effect of package movement.
4. Define last-client revocation transaction: bearer removal + issuer tombstones + trust generation + durable CleanupTicket in one trust commit. Persist ticket before any integration effect. On restart, replay pending tickets before allowing affected source use/pairing. A failed trust write has no external cleanup; an indeterminate commit latches deny.

### Central S1.2 prerequisite subset / S2.1 completion: integrations lifecycle

1. In S1.2, move the trust-coupled connection/attempt/cleanup records, lifecycle/reservation registry and durable revocation handoff from application/connections into integrations with the trust split. Include additional lifecycle functions if required for one real owner. Catalog presentation and Reader normalization may finish in S2.1; there is no migration-only adapter or duplicate owner.
2. Load integrations state before processing trust cleanup tickets. For each ticket, atomically mark exactly affected attempts/sources unusable and write per-source cleanup progress. Replaying the same ticket is idempotent and cannot target another client/Person/source generation.
3. Perform provider logout/revoke and source-specific cache cleanup first; persist runtime completion. Then delete only the exact credential slot; persist vault completion. A missing credential does not imply Gmail index cleanup is complete. Only after integrations durably records all required steps does it acknowledge the trust ticket.
4. Replace `retryPersonCleanupLocked` with reserve → unlock → provider/vault I/O → lock+CAS/persist progress. Reconnect cannot rebind a runtime while prior cleanup is outstanding. Current-client checks run again immediately before commit.
5. In S2.1, node supplies connection-scoped runtime factories. Remove singleton per-provider setters and legacy secret/OAuth exceptions. Publish snapshots without network reads and without map/JSON round-trip ownership inference.

### Central S2.1: generic source read authority

Move provider/View selection and domain parsing out of `authorization.SourceService`. Introduce typed SourceResolver/Fence/Reader; implement the seven-step flow above. Delete legacy communication adapters and all `any` domain result casts. Move stage state only into authority; move producer/latches out of Admissions. Migrate Rust provider transport and HTTP DTOs together so source/grant signatures and query digest agree. No API aliases.

### Central S1.4 inference boundary / S2.5 node closure

1. In S1.4, split `Gateway.ServeHTTP`: transport handles auth/decode/encode; inference service handles purpose routing, validated transcript, bounded provider work, usage certainty and content-free traces. Paired transport exposes no provider-native replay. Move provider profile/credential config and probe orchestration from Console into inference.
2. Replace old per-request exact model-recipient/allow-external semantics using [canonical contracts §§1–2](2026-10-02-canonical-contracts.md), which define the exact coordinated Rust+Go source-processing/Gateway boundary and strict inference schema 2. Ordinary model invocation is not approval; retain all source review and Health-local privacy-transform requirements. Do not simply remove the boolean checks in isolation.
3. Collapse env-selected headless and console composition into one node startup path. Fixture execution supplies configuration to that same node/service, never a second bearer/auth implementation.
4. At central S2.5 closure, `cmd/floe-server/main.go` keeps environment/argument load, signals and exit reporting. Node owns initialization, recovery, optional-module status and Close/shutdown. Console and application package are deleted only once their mappings are implemented and callers removed.
5. Update HTTP management forms/DTOs and server README/config examples to the same snapshot. Operator-only provider details never appear in app discovery/context; plaintext credentials never appear in persisted config or logs.

## 6. Specific source/test findings that change the plan

1. `microsoftauth/identity.go:21`, `requiresProviderIdentity`: mutable scoped credential name no longer equals the bare Calendar namespace after BindCredential. Derive identity requirement from immutable provider profile/namespace. Existing unscoped OAuth identity tests do not establish the bound production path.
2. `connectors/microsoftcalendar/client.go:211`, `parseEventTime`: nonempty strings shorter than 10 bytes can reach `DateTime[10:]`. Validate strict timestamp length/shape before slicing. This is a static defect, not a runtime-tested diagnosis.
3. `application/person.go:170`, `retryPersonCleanupLocked`: provider logout can run for 20 seconds while Console mutex is held. Preserve durable deny/cleanup semantics while moving I/O outside owner locks.
4. `application/console_test.go:355`, fixture call rewrites GitHub/Slack requests by deleting `secret`; `fixture.pair` uses legacy pairing. Test names or request literals alone do not describe exercised input. The later behavior ledger must resolve helpers explicitly.
5. `googleauth/runtime_test.go:106`, malformed persisted Calendar identity test writes under Gmail credentialName. It can pass because Calendar credential is absent before malformed identity is ever decoded. Record this coverage weakness, not an assumed malformed-identity guarantee.
6. Several invalid-state tests omit required instance/execution-owner/trust fields and can fail earlier than the named condition. Trace preconditions and actual decisive branch statically; mark uncertainty where runtime is intentionally not checked.
7. `connectors/gmail/contract.go` duplicates common connector DTOs; `gmail/index.go` and `microsoftmail/client.go` duplicate communication payloads. Views/integrations must become sole owners rather than maintaining aliases.
8. `workoauth.Runtime.load` lacks Google/Microsoft credential-generation stale-load protection; its typed setup port must not claim identical identity/rebind guarantees without implementing them. No provider identity may be invented for unavailable provider support.
9. `connections.ValidatedConnectorScope` permits up to 128 Home Assistant entities while concrete Reader accepts 16. Adopt the smaller implemented provider cap in the canonical typed source scope; reject before credential/provider work.
10. `authorization.SourceService.readRemoteView` defaults unrecognized non-work View IDs to logistics. Target registered closed View IDs fail explicitly; no implicit fallthrough.
11. Several `ConnectionSnapshot` methods currently fetch private source data. A catalog/list operation must use cached normalized readiness; an admitted Reader performs actual fetching. This is a deliberate behavior change, not a mechanical move.
12. `inference/synthetic.go`, `application/test_target.go`, HTTP `TestRequest`, and embedded dashboard Test buttons are live product connectivity checks. Preserve/rewrite them; `net/http/httptest` in a production file is not evidence of automated test-only code.

## 7. Later legacy test extraction and safe deletion procedure

This is an execution method, **not a completed behavior ledger**.

For each of the 266 baseline test functions, enumerate every distinct assertion and every table/mutation/subtest/fixture-derived case. A ledger entry must contain:

- Stable ID and baseline commit/path/line/function/subtest/case label.
- `LEGACY ASSERTION` classification.
- Full preconditions, including shared helpers, environment flags, OS/cgo requirements, fake time, runtime rebinding, omitted state fields and HTTP body rewrites.
- Exact action/input in natural language, expected result/error/status, provider/vault/persistence side effects or required absence, state-transition ordering and recovery/concurrency branch.
- Edge/failure cases and gaps: an assertion that only checks non-nil error does not prove its named rejection path; a sleep timeout is a legacy scheduling expectation.
- Target disposition `retain` (semantic requirement supported independently), `reassess` (shape, accidental behavior, incomplete coverage or changed owner/policy), or `obsolete` (removed legacy mode/route/compatibility), with evidence and rationale. One test may split into several dispositions.
- Target owner and proposed future verification scope. Do not create replacement tests in the extraction/deletion pass.

Deletion is allowed only after an independent coverage audit matches all 54 files, all 266 tests, all table/subtest cases and all test-only helper declarations to ledger references. Remove whole `*_test.go` files only when their helpers have no surviving test consumer; preserve product/shared fixture code. Git baseline provides recoverability; do not delete the baseline object/history.

The later `go-audit.json` must list baseline test files/symbols/cases, ledger IDs, removed paths/ranges, preserved helper/fixture rationales and residuals. A static postdelete search inventories testing imports, Test/Benchmark/Example/Fuzz entry points, build tags, `httptest` use and fixture references. Every residual is explained, especially the live synthetic probe. Go's automatic `_test.go` discovery requires no explicit manifest registration cleanup; do not remove go.mod dependencies speculatively.

All 19 connector JSON fixture files remain until Rust and Flutter consumers are inventoried and their behavior is extracted/relocated. `fixtures/remote-authorization/v1.json` and `producer-v1.json` are outside this server scope and cross-language; deleting Go tests never authorizes deleting those fixtures.

## 8. Verification and completion criteria

Planning verification is static only. The all-file ledger must equal the frozen 171-path set with no pending entries and unchanged baseline hashes. The symbol map must cover every file, all top-level declarations and each explicitly split owner field. The implementation review must see no new duplicate owner or fallback path.

At a later approved implementation checkpoint, check import graph and residual symbols statically before any permitted execution: no application/authorization/connections remnants; no domain import of transport DTOs/concrete providers; no authority View-ID/provider switch; no context-dropping legacy adapter; no runtime issuer map outside trust; no plaintext credential in public DTO/persisted config/log. Residual external protocol versions, OAuth namespaces and signature domains are retained intentionally.

Tests, builds, compiler checks, formatter and Go commands remain **NOT RUN** in this task. Their absence is not a pass. Do not call the refactor validated until the user-approved postcutover verification phase has reconstructed tests from accepted target requirements and exercised real Apple/Keychain/provider boundaries as separately authorized. No live account, signing configuration, provider write or user-data reset is part of this proposal.

## 9. Canonical inference and exact symbol-map precedence

[Canonical contracts §§1–2](2026-10-02-canonical-contracts.md) own the exact routes, strict DTO fields, nullable usage/accounting, error/status mapping and Rust counterpart. This is a deliberate paired-inference schema 2 break in one snapshot, not a compatibility version bump. AppWire and signed source protocol namespaces retain their separately specified versions. Source inventory is never inference inventory. No purpose absence is inferred from 401/403/404/timeout/malformed schema/provider unreadiness. `not_configured` and `disabled` are explicit successful authenticated inventory states; selection/fallback remains Rust Inference-owned.

The revised [declaration map](2026-10-02-server-symbol-map.md) assigns one exact owner/new symbol or deletion to each baseline declaration. `Gateway::{Config,Route,Target,CodexClient,classForPurpose,validRequest,writeError}` no longer share a generic dual destination: private provider/config types, owner validation and HTTP decoding/error encoding have their respective exact paths. Genuine type/handler splits enumerate fields/branches. `cmd/floe-server/main.go::legacyGateway` is deleted. Concrete import/route/caller edges accompany the mapping; lexical inventory is supplemental evidence, not compiler-resolved proof. The machine map preserves all baseline anchors and the original read ledger is unchanged.
