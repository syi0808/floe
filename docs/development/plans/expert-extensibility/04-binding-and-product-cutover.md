# 04: persistent source binding, exact-target authority, interaction and product cutover

Prerequisite: checkpoint 03, including the 03-B/F runner-binding closure, is complete on `88343b5f1774ef4dbc846b0d834114a8e18bf094`. Checkpoint 04 turns the current invocation-time source selection into explicit Experts-owned configuration and pins that configuration to Task admission. It also completes the settings/recovery product path. Checkpoint 05 remains deletion/conformance work; do not move its broad cleanup into this checkpoint unless an obsolete caller must be removed to make 04's canonical path unique.

The proposal-card golden exception remains in force: leave the commented assertion in `apps/client/test/features/actions/agent_proposal_card_test.dart` and `apps/client/test/goldens/agent_proposal_card.png` unchanged. Do not regenerate that golden or count it as passed.

Floe is pre-stable. This checkpoint deliberately changes durable Registry/Task meaning. Use one direct cutover, coordinated same-snapshot Rust/FFI/Flutter changes and explicitly selected fresh development profiles. Do not add old/new binding decoders, nullable migration state, compatibility fallbacks or an automatic database reset.

## Purpose and exit state

Checkpoint 04 is complete only when all of the following are true:

1. every Expert source requirement has a supported contract version and an Experts-owned persisted selection set;
2. a persisted selection contains bounded Context-owned source references only — never credentials, bearer tokens, grants, grant/source authority, provider health, payloads or processing consent;
3. configuration can be empty and incomplete; a required missing selection never hides the Expert card and is never represented as an Access grant problem;
4. source candidates are discovered only by an explicit settings/setup query, never during Task/source execution;
5. product clients select opaque candidate IDs; App resolves them to current canonical Context source references and clients cannot submit connector/owner/resource tuples;
6. default binding is allowed only during the explicit first-party bundle setup/configuration operation and never on ordinary turn admission, resume, startup inspection or connector arrival;
7. connecting Source B later never modifies an assignment already bound to A;
8. Directory publication carries an immutable execution selection beside the exact 03 admission identity;
9. Task admission durably stores that selection and one canonical selection digest;
10. Vault Task admission validates the proposed selection against the current Registry before persisting it, closing the Directory-resolve -> durable-admit race;
11. an active Task never adopts a newer binding; binding/assignment changes fence later source reads, model dispatch/release and terminal settlement instead of rerouting;
12. a completed Task remains historical/replayable without rerunning or requiring the current binding to equal its old selection;
13. Expert runtime reads only the targets named by its admitted selection; unselected grants/connections/resources are invisible to the Expert path;
14. the existing Manager direct-read path remains product-owned and does not inherit Expert bindings;
15. Calendar reads use exactly the selected connection/resources; adding another calendar to the product connection does not widen an old binding;
16. remote Mail/Work/Logistics reads classify and read only selected target tuples, while retaining signed producer/grant/current-policy revalidation;
17. local Attention/Contacts/Wellbeing reads verify the exact selected native source identity and device owner before payload I/O;
18. local Tasks/Confirmed Memory use Context-owned canonical intrinsic source identities, not invented Access grants;
19. `relationships.confirmed_interactions` no longer turns `CapabilityUnavailable` into `Ready([])` and no production Expert path calls its legacy unauthorised server convenience endpoint;
20. missing required binding produces an Experts-owned durable binding interaction; selected-source access/reconnect/review problems continue to use Access/Connections-owned source interactions;
21. an old source-access review from Task selection A cannot mutate authority after that assignment is rebound to B;
22. linked resume is still a fresh Run/Task and therefore receives current binding; same-Task recovery keeps the admitted selection;
23. first-party Observe consumer policy is derived from the trusted shipped bundle **and the explicit bindings that select that exact source**, plus Manager direct-read consumers where applicable; installation alone grants nothing;
24. binding change never itself creates/changes a grant and grant change never rewrites an Expert binding;
25. binding mutation is per-assignment CAS with durable exact-command rejoin; it does not use the Registry-global revision as the semantic binding revision;
26. Registry settlement still updates only Expert private state and preserves binding state;
27. Actions treats a changed binding as stale before new action publication/dispatch, while an already durable uncertain external write still follows reconciliation rather than blind cancellation/retry;
28. App/protocol/FFI expose generic Expert list/detail, source candidates and replace/remove selection intents without exposing internal authority;
29. Flutter renders manifest-provided Expert metadata and generic requirement/source settings, including an unknown test Expert ID without package-specific UI code;
30. current architecture documents state the implemented binding path and 04 alone is marked Complete. Checkpoint 05 remains Not started.

## Current checkpoint-03 anchors

Line numbers are orientation only; search symbols on the execution HEAD.

| Concern | Current surface | 04 cutover |
|---|---|---|
| Manifest requirement | `crates/modules/experts/src/manifest.rs::ExpertSourceRequirement` | Add exact source contract version and honest multiplicity. |
| Assignment | `crates/modules/experts/src/registry.rs::PackageAssignment` | Add per-assignment binding state/revision and binding operation receipt. |
| Directory/Task identity | `ExpertAdmissionIdentity`, `DirectoryEntry`, `TaskRecord` | Add immutable admitted execution selection beside admission identity. |
| App endpoint | `RegisteredExpertEndpoint` | Store admitted selection and fence it; runner remains pinned as in 03. |
| Generic host | `BuiltinExpertHost::read_requirement` | Resolve requirement against admitted selection; no current-source discovery. |
| Context selector | `application/expert_sources.rs::read_declared_source` | Split candidate discovery from exact selected-target read. |
| Calendar | `CurrentCalendarContextReader` | Stop selecting `core.calendar_connection` at runtime; verify/read exact admitted refs. |
| Remote Views | `remote_sources.rs::read_remote_view/classify_remote_sources` | Add exact-target Expert read; existing current-selection path may remain only for legitimate Manager/product callers. |
| Personal native | Attention/People/Wellbeing readers | Admit exact selected connector/connection/execution-owner/resource. |
| Confirmed interactions | App `LocalExpertSource::ConfirmedInteractions` -> `ServerSourceClient::read_confirmed_interaction_view` | Remove production caller; no fake empty success. |
| First-party Observe | `first_party_observe.rs` | Consumer set becomes trusted-shipped + exact selected bindings, not every declaration. |
| Interactions | Conversation `ReviewedTarget` + App publication/resolution | Add binding configuration interaction and pin source-access reviews to Task selection where applicable. |
| Product API | `expert_services.rs`, protocol/FFI, Flutter Registry settings | Add generic settings/detail/candidate/replace-selection operations. |

## Frozen contract decisions

### Context-owned selected source reference

Add one pure value contract at `floe-context-contract`, conceptually:

```text
SourceSelectionReference
  connector_id: ConnectorId
  connection_id: ConnectionId
  execution_owner_id: ExecutionOwnerId
  capability_id: String
  resource: ResourceHandle
  contract_version: u32
```

Validation rules:

- all existing bounded ID validators are reused;
- capability is a bounded canonical identifier;
- contract version is positive;
- wildcard resources and control characters remain invalid;
- the reference does not contain Person because the owning assignment already scopes Person;
- it does not contain device request identity; `execution_owner_id` identifies the source producer and therefore naturally pins native sources to their device owner;
- no source/grant authority revision is persisted in it.

Add the explicit normal dependency and architecture-policy edge:

```text
floe-experts -> floe-context-contract
```

This is a legitimate contract dependency: Experts owns the saved choice, Context owns what a source target identifier means. Do not re-export these values through `agent_contract` to hide the edge.

Canonical ordering for references is:

```text
connector_id
connection_id
execution_owner_id
capability_id
resource
contract_version
```

Every persisted and admitted target set is sorted by that full tuple and rejects duplicates.

### Manifest source contract version and multiplicity

Extend `ExpertSourceRequirement` with:

```text
contract_version: u32
```

The admitted selected reference must have the same capability and contract version.

The current blanket `maximum_sources = 1` is not honest for every built-in requirement. Preserve existing multiplicity that the runtime already supports:

- Calendar selections may contain multiple exact calendar resources, bounded by `MAX_REQUIREMENT_SOURCES`;
- remote Mail/Work/Logistics may contain multiple exact connected sources, bounded by the same limit, preserving current multi-source merge capability;
- Attention, Contacts, Wellbeing, local Tasks and Confirmed Memory are single-source;
- Confirmed Interactions remains optional/single-source but has no current authorised product candidate until a real source implementation exists.

Mandatory source minimum remains 1; optional source minimum remains 0. Empty persisted configuration is legal even for a mandatory requirement — it means unconfigured and blocks use, not invalid storage.

This is a durable manifest meaning change. Target schema numbers on the current baseline are:

```text
EXPERT_MANIFEST_SCHEMA_VERSION: 1 -> 2
EXPERT_REGISTRY_SCHEMA_VERSION: 2 -> 3
Vault Task schema: 3 -> 4
```

Use the next exact version if source moved before execution, but do not keep an old decoder. Document the fresh-development-profile requirement.

### Per-assignment binding state

Add one binding state to each `PackageAssignment`:

```text
ExpertBindingState
  schema_version
  revision                  # starts at 1
  entries: [RequirementBinding]
  last_operation: optional BindingOperationReceipt

RequirementBinding
  requirement_key
  capability
  contract_version
  selected: [SourceSelectionReference]

BindingOperationReceipt
  operation_id
  command_digest
  resulting_revision
```

The entries are canonical and cover every manifest source requirement exactly once. Install creates an empty entry for each requirement.

Binding revision changes only when configuration changes. Expert invocation completion/private-state settlement does not change it.

A binding mutation receives the App operation ID as trusted host metadata; the client does not send a second nested operation ID. The command digest binds:

- Registry instance;
- Person;
- assignment/install/package/definition;
- requirement key/capability/version;
- expected binding revision;
- exact canonical selected references.

Mutation semantics:

1. current revision == expected: validate/apply exact target set, increment once and record operation receipt;
2. same operation ID + same digest + current revision == recorded resulting revision: exact rejoin, no mutation;
3. same operation ID with another digest, stale expected revision, or replay after an intervening mutation: Conflict;
4. empty target list removes the selection and is valid even for a required requirement;
5. nonempty target lists must not exceed manifest maximum and every target must match the requirement capability/version;
6. minimum cardinality is an execution completeness rule, not a storage rule.

Keep operation receipts bounded to the minimum durable window required by the existing owner-operation/retry contract; do not grow an unbounded log.

### Product candidate identity

Product wire never carries `SourceSelectionReference`.

Context/App projects candidates as:

```text
ExpertSourceCandidate
  candidate_id              # 64 lowercase hex
  requirement_key
  safe title
  safe detail
  availability/status
  selected
```

`candidate_id` is a canonical SHA-256 digest over a domain tag plus the complete `SourceSelectionReference`. It is configuration identity only, not authority.

Selection command carries candidate IDs. At execution App recomputes the current candidate set for the exact assignment/manifest/requirement and resolves every ID. Unknown, duplicate or drifted candidate IDs fail closed before Experts mutation.

A previously selected source that is no longer live remains visible as a saved/unavailable item derived from the persisted reference, so the UI never silently substitutes a new source.

### Context candidate catalog

Candidate enumeration is a read-only Context/Connections operation. It performs no source payload read and no grant mutation.

Context owns the candidate constructors. App only injects current owner snapshots/drivers. Candidate identity must come from:

- remote connection snapshots plus the current pinned remote producer identity;
- current Calendar connection and its exact selected calendar resources;
- Access-defined native personal source identities for Attention/Contacts/Wellbeing;
- canonical Context-owned intrinsic local identities for `floe.tasks` and `memory.confirmed`.

For the two intrinsic local capabilities, define Context constants for one local projection service rather than inventing Access grants. The stable namespace on this cutover is:

```text
connector_id   = floe.local.context
connection_id  = floe.local.context
execution_owner_id = device:<verified device id>
resource       = exact capability id (floe.tasks or memory.confirmed)
contract_version = 1
```

These values mean “the local Context projection service on this verified device”; they are not a remote connection and carry no read permission.

`relationships.confirmed_interactions` currently has no supported authorised server View route. Its candidate set is therefore empty in 04. Remove the production call to `ServerSourceClient::read_confirmed_interaction_view`. Do not add a new Go/server feature just to manufacture a candidate. The convenience adapter may remain caller-zero for checkpoint 05 deletion.

### Admitted execution selection

Add an Experts-owned immutable selection:

```text
ExpertExecutionSelection
  schema_version
  binding_revision
  requirements: [AdmittedRequirementSelection]
  digest: [u8; 32]

AdmittedRequirementSelection
  key
  capability
  contract_version
  minimum_sources
  maximum_sources
  selected: [SourceSelectionReference]
```

The digest is one canonical domain-separated SHA-256 over the complete sorted requirement/selection set and binding revision. Do not add separate per-layer selection digests.

Directory publication joins the exact admission to the current assignment binding and carries the selection into the endpoint.

`TaskRecord` / `VaultTaskRecord` persist both:

```text
admission: ExpertAdmissionIdentity
selection: ExpertExecutionSelection
```

They remain immutable across Task transitions and exact replay.

Completed Task replay returns the stored historical result without requiring the current binding to match. Any nonterminal execution/recovery uses the stored selection and current fences; it never adopts current settings.

### Requirement read outcome

Do not encode “no binding” as a fabricated `SourceAccessRequirement`.

Replace the package-facing dependency on detailed `SourceReadOutcome<...>` with an Experts-owned/bundle-generic outcome that only tells package judgment what it may rely on:

```text
RequirementReadOutcome<T>
  Ready(T)
  Unavailable(SourceUnavailable)
  NeedsUserAction
```

The trusted host retains the internal reason:

- missing/incomplete selection -> Experts binding requirement;
- selected target access/reconnect/system permission -> real Context `SourceAccessBlockers`;
- temporary source failure -> Unavailable.

Package code never receives a fake grant target. Optional requirements may continue reasoning on `Unavailable` or `NeedsUserAction` according to package semantics while preserving the limitation. `Ready(empty)` is legal only after a successful authorised read whose domain result is genuinely empty.

## 04-A: contracts, persistence and binding mutation

### A1. Add source-reference contract and dependency edge

Implement `SourceSelectionReference` in `context-contract` and add `experts -> context_contract` to Cargo and `module-dependencies.json`. Verify the DAG.

Add round-trip, bounds, wildcard, duplicate and canonical ordering tests.

### A2. Upgrade manifest and Registry binding state

Add requirement contract version and honest built-in multiplicities.

Change Registry schema to the new direct-cutover shape and initialise every installed assignment with an empty binding state at revision 1.

Restore validation must prove:

- binding entries exactly match the installed manifest requirements;
- keys/capability/version/cardinality are canonical;
- selected refs validate and are sorted/unique;
- binding revision is nonzero;
- operation receipt, if present, is bounded and matches the current/recent revision semantics;
- one Person cannot use another Person's assignment;
- no grant/source authority/token fields exist.

### A3. Experts binding mutation API

Add owner methods for:

- read exact assignment binding;
- replace/remove one requirement selection with expected binding revision and trusted operation ID;
- validate one `ExpertExecutionSelection` against current assignment/manifest;
- project the immutable execution selection for Directory publication.

Do not add source discovery to Experts. Mutation receives canonical refs already resolved by App/Context.

Private-state settlement from checkpoint 03 must update only `private_state` and preserve binding bytes/revision/operation receipt exactly. Add a regression where source binding changes while another Task settles a different assignment/private state.

### A4. Vault persistence and direct schema cutover

Update Vault Registry serialization and Vault Task record to the new schemas. No old-row decoder or auto-reset.

Task admission in Vault must load the current Registry in the same transaction and reject a proposed Task when its admission/selection no longer equals current callable assignment state. This closes:

```text
Directory resolves selection A
-> user changes binding to B
-> stale Task tries to persist A
```

before source/model I/O.

Memory Task test repositories must implement the same semantic check or a test-controlled equivalent; do not weaken production to accommodate fixtures.

Focused gate:

```sh
cargo test -p floe-context-contract
cargo test -p floe-experts
cargo test -p floe-vault registry
cargo test -p floe-vault task
python3 tools/architecture/check_boundaries.py
git diff --check
```

## 04-B: source candidates and initial configuration

### B1. Context candidate service

Add a Context-owned source-candidate application service. It accepts Person/device plus a pure capability/version requirement and obtains source metadata through owner ports. Context must not depend on Experts.

Candidate enumeration:

- Remote Mail/Work/Logistics: current connection snapshots plus pinned paired-server producer; one exact ref per connection/view resource.
- Calendar: current Calendar connection plus one exact ref per selected calendar resource. Native EventKit and hosted Calendar retain their real connector/execution owner.
- Attention/Contacts/Wellbeing: exact Access canonical native source identities and selected resource.
- Tasks/Confirmed Memory: the intrinsic local identities frozen above.
- Confirmed Interactions: no candidate at the current product baseline.
- unsupported capability/version: empty candidate set or explicit unsupported result, never guessed connector matching.

The catalog must not read source payloads and must not enumerate Access grants as candidate identity.

### B2. Explicit initial defaults

Default binding is a configuration operation, not runtime behavior.

During the first shipped-bundle install/setup only:

1. App asks Context for candidates;
2. product policy may select an unambiguous default set;
3. App applies it through the same Experts binding mutation API used by explicit settings.

Rules:

- one intrinsic target -> may select it;
- one remote account -> may select it;
- one Calendar connection with its explicitly selected Calendar resources -> may select that exact resource set within manifest maximum;
- multiple alternative remote accounts/connections -> leave unconfigured;
- zero candidates -> leave unconfigured;
- adding/connecting a source after initial setup never auto-selects it;
- reinstall/ensure never overwrites any saved selection or disablement;
- default selection never creates/changes a grant.

Add tests with A already selected, then connect B; A remains the binding.

### B3. Candidate command race

A product candidate ID is resolved again at mutation execution. If the connection/resource/producer disappeared or changed since the UI read it, fail Conflict/Unavailable and leave binding unchanged.

Do not store candidate display labels or ephemeral health inside Registry.

## 04-C: Directory/Task admission and binding fences

### C1. Publish selection with Directory entry

Change the callable Registry projection so App receives:

```text
AgentCard
ExpertAdmissionIdentity
ExpertExecutionSelection
```

`DirectoryEntry` / resolved entry carry the selection. Atomic publication identity/equality includes it.

An unconfigured required source does **not** remove the card. The endpoint is published with an incomplete admitted selection; attempting that requirement produces a binding interaction.

A binding mutation commits Registry first, then republish the in-memory Directory from current Registry. Publication failure never rolls back an already durable binding; the next owner inspection/turn repopulates Directory. Every turn/resume preflight republishes from current Registry as today’s setup path requires. Selection fencing prevents a stale in-memory Directory from using a superseded binding.

### C2. Persist and compare selection on Task

Add selection to Task/Vault records and all transition/replay validation.

New Task flow:

```text
Directory resolve { admission, selection, endpoint }
-> Vault admit transaction rechecks current Registry admission + selection
-> persist exact admission + selection
-> Working
-> endpoint with the same selection
```

No current setting is re-read to reroute the Task.

### C3. Current binding fence

Define one Experts-owned validation operation over a current Registry snapshot:

```text
validate_current_execution_selection(
  person,
  admission,
  admitted_selection,
  require_enabled
)
```

Use it at least:

- Task durable admission;
- endpoint before package execution;
- before every requirement/source read;
- immediately before each Expert model dispatch;
- immediately after model response before result use;
- before final ExpertReport release;
- inside assignment-local Task settlement transaction.

If assignment is disabled, requirement binding removed/replaced, package/definition changes, or binding revision/digest differs, return a stale/conflict/capability failure. Never resolve a new target.

A zero-source Expert still carries an empty canonical selection and fences assignment/package identity.

### C4. Race regressions

Use deterministic barriers, not sleeps:

- resolve A -> change to B -> stale Task durable admit: rejected before endpoint call;
- admit A -> change to B before source read: no A/B payload I/O;
- read A -> change to B before model dispatch: model not dispatched;
- model response for A -> change before report release: no terminal successful release;
- same Task crash/recovery retains A and is fenced if A configuration is superseded;
- linked resume after configuration change admits B;
- completed A Task replay still returns its historical Task result without runner/source/model replay.

## 04-D: exact-target Context reads

### D1. Host uses admitted requirement selection

`DelegatedMessageExperts` stores the endpoint's `ExpertExecutionSelection`.

On `read_requirement(key, query)`:

1. verify key exists in manifest and admitted selection with identical capability/version/cardinality;
2. current-binding fence;
3. if selected count is below required minimum:
   - capture an Experts-owned binding requirement;
   - return package-facing `NeedsUserAction` without Context source I/O;
4. if optional selection is empty:
   - return an honest unconfigured/unavailable package outcome without source I/O and without fabricating evidence;
5. otherwise call Context exact-target acquisition with **only** the admitted selected references;
6. capture real SourceAccess blockers separately;
7. record every ready dependency exactly.

A query cannot carry a connector, connection, execution owner or alternate resource selector. Reject any capability-specific query field that attempts to smuggle target identity.

### D2. Remote View exact-target API

Add an Expert-specific exact-target remote read path. It accepts the admitted references and for each one:

- validates capability/version/resource canonical form;
- examines only the exact connector/connection/execution-owner tuple;
- checks at most one live grant for that exact selected target/resource;
- when grant is absent/paused/review-required/mismatched, emits a blocker for that selected target only;
- verifies consumer/category/Read/Assistant/processing restrictions;
- performs signed producer preview and exact source/revision check;
- reads/releases under the exact grant and consumer;
- returns one dependency per contributing source;
- merges only successful selected contributors using current bounded merge semantics.

Unselected grants are not candidates, blockers or contributors.

The existing current-selection remote path may remain only where a legitimate Manager/product-owned direct tool still uses it. Residual audit must prove no Expert requirement reaches it.

### D3. Calendar exact-target API

Group admitted Calendar references by exact source identity and read only their listed calendar resources.

Validate current Calendar owner metadata against:

- exact connection;
- connector/provider;
- execution owner/device;
- selected calendar resources;
- contract version.

A newly added Calendar resource on the same current connection is ignored until explicitly selected. A removed selected resource yields a typed current-source/configuration limitation; it never widens to another calendar.

Native reads retain subject/grant/current authority checks. Remote Calendar retains signed server source/admission/release checks. Selection reference does not substitute for either.

### D4. Native personal exact targets

Attention/Contacts/Wellbeing drivers receive and validate the admitted reference before OS payload access. No “pick current platform source” fallback after Task admission.

A selection bound to another device/connector/connection fails closed and does not scan another grant.

### D5. Intrinsic Context sources

Tasks and Confirmed Memory validate the admitted intrinsic local source reference against the verified execution device and canonical Context constants, then project the existing bounded local view.

No DataAccessGrant is created for these internal projections merely because they participate in binding.

### D6. ConfirmedInteractions correction

Remove the special:

```text
CapabilityUnavailable -> Ready([])
```

behavior from `read_declared_source`.

Remove the production App call to `ServerSourceClient::read_confirmed_interaction_view`. At this baseline no authorised server source route exists, so the optional requirement has no candidate and reads as unconfigured/unavailable, not successful empty evidence.

Do not add a Go endpoint in this checkpoint. The now-caller-zero provider convenience method is a checkpoint-05 deletion target.

Update Relationships tests so an unavailable confirmed-interaction source is represented as a limitation while real authorised empty results, if a future source provides them, remain distinguishable.

## 04-E: first-party authority policy and durable interactions

### E1. First-party policy follows exact trusted selections

Refactor `first_party_observe` policy inputs.

For each target connection/resource/view, Expert consumers are:

```text
trusted product-shipped registrations
INTERSECT
currently installed/active assignments
INTERSECT
assignments whose explicit binding selects this exact target
```

Manager direct-view `assistant` remains independently derived from Context's Manager tool catalogue.

An arbitrary installed/supplied extension is never automatically first-party, even if it selects the source.

Installation without binding contributes no consumer.

Binding mutation itself does not mutate a grant. Therefore:

- selecting a new consumer on an already reviewed source may make current product policy differ and a later read asks for review;
- removing a binding blocks Expert use immediately but does not silently revoke a grant;
- existing prospective policy fingerprint detects consumer-set drift before any later grant mutation.

Retain exact sorted consumer/category/purpose/processing fingerprint semantics from 01.

### E2. Binding interaction kind

Extend the existing durable interaction system, not a second recovery mechanism.

Add an interaction kind/requirement and reviewed target conceptually:

```text
UserInteractionKind::ExpertBinding
InteractionRequirementKind::ConfigureExpertBinding

ExpertBindingTarget
  registry_instance_id
  assignment_id
  package
  definition_revision
  requirement_key
  capability
  contract_version
  minimum_sources
  maximum_sources
  expected_binding_revision
  admitted_selection_digest
```

It contains no candidate/source reference because the blocked run has not reviewed one.

Include it in the existing canonical target digest. Do not introduce a second digest.

Safe product projection contains only assignment/package/requirement identity and display-safe labels. Internal Registry instance/digests/revisions remain backend-only where not needed by the UI.

### E3. Binding interaction actions/resolution

A binding card is navigation/configuration, not an inline grant approval. It offers backend-projected actions such as:

- Open Expert settings;
- Refresh;
- Dismiss/Not now.

Do **not** offer `Allow` as if choosing a source were an authority grant.

The settings mutation is a separate Experts owner command. Returning from settings alone does not resolve the card.

Explicit Refresh re-reads Experts owner state:

- same assignment/package/definition and now-complete requirement -> Resolved;
- still incomplete -> Pending;
- assignment/package/manifest identity no longer names the same requirement -> Superseded/replacement;
- terminal/expired/wrong device use existing lifecycle semantics.

Resolution never chooses a candidate for the user.

Resolved binding interaction may trigger the existing linked-resume claim. The child is a fresh Run and gets current binding through fresh Task admission.

### E4. Source-access interactions bind Expert Task selection

When an Expert Task produces a real SourceAccess requirement for selected target A, store enough admitted Task selection identity with the reviewed internal target to prove that A was the Task's configured target.

Before any inline Access/Connections mutation:

1. verify the origin Task/journal as today;
2. load its durable admitted selection;
3. verify current assignment binding still matches the Task selection;
4. verify reviewed source target is contained in that selection;
5. only then compare live grant/source/policy and mutate.

If user rebound assignment A -> B, old A review is Superseded before grant mutation. It does not grant A and then resume on B.

Manager direct-source interactions have no Expert selection context and retain their current owner validation.

### E5. Interaction/product wire

Extend Conversation storage, App projection, protocol/FFI and Flutter parser with:

- ExpertBinding interaction kind/target;
- OpenExpertSettings action/navigation.

No credentials, grant authority, execution-owner IDs, selection refs or policy fingerprints are exposed to Flutter.

Retain one linked-resume implementation and one interaction CAS/operation identity.

## 04-F: App/FFI generic Expert settings and usable Flutter UI

### F1. Experts/App service surface

Keep enable/disable Registry configuration semantics and add generic settings operations. Recommended owner intents:

```text
inspect Expert settings/list
inspect one assignment/detail
list source candidates { assignment_id, requirement_key }
replace source selection {
  assignment_id
  package id/version
  definition_revision
  requirement_key
  expected_binding_revision
  candidate_ids[]
}
```

Empty `candidate_ids` removes selection.

Do not let callers supply Person/device, consumer, connector ID, execution owner, resource, grant ID or authority.

Caller Person/device come from `CallerContext`. App resolves assignment/manifest and current candidate IDs before calling Experts mutation.

Use the existing owner operation ID as the durable binding operation identity; do not add a second wire operation ID.

Converge the worker to one Experts owner path rather than adding unrelated per-requirement WorkerAction variants.

### F2. Product settings projection

Provide safe generic settings values:

```text
ExpertSettingsEntry
  assignment/install/package/definition
  manifest name/description/domain tags/skills
  enabled
  binding revision
  requirements[]

RequirementSettings
  key
  capability
  required/optional
  min/max
  selected candidate ids/status
```

Candidate detail is projected separately/currently. Do not leak `SourceSelectionReference`.

Use manifest metadata from the backend. Flutter must stop relying on hard-coded package ID -> title/description for this settings surface.

### F3. Protocol/FFI

Add bounded DTOs/commands/queries for the settings surface and candidate IDs. Strictly reject:

- nil/foreign assignment IDs;
- empty requirement keys;
- duplicate candidate IDs;
- too many candidate IDs;
- arbitrary extra target fields;
- stale expected binding revision.

FFI only converts DTO -> App owner intent and result -> DTO; it does not enumerate connectors or decide compatibility.

The Rust producer and Dart parser must have one tracked fixture for the new settings/detail shape if the existing hand-written tests cannot prove the same bytes. Prefer a real Rust serializer fixture over parallel copies.

### F4. Flutter

Extend `features/experts` into a generic list/detail settings surface:

- use manifest-provided name/description;
- enable/disable still works;
- each declared requirement shows required/optional and current selection;
- candidate picker is single- or multi-select according to max cardinality;
- saved-but-unavailable selection is shown explicitly;
- no candidates shows a truthful unavailable state;
- selection save uses expected binding revision and reloads on Conflict;
- choosing a source does not imply “permission granted”;
- separate access/reconnect state links to existing Connections/interaction flows;
- an unknown `example.test.expert` renders without new UI branch.

Interaction `OpenExpertSettings` navigates directly to the exact assignment/requirement.

Use Floe design tokens/squircle components/localization; do not revive the removed Calendar-specific Expert settings screen.

## 04-G: race, recovery, authority and Actions regression matrix

Mandatory executable cases:

| Scenario | Required result |
|---|---|
| Source A bound, B later connected | Existing binding remains A; Task reads only A. |
| A/B grants both live, binding A | Expert classification ignores B entirely. |
| Binding A -> B before Task durable admit | Stale A admission rejected transactionally; no payload/model. |
| Binding A -> B after Task admit | Existing Task never reads B; later stages are fenced. |
| Binding removed | New Task remains callable but required read emits ExpertBinding interaction. |
| Assignment disabled | New delegation blocked; active Task later stage fenced; completed replay retained. |
| Calendar resource added to connection | Old selection does not widen. |
| Selected Calendar removed | Typed limitation/review; no alternate calendar. |
| Selected remote credential expires | Reconnect interaction for exact selected target; no another account fallback. |
| Selected grant absent | Review for exact target; no grant created until explicit Access decision. |
| Old Access review for A, then binding B | Review superseded before grant mutation. |
| Old binding interaction then user configures requirement | Explicit refresh resolves; linked resume is fresh Task/current selection. |
| Candidate ID stale between UI read/save | Binding unchanged; conflict/unavailable. |
| Same binding operation retransmitted after lost ack | Exact rejoin, no revision double-increment. |
| Same operation ID/different selection | Conflict. |
| Private-state Task settlement concurrent with binding change | Both preserve unrelated fields or same-assignment race conflicts; no lost binding. |
| Multiple selected remote sources | Merge only selected successful contributors; dependency for every contributor. |
| One selected contributor blocked | No truncated successful aggregate that silently drops it when requirement semantics require full selected set. |
| Optional unconfigured source | Honest limitation; never `Ready(empty)` evidence. |
| ConfirmedInteractions unavailable | Not `Ready([])` and no unauthorised server call. |
| First-party binding added after old review | Old fingerprint/review superseded on consumer-set drift. |
| Extension selects source | No automatic first-party consumer/grant. |
| Binding changes after proposal, before action publication | Proposal becomes stale/blocked before new dispatch. |
| Binding changes after durable external-write intent | Do not blind retry/cancel; uncertain-write reconciliation remains authoritative. |

Use deterministic barriers/channels for race tests.

### Actions integration rule

Task binding is configuration provenance, not Act authority.

Before creating/publishing a new Expert-origin Calendar action, Vault/Actions revalidate:

- exact durable Task/admission/proposal artifact;
- exact contributor dependency/current Access fence;
- Task admitted selection is still current and contains the proposal's selected Calendar target;
- existing approval/current source/action policy.

After durable pre-dispatch external intent exists, recovery follows the existing external idempotency/lookup path even if binding later changes. Do not erase an uncertain provider effect because settings changed.

## 04-H: residual audit, verification and documentation convergence

### Residual audit

Search at minimum:

```text
select_current_calendar_connection
CurrentCalendarSelection
classify_remote_sources
read_remote_view(
read_confirmed_interaction_view
CapabilityUnavailable.*Ready
ConfirmedInteractions
core.calendar_connection

SourceSelectionReference
ExpertBindingState
ExpertExecutionSelection
binding_revision
selection_digest
RequirementReadOutcome

BuiltinExpertKind
floe.builtin.
first_party_observe
builtin_consumers

ConfigureExpertBinding
ExpertBindingTarget
OpenExpertSettings
SelectResource
ResourcePicker

experts.registry
experts.binding
experts.settings
agentCapabilityTitle
agentCapabilityDescription

grant_id
source_authority
bearer
credential
```

Interpret by owner:

- Manager/product-owned current source selection may legitimately retain current-selection helpers;
- candidate discovery may enumerate connection metadata;
- Access/Context current authority paths legitimately use grant/source authority;
- historical plans/tests may name removed symbols;
- provider convenience `read_confirmed_interaction_view` may remain caller-zero for 05 deletion.

There must be **no Expert runtime path** that:

- discovers a connection/grant outside admitted selection;
- calls current Calendar selection;
- calls unbound remote-view classification;
- falls back from A to B;
- turns optional source failure into fake successful empty evidence;
- lets Flutter submit a technical source reference;
- treats binding as grant authority.

### Targeted gates

Run owner tests as substeps land. Final targeted gate must include at least:

```sh
cargo test -p floe-context-contract
cargo test -p floe-experts
cargo test -p floe-context
cargo test -p floe-access
cargo test -p floe-vault
cargo test -p floe-app registered_runner_ --lib -- --test-threads=1
cargo test -p floe-app first_party_observe --lib -- --test-threads=1
cargo test -p floe-app interaction_resolution --lib -- --test-threads=1
cargo test -p floe-app vault_registry --lib -- --test-threads=1
cargo test -p floe-actions
cargo test -p floe-protocol
cargo test -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

(
  cd apps/client
  flutter test test/features/experts
  flutter test test/features/conversation
  flutter test test/features/connections
)
```

Use real target names if repository test layout changes. A zero-test filter is not evidence.

### Broad gate

Checkpoint 04 changes shared contracts, durable storage, authority-sensitive source execution, protocol/FFI and Flutter product settings. Use the known serial Rust workspace qualification:

```sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
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

This proves serial full-workspace reliability only. If default-parallel tests are also run, report them separately and never increase production deadlines/ignore tests to make them pass.

Go/server code is **not intended to change** in 04: ConfirmedInteractions remains unsupported rather than adding an unauthorised feature. If implementation evidence forces a server change, stop and re-evaluate scope; if it is still necessary, run from `server/`:

```sh
go test -race ./...
go vet ./...
```

and explain why the owner boundary expanded.

iOS is not a required 04 acceptance gate. Report it as not executed if not run. No Android parity work.

Keep the proposal-card golden assertion/PNG byte-identical to the task start commit.

### Documentation convergence

After implementation only, update current docs to state:

- Experts owns persisted source selection; binding is not permission;
- Context owns candidate identity/exact-target acquisition;
- Task admission pins the immutable execution selection;
- current binding fences later use but does not reroute a Task;
- Manager direct-source selection remains product-owned;
- missing binding uses ExpertBinding interaction; source authority review remains Access/Connections;
- first-party Observe policy includes only trusted assignments explicitly selecting the exact target, plus Manager direct consumers;
- product settings expose generic Expert requirements/candidates without authority;
- ConfirmedInteractions has no current supported source and is never faked as empty evidence.

Do not document checkpoint-05 deletion/conformance as complete.

Only after all targeted/broad gates and UI acceptance pass, mark 04 Complete in `README.md` with actual commit SHAs. Leave 05 Not started.

## Recommended commit boundaries

Prefer these only when each intermediate tree is coherent:

1. **04-A** — source reference + manifest/binding Registry contracts, schema cutover and persistence;
2. **04-B** — Context candidate catalog + explicit initial default configuration;
3. **04-C** — Directory/Task execution selection + Vault admission/fences;
4. **04-D** — exact-target remote/Calendar/personal/intrinsic execution + ConfirmedInteractions correction;
5. **04-E** — selection-aware first-party policy + durable binding/source interaction convergence;
6. **04-F** — App/protocol/FFI generic settings/candidates + Flutter list/detail UI;
7. **04-G** — race/Actions/recovery regressions;
8. **04-H** — residual audit, broad verification, architecture/status convergence.

Combine adjacent commits if splitting would require an unbound runtime, an old/new storage decoder, source fallback or a temporary authority bypass.

## Required completion report

Report:

1. start local/origin HEAD, worktree, source delta and actual 04-A..H commit SHAs;
2. final `SourceSelectionReference` and direct dependency/policy edge;
3. final manifest requirement contract-version/multiplicity changes;
4. assignment binding state, revision and exact-operation rejoin algorithm;
5. durable schema versions and fresh-profile limitation;
6. candidate catalog owners and per-capability canonical target construction;
7. explicit first-install default policy and proof connector arrival never auto-rebinds;
8. final `ExpertExecutionSelection` / digest and Directory/Task/Vault persistence;
9. each current-binding fence and race test proving no reroute;
10. exact-target remote/Calendar/personal/intrinsic read algorithms;
11. proof unselected source B is not enumerated/read/blocked;
12. ConfirmedInteractions correction and remaining caller-zero 05 cleanup;
13. first-party consumer derivation from trusted exact selections and policy-fingerprint drift tests;
14. binding interaction internal/safe wire shape and linked-resume behavior;
15. proof old SourceAccess review A cannot mutate after rebind B;
16. App/protocol/FFI intents and proof clients cannot submit source refs/authority;
17. Flutter generic unknown-Expert settings UI and exact tests;
18. Actions binding-fence behavior before dispatch and uncertain-write behavior after durable intent;
19. targeted/broad exact commands/results, ignored tests, serial-vs-parallel qualification and unavailable platform gates;
20. residual-search classification and proposal-card golden byte comparison;
21. current architecture/status docs changed;
22. final local HEAD, clean worktree, **no push**, and checkpoint 05 still Not started.
