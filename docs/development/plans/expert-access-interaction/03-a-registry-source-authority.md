# Checkpoint 03-A — Registry source-authority removal

- **Baseline:** main at bbce2fa2ae2da4bffb05185f39d8d50aa3e6b82a
- **Depends on:** Checkpoint 02 complete
- **Goal:** make Expert Registry purely about Expert/package state and remove all built-in source-permission snapshots/gating before Calendar-specific persistence is deleted.

Do not touch Calendar grant mapping persistence in this substep except for compile-only fallout. 03-B owns that cutover.

---

## 1. Current line-level anchors

| File | Current symbol / line | Problem |
|---|---:|---|
| crates/modules/experts/src/registry/expert_setup.rs | 22 ExpertSetupSpec | persists source declaration into Registry setup mechanics |
| same | 34 BuiltinExpertSetup | carries sources |
| same | 49 install_builtin_experts_enabled | installs source-granted view handles |
| same | 245 enabled_builtin_expert_cards | card eligibility still source-coupled |
| same | 315 assignment_has_mandatory_source | Registry source gate |
| same | 334 assignment_source_grant | Registry source authority |
| same | 469 SourceGrants | host-facing source authority projection |
| crates/modules/experts/src/builtin_setup.rs | 1–120 | BuiltinSourceEvidence and refresh orchestration |
| crates/modules/experts/src/registry.rs | 49 BuiltinExpertSetupReceipt | stores sources |
| same | 59 BuiltinSourceBinding | source/view binding |
| same | 84 BuiltinSourceState | dynamic source state |
| same | 210 PackageAssignment | granted_view_handles |
| same | 250 RegistrySnapshot | Calendar source collections still present; delete in 03-B |
| same | 350 RegistryConfigurationTarget | CalendarView target; final deletion in 03-C |
| same | 920 resolve | source/view grant-based Expert resolution |
| crates/app/src/vault_host.rs | 2417 ensure_builtin_experts | feeds connection/source evidence into Registry |
| same | 2445 builtin_source_bindings | synthesizes source availability |
| crates/app/src/vault_host/expert_setup.rs | 1–90 | store refresh accepts source bindings |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | common endpoint setup | constructs SourceGrants |
| crates/experts/builtin/src/host.rs | 165+ | source_grant/source_granted/require_mandatory_source |

Line numbers are baseline navigation aids. Symbols are authoritative.

---

## 2. Final Registry contract

### 2.1 ExpertSetupSpec

Registry installation spec contains package topology only:

~~~text
ExpertSetupSpec
  expert
  packages
~~~

Remove required_sources and mandatory_source from ExpertSetupSpec. Static source declarations remain in BuiltinExpertDeclaration and are consumed by product/runtime policy, not persisted into Registry.

### 2.2 BuiltinExpertSetup

Target:

~~~text
BuiltinExpertSetup
  instance_id
  expected_revision
  setup_id
~~~

Remove sources.

### 2.3 BuiltinExpertSetupReceipt

Target:

~~~text
BuiltinExpertSetupReceipt
  setup_id
  person_id
  expected_revision
  assignments
~~~

Remove sources.

### 2.4 BuiltinExpertAssignmentReceipt

Target:

~~~text
BuiltinExpertAssignmentReceipt
  expert
  tool_installation_id
  expert_installation_id
  tool_assignment_id
  expert_assignment_id
~~~

Remove required_sources, mandatory_source and granted_view_handles.

### 2.5 PackageAssignment

The baseline granted_view_handles field is the remaining generic Registry view authority. At this baseline its production use is the obsolete Calendar/setup path plus old invocation/test scaffolding; common built-in runtime no longer reads source through it.

Delete granted_view_handles from PackageAssignment unless a current non-test production caller is found during implementation. If such a caller exists, stop and document it before retaining the field.

Do not keep granted_view_handles only for speculative future third-party support.

Tool assignment linkage remains through granted_tool_assignments.

---

## 3. Install/restore semantics

### 3.1 install_builtin_experts

Rewrite installation so each spec installs the declared packages and assignments with no source-derived state.

Required behavior:

- same setup_id/person replays idempotently if package/assignment topology matches;
- connector/source changes never change Registry revision;
- all built-in installations/assignments can be enabled regardless of source availability;
- Registry does not mint synthetic source view UUIDs;
- package/tool integrity remains fail-closed.

### 3.2 validate_builtin_setups

Validate only:

- setup identity/person uniqueness;
- one receipt per Expert;
- installation/package linkage;
- tool assignment linkage;
- Expert package implementation and metadata;
- assignment/install enabled/private state validity.

Delete every comparison to source bindings or granted views.

### 3.3 restore

Remove BuiltinSourceBinding/BuiltinSourceState validation.

Do not add serde defaults or compatibility fields for old snapshots. A stale serialized Registry shape is disposable pre-stable state and may fail open with the documented reset requirement.

---

## 4. Card eligibility

Rewrite enabled_builtin_expert_cards / enabled_expert_card so source state is irrelevant.

An enabled built-in card requires:

1. assignment belongs to Person;
2. assignment enabled;
3. installation enabled;
4. package kind Expert;
5. package implementation Builtin for the expected Expert;
6. required tool assignment/package linkage intact;
7. metadata/card validation succeeds;
8. execution placement is available at runtime.

Delete:

- assignment.granted_view_handles.is_empty gating;
- assignment_has_mandatory_source;
- source-state comparisons.

Regression: Schedule remains in the Manager catalogue with no Calendar grant and with a paused/review-required Calendar grant.

---

## 5. Delete built-in source refresh orchestration

### modules/experts/src/builtin_setup.rs

Delete BuiltinSourceEvidence.

Delete refresh semantics based on source state. Simplify ensure_builtin_experts to:

~~~text
if setup absent and install is allowed:
  install source-independent setup
if setup exists:
  validate/leave it
~~~

BuiltinExpertRefresh may remain only if InstallIfAbsent vs ExistingOnly is still required by the caller lifecycle. Rename it if its current refresh name becomes misleading.

Delete BuiltinExpertStore.refresh and refresh_builtin_expert_sources.

### App

Delete builtin_source_bindings from crates/app/src/vault_host.rs.

ensure_builtin_experts no longer needs FloeCore, LocalContextHost, paired_server or connector snapshots solely to update Registry source state. Narrow its parameters accordingly.

Delete source_id and builtin_source_handle helpers if they become unused.

VaultBuiltinExperts no longer imports BuiltinSourceBinding and has no refresh method.

Connector connect/disconnect/pause must no longer advance Expert Registry revision.

---

## 6. Remove SourceGrants from common built-in runtime

Delete SourceGrants.

Delete BuiltinExpertHost::source_grant and source_granted.

Delete require_mandatory_source.

Delete Registry-derived granted_context filtering.

### Mandatory sources

Each Expert attempts the actual source read it needs. The owning Context/Access path determines whether it is Ready, unavailable, denied/review-required or a hard failure.

Do not replace Registry preflight with a different boolean preflight in App.

### Optional Calendar enrichment

Commitments, Focus & Attention and Wellbeing already have SourceReadOutcome Calendar handling. Remove the source_granted guard and always call calendar_views when the Expert chooses optional Calendar enrichment.

optional_calendar_views remains the place that converts expected Calendar absence into optional context issues.

### Confirmed memory / tasks / confirmed interactions

Remove Registry permission gating.

Use the actual owner/runtime signal:

- conversation context already supplied to the Expert is authorized context; granted_context becomes a plain bounded clone;
- memory_context and task_view are called only according to their host/runtime availability, not Registry source state;
- optional source absence is recorded through existing source issue/outcome contracts where available;
- hard integrity/storage errors remain hard.

If one optional source lacks an expected-unavailable representation and the removal exposes a hard error, add the smallest owner-correct optional acquisition wrapper. Do not reintroduce Registry permission.

### Mandatory source dispatches

Remove require_mandatory_source from Communication, Commitments, Relationships, FocusAttention, Wellbeing, WorkContext and LifeLogistics.

Their first authoritative source read becomes the admission point.

---

## 7. Separate Expert identity settlement from source evidence

### 7.1 Resolve built-in assignment without views

Introduce a Registry operation with semantics such as:

~~~text
resolve_builtin(
  instance_id,
  person_id,
  assignment_id,
  expected_revision,
  expected_expert_id
) -> ResolvedExpert
~~~

It validates Registry/package/tool/private-state identity only.

Do not pass source/view handles.

Keep or delete the old resolve(view handles) path based on actual production callers. Current baseline search shows the old RegistryAssignments/ExpertInvocation path is test-only after Checkpoint 02. Prefer deleting dead invocation/source-grant scaffolding rather than preserving it for hypothetical compatibility.

### 7.2 Evidence identity

The stateful Schedule result currently obtains a fake Registry view_handle from its assignment. Replace that with Context evidence identity.

Breaking contract change is allowed:

~~~text
ExpertResult.view_handle         -> evidence_id
ExpertFocusProposal.view_handle  -> evidence_id
AgentActionOrigin.view_handle    -> evidence_id
~~~

For Schedule, derive evidence_id from the exact ContextDependency.observation_id that corresponds to the result source_handle.

Validation rules:

- evidence_id non-nil;
- proposal.evidence_id == result.evidence_id;
- recorded proposal dependency observation_id == evidence_id;
- Registry never validates evidence_id as a granted view;
- Actions reauthorize the ContextDependency/current source before publication/dispatch.

Update Rust serializers, action origin, Vault proposal validation, Flutter AgentExpertResult parser and fixtures in the same semantic change. Do not keep view_handle as deprecated alias.

### 7.3 Registry result validation

validate_recorded_result / validate_historical_result must resolve Expert identity/private state without Calendar view lookup.

Remove calendar_view(result...) checks from expert_identity_matches and proposal persistence.

---

## 8. Registry snapshot cleanup staged for 03-B

Do not delete calendar_views/calendar_setups/revoked_calendar_setups in the first 03-A commit if Calendar setup persistence still compiles against them.

However, after the source-independent built-in path is green, no new code may depend on those fields. 03-B deletes them and the files that own them.

03-A exit report must list every remaining caller of:

~~~text
calendar_views
calendar_setups
revoked_calendar_setups
CalendarViewBinding
CalendarExpertSetupReceipt
~~~

All remaining callers must be within the obsolete Calendar setup/access vertical scheduled for 03-B/03-C.

---

## 9. Targeted tests

### Registry

- built-in setup install succeeds with zero source input;
- idempotent replay ignores connector state because connector state is absent;
- all eight enabled built-ins yield cards with no source grants;
- disabling Expert assignment/install removes its card;
- changing Calendar connection/source does not change Registry revision;
- package/tool linkage corruption still fails;
- private state settlement CAS still fails on stale revision.

### Runtime

- Schedule with no Calendar access remains listed and returns the existing blocked source result;
- Commitments/Focus/Wellbeing optional Calendar absence is degraded context, not hidden card;
- mandatory source absence is discovered at source read, not Registry preflight;
- no source read is authorized merely because Expert is enabled.

### Evidence

- successful Schedule result evidence_id equals the recorded dependency observation id;
- /focus proposal evidence_id matches the same dependency;
- forged evidence_id is rejected;
- Registry source collections are not consulted for proposal publication.

---

## 10. Residual gate

At 03-A exit, production search must be zero for:

~~~text
BuiltinSourceBinding
BuiltinSourceState
BuiltinSourceEvidence
SourceGrants
assignment_source_grant
assignment_has_mandatory_source
refresh_builtin_expert_sources
source_grant(
source_granted(
require_mandatory_source
builtin_source_bindings
~~~

Search granted_view_handles. Expected zero production matches unless an actual non-test runtime caller is documented. Test-only legacy callers should be deleted/ported, not used to justify retention.

Search view_handle in ExpertResult/ExpertFocusProposal/AgentActionOrigin. Expected zero after evidence_id cutover.

CalendarExpertSetup / calendar_views Registry matches may remain only in the 03-B/03-C obsolete vertical.

---

## 11. Verification

Run focused Experts/builtin/App/Vault proposal tests after each semantic patch.

Before 03-A is complete:

~~~sh
cargo check --workspace --lib
cargo test -p floe-experts-builtin
cargo test -p floe-experts
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Use actual workspace package names if they differ and record exact commands.

Do not proceed to 03-B while SourceGrants or Registry source card gating remains.