# 06: Standing personal Observe convergence

Prerequisite: 05 complete.

This checkpoint applies the same connection-owned standing Observe model to native personal sources where that meaning is correct. It deliberately does not force contextual Feasibility, exact-recipient consent or Act into the standing model.

## Scope

Converge:

- Contacts / people.identity;
- Attention / attention.coarse;
- Wellbeing / wellbeing.derived.

Keep separate:

- Feasibility query-specific permission and reviewed destination/event/query;
- model recipient consent;
- Actions.

## Exit state

1. standing personal source resource scope belongs to Connections/source owner, not a grant side table;
2. Contacts selected handles are connection resources, not grant-owned selection;
3. standing personal grants use stable source + logical View resource;
4. product-derived first-party consumers use the same shipped-manifest policy rules as Calendar/remote Views;
5. source subject changes advance SourceAuthority and stale old dependencies;
6. Feasibility retains its contextual reviewed-query authority without pretending to be Connection Use with Floe.

## 06-A: personal connection/resource ownership

Current anchors:

- crates/modules/access/src/application/personal_grants.rs
- crates/adapters/vault/src/vault/personal_grants.rs
- crates/modules/context/src/application/personal_sources.rs
- personal source constants in Access/Context
- Flutter personal access cards.

For Contacts, move selected_handles into the Connections-owned source resource set for that connection/device.

For Attention/Wellbeing, connection resources are the canonical logical/provider resource set exposed by the source; usually one bounded View resource or source class.

Do not store the same handles/resources in DataAccessGrant and a personal_grant auxiliary table.

## 06-B: grant simplification

Standing personal grants follow the same invariant:

~~~
stable source identity
+ logical View resource
+ first-party consumers
+ Read/Assistant/LocalOnly
~~~

The current leaf resources/subject fingerprint are checked from Connections/current source state and written to ContextDependency.source_resources/source_authority.

Remove personal reviewed consumer-policy state already made obsolete by 05.

## 06-C: source candidates and Expert binding

Personal source candidates should represent a usable connection/View target, not per-contact/resource leafs.

Keep exact connector/connection selection where more than one provider/account can satisfy a capability. Do not automatically switch a broken binding to another provider.

A Contacts resource-set change must not change the Expert binding candidate ID.

## 06-D: Feasibility explicit exception

Feasibility currently binds a reviewed event/destination/query and is not a standing connection-wide source scope.

Do not:
- move Feasibility query into generic Connection resources;
- make Use with Floe authorize arbitrary future locations/destinations;
- delete exact query/subject validation because other personal sources simplified.

It may still reuse stable source identity and the ConsumerPolicyAuthority-free dependency model, but its contextual approval remains explicit.

Document this exception in current architecture so later cleanup does not accidentally broaden it.

## Tests

1. Contacts handle set changes -> SourceAuthority advances, standing grant unchanged.
2. Contacts Expert binding candidate unchanged across handle changes.
3. next people read uses new handles and records them as source_resources.
4. old people dependency becomes stale.
5. Attention/Wellbeing subject drift fails closed without grant rewrite.
6. third-party Expert receives no automatic personal grant.
7. Feasibility different destination/query still requires separate contextual review.
8. paused standing personal grant blocks source read.
9. connection disconnect/revoke blocks all standing personal Observe.

Delete tests that require grant-owned selected_handles for Contacts.

## Residual audit

Search:

~~~
selected_handles
personal_grant_consumer_policy
reviewed_subject
source_and_scope
ATTENTION_RESOURCE
PEOPLE_RESOURCE
WELLBEING_RESOURCE
~~~

Retain only legitimate provider/resource constants and Feasibility contextual records. Standing resource selection must have one owner.

## Verification

~~~
cargo test -p floe-connections
cargo test -p floe-access personal
cargo test -p floe-context personal
cargo test -p floe-vault personal
cargo test -p floe-app personal
(
  cd apps/client
  flutter test test/features/connections
)
python3 tools/architecture/check_boundaries.py
git diff --check
~~~
