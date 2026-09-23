# Checkpoint 03 — Expert Registry and Access authority convergence

- **Status:** next
- **Start baseline:** `main` at `605250d0b8f3bec476299673a8975fa82a073c52`
- **Precondition:** Checkpoint 02 is complete; Schedule no longer depends on CalendarExpertSetup, Registry Calendar view selection, or a Schedule-specific endpoint.

## Goal

Remove the second source-permission authority from Expert Registry and remove the Calendar-Expert-specific grant binding/persistence model.

After this checkpoint:

- Expert Registry answers only Expert/package installation and enablement questions;
- Access/DataAccessGrant is the sole authority for whether a consumer may Observe source data;
- an enabled Expert remains discoverable even when its mandatory source is disabled, unconfigured or awaiting review;
- Calendar grants are bound to connection/source/resource/consumer policy, not to Schedule installation/assignment/view identities;
- CalendarExpertSetup, CalendarExpertOverview and CalendarAccessConfiguration no longer exist as product/runtime concepts;
- stale local development Calendar-Expert persistence is reset/rejected rather than migrated into new authority.

Checkpoint 02 is complete. R4.5 already establishes the canonical first-party Calendar consumer policy required by the common runtime. Checkpoint 03 reuses that policy while removing obsolete Registry/setup/mapping authority; it must not introduce a second consumer list.

## Baseline anchors

- crates/modules/experts/src/registry/expert_setup.rs:22 — ExpertSetupSpec
- crates/modules/experts/src/registry/expert_setup.rs:49 — install_builtin_experts_enabled
- crates/modules/experts/src/registry/expert_setup.rs:236 — enabled_expert_cards
- crates/modules/experts/src/registry/expert_setup.rs:294 — assignment_has_mandatory_source
- crates/modules/experts/src/registry/expert_setup.rs:313 — assignment_source_grant
- crates/modules/experts/src/registry/expert_setup.rs:448 — SourceGrants
- crates/modules/experts/src/registry/calendar_setup.rs:72 — CalendarExpertSetup
- crates/modules/experts/src/registry/calendar_setup.rs:125 — CalendarExpertOverview
- crates/modules/experts/src/registry/calendar_setup.rs:231 — install_calendar_expert
- crates/modules/experts/src/registry/calendar_setup.rs:414 — validate_calendar_setups
- crates/modules/experts/src/calendar_access.rs:74 — install_calendar_expert
- crates/modules/experts/src/calendar_access.rs:93 — apply_calendar_access
- crates/adapters/vault/src/vault/calendar_grants.rs:50 — initialize_calendar_grant_store
- crates/adapters/vault/src/vault/calendar_grants.rs:188 — authorize_calendar_grant
- crates/adapters/vault/src/vault/calendar_grants.rs:331 — calendar_grant_connection_id
- crates/adapters/vault/src/vault/calendar_grants.rs:1150 — install_calendar_expert_with_connection
- crates/app/src/vault_host.rs:1576 — WorkerAction::CalendarExperts
- crates/app/src/vault_host.rs:1645 — WorkerAction::CalendarAccess
- crates/app/src/vault_host.rs:2462 — builtin_source_bindings
- crates/app/src/vault_host.rs:2521 — builtin_setup_specs

## 1. Define the final Registry meaning

### 1.1 Built-in Expert setup is source-independent

The Registry stores:

- built-in Expert package identity/version/metadata;
- installation identity and enabled state;
- assignment identity and enabled state;
- private Expert state / settlement revision where needed.

It no longer stores the current answer to “is Calendar/Mail/etc readable?”

The declaration may still contain static metadata:

~~~text
required_sources
mandatory_source
~~~

That metadata is owned by the Expert package and is useful to dispatch/runtime. It is not a grant.

### 1.2 Remove source snapshots from BuiltinExpertSetup

Refactor BuiltinExpertSetup so its durable identity does not include current source bindings.

Current source-dependent fields to remove from the built-in setup path:

- BuiltinSourceBinding list;
- BuiltinSourceState as a Registry access decision;
- granted view handles derived from source availability;
- source refresh that rewrites assignment view grants when a connector appears/disappears.

The resulting setup should be stable across connector changes.

A conceptual target is:

~~~text
BuiltinExpertSetup
  instance_id
  expected_revision
  setup_id
~~~

with assignments derived entirely from the static ExpertSetupSpec declarations.

If setup_id can be deterministically owned by Vault/Registry and need not cross a public boundary, narrow it rather than carrying extra caller authority.

### 1.3 Simplify ensure_builtin_experts

crates/modules/experts/src/builtin_setup.rs currently treats current source evidence as input and refreshes Registry source state.

Change it to “ensure the built-in packages/assignments exist and are enabled according to Expert settings”.

Delete concepts whose only purpose was source availability:

~~~text
BuiltinSourceEvidence
BuiltinExpertRefresh source refresh semantics
refresh_builtin_expert_sources
~~~

If BuiltinExpertRefresh still has a real lifecycle meaning, e.g. install-at-session-start vs inspect-only, keep only that lifecycle meaning. Do not keep source refresh naming.

App’s ensure_builtin_experts() in crates/app/src/vault_host.rs must stop calling calendar_connector_snapshot() or paired-server presence merely to decide Registry source state.

Connection/source health belongs to Connections/Context at use time.

## 2. Change enabled_expert_cards()

At crates/modules/experts/src/registry/expert_setup.rs:236:

remove these eligibility conditions for built-in Experts:

- granted_view_handles must be non-empty;
- assignment_has_mandatory_source();
- dynamic source state must be Available.

An Expert card is eligible when the Expert/package/assignment itself is enabled and structurally valid.

The runtime then discovers source absence through its declared source reads.

This is required for chat-native permission recovery. If the card disappears when a source is paused, Manager cannot delegate to the Expert that knows which evidence it needs.

### 2.1 Keep package integrity checks

Do not relax:

- person/assignment ownership;
- installation enabled;
- assignment enabled;
- required tool assignment linkage;
- package kind/revision;
- Expert metadata validity.

Only remove source-access authority from Registry.

### 2.2 Third-party future semantics

Do not interpret this change as “all third-party Experts can read all sources”.

Third-party authorization remains Access-owned and explicit. If generic Registry fields such as granted_view_handles are still needed as package configuration or declaration references, keep them only with that non-authoritative meaning and document it.

If the field is unused after built-in migration and has no real third-party consumer yet, delete it instead of preserving speculative state.

## 3. Delete SourceGrants as runtime authority

At expert_setup.rs baseline line 448, SourceGrants copies setup/source state into the App Expert host.

Delete SourceGrants and BuiltinExpertHost::source_grant()/source_granted() as permission decisions.

Replace Expert-side behavior with actual source acquisition outcomes from Context:

~~~text
Expert asks for Calendar
  -> Context/Access returns Ready / Unavailable / NeedsUserAction
~~~

Static “optional vs mandatory” remains Expert-domain knowledge:

- mandatory source unavailable/user-action => blocked/no-conclusion Expert output;
- optional source unavailable => continue without that evidence when domain rules allow it.

The Expert must not preflight a Registry boolean before performing the source request.

This also removes the current mismatch where Registry says Granted while DataAccessGrant denies.

## 4. Delete the Calendar Expert Registry vertical

Delete production concepts in crates/modules/experts/src/registry/calendar_setup.rs:

~~~text
CalendarExpertSetup
CalendarExpertSetupResult
CalendarExpertOverview
CalendarAccessConfiguration
CalendarAccessChange
CalendarViewBinding as Schedule-specific setup authority
calendar_setups
revoked_calendar_setups
~~~

Before deleting CalendarViewBinding, inspect whether another non-Expert domain legitimately uses it. If it is only the old Schedule setup representation, delete it. If a provider-neutral Calendar resource selection type is needed, move that meaning to Connections/Context contracts rather than renaming the old Registry type.

Delete crates/modules/experts/src/calendar_access.rs once all callers use Access/Connections directly.

Update crates/modules/experts/src/lib.rs exports accordingly.

Do not leave deprecated aliases.

## 5. Replace Calendar grant mapping with source-owned grant lookup

### 5.1 Current defect

calendar_grant_mappings currently binds Access state to Schedule setup/Registry identities.

That creates invalid states such as:

~~~text
Registry Schedule enabled
calendar_grant_mapping missing
DataAccessGrant missing
~~~

and makes pause/remove depend on a mapping that may not exist.

### 5.2 Target grant identity

A Calendar Observe grant is identified by Access semantics:

- Person;
- connector;
- stable connection;
- execution owner/source authority;
- selected resource handles;
- operation Read;
- purpose Assistant or the applicable product purpose;
- approved first-party consumers;
- processing restriction;
- grant authority/revision.

No Expert installation id, assignment id, setup id or Registry view handle belongs in that identity.

### 5.3 Vault changes

In crates/adapters/vault/src/vault/calendar_grants.rs:

- remove calendar_grant_mappings table/schema if it only exists for CalendarExpertSetup lookup;
- remove calendar_grant_connection_id(setup_id);
- remove install_calendar_expert_with_connection();
- remove mapping verification against Registry installation/assignment/view ids;
- retain or extract the actual DataAccessGrant persistence and native subject review data under generic Access ownership;
- add indexed source/connection lookup if needed to efficiently find the current grant.

Prefer one generic DataAccessGrant store/query path over a Calendar-specific parallel store.

If Calendar requires extra reviewed native subject evidence, keep that evidence keyed by grant/source authority, not Expert setup id.

### 5.4 Authorization

authorize_calendar_grant() may remain as a Calendar-shaped Access convenience if it performs a real Calendar-specific scope/fingerprint check, but it must:

- resolve the grant from Access source/connection state;
- validate consumer explicitly;
- never consult Expert Registry enabled bits;
- never consult Schedule setup ids.

If the generic DataAccessGrant admission service can fully express the same checks, delete authorize_calendar_grant() and use the generic admission path.

## 6. Consumer-policy follow-through

Checkpoint 02 R4.5 already establishes Calendar consumer identity: product composition derives the explicit first-party consumer set from built-in declarations and passes that set into Access grant creation. Grants contain real consumer ids; `calendar.expert` is not a group alias.

Checkpoint 03 preserves that rule while ownership moves from CalendarExpertSetup/mappings to connection/source-owned Access state.

Requirements:

- Access remains independent of the built-in Expert catalogue;
- do not duplicate the Calendar consumer list in Vault, protocol, Flutter or connection UI;
- new connection-owned Calendar grant creation reuses the same product-composition policy;
- ContextDependency continues to record the actual admitted consumer;
- extension/third-party consumers remain excluded unless separately granted;
- adding a new built-in Calendar consumer is covered by declaration/policy tests, not an Access special case;
- do not revive `calendar.expert` as compatibility authority after the old vertical is removed;
- do not silently broaden legacy grants during schema cleanup.

If direct Manager/assistant Calendar reading becomes a real current path, add that consumer through the same product policy with a focused runtime test. Do not pre-authorize hypothetical consumers.

## 7. App/Worker API removal

At crates/app/src/vault_host.rs baseline lines 1576 and 1645 remove WorkerAction paths that exist only for Calendar Expert management:

~~~text
CalendarExperts
CalendarAccess
~~~

Retain a source/access command only if it is the canonical connector/access command used by connection detail and conversation interaction.

Delete or refactor:

- crates/app/src/vault_host/calendar_access.rs;
- crates/app/src/expert_services.rs CalendarExpertInstall / InstallCalendar / ExpertInspection::Calendar;
- related LocalOperationIntent variants;
- result fields named calendar_experts.

After checkpoint exit, Experts service should expose generic Registry operations only. Source permission operations belong to Access/Connections.

## 8. FFI/protocol deletion

Delete the old Calendar Expert wire surface:

~~~text
experts.calendar.install
experts.calendar.inspect
CalendarExpertInstallDto
CalendarExpertOverviewDto
CalendarGrantConfigurationDto fields that carry setup_id / Registry revision
~~~

The canonical source permission API must be connection/access-oriented.

It is acceptable to introduce a replacement command in the same checkpoint because internal wire compatibility is not required. Do not keep both command families.

Update:

- crates/bindings/protocol/src/dto/commands.rs;
- crates/bindings/protocol/src/dto/queries.rs;
- crates/bindings/protocol/src/dto/experts.rs;
- crates/bindings/protocol/src/dto/access.rs;
- protocol exports;
- FFI app_wire conversion;
- C ABI tests/fixtures where the app-wire command set is enumerated;
- Flutter local_owner_gateways.dart.

Use owner-language naming. A permission command should not mention Expert installation.

## 9. Local development persistence policy

Do not write a migration that converts old Calendar setup/Registry enabled bits into active DataAccessGrant.

That would reinterpret an old implementation flag as user authorization.

Because this repository is pre-stable/local-development-only:

- replace the schema;
- reject/reset the obsolete Floe-owned local grant/setup state where necessary;
- document the exact development profile reset requirement;
- preserve unrelated credentials and uncertain external action records.

Add a regression asserting that an old/orphan Calendar Expert mapping is not auto-promoted to a new grant.

## 10. Tests

### Registry

- all eight built-in Experts install independently of current source availability;
- enabled_expert_cards includes enabled Schedule even with no Calendar connection;
- disabling the Expert itself removes its card;
- adding/removing/pausing a connection does not mutate Registry revision merely to express source health;
- source state is absent from Registry snapshot serialization.

### Access

- active Calendar grant continues to authorize each canonical first-party consumer established in Checkpoint 02 R4.5;
- paused/review-required grant returns NeedsUserAction;
- foreign consumer denied;
- third-party Expert denied without explicit grant;
- resource scope cannot exceed selected connection resources;
- source authority/fingerprint drift requires review;
- grant lookup does not use setup/installation/assignment ids.

### Persistence/reopen

- grant survives Vault reopen with the same source/authority;
- pause survives reopen;
- revoke survives reopen;
- stale authority fails closed;
- no calendar_grant_mappings/Calendar setup schema is required;
- old disposable profile is rejected/reset according to the documented development policy, never silently migrated.

### Conversation availability

With Schedule enabled and Calendar unavailable:
- Schedule card remains in Manager catalog;
- delegation occurs when Manager chooses it;
- Calendar read returns NeedsUserAction;
- root Run can still complete with Manager limitation response.

## 11. Residual audit

Search:

~~~text
CalendarExpertSetup
CalendarExpertOverview
CalendarAccessConfiguration
CalendarAccessChange
CalendarViewBinding
calendar_setups
revoked_calendar_setups
install_calendar_expert
apply_calendar_access
calendar_grant_mappings
calendar_grant_connection_id
install_calendar_expert_with_connection
SourceGrants
assignment_source_grant
assignment_has_mandatory_source
BuiltinSourceBinding
BuiltinSourceState
BuiltinSourceEvidence
experts.calendar.install
experts.calendar.inspect
calendar_experts
~~~

Every production match must be removed unless the same word now refers to a genuinely different non-Registry source concept. Do not keep comments/tests describing the removed architecture as current.

## 12. Verification

Targeted:
- floe-experts Registry tests;
- floe-access DataAccessGrant/admission tests;
- floe-vault grant/reopen tests;
- floe-app local product/wire tests;
- protocol tests;
- Schedule missing-source conversation integration test.

Broad:
- cargo check --workspace --lib
- cargo test --workspace --no-fail-fast
- cargo build -p floe-ffi
- python3 tools/architecture/check_boundaries.py
- git diff --check

If persistence schema changed, use a fresh isolated development profile for manual/macOS smoke verification. Do not overwrite the developer’s normal profile automatically.

## 13. Checkpoint exit criteria

Checkpoint 03 is complete when:

- Expert Registry has no current source-permission authority;
- all eight built-in Experts remain installed/eligible based on Expert state, not source state;
- SourceGrants runtime gating is gone;
- DataAccessGrant is the only Observe consumer authority;
- Calendar grant identity contains no Schedule/Registry setup identity;
- connection-owned grant creation reuses the single canonical first-party consumer policy and contains no `calendar.expert` compatibility authority;
- CalendarExpertSetup/Overview/AccessConfiguration production APIs and wire commands are gone;
- obsolete local Calendar Expert grant state is not migrated into authorization;
- source absence is observed only at Context/Access read time;
- residual search is clean;
- targeted and broad Rust/FFI/protocol gates pass.

Checkpoint 04 can now simplify the product ceremony because there is one real Observe authority to control.
