# Modules and ownership

This is the stable semantic ownership map for the Rust workspace. Package paths match the current workspace layout.

## Accepted processing decision versus current implementation

[ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) replaces exact model-recipient consent with Access-owned source processing policy, removes concrete model routing from product intent, and selects Gateway-primary/local-fallback reasoning for Manager, shipped Experts and Learner. Health owns a separate mandatory local privacy operation, with Context retaining its sensitivity and transform provenance. These are accepted target contracts; this document does not claim their code cutover is complete.

The table and detailed rules below describe the pre-cutover implementation where recipient contracts/consent, product profiles and existing routing still exist. They are implementation anchors to replace, not requirements to preserve those representations. Update the affected rows/rules as each owner cutover lands under the [single convergence plan](../development/plans/reasoning-source-processing-convergence.md).

| Layer | Package / path | Owns |
|---|---|---|
| Contract | `floe-kernel` — `crates/contracts/kernel` | IDs and small shared values |
| Contract | `floe-context-contract` — `crates/contracts/context` | source-view, provenance and recipient value contracts |
| Contract | `floe-agent-contract` — `crates/contracts/agent` | Agent Card, Message, Task, Artifact and endpoint contracts |
| Runtime | `floe-execution` — `crates/runtime/execution` | cancellation, budgets and execution limits; no business state |
| Runtime | `floe-agent-runtime` — `crates/runtime/agent` | role-neutral LLM / Tool / Delegate loop |
| Module | `floe-access` — `crates/modules/access` | grants, authority, exact recipients, dispatch/release admission and revocation fences |
| Module | `floe-connections` — `crates/modules/connections` | Connection, OAuth and pairing lifecycle; durable Calendar and standing personal source identity, resources, local configuration CAS, `SourceAuthority` and trusted native subject identity |
| Module | `floe-inference` — `crates/modules/inference` | model profiles/routes, model attempts, Access-consumed dispatch through `ModelProvider`/`PreparedModelTransport`, and model usage ownership |
| Module | `floe-knowledge` — `crates/modules/knowledge` | Memory, Playbook records and bounded learning |
| Module | `floe-day` — `crates/modules/day` | Calendar event mirror, sync freshness/status and independent mirror CAS; Tasks, Notes and Day-domain projection, not source configuration |
| Module | `floe-context` — `crates/modules/context` | authorized projections, source acquisition, provenance, coverage and freshness |
| Module | `floe-actions` — `crates/modules/actions` | proposals, review/approval, idempotent external actions and outcome reconciliation |
| Module | `floe-experts` — `crates/modules/experts` | Expert directory, eligibility, assignment, Task ownership and endpoint dispatch |
| Module | `floe-conversation` — `crates/modules/conversation` | Session, root Run, transcript, exact validated-batch continuation and coverage reauthorization, finalization and durable user interactions (origin, reviewed target, lifecycle, decision intent, resume linkage) |
| Extension | `floe-experts-builtin` — `crates/experts/builtin` | built-in domain Expert endpoint implementations |
| Platform | `floe-diagnostics` — `crates/platform/diagnostics` | privacy-safe tracing, correlation and diagnostic export |
| Platform | `floe-native` — `crates/platform/native` | native host drivers and secure-key/platform access |
| Adapter | `floe-provider-adapters` — `crates/adapters/providers` | model/source/control transports and OS/HTTP adapter implementations |
| Adapter | `floe-vault` — `crates/adapters/vault` | encrypted storage engine and owner-specific repository adapters |
| Composition | `floe-app` — `crates/app` | concrete service construction and typed service handles |
| Binding | `floe-protocol` — `crates/bindings/protocol` | versioned product wire DTOs only |
| Binding | `floe-ffi` — `crates/bindings/ffi` | ABI, host lifetime and DTO conversion |

## Ownership rules

A stateful concept has one semantic owner. Storage location, call-site convenience or composition does not create a second owner.

Built-in Expert implementations depend on owner contracts and dispatch APIs, not
directly on the generic Agent Runtime or Conversation implementation. Conversation
uses the Agent Runtime's schema validation rather than declaring a second direct
schema-validator dependency. Test-only wiring stays in dev-dependencies: Day's
Tokio harness, Conversation's clock fixtures, App's base64 fixtures and Vault's
Inference records/Tokio harness are not production dependencies of those packages.

- **App** constructs and injects services and owns bounded first-party product policy composition. Each supported connector/View has one final policy derived from trusted shipped Expert capability declarations; its digest is the digest of the same policy used at activation, independent of Registry, binding, connection identity and source resources. It chooses the product-supported connector Views and actual built-in readers. Connections receives source setup/resources as product intent; one App `ConnectionObserve` product contract receives connection-level inspect, review and enable/disable intent for native Calendar, hosted Views, Contacts, Attention and Wellbeing. Access remains the grant authority. The client never supplies leaf resources as standing permission scope, and echoes only a backend-produced compare-only review expectation on enable.
- **Protocol/FFI** convert product intent and results. They do not decide execution topology.
- **Adapters** implement owner-defined ports. A provider may hold credentials or perform transport without inheriting Access or Context policy.
- **Vault** may physically store multiple owners' records. Shared storage does not grant cross-module table or policy ownership. Fresh Vaults create only current owner schemas; obsolete local policy/review tables fail reopen explicitly with `UnsupportedVersion`, without migration, deletion or key replacement.
- **Inference and Connections** are separate domains. Model routing does not infer source authorization, and a Connection does not authorize model processing.
- **Standing source truth is Connections-owned.** A `SourceConnection` binds the Person, connector, stable connection and execution owner to lifecycle, resource mode, current resources, local configuration revision, `SourceAuthority` and (for native sources) the current trusted subject fingerprint. Reviewed native creation persists a Ready revision-1 source directly; combined resource and subject review is one expected-revision CAS and one source-authority advance. Resource membership, effective mode, subject identity or disconnect changes advance source authority at this boundary; label-only changes may advance only the configuration revision. Server/producer revisions are not the local CAS revision.
- **Calendar mirrors are Day-owned observations.** Day persists imported events, provenance, per-source sync status and freshness under a separate mirror revision. Sync, partial import and transient provider failure cannot change Connections revision or source authority. App reloads the current source to validate imports and composes connector health from source plus mirror; a missing or stale mirror cannot supply connection authority. Flutter configures through a Connections gateway and uses the Day gateway only for mirror/sync work.
- **Context and Access** are complementary: Context establishes evidence/projection semantics; Access decides whether data may be acquired, dispatched or released for the exact authority.
- **View sensitivity is type-owned.** People and Attention project Personal evidence; Wellbeing projects HighlySensitive evidence independently of its consumer. App constructs the delegated Expert allowance from the exact Run-pinned manifest class plus Personal; the manifest never relabels evidence. Context's canonical model assembler unions caller-admitted classes with actual post-role-filtered evidence classes and adds Personal for projected Persona/Memory, then sorts and deduplicates. Access still denies external HighlySensitive dispatch; classification does not imply a Health privacy transform or Gateway eligibility.
- **Standing Observe grants name stable sources.** `GrantSourceBinding` contains only Person, Connection, Connector and execution owner; a grant's source cannot be replaced by review. `GrantAuthority` is the sole standing permission/state/scope epoch. Connections owns current Calendar, Contacts, Attention and Wellbeing `SourceAuthority`. Contacts uses Selected physical source resources; Attention and Wellbeing use AllAvailable singleton resources. Their grants name only the logical connection/View resource, never Contacts handles. Source epoch drift invalidates evidence without inherently mutating the standing grant or Expert binding. Remote View grants are resolved from `data_access_grants` by exact stable source and logical View resource; there is no mapping store.
- **Standing consumers reload Connections.** Context acquisition/candidates, App connection Observe review, Access admission and reviews, and Actions proposal/dispatch checks use the current `SourceConnection`, not a Vault source side table. Native EventKit and hosted Google/Microsoft Experts each select one `calendar.timeline:<connection>` View and Access grants that one logical resource. Native Context reads every current Calendar resource at acquisition; Contacts reads every currently selected handle and records those exact handles in `source_resources`. Attention and Wellbeing use their current singleton resources and native subject. Hosted Context uses generic remote View preview/admission/read/release, with provider leaves resolved by the server connection runtime. Standing Observe permission remains Access-owned.
- **Permission and provenance resources differ.** `ContextDependency.resources` names grant permission scope; `source_resources` names the exact provider or leaf set observed, with a separate current `source_authority`. The canonical connection/View resource formatter is shared by the context contract. Hosted generic signed View preview carries the current server `calendar_ids` separately from the logical resource; Context records and reauthorizes that exact signed set. Exact-recipient processing review binds both resource sets, `GrantAuthority` and `SourceAuthority`. Vault grant transactions validate the current grant without waiting for provider or Connections I/O.
- **Logical multi-source views preserve physical authority.** Context may deterministically merge a bounded set of source payloads, but the authorized read retains one dependency/scope binding per contributing source and every dependency is recorded and reauthorized independently.
- **Registry binding is not source permission.** Experts owns per-assignment selected source references and binding revisions as configuration. Context owns candidate identities and exact-target acquisition; Connections owns source/resource lifecycle; Access owns Observe authority over the exact source. Native and hosted Calendar resource edits advance their owning source authority and stale old dependencies, but do not re-review the logical grant or mutate Expert bindings. A hosted server edit advances producer revision/epoch; Flutter mirrors the current resource set into local Connections, which advances local `SourceAuthority` on a real membership change. Trusted first-party Calendar consumers come from shipped manifest capability declarations, not Registry assignment or binding state. Task admission persists the immutable selected execution snapshot and later binding changes fence, never reroute, pending execution. No Registry setup, view or assignment state authorizes a source read.
- **Experts** own Task/A2A semantics; the generic Agent Runtime does not know built-in Expert packages.
- **Actions** own Calendar Create/Update/Delete, manual and Expert-origin admission, immutable review/approval, pre-dispatch intent, native outcome evidence, explicit reconciliation and Day collection tickets through one encrypted repository. Product inputs use opaque destination choices or Day Event references; raw targets and source preconditions are resolved inside the owner. Experts prove historical Task/artifact provenance through an inward port implemented by Vault; Actions owns effect policy, without an Actions-to-Experts dependency. Native adapters implement single-use execution and causal receipt lookup. App constructs and forwards owner handles, and intelligence may propose inert artifacts without directly mutating providers.

## Dependency enforcement

The allowed compile-time DAG is machine-readable in
[`tools/architecture/module-dependencies.json`](../../tools/architecture/module-dependencies.json).
Run:

```sh
python3 tools/architecture/check_boundaries.py
```

The checker reads the actual Cargo manifests. Do not add a second hand-maintained "current dependency graph" to documentation.
