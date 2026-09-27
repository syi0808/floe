# 02: Stable grant source and dependency semantics

Prerequisite: 01 complete.

This checkpoint removes live source epoch from standing grant identity and separates logical permission scope from the exact leaf resources observed by Context.

## Exit state

1. GrantSourceBinding is stable connection identity and contains no SourceAuthority.
2. GrantScope resources are logical permission resources, not current Calendar/contact/etc. leaf IDs.
3. ContextDependency records SourceAuthority separately and records exact source_resources used by the observation.
4. resource-scope changes do not mutate a standing grant.
5. replay/release/recipient lineage reauthorize both grant and current source facts.
6. Vault's grant schema/query identity no longer indexes source incarnation/epoch.

## 02-A: contract cutover

Primary file:

- crates/contracts/context/src/lib.rs

Change GrantSourceBinding to contain only:

- person_id;
- connection_id;
- connector;
- execution_owner.

Delete:
- source_authority field;
- source_authority accessor;
- same_identity behavior that treats an epoch as grant-source identity.

Keep strict validation of Person/Connection/Connector/owner.

Change ContextDependency:

- retain grant_id and grant_authority;
- retain stable source: GrantSourceBinding;
- retain resources as the logical Access grant resources;
- add source_authority: SourceAuthority;
- add source_resources: Vec<ResourceHandle> for the exact leaf resources/provider scope actually used;
- retain categories, operation, purpose, consumer, processing and observation/query/lease/process/freshness identity;
- ConsumerPolicyAuthority remains until 05.

Validation rules:

- source_resources is non-empty for source-backed evidence and bounded/canonical;
- dependency resources must still be admitted by GrantScope;
- source_resources are not required to be GrantScope resources;
- current source reauthorization compares source_authority and source_resources to the Connections/provider owner;
- source_resources are provenance, not a new grant scope.

Update identity/canonical serialization and size limits deliberately. Since stored local dependencies are disposable, no old decoder.

## 02-B: canonical logical View resource

Create one shared helper at the Context contract/Context owner boundary for a connection-scoped logical View handle. Do not keep separate Calendar and remote naming schemes.

Canonical form:

~~~
<view-id>:<connection-id>
~~~

Examples:

- calendar.timeline:<connection>
- mail.communication:<connection>
- work.context:<connection>
- life.logistics:<connection>
- people.identity:<connection>
- attention.coarse:<connection>
- wellbeing.derived:<connection>

The helper must validate both components and return ResourceHandle. Remove duplicate formatting helpers once callers migrate; remote_view_resource may become the canonical helper or be replaced directly, but two equivalent formatters must not remain.

For standing connection Observe, one grant should normally name one logical View resource. Multiple logical resources remain representable only where a real owner use case exists.

## 02-C: DataAccessGrant/Vault schema cutover

Change Vault grant storage and lookup:

- crates/adapters/vault/src/vault/access_grants.rs
- DataAccessGrant constructors/mutations
- source-index SQL and cleanup code
- any repository implementation that queries source_incarnation/source_epoch as part of grant identity.

Delete source authority columns/index predicates from DataAccessGrant persistence.

Grant lookup identity is stable source + logical resource/consumer as required by the owner operation. Live SourceAuthority is checked separately during admission.

No migration table, dual query or fallback. Recreate development profile/schema directly.

## 02-D: admission and replay cutover

Update:

- crates/modules/access/src/application/dependency.rs
- admission.rs
- release.rs
- calendar_read.rs
- remote_view.rs
- personal_read.rs
- Context source/read code
- Vault context dependency validation
- Conversation coverage reauthorization.

Required order for a read/replay:

1. resolve stable connection/View target;
2. load active grant and prove GrantAuthority/consumer/purpose/processing;
3. load/observe current source state and SourceAuthority;
4. prove current source_resources admit the intended acquisition;
5. acquire;
6. record ContextDependency with both authorities and exact observed resources;
7. recheck before release where the existing path already fences output.

A source epoch change can invalidate historical evidence without changing or pausing the grant.

## 02-E: processing/recipient lineage

Inspect crates/contracts/context/src/processing.rs and all ProcessingSourceScope builders.

Exact-recipient review must not lose source specificity after grant scope becomes logical. Ensure the reviewed processing source scope binds:

- stable connection/View;
- current SourceAuthority;
- exact source_resources represented in the model input.

Do not use a broad logical View handle as proof that every current/future leaf resource was reviewed for transfer.

## Tests

Add/replace tests proving:

1. same grant remains byte/authority-identical across a Connections resource update;
2. old dependency fails current-source reauthorization after SourceAuthority advance;
3. new dependency records the new SourceAuthority and exact source_resources;
4. grant revocation still blocks regardless of source epoch;
5. foreign stable connection cannot borrow another grant;
6. processing recipient scope changes when exact observed resources change;
7. DataAccessGrant persistence reopens without source epoch columns;
8. no old ContextDependency decoder accepts the prior shape.

Delete tests that assert grant source identity includes SourceAuthority.

## Residual audit

Search for:

~~~
GrantSourceBinding
.source_authority()
source_incarnation
source_epoch
same_identity(
remote_view_resource(
ResourceHandle::try_new(calendar_id
scope().resources()
~~~

Classify scope.resources uses carefully: legitimate logical grant scope remains; leaf-resource assumptions must be removed.

## Verification

~~~
cargo test -p floe-context-contract
cargo test -p floe-access
cargo test -p floe-context
cargo test -p floe-vault
cargo test -p floe-conversation
cargo test -p floe-app
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Update current authority/provenance architecture in the same code change.
