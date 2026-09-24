# Checkpoint 04-A — Product policy and first-party consumer authority

- **Baseline:** main at 833b9f191fa6d1c14a11efa07f6f382e48627d9d
- **Depends on:** Checkpoint 03 complete
- **Goal:** fix the durable product decision first, then make App composition the one source of default first-party Observe policy.
- **Scope guard:** do not change connection UI except compile fallout.

---

## 1. Current anchors

| Surface | Symbol / current behavior | Problem |
|---|---|---|
| docs/decisions/0028-pairing-integrated-authority-and-connection-permissions.md | all Observe starts denied | conflicts with new concrete-connection ceremony |
| docs/product/integrations-and-privacy.md | successful read connection does not imply AI use | needs Observe vs external-processing clarification |
| crates/app/src/vault_host/calendar_access.rs | calendar_first_party_consumers() | good App-composition precedent, Calendar-only |
| crates/experts/builtin/src/catalog.rs | BuiltinExpertKind declarations / required_sources | source-to-built-in reader inventory |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | read_source_view(... request.agent_id ...) | remote built-ins use actual package identity |
| same | ASSISTANT_CONSUMER / personal reader paths | root/legacy reader identities are real current readers |
| crates/experts/builtin/src/relationships/dispatch.rs | contacts.expert | current Contacts Expert reader |
| crates/experts/builtin/src/focus_attention/dispatch.rs | attention.expert | current Attention Expert reader |
| crates/modules/context/src/lib.rs | ASSISTANT_CONSUMER | current general-assistant identity |
| apps/client/.../agent_personal_access_settings.dart | selectedConsumers | client chooses source consumers |
| crates/bindings/protocol/src/dto/agent.rs | Personal/Contacts Review consumers | consumer policy crosses product wire |
| apps/client/.../server_connector_panel.dart | consumer picker | raw remote consumer choice |
| same | _remoteViewsFor | source-product policy lives in Flutter |

Symbols are authoritative. Re-find current line numbers during implementation.

---

## 2. Amend the durable decision before code

Amend ADR 0028 rather than adding a competing ADR.

Required decision:

### Pairing vs connector connection

Pairing itself still grants **zero connector Observe**.

A concrete first-party source/account connection is a separate user action. Once it completes with current resource selection, Floe creates/reviews its default first-party Observe policy.

~~~text
pair device/server
  -> no connector grant

connect Gmail / Calendar / Contacts / etc.
  -> source/account/system authority
  -> selected resources
  -> default first-party Observe review
~~~

### Resource edits

Replace the old rule that source selection never expands grants with:

- if Use with Floe is active, explicit resource selection and Observe scope converge as one product interaction;
- if Use with Floe is paused, resource selection may change while Observe remains paused;
- provider-side changes the Person did not initiate never silently widen a grant.

### External processing

State explicitly:

- default Observe does not approve a new external model recipient;
- exact-recipient consent remains separate and fail-closed;
- Checkpoint 05 provides contextual interaction for missing recipient consent.

### Act

State explicitly:

- connection/default Observe does not grant write/send/create authority.

Update product privacy language in the same semantic commit.

---

## 3. Canonical policy owner

Introduce or expand one App composition policy with semantics such as:

~~~text
FirstPartyObservePolicy
  connector/source
  expected view/resource families
  exact first-party consumers
  data categories
  purpose
  source-transport processing requirement
~~~

This is product-composition data, not durable user state.

Requirements:

- App composition may depend on product declarations and Access contracts;
- Access/Vault does not know BuiltinExpertKind;
- Flutter does not know approved consumers;
- server connector catalog does not decide Expert/model consumers;
- no duplicate consumer list across App files.

Extend the Calendar composition precedent instead of adding parallel UI helpers.

---

## 4. Actual consumer inventory

Policy must be derived from current production readers, not imagined future consumers.

### Calendar

Keep the Checkpoint 02/03 rule:

~~~text
BuiltinExpertKind::ALL
  -> declaration.required_sources contains Calendar
  -> GrantConsumer::builtin(package_id)
~~~

Tests derive the set rather than duplicating it.

### Remote built-in views

Production read_source_view passes request.agent_id. Map logical view to BuiltinContextSource in App:

~~~text
mail.communication -> Mail
work.context        -> WorkContext
life.logistics      -> Logistics
~~~

Derive exact package ids whose declarations use that source and verify each has a current production read path.

Do not grant assistant merely because old Flutter defaulted to it.

### Device personal sources

Inventory current readers separately:

- general assistant only where a real root path reads the source;
- Relationships current Contacts reader;
- FocusAttention current Attention reader;
- current Wellbeing/Feasibility readers.

Checkpoint 04 does not require gratuitous renaming of every legacy internal consumer string. But:

- UI may no longer choose these identities;
- mapping to a product source lives in one composition policy;
- if a consumer is only an obsolete proxy, migrate runtime + policy together instead of preserving a permission alias.

Report every intentionally retained non-package identity.

### Third-party

No GrantConsumer::Extension is default-added.

Unknown built-in consumer strings are rejected.

---

## 5. Connector/view capability policy

Move remote view policy out of Flutter.

The baseline UI mapping is incomplete and cannot be authority. Gmail already exposes more than mail.communication in server descriptors.

Implementation sequence:

1. inventory current server connector descriptors/capabilities;
2. intersect with Floe-supported logical source views;
3. intersect with at least one current first-party reader;
4. produce a bounded deterministic expected source-view set;
5. reject unknown connector/view combinations.

Use exact current connector ids from server source. Do not perpetuate stale Flutter aliases.

Do not auto-grant low-level views such as mail.body unless a current canonical Floe source path actually uses them.

Add coverage for Gmail, Microsoft Mail, current work-context connector ids, Home Assistant, Google Calendar, and Microsoft Calendar.

---

## 6. Remove caller-supplied consumer policy from target contracts

Target product commands do not accept:

~~~text
consumers
consumer
GrantScope
purpose
processing restriction
~~~

from Flutter.

Migration targets:

- PersonalAccessChangeDto::Review.consumers;
- ContactsAccessChangeDto::Review.consumers;
- AgentPersonalAccessGateway review consumer arguments;
- Attention consumer checkboxes;
- remote view preview/review consumer input.

Internal Access functions may still take validated consumer/scope values produced by App.

---

## 7. Processing boundary

Keep two meanings separate:

1. source acquisition may require an exact producer/recipient boundary;
2. sending source-backed evidence to an external model is Inference recipient authority.

Preserve source-transport restrictions required by signed remote source access.

Do not interpret default connection Observe as model-provider approval.

Add a focused test proving connection/default Observe leaves external-model consent unchanged.

---

## 8. Tests

### Policy derivation

- every default consumer corresponds to a current production reader;
- Calendar consumer set derives from declarations;
- remote view consumer set derives from actual package readers;
- unknown/third-party consumer excluded;
- supported connector capability with no Floe reader is not auto-granted;
- server connector capability drift cannot silently auto-grant a new logical view.

### Drift protection

Tests fail if:

- a new built-in starts reading a source but policy coverage is absent;
- a removed reader remains in hard-coded policy;
- a new remote view becomes default-granted without explicit product mapping.

---

## 9. Residual gate

~~~sh
rg -n 'selectedConsumers|_remoteViewsFor|calendar_first_party_consumers|consumers.*Review|consumer.*Dropdown' apps/client crates/app crates/bindings
~~~

At 04-A exit, migration-target caller consumer fields may remain only if explicitly listed for 04-C/04-D.

There must be exactly one production App entry point for default first-party Observe policy.

---

## 10. Verification

~~~sh
cargo check -p floe-app -p floe-access -p floe-experts-builtin
cargo test -p floe-app
cargo test -p floe-access
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Use actual package names if they differ.

Do not proceed to 04-B if App cannot derive exact policy without asking Flutter for consumers.
