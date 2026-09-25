# 04: persistent binding through runtime, authority and product

Prerequisite: 03 complete. This checkpoint is one coordinated semantic cutover, with substeps for implementation. It closes only when users can configure and recover bindings through the actual product path. No backend-only completion, second resume system or old/new source-selection fallback.

## 04-A: owner model and settings operations

Proposed owner concepts (freeze exact names/fields before implementation):

```text
Experts: ExpertBindingSet
  assignment identity
  manifest/requirement identity
  binding revision
  entries: requirement ID -> bounded canonical source-reference set

Context/Connections: source reference values
  exact stable connection/source identity
  capability/View + compatible contract version
  explicit resource selection and device/producer binding where necessary
```

A source reference expresses the selected object, not permission. Registry must not become a copy of credentials, grant state, current source authority, processing consent or connection lifecycle. Runtime resolves current owner facts through that reference. An opaque ID still needs an owner and unambiguous lookup semantics.

Use one binding-set revision per affected assignment, not a global revision that invalidates unrelated Experts. Validate nonempty/unique requirement IDs, supported versions, multiplicity, bounded bytes/items, canonical ordering, exact Person ownership and allowed device/producer identity. No magic `all sources now and in future` wildcard. An explicit configured set may have multiple selected sources. Empty/unconfigured is a real state, not a migration placeholder.

Commands are CAS-protected and idempotent by exact intent: assignment, manifest revision, expected binding revision and canonical selection. Reusing an operation ID with different targets conflicts. Reading candidate metadata is read-only and must not acquire source content or mutate permission. Use current owner capabilities, not App-maintained expert-to-provider tables.

Defaults run only during explicit install/setup/configuration under product policy. A unique compatible candidate may be preselected and persisted without pretending that a grant exists. Ambiguous candidates require a choice. Startup inspections and task invocations never create bindings. Reinstall preserves explicit user choices; a manifest upgrade that changes a requirement needs bounded reconfiguration rather than silent widening.

## 04-B: immutable Task execution selection

Extend the existing Experts admission/repository path to resolve and persist an execution selection before any source read:

```text
exact assignment
+ package/manifest/definition revision
+ binding-set revision
+ requirement -> exact source/resource targets
+ canonical selection digest
```

Reuse the existing Task/request journal identities. `delegation_request_digest` remains the canonical caller-request digest; owner-resolved selection is a distinct, explicitly named admission record associated with that request, not a competing digest of the same request. The LLM and Flutter do not supply an authoritative selection digest or consumer identity.

The Task admits an immutable copy or an immutable revision-addressable reference sufficient for deterministic replay. Validate it before persistence. No mutable global registry lookup may silently substitute newer targets on execution or retry. Use one producer of selection identity and reuse it across read, interaction and replay boundaries.

### Lifetime and change semantics

| Event | Required behavior |
|---|---|
| Source B is connected while Task is pinned to A | Task reads only A; no discovery/fallback to B. |
| Assignment binding changes A -> B | Existing Task never uses B. This plan invalidates subsequent acquisition/dispatch/release under the superseded active selection; use owner fences/cancellation rather than rerouting. |
| Binding removed or assignment disabled | Block new admission and fence later operations of affected active Tasks. Mere UI disposal is not revocation or cancellation. |
| Grant revoked/paused or source identity changes | Access blocks later admission/release independently; no Registry mutation is required to revoke source access. |
| Unrelated assignment is configured | Does not invalidate this Task via a global Registry revision. |
| Task query or exact retry | Uses the stored selection, with current authority revalidation; does not adopt latest settings. |
| Budget Continue / pending batch recovery | Keeps original selection and validated work identity; cannot obtain broader source scope. |
| Linked resume | Fresh Run/Task admission under the current explicit configuration; old consent cannot authorize changed source/consumer/recipient scope. |
| Crash / partial settings acknowledgement | Exact-command rejoin observes committed binding; no duplicate mutation or hidden reset. |

Revocation cannot recall bytes already transmitted or a provider effect already accepted. Preserve that limitation. For terminal historical evidence, keep Context/Access provenance and reuse rules; do not rewrite or erase history when a binding changes. A new active-task selection fence is not a duplicate source-grant authority.

Use owner-level persistence/transaction boundaries. Never hold a global Vault transaction across source/model I/O. Close races between resolve, persist and dispatch by validating the same selection revision at the real handoff/release gates. Tests must pause those boundaries deliberately, not just run sequential settings updates.

## 04-C: one exact-target read path

Primary existing anchors:

- `crates/modules/context/src/ports/source_reader.rs`, `application/service.rs`.
- `crates/modules/context/src/application/remote_sources.rs`, `remote_views.rs`, `native_calendar_view.rs`, `personal_sources.rs`.
- `crates/app/src/vault_host/remote_views.rs` and `conversation_turn/expert_host.rs`.
- Generic host/runner and all builtin dispatch callers converted in 03.
- `crates/modules/experts/src/task.rs`, `crates/adapters/vault/src/repositories/task.rs` for selection persistence.

Runtime exposes requirement ID + bounded query. The trusted host resolves that key only inside the admitted assignment selection and supplies the exact targets to Context. Reject undeclared requirement keys, foreign binding handles and source selectors smuggled into query JSON. Query time range, cursor and filters may vary only within the admitted resources and owner limits.

`core.calendar_connection(person_id)` and full grant enumeration may check current identity/authority but must no longer decide which source the Expert uses. Credential reload and exact connection revalidation remain allowed. Replace current global source selection; do not leave a fallback to it when a binding lookup fails.

For multi-source reads, keep target-specific grant/source/policy authority and every `ContextDependency`. Classify only selected targets. Unselected source failure cannot block the read; a selected source failure cannot disappear from a complete aggregate. Preserve bounded pagination/coverage: a cursor belongs to its selected source, no complete flag is invented after truncation, and an Action does not infer free time from incomplete evidence.

Return `Ready(empty)` only for a successful empty observation. Return `Unavailable` for an unavailable configured source and trusted setup/review requirements for recoverable configuration/permission states. Package reasoning determines the domain meaning of optional absence without laundering it into empty source evidence.

## 04-D: exact subjects and reviewed default grants

Inspect and converge `crates/app/src/first_party_observe.rs`, `connection_observe.rs`, `vault_host/remote_observe.rs`, `calendar_access.rs`, `personal_grants.rs`, Access grant APIs and Vault policy repositories.

Remove built-in enum and ad-hoc `attention.expert`/`contacts.expert` assumptions from generic authority mapping. The trusted installation/assignment supplies the real subject; distribution/trust metadata is not a caller-controlled ID prefix. Preserve typed distinctions needed to keep extensions from impersonating first-party or Manager consumers.

Product default policy is a composition of declared requirements, explicit selections, product policy and reviewed exact consumer/capability scope. The grant may cover a reviewed set of consumers, but a different assignment still needs its own configured selection and current admission. New installation is not authority; a manifest update or new package must not join an old approval automatically.

Keep Manager direct tools usable as repaired in 01. Manager selection stays product-owned; the generic read pipeline may be reused without adding a fictitious Manager Expert installation. Inference role/profile matching is separate from source subject and exact recipient consent. Never grant external processing merely because Observe or binding is enabled.

Freeze the exact consumer-set/revision digest in review targets where a mutation changes that set. Review -> package install -> approve must reject or require a fresh review, not approve the enlarged scope.

## 04-E: existing durable interactions, new configuration target

Existing anchors: `crates/modules/conversation/src/domain/interaction.rs`, `application/interactions.rs`, `application/resume.rs`, `ports/interactions.rs`; `crates/app/src/vault_host/review_snapshot.rs`, `interaction_resolution.rs`, `interaction_owners.rs`, `conversation_turn/interaction_publication.rs`; Vault interaction repositories.

Separate two requirements:

| Requirement | Mutation owner / target |
|---|---|
| No source chosen for an Expert requirement | Experts binding settings; exact assignment, requirement, manifest revision and expected binding revision. |
| Selected source lacks current permission/identity | Existing Access/Connections review/reconnect; exact selected source/resource, consumer and authority. |

Add the smallest necessary typed configuration target to the existing interaction mechanism; do not represent unselected sources with invented resource IDs, grants or producer fingerprints. Owner-produced binding requirements are trusted host data; arbitrary model artifacts cannot publish them.

Include admitted selection identity where needed to distinguish identical source requirements from different assignments. Preserve canonical origin verification, deterministic publication, immutable reviewed target, decision CAS, exact-command rejoin and per-origin resume slot. A source/manifest/binding/consumer change after publication supersedes the old target instead of upgrading its meaning.

A navigation return or successful Observe mutation is not sufficient to mark the original requirement satisfied. Ask the appropriate owner whether the exact assignment/requirement or selected consumer operation is now configured/admissible. Do not read source payload merely to check settings completion.

The original Run completes with its limitation. A linked resume remains a fresh Run, not a waiting Task, budget continuation or a blind retry of uncertain external actions. Owner mutation and interaction resolution use existing durable operation identity so a crash between them is recoverable without applying the mutation twice.

## 04-F: admitted App/FFI API and usable Expert settings

Existing anchors: `crates/app/src/expert_services.rs`, `local_operations.rs`, `worker.rs`, `vault_host.rs`; `crates/bindings/protocol/src/dto/experts.rs`, `commands.rs`, `queries.rs`, `interactions.rs`; `crates/bindings/ffi/src/app_wire.rs`, `conversion/owners.rs`; `apps/client/lib/app/runtime/local_owner_gateways.dart` and `features/experts/`.

Expose only generic intents: list/detail, enable/disable, read binding candidates, replace/remove requirement selection with expected revision. Exact DTO names are finalized at the owner before implementation. Obtain Person/device/epoch from `CallerContext`; product payloads never supply authoritative consumer identity, bearer, arbitrary endpoint or preapproved grant flags. Reuse current bounded submit/read/release correlation and accepted-decode-before-release behavior.

Create a generic Expert gateway/controller and a discoverable list/detail/settings surface. Display package metadata, enabled state, requirement selection and derived current availability. Unknown registered Expert IDs use the same UI. A minimal bounded package settings schema is acceptable only where needed; a dynamic UI plugin framework is not required.

Connections owns account connection, resource scope and Use with Floe/Observe; Experts settings owns which admitted source candidates are selected for that assignment. Link to the relevant authority owner for review instead of persisting a second permission toggle. An unconfigured Expert remains visible and may be callable.

Use the existing design system, accessible controls and localization. Keep safe user-facing source labels distinct from opaque technical identity. Review changes disclose the actual scope/consumers; no duplicate Calendar Expert setup dialog or separate resume gateway.

## Tests and deletion gate

| Test group | Mandatory cases |
|---|---|
| Settings | Zero/one/multiple candidates, explicit selection, CAS conflict, exact retry, reopen, unrelated assignment update, manifest drift, disablement preserved. |
| Admission | Foreign assignment/handle, undeclared requirement, source selection injection, same request after changed settings, deterministic selection identity. |
| Runtime | A selected/B unselected; B add/pause/revoke irrelevant; A failure never falls back; no connector discovery during invocation; all contributors retain provenance. |
| Races | Change binding/grant at resolve/persist/read/model handoff/release; affected Task blocks, unrelated Task remains valid. |
| Interaction | Missing binding -> settings -> exact satisfaction -> linked resume; old review after changed selection or consumer set rejected; crash/rejoin without duplicate mutation. |
| Product wire/UI | Generic unknown Expert, settings routes, malformed/foreign command rejection, backend DTO -> Dart fixture, navigation refresh not implicit execution/cancellation. |
| Actions | Proposal retains exact contributor/target and approval; binding selection never grants Act or uncertain-write retry. |

Delete global grant-driven selection from Expert production reads, common typed host union methods, builtin-derived consumer policy, and old fake fixtures that model authority as configuration. Keep legitimate source validators, native/provider adapters and Access revocation checks. Every remaining source-enumeration call must be setup/metadata or exact current-state observation, not invocation-time reselection.

Run all relevant targeted tests, Rust/dependency gates and full product/macOS gate in [06](06-verification.md); run Go tests when source protocol or server behavior is touched. Update current architecture, authority and actual product/design docs. 04 closes only after a user can configure and recover an Expert through the real UI while the A/B isolation acceptance holds.
