# 03: generic Expert Registry and endpoint registration

Prerequisite: checkpoint 02 is complete on `12d025a7c4f32eea6d3026396b2449067238e337`. Checkpoint 03 removes the remaining closed-world Registry/setup/runtime wiring. It does **not** implement persistent per-assignment source selection; checkpoint 04 owns binding and exact selected-target admission.

The direct-cutover rule from 02 still applies: do not preserve `BuiltinExpertSetup`, synthetic TimelineRead Tool packages, package-ID-only assignment lookup, old/new Directory publication, or compatibility decoders merely to split commits. Floe is pre-stable. If durable Registry/Task shapes become incompatible, bump their internal schema/fail closed and report that an old development profile must be explicitly recreated; never silently reset storage.

The proposal-card golden exception from checkpoint 01/02 is unrelated to 03. Leave the commented proposal-card golden assertion and its PNG unchanged.

## Purpose and exit state

Checkpoint 03 is complete only when all of the following are true:

1. Experts owns one generic manifest/install/assignment model for bundled Experts; Registry does not encode `Builtin`, `TimelineRead`, or an exactly-one synthetic Tool topology.
2. `BuiltinExpertSetup*`, `builtin_setups`, synthetic Tool installations/assignments, `required_tools.len() == 1` and `granted_tool_assignments` are gone from production.
3. the product-shipped bundle supplies Experts-owned registration descriptors (manifest + bundle-owned runner) and App does not enumerate Schedule/Communication/etc.;
4. App first-party Observe consumer derivation uses the shipped bundle's declared source requirements, not `BuiltinExpertKind::ALL`; arbitrary installed packages still do not receive first-party grants;
5. the common bundle host has a bounded model/read/settle surface and no Calendar/People/Wellbeing/etc. method union;
6. current source selection still happens through one Context-owned resolver; Registry contains no connection, grant, credential or authority and 04 remains the only binding cutover;
7. Directory publication atomically replaces one owner’s registration set while preserving unrelated entries; readers never observe an empty/half-published bundle;
8. every callable Directory entry carries exact assignment + installation/package + definition identity;
9. Task admission persists that exact identity internally and pins the resolved endpoint once; execution does not re-resolve by package ID after admission;
10. active Tasks keep the admitted endpoint/identity across a later Directory refresh, while new Tasks see the new callable set;
11. stateful settlement uses the admitted assignment identity directly; it does not search `builtin_setups`, use `resolve_builtin`, or choose the first assignment by package ID;
12. Expert private-state settlement cannot overwrite unrelated Registry changes through a staged whole-registry snapshot;
13. a non-builtin-ID package supplied through the same registration/install/publication seam can be discovered and delegated without editing common Agent/App registration code;
14. current Registry product DTO/Flutter models no longer expose obsolete synthetic tool/view grant counts;
15. no individual Expert enum/name dispatch remains in App production.

## Current checkpoint-02 anchors

Line numbers are orientation only; search symbols on the execution HEAD.

| Concern | Current surface | Problem 03 removes |
|---|---|---|
| Registry topology | `crates/modules/experts/src/registry.rs` | `PackageImplementation::{TimelineRead,Builtin}`, one required Tool, tool assignment linkage, builtin setup receipts |
| Setup | `registry/expert_setup.rs`, `builtin_setup.rs` | exactly two packages per Expert; setup identity does not bind manifest contents |
| Bundle metadata | `crates/experts/builtin/src/catalog.rs` | declaration is converted by App into synthetic Tool + Expert packages |
| App setup | `crates/app/src/vault_host/expert_setup.rs`, `vault_host.rs::{ensure_builtin_experts,builtin_setup_specs,expert_setup_spec}` | App knows packaging and `BuiltinExpertKind::ALL.len()` |
| App Directory | `OpenVault::sync_expert_directory` | unregister-all/register-all, built-in filter and one shared builtin endpoint |
| First-party policy | `crates/app/src/first_party_observe.rs::builtin_consumers` | App enumerates `BuiltinExpertKind::ALL` |
| Runner registration | `conversation_turn/expert_dispatch.rs::registered_experts` | explicit array of eight package IDs/runners in App |
| Host | `floe-experts-builtin::BuiltinExpertHost` + App `DelegatedMessageExperts` | common host exposes domain methods such as calendar/people/wellbeing/attention |
| Directory | `crates/modules/experts/src/directory.rs` | entry keyed by agent ID only; resolve returns only endpoint; publication is one-entry mutations |
| Task | `crates/modules/experts/src/task.rs::TaskRecord` | durable identity stops at agent ID + definition revision; endpoint is resolved twice |
| Vault Task | `crates/adapters/vault/src/vault/tasks.rs::VaultTaskRecord` | no exact assignment/package admission identity |
| Settlement | `stateful_settlement.rs`, `ExpertSettlement`, Vault Registry settlement | settlement discovers assignment through `builtin_setups` and stages a whole Registry snapshot |
| Product Registry | `RegistryOverview`, Flutter `agent_registry.dart` | obsolete `granted_tool_count` / `granted_view_count` assumptions |

## Frozen design

### Experts-owned manifest

Replace the current `AgentPackage + PackageImplementation + ExpertPackaging/ExpertSetupSpec` topology with one Experts-owned Expert manifest. Exact names may follow repository naming, but the semantic fields are fixed:

```text
ExpertManifest
  schema_version
  package: PackageRef              # kind must be Expert
  publisher
  definition: AgentDefinition      # card + explicit definition_revision
  data_class
  prompt_contract: ContractRef     # package prompt source/revision identity
  result_contracts: [ContractRef]  # package result media/schema identities
  source_requirements: [ExpertSourceRequirement]
  capability_requirements: [CapabilityRequirement]
  state_schema_version
```

A small bounded `ContractRef { id, revision }` (or equivalent) is Experts-owned. It is descriptive identity, not executable code.

A source requirement is package configuration intent, never authority:

```text
ExpertSourceRequirement
  key                 # package-local stable key
  capability          # existing Context capability/View contract ID
  minimum_sources
  maximum_sources
```

Rules:

- keys are unique within one manifest;
- capability IDs are bounded canonical identifiers;
- `0 <= minimum <= maximum <= bounded limit`;
- minimum zero means optional; positive means required;
- zero-source Experts use an empty list;
- multiple requirements and multi-source requirements are representable;
- no credential, connection ID, grant ID, source authority, provider route or selected target is stored here;
- `definition.card.id/version` must equal `package.id/version`;
- a manifest change under the same `PackageRef` is a conflict rather than silent mutation; normal package/definition versioning is the replacement path.

`capability_requirements` names trusted runtime capabilities, if any, by stable ID. It does not create Tool package installations or grants. Empty is valid.

Keep `PackageRef` / `PackageKind` in the shared agent contract where other owners already use them, but Registry manifests in this checkpoint are Expert manifests and must require `PackageKind::Expert`.

### Generic install operation and receipt

Replace `BuiltinExpertSetup*` / `builtin_setups` with a generic bundle/install operation receipt. The concrete type names may vary, but the stored receipt must bind:

```text
operation_id
person_id
expected_registry_revision at first execution
canonical manifest-set digest
installed entries:
  package
  installation_id
  assignment_id
```

The canonical digest covers the exact sorted/canonical manifests being installed. It must include package identity, definition revision, metadata, data class, prompt/result contract identity, source/capability requirements and state schema identity.

Idempotency rules:

- same operation ID + same Person + same original expected revision + same manifest-set digest rejoins without mutation;
- same operation ID with changed manifests/specs conflicts;
- a second default-install operation for a Person that already owns the shipped bundle does not create duplicate assignments;
- startup/default ensure never re-enables a disabled installation/assignment or overwrites user configuration;
- Person isolation is exact;
- source availability is irrelevant to installation.

The Registry snapshot stores generic install receipts only if a durable receipt is still needed. There is no `builtin_setups` collection.

### Assignment shape

Remove synthetic Tool linkage from `PackageAssignment`:

```text
id
person_id
installation_id
enabled
private_state
```

Remove `granted_tool_assignments`, `validate_tool_linkage`, and any “first/only tool assignment” logic.

Remove `granted_tool_count` from `AssignmentOverview`. The Flutter model’s `granted_view_count` is also obsolete topology/configuration leakage and must be removed in the same product cutover. Checkpoint 04 will expose real binding/configuration state at its owner rather than manufacturing a count here.

### Definition revision

The current App constant `DEFINITION_REVISION = 1` is not sufficient. Definition revision is supplied by the package registration/manifest and is part of invocation identity.

A change that alters callable prompt/result/source/capability contract must not silently reuse the old definition identity. The bundle owns the revision it publishes; Directory/Task merely preserve it.

## 03-A: generic manifest, install and durable Registry

### A1. Replace closed-world Registry topology

In `floe-experts`:

- introduce the manifest/requirement/contract identity above;
- replace `AgentPackage` production use with the manifest;
- delete `PackageImplementation`;
- delete synthetic TimelineRead package creation;
- delete `ExpertPackaging` and two-package `ExpertSetupSpec`;
- delete `BuiltinExpertAssignmentReceipt`, `BuiltinExpertSetupReceipt`, `BuiltinExpertSetup`, `BuiltinExpertSetupResult`;
- delete `builtin_setups`;
- delete `required_tools` / exactly-one-tool validation when their only role is this synthetic topology;
- delete `granted_tool_assignments` and tool-link validation;
- preserve package installation, assignment enablement, Person isolation and private Expert state.

Do not recreate TimelineRead under a different name.

### A2. Generic resolver

Replace `resolve_builtin` with an exact generic assignment resolver, conceptually:

```text
resolve_assignment(
  registry_instance,
  person,
  assignment_id,
  expected package,
  expected definition revision
) -> ResolvedExpert
```

It verifies:

- exact Person + assignment;
- installation references the exact package;
- exact manifest exists;
- package/definition identity matches;
- assignment/installation are enabled for **new admission**;
- private-state shape is valid.

It does not inspect source availability or permissions.

A separate historical/settled identity validation may ignore current enabled state where existing Actions/history semantics require inspection of a previously admitted Task. Preserve the current distinction between “historically valid settlement” and “currently active assignment”.

### A3. Install/ensure service

Replace `BuiltinExpertStore`, `BuiltinExpertRefresh`, `ensure_builtin_experts`, and App `VaultBuiltinExperts` with generic bundle equivalents.

Preserve current product lifecycle:

- Session/create/default initialization may install the shipped bundle if absent;
- ordinary conversation turn and linked resume use ExistingOnly semantics and do not silently install;
- ensure of an existing bundle validates exact operation/manifest identity and leaves user enablement unchanged;
- conflict/retry classification remains typed.

### A4. Persistence cutover

The Registry JSON shape changes incompatibly. Introduce an Experts-owned Registry schema version distinct from the general Agent protocol version, or another explicit fail-closed marker. Do not decode the old builtin setup shape.

Task admission identity added in 03-D also changes `VaultTaskRecord`; bump the Vault task store schema (currently version 2) rather than accepting old rows with missing identity. Do not automatically delete/reset a user profile. Tests use fresh stores; product behavior on an old development profile must be a clear unavailable/fresh-profile requirement and reported as such.

### A5. Product Registry DTO

Update Registry overview/wire/Flutter tests in the same direct cutover:

- remove obsolete granted Tool/View counts;
- retain installation/assignment ID, enabled state, state revision and completed invocation data that still have current meaning;
- configuration commands remain enable/disable by exact installation/assignment ID;
- do not expose source bindings yet; 04 owns them.

Focused gate after A:

```sh
cargo test -p floe-experts
cargo test -p floe-vault registry
cargo test -p floe-protocol
cargo test -p floe-ffi
(
  cd apps/client
  flutter test test/features/experts/agent_registry_test.dart
)
python3 tools/architecture/check_boundaries.py
git diff --check
```

If a filter executes zero tests, run the containing package/test target and report the real command.

## 03-B: bundle-provided manifest + runner registrations

### B1. Add the explicit dependency edge

The registration descriptor is Experts-owned. Add a normal:

```text
floe-experts-builtin -> floe-experts
```

dependency and the matching `module-dependencies.json` policy edge in the same commit. Re-run the graph checker and verify the graph remains acyclic. Do not move registration semantics into `agent_contract` to avoid the edge.

### B2. Generic registration descriptor

Add an Experts-owned generic descriptor, conceptually:

```text
ExpertRegistration<Runner>
  manifest: ExpertManifest
  runner: Runner
```

The runner type remains bundle-owned. For the current in-process bundle, make the bundle host/runner callable through one object-safe or otherwise type-erased bundle seam so App can hold a registration without knowing the specific Expert enum variant.

The shipped bundle exposes one collection such as `registrations()`. The explicit list of eight remains, if needed, **inside the bundle crate only**. App must not build that array.

`BuiltinExpertKind` may remain private to `floe-experts-builtin` as an implementation convenience, but remove its public re-export/use from App production. Package-local dispatch/tests may use package-owned constants instead.

### B3. First-party Observe consumers from manifests

Refactor App first-party Observe policy derivation so bundled Expert consumers come from the shipped first-party registration manifests’ declared source capabilities.

Important trust rule:

- only the product-shipped first-party bundle contributes to automatic first-party Observe policy;
- a package merely installed in Registry/Directory does **not** become first-party and is not auto-granted;
- Manager `assistant` behavior from checkpoint 01 remains derived from the Manager tool catalogue;
- Calendar and remote policy fingerprints continue to cover the final exact consumer set.

Adding a ninth shipped Expert that declares an existing remote/Calendar capability should update the relevant first-party consumer set without an App enum/name edit.

### B4. App consumes registrations generically

App composition receives/constructs the shipped registration collection once and uses it for:

- default manifest installation/ensure;
- endpoint construction/publication;
- first-party consumer derivation.

Delete App `builtin_setup_specs`, `expert_setup_spec`, `expert_packaging`, `BuiltinExpertKind::ALL.len()`, package ID filters and individual registration arrays.

## 03-C: generic bundle host over one Context source resolver

This checkpoint must not merely rename `BuiltinExpertHost`. The current domain-method union means common host changes when a new Expert combines existing capabilities.

### C1. Stable bundle host surface

Converge the bundle host to a small generic surface:

- bounded model execution / policy;
- read one **declared requirement** by package-local requirement key with a bounded query;
- trusted dependency/evidence capture owned by the host;
- current conversation/context projection where it is a generic input;
- generic stateful settlement.

Package code performs typed JSON decode/validation for Calendar/People/Wellbeing/etc. The common host does not expose `calendar_views`, `people_view`, `wellbeing_view`, `attention_view`, `work_context_views`, and similar per-domain methods.

The host must verify that a requested requirement key exists in the **admitted manifest** and use the admitted package identity as the source consumer. A free-form caller/model string may not choose another consumer/capability.

### C2. Context owns current source resolution

Add or consolidate one Context-owned declared-source service (proposed location `crates/modules/context/src/application/expert_sources.rs`, exact name optional) that maps the manifest capability to the existing current source acquisition implementations.

It may internally call the existing:

- remote `SourceReader`;
- native/remote Calendar acquisition;
- personal Attention/People/Wellbeing acquisition;
- current tasks/memory/confirmed-interaction projections.

It returns the existing typed `SourceReadOutcome` semantics with bounded payload and records every contributor dependency. App constructs/injects drivers; it does not own candidate selection or grant matching.

This is the **single current selection implementation** allowed to remain until 04. It may choose current product state exactly as today, but it does not persist a binding and never falls back through a second legacy reader. 04 replaces its selection input with the admitted assignment binding while keeping the package-facing requirement API stable.

### C3. Package-local decoding

Move the remaining typed acquisition helpers into bundle/package code:

- Schedule decodes/validates Calendar views;
- Relationships decodes People/confirmed-interaction views;
- Focus decodes Attention/Calendar/Work;
- Wellbeing decodes Wellbeing/Calendar;
- other Experts decode their existing capability payloads.

Optional-source behavior and blocker capture from the current tests must remain unchanged.

### C4. Stateful settlement uses admitted identity

The bound endpoint passes exact admitted assignment/package/manifest identity to the trusted request/settlement path. `StatefulExpertSettlement` must stop reading `builtin_setups`, stop `from_package_id`, and stop finding an assignment by package ID.

## 03-D: exact Directory/Task admission and atomic publication

### D1. Experts-owned admission identity

Introduce an Experts-owned internal identity, conceptually:

```text
ExpertAdmissionIdentity
  registry_instance_id
  assignment_id
  installation_id
  package: PackageRef
  definition_revision
```

It is not a source grant and does not go on the public Flutter Task wire.

`DirectoryEntry` carries this identity plus the `AgentDefinition`.

### D2. Atomic publication

Replace App’s unregister-all/register-all sequence with an Experts-owned atomic publication API.

Use an explicit publication owner/set ID so one bundle update replaces only entries that owner previously published and preserves unrelated registrations. The operation:

1. validates the full candidate set outside/inside the Directory owner as appropriate;
2. rejects duplicate agent IDs, duplicate assignment identities, invalid definition/package mismatches and cross-owner collisions;
3. acquires the Directory write lock once;
4. swaps the owner’s old set for the complete new set;
5. increments Directory revision once if changed;
6. leaves the old set visible until the new set is fully valid.

Exact re-publication may be idempotent without revision churn.

Do not create a transient empty catalog.

### D3. Publication is joined from Registry + bundle

For one Person:

- enumerate enabled Expert assignments from the current Registry;
- match each assignment’s exact `PackageRef` to exactly one supplied registration manifest;
- preserve current supported multiplicity: at most one callable assignment for a package/public agent ID; ambiguity is a conflict, never “first assignment wins”;
- construct an endpoint bound to that registration runner and exact `ExpertAdmissionIdentity`;
- publish all callable entries atomically.

Source availability/configuration is not part of callable publication. A zero-source Expert and an Expert with an unconfigured required source are both callable when assignment/install are enabled.

### D4. TaskRecord persists exact identity

Extend `TaskRecord` / `VaultTaskRecord` with the exact admitted Expert identity. Keep `TaskSnapshot` product-visible shape unchanged.

Rules:

- Submitted/Working/Completed and post-admission failure/cancellation records carry exact admitted identity;
- if the implementation preserves a durable pre-admission Rejected Task, only that pre-admission rejected state may omit the identity;
- identity is immutable across every Task transition;
- exact-admission/replay checks compare it;
- Task store schema is bumped as described in A4.

### D5. Resolve once, pin endpoint

`TaskCoordinator::execute` currently resolves the Directory before admission and again before endpoint execution. Remove the second package-ID resolution.

For a new callable Task:

1. resolve one Directory entry;
2. receive exact admission identity + endpoint Arc;
3. persist that identity with Task admission;
4. transition to Working;
5. execute the already resolved endpoint.

A Directory refresh after step 1 cannot reroute this active Task. A new Task resolves against the new Directory publication.

Completed/replayed Tasks return their stored result without requiring the assignment to remain currently enabled.

### D6. Definition identity

Remove App’s fixed `DEFINITION_REVISION = 1`. The bundle manifest supplies the definition revision; Registry/Directory/Task all preserve the exact same value.

A Task replay with the same Task ID but changed selected agent/definition/request/admission identity conflicts.

## 03-E: assignment-local settlement, no staged Registry replacement

Checkpoint 02 still carries a full staged `RegistrySnapshot` in `ExpertSettlement`. Remove that whole-snapshot mutation authority.

### E1. Settlement metadata

Change settlement to carry only the assignment-local state change and immutable identity needed for validation, conceptually:

```text
registry_instance_id
assignment_id
package
definition_revision
invocation_id
expected_private_state_revision
next_private_state_revision / validated state delta
dependencies
task_result
```

The endpoint/extension never supplies an unrestricted replacement Registry snapshot.

### E2. Vault apply under current Registry

Inside the Vault transaction:

1. load the **current** Registry;
2. verify exact instance/assignment/installation/package identity against the Task’s persisted admission identity;
3. verify the assignment private-state revision is the expected one;
4. apply only the allowed invocation-completion state delta;
5. preserve unrelated packages/installations/assignments/configuration changes already committed;
6. atomically settle Task + assignment state as today.

An unrelated Registry revision change must not cause the active Task to overwrite or discard it. Concurrent mutation of the same assignment state conflicts.

The assignment being disabled after Task admission does not rewrite the Task’s identity. Historical settlement/inspection validates the admitted identity; **new** admission still requires current enabled assignment/installation.

### E3. Actions/history callers

Update `validate_settled_invocation`, proposal inspection/publication and tests to validate against exact settled assignment/package identity without TimelineRead data-class discovery. `data_class` comes from the manifest/admitted identity or the Actions proposal evidence produced from that manifest, not a synthetic Tool package.

Preserve current active-assignment check for publishing a consequential action where product policy requires it.

## 03-F: product/Vault/App migration and tests

### F1. Vault API names

Delete builtin-only Vault methods and wrappers:

- `builtin_expert_overview`;
- `install_builtin_experts*`;
- `enabled_builtin_expert_cards`;
- builtin-specific setup save exceptions.

Replace them with generic Registry bundle/install/read APIs. Physical storage remains Vault implementation of Experts-owned ports.

### F2. App OpenVault

Replace:

- `builtin_expert_endpoint`;
- `sync_expert_directory`;
- `ensure_builtin_experts`;
- `VaultBuiltinExperts`;
- App `BuiltinExpertKind` import;

with generic bundle registration/install/publication composition.

App may still compose the concrete shipped `floe-experts-builtin` bundle, but it cannot know the eight individual package names.

### F3. Extensibility acceptance test

Add a statically controlled test bundle registration with a non-builtin ID, for example `example.test.expert`, using an existing generic capability or zero source requirements.

Drive it through the same production-shaped seam:

```text
registration manifest
-> generic Registry install/assignment enable
-> atomic Directory publication
-> TaskCoordinator catalog
-> Delegation
-> bound endpoint/runner
-> ExpertReport/Task completion
```

No common App/Agent enum or match is modified for the test package.

Also demonstrate:

- zero-source Expert is callable;
- required-but-currently-unconfigured source does not hide the card;
- disabling the assignment removes it from **new** catalog/delegation;
- an already admitted Task completes on its pinned endpoint across the refresh;
- two callable assignments for the same public agent/package are rejected as ambiguous.

### F4. Test migration

Rewrite/delete historical topology assertions:

| Existing test area | Required disposition |
|---|---|
| `crates/experts/builtin/tests/registry.rs` | Rewrite against generic manifests/install receipts; retain source-independent cards, Person isolation, idempotency, disable preservation. |
| `app/tests/vault_registry/builtin_setup.rs` | Move install/reopen/CAS assertions to generic Registry APIs; delete synthetic pair counts. |
| `modules/experts/tests/delegation.rs` | Add exact admission identity, atomic publication, pinned active Task and non-builtin bundle tests. |
| `stateful_settlement.rs` tests | Use exact admitted assignment/package; no `builtin_setups`. |
| Actions/proposals tests | Select assignment by exact package/receipt, never `granted_tool_assignments.is_empty()`. |
| App first-party Observe tests | Expected consumers derive from shipped registration manifests; preserve arbitrary-extension denial. |
| Flutter Registry tests/support | Delete granted Tool/View count schema assumptions. |
| App enum-registration tests | Replace expected `BuiltinExpertKind::ALL` list with the supplied registration collection. |

## Dependency gate

After adding the normal bundle -> Experts edge:

```sh
python3 tools/architecture/check_boundaries.py
cargo metadata --no-deps --format-version 1 >/tmp/floe-cargo-metadata.json
```

Required DAG invariants:

- Experts does not depend on builtin, Context, Access, provider adapters, Vault, App or FFI;
- builtin may depend on Experts for manifest/registration descriptors;
- App composes builtin + Experts + Context adapters;
- Inference and Connections remain independent;
- source authority remains Access/Context-owned.

Do not relocate manifest/registration policy into `agent_contract` merely to hide a dependency.

## Focused verification by substep

Run the smallest real owner gates after each substep. At minimum before 03-G:

```sh
cargo test -p floe-experts
cargo test -p floe-experts-builtin
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-app
cargo test -p floe-actions
cargo test -p floe-protocol
cargo test -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

(
  cd apps/client
  flutter test test/features/experts/agent_registry_test.dart
)
```

A zero-test filter is not evidence. Run the containing target/package and report the actual command.

## 03-G: residual audit, broad verification and documentation convergence

### Residual audit

Search at least:

```text
PackageImplementation
TimelineRead
BuiltinExpertSetup
BuiltinExpertAssignmentReceipt
builtin_setups
BuiltinExpertStore
BuiltinExpertRefresh
ensure_builtin_experts
install_builtin_experts
builtin_expert_overview
enabled_builtin_expert_cards
ExpertPackaging
ExpertSetupSpec
required_tools
granted_tool_assignments
granted_tool_count
granted_view_count
resolve_builtin
validate_tool_linkage

BuiltinExpertKind::ALL
BuiltinExpertKind::
from_package_id(
registered_experts(
builtin_setup_specs
expert_setup_spec
expert_packaging
DEFINITION_REVISION

calendar_views(
people_view(
wellbeing_view(
attention_view(
work_context_views(

directory.unregister
directory.register
staged_registry
first assignment
assignments.first
find(|assignment| ... package id
```

Allowed residuals:

- bundle-private `BuiltinExpertKind` or package-local constants/tests, if no App/common runtime depends on them;
- historical/checkpoint documentation;
- domain-specific Context/package implementation functions that are no longer common host methods;
- 04 plan references to future bindings.

There must be no production App enumeration of individual Experts, no package-ID-only assignment selection, no synthetic Tool topology and no staged whole-Registry settlement.

### Broad gate

Checkpoint 03 changes durable Rust shapes, Registry/App wire and Flutter Registry parsing. Run:

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi

(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)

git diff --check
```

Do not update the proposal-card golden. Other active golden tests follow the normal verification skill: inspect a legitimate intended UI change before updating any golden.

Go server behavior is not intended to change. If server code/protocol behavior changes unexpectedly, run `go test -race ./...` and `go vet ./...` from `server/` and explain the expansion. Do not add server routes for Registry registration.

iOS is not a required 03 acceptance gate; report it as not executed if not run.

### Documentation convergence

Update current architecture only after implementation establishes the truth:

- Registry manifest/install/assignment ownership;
- first-party bundle registration seam;
- exact Directory/Task admission identity and pinned endpoint;
- generic requirement host + current Context resolver;
- assignment-local atomic settlement;
- explicit statement that persistent selected-source binding is still incomplete until 04.

Do not document 04 binding or settings UI as implemented.

Only after all gates pass, mark **03 Complete** in the plan README with actual implementation/verification commit SHAs. Leave 04 Not started.

## Recommended commit boundaries

Prefer these commits when each intermediate tree is compile/test coherent:

1. **03-A** — generic manifest/install Registry + product Registry DTO cutover;
2. **03-B** — bundle registration descriptors + dependency edge + first-party manifest consumers;
3. **03-C** — generic host/Context declared-source resolver;
4. **03-D** — exact Directory publication + Task/Vault admitted identity;
5. **03-E** — assignment-local settlement + Actions/history migration;
6. **03-F** — App/Vault cleanup + non-builtin extensibility tests;
7. **03-G** — broad verification + architecture/status convergence.

If a boundary would require old/new setup, old/new Task identity, or staged-registry compatibility in production, combine adjacent steps. Never introduce a compatibility wrapper solely to preserve this commit split.

## Required completion report

Report:

1. starting local/origin HEAD and worktree state;
2. 03-A through 03-G actual commit SHAs;
3. final Expert manifest/source requirement/install receipt shapes;
4. exact bundle -> Experts dependency/policy edge;
5. App production references to individual Expert enum/names after the cutover;
6. first-party Observe consumer derivation and proof arbitrary installed packages are not auto-granted;
7. final common host methods and Context declared-source resolver path;
8. exact Directory publication algorithm and atomicity tests;
9. persisted Task admission identity and proof endpoint is resolved/pinned once;
10. behavior of active Task versus new Task across assignment disable/Directory refresh;
11. removal of `builtin_setups`, synthetic TimelineRead Tool packages and tool assignment linkage;
12. assignment-local settlement shape and proof unrelated Registry changes are preserved;
13. non-builtin-ID package execution through the production-shaped registration seam;
14. Registry/Task schema-version cutover and any old-development-profile limitation;
15. Flutter/product Registry DTO changes;
16. targeted and broad verification commands/results;
17. residual-search results with narrowly justified bundle-private matches;
18. confirmation proposal-card golden code/PNG remained unchanged;
19. current architecture/status docs changed;
20. final local HEAD and next checkpoint 04, without starting it.
