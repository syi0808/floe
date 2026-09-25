# 03: generic Expert Registry and endpoint registration

Prerequisite: 02 complete. This checkpoint removes closed-world registration and TimelineRead-shaped package topology. It does not claim persistent source binding is complete; 04 owns that cutover.

## Baseline anchors

| Existing path | Relevant surface |
|---|---|
| `crates/modules/experts/src/registry.rs` | `AgentPackage`, `PackageImplementation`, `ExpertRule`, `RegistrySnapshot`, `ResolvedExpert`, required tool validation |
| `crates/modules/experts/src/registry/expert_setup.rs`, `builtin_setup.rs` | Built-in receipts/install helpers, exactly-two package topology, builtin-only card and assignment lookup |
| `crates/modules/experts/src/directory.rs`, `dispatch.rs`, `task.rs` | Generic registry of endpoints and Task lifecycle to reuse |
| `crates/experts/builtin/src/catalog.rs`, `host.rs`, `lib.rs` | Declarations, built-in enum export and host union |
| `crates/app/src/vault_host/expert_setup.rs` | Product bundle installation composition |
| `crates/app/src/vault_host.rs` | `OpenVault::sync_expert_directory`, builtin-only endpoint field and filters |
| `crates/app/src/vault_host/conversation_turn/expert_dispatch.rs`, `expert_host.rs` | Explicit eight-runner registration and domain-specific host readers |
| `crates/adapters/vault/src/vault/registry.rs`, `repositories/task.rs` | Persisted topology, restore and settlement linkage |

## 03-A: manifest and installation contracts

Define the manifest at Experts, keeping pure wire/endpoint values in existing contracts only where genuinely shared. At minimum describe package identity/version, discovery metadata, package-owned prompt/result identity, declared source requirements, necessary tool capabilities, execution constraints and bounded configuration/state schema identity.

Requirements use a package-local key plus a supported capability/View contract and multiplicity/requiredness. They do not contain credentials, resolved provider routes or authority. Zero-source Experts and multiple-capability Experts must be representable without fabricating a TimelineRead Tool.

Delete `PackageImplementation::{TimelineRead,Builtin,Declarative}` and `ExpertRule::FindFocusWindow` as generic execution semantics. Remove `required_tools.len() == 1`, forced `[Tool, Expert]` installation pairs and synthetic Tool records whose only purpose was old Calendar setup. Preserve genuine tool linkage only where a real consumer needs it.

Replace `BuiltinExpertSetup*`/`builtin_setups` with the minimal generic installation/assignment mechanism. Keep exact-operation idempotency, CAS, Person isolation, package identity validation and explicit enabled state. Repeating a setup with changed manifest/specs under the same operation identity conflicts. Existing user disablement or explicit configuration wins over startup defaults.

No source permission enters Registry. Source absence does not remove a callable card. Restore fails closed on malformed topology; it never silently manufactures a missing installation.

## 03-B: bundle-provided registrations

The product-shipped bundle supplies generic manifest + runner/endpoint registrations. App loops over that supplied collection and wires real services; it does not list Schedule/Communication/etc., test an ID prefix or filter `BuiltinExpertKind::ALL`.

Keep `BuiltinExpertKind` private to the bundle only if it is useful. Bundle registration changes are the only production wiring changes required for an additional bundled Expert using existing capabilities. Registering a separate package collection in a test must work through the same public seam.

Preserve one execution path through Directory -> TaskCoordinator -> AgentEndpoint -> bounded Expert execution -> shared Inference/Context. Native versus remote endpoint transports may be distinct adapters when real, but built-in versus extension is not a different lifecycle.

An extension runner receives the admitted identity rather than a free-form caller-selected consumer. Same-process arbitrary malicious Rust is not sandboxed by a trait; this plan does not claim dynamic untrusted-code isolation. Test supplied packages remain statically controlled code, and all source/authority APIs still enforce exact identity.

## 03-C: generic host surface, no forwarding facade

Reuse the existing model port, generic source-read outcome and execution scope. Change the common host to expose bounded model execution, declared requirement reads, evidence recording and generic settlement. Move Calendar/People/Wellbeing payload helpers into their domain owner or package, rather than adding a common host method for each Expert.

During this checkpoint, the single existing Context source-selection implementation may still resolve a declared requirement using the current product state. Explicit per-assignment selection is intentionally unfinished until 04. Do not invent a fake binding, persist grants in Registry, add an old/new reader branch or call this final bound execution. 04 replaces that one resolver with admission-pinned exact targets.

Move remaining source acquisition policy out of App to its current Context/Access owner as callers converge. App may construct drivers and inject ports; it must not own grant matching, source fallback or domain output validation. A forwarding-only layer introduced merely to hide the old host union is not a new semantic boundary.

## 03-D: exact assignment identity and atomic Directory publication

Checkpoint 00 identity decision: Directory publication carries the exact `PackageAssignment.id`, installation's `PackageRef` (ID/version) and `AgentDefinition.definition_revision` alongside the endpoint. Task admission resolves that published tuple against the same Registry snapshot and persists it with the Task/invocation key; a package-ID-only or first-assignment lookup is invalid. Replace the current `DirectoryEntry` agent-ID-only key/`resolve(agent_id, definition_revision)` assumption as one cutover, rejecting ambiguous publication rather than selecting one assignment. A Registry-global revision may fence publication, but must not substitute for exact assignment and definition identity on an admitted Task.

Preserve the currently supported assignment multiplicity unless a separately accepted requirement changes it. Each published agent resolves one exact admitted assignment and definition. Reject ambiguity; never `find(first)` by package ID. Configuration revisions must correspond to the actual assignment/definition used by admission, not unrelated global Registry changes.

Replace unregister-all/register-all publication with an atomic validated snapshot or equivalent consistent update under the existing Directory owner. Preserve unrelated registrations, monotonic revisions and exact-definition resolution. Queries must not observe the transient empty catalog or half a bundle. Existing active Tasks keep their admitted identity; new invocations see the new callable set.

Keep package/endpoint availability separate from temporary source failure. Tests must show that enabling a zero-source or unconfigured Expert advertises it, while disabling its assignment prevents new delegation.

## Dependency placement gate

Checkpoint 00 decision: the generic registration descriptor and publication API are Experts-owned semantics. The current built-in crate has only a **dev** dependency on `floe-experts`; the normal manifest and `module-dependencies.json` do not permit `builtin -> experts`. In 03 add that explicit normal dependency and policy edge together, provided the rechecked graph remains acyclic (Experts has no builtin edge). Do not move the descriptor to `agent_contract` merely to avoid this edge. App continues to compose the supplied registrations, rather than introducing a second built-in registration lifecycle.

Reinspect current manifests and `tools/architecture/module-dependencies.json`. At the baseline, Experts allows only agent-contract, execution and inference; builtin does not directly depend on Experts. This plan does not pretend those edges already exist.

Prefer Experts-owned source-reference/selection ports implemented at composition boundaries and pure Context value contracts. If direct use of those contracts or a bundle's use of the generic Experts registration API requires a new edge, add the smallest explicit edge to both manifest and policy in this checkpoint and test it. Never allow Experts/Context/Access/Conversation to depend on builtin, providers, Vault or FFI. Do not relocate business policy into an unrelated shared crate to evade the checker. Preserve the Inference/Connections separation and the real adapter DAG.

## Test and fixture migration

| Anchor | Required treatment |
|---|---|
| `crates/experts/builtin/tests/registry.rs` | Preserve source-independent cards, Person isolation, idempotency and user disablement; rewrite forced package-pair fixtures. |
| `crates/app/src/vault_host/tests/vault_registry/builtin_setup.rs` | Move generic install/reopen/CAS assertions to generic registry tests; delete bundle-special setup assumptions. |
| `crates/app/src/vault_host/tests/schedule_host.rs` | Inspect current role; remove synthetic old topology and rewrite needed proposal fixtures through generic settled Task evidence. |
| `crates/modules/experts/tests/delegation.rs` | Add non-builtin-ID registrations, exact assignment/definition, duplicate identity and consistent catalog publication. |
| Inline App host tests | Replace enum-derived expectations with registrations supplied by the test bundle. No test-only production switch. |

Proposed tests: `unknown_package_uses_common_endpoint`, `zero_source_expert_is_callable`, `unconfigured_required_source_does_not_hide_card`, `reinstall_preserves_user_disable`, `changed_manifest_cannot_rejoin_install`, `directory_refresh_is_atomic`, `ambiguous_assignment_is_rejected`. Names are suggestions; record actual executable names in the report.

## Completion gate

No individual Expert name/enum dispatch in App production; no builtin-only Directory filter; no shared FindFocusWindow or TimelineRead requirement; no source permission state restored to Registry; no obsolete setup adapters/re-exports/tests. Demonstrate a supplied non-builtin-ID package through the real generic registration/delegation seam. Full bound execution is tested at 04/05, not claimed here.

Run focused install/restore/delegation tests, Rust and dependency gates from [06](06-verification.md), and the product gate for any affected wire changes. Update actual owner/runtime descriptions and record the remaining single selection cutover at 04 in the report, not as a permanent architecture exception.
