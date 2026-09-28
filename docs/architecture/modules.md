# Modules and ownership

This is the stable semantic ownership map for the Rust workspace. Package paths match the current workspace layout.

| Layer | Package / path | Owns |
|---|---|---|
| Contract | `floe-kernel` — `crates/contracts/kernel` | IDs and small shared values |
| Contract | `floe-context-contract` — `crates/contracts/context` | source-view, provenance and recipient value contracts |
| Contract | `floe-agent-contract` — `crates/contracts/agent` | Agent Card, Message, Task, Artifact and endpoint contracts |
| Runtime | `floe-execution` — `crates/runtime/execution` | cancellation, budgets and execution limits; no business state |
| Runtime | `floe-agent-runtime` — `crates/runtime/agent` | role-neutral LLM / Tool / Delegate loop |
| Module | `floe-access` — `crates/modules/access` | grants, authority, exact recipients, dispatch/release admission and revocation fences |
| Module | `floe-connections` — `crates/modules/connections` | Connection, OAuth and pairing lifecycle; durable Calendar source identity, resource scope, local configuration CAS, `SourceAuthority` and trusted native subject identity |
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

- **App** constructs and injects services and owns bounded first-party product policy composition. It chooses the product-supported connector views and actual built-in readers, but Access remains the Observe/grant authority and Flutter supplies only explicit connection intent.
- **Protocol/FFI** convert product intent and results. They do not decide execution topology.
- **Adapters** implement owner-defined ports. A provider may hold credentials or perform transport without inheriting Access or Context policy.
- **Vault** may physically store multiple owners' records. Shared storage does not grant cross-module table or policy ownership.
- **Inference and Connections** are separate domains. Model routing does not infer source authorization, and a Connection does not authorize model processing.
- **Calendar source truth is Connections-owned.** A `SourceConnection` binds the Person, connector, stable connection and execution owner to lifecycle, resource mode, current resources, local configuration revision, `SourceAuthority` and (for native sources) the current trusted subject fingerprint. Resource membership, effective mode, subject identity or disconnect changes advance source authority at this boundary; label-only changes may advance only the configuration revision. Server/producer revisions are not the local CAS revision.
- **Calendar mirrors are Day-owned observations.** Day persists imported events, provenance, per-source sync status and freshness under a separate mirror revision. Sync, partial import and transient provider failure cannot change Connections revision or source authority. App reloads the current source to validate imports and composes connector health from source plus mirror; a missing or stale mirror cannot supply connection authority. Flutter configures through a Connections gateway and uses the Day gateway only for mirror/sync work.
- **Context and Access** are complementary: Context establishes evidence/projection semantics; Access decides whether data may be acquired, dispatched or released for the exact authority.
- **Standing Observe grants name stable sources.** `GrantSourceBinding` contains only Person, Connection, Connector and execution owner; a grant's source cannot be replaced by review. Connections owns current Calendar `SourceAuthority`, while the bounded personal policy/source record owns it for Contacts, Attention and Wellbeing until checkpoint 06. Source epoch drift invalidates evidence without inherently mutating the standing grant or generic remote View mapping.
- **Calendar consumers reload Connections.** Context acquisition/candidates, Access admission and reviews, and Actions proposal/dispatch checks use the current `SourceConnection`, not Day mirror fields. Native and remote Calendar leaf grant resources remain bounded pre-03/pre-04 implementation details, not the final logical View permission model. Standing Observe permission remains Access-owned.
- **Permission and provenance resources differ.** `ContextDependency.resources` names grant permission scope; `source_resources` names the exact provider or leaf set observed, with a separate current `source_authority`. The canonical connection/View resource formatter is shared by the context contract. Exact-recipient processing review binds both resource sets and source authority. Vault grant transactions validate grant and policy authority without waiting for provider or Connections I/O.
- **Logical multi-source views preserve physical authority.** Context may deterministically merge a bounded set of source payloads, but the authorized read retains one dependency/scope binding per contributing source and every dependency is recorded and reauthorized independently.
- **Registry binding is not source permission.** Experts owns per-assignment selected source references and binding revisions as configuration. Context owns candidate identities and exact-target acquisition; Connections owns source/resource lifecycle; Access owns Observe authority over the exact source. Task admission persists the immutable selected execution snapshot and later binding changes fence, never reroute, pending execution. No Registry setup, view or assignment state authorizes a source read.
- **Experts** own Task/A2A semantics; the generic Agent Runtime does not know built-in Expert packages.
- **Actions** own consequential external-effect lifecycle, including uncertain outcomes. Intelligence may propose but does not directly mutate providers.

## Dependency enforcement

The allowed compile-time DAG is machine-readable in
[`tools/architecture/module-dependencies.json`](../../tools/architecture/module-dependencies.json).
Run:

```sh
python3 tools/architecture/check_boundaries.py
```

The checker reads the actual Cargo manifests. Do not add a second hand-maintained "current dependency graph" to documentation.
