# Implement encrypted Experts binding and registry storage

S2 is authorized after G1. Source only. No formatter, compiler, build, tests, checker, dependency resolution, commit or publication. Read root AGENTS and canonical/Rust/Vault cutover plans.

## Exclusive files and constructors

Create only:

- crates/adapters/vault/src/vault/expert_binding_reviews.rs
- crates/adapters/vault/src/repositories/expert_binding_reviews.rs
- crates/adapters/vault/src/repositories/expert_registry.rs

Native coordinator owns vault.rs/lib.rs/repositories module exports and existing vault/registry.rs. Report export/visibility needs rather than editing those. Existing registry_on(connection) is pub(super). Native can expose registry_payload/update_registry physically when needed; do not duplicate owner registry policy.

Implement VaultExpertRegistryRepository<Keys>::new(vault:Arc<EncryptedAgentVault<Keys>>,actor:OwnerActor)->Result<Self,AgentFailure> and VaultExpertBindingReviewRepository<Keys> with the same constructor. These implement the exact RegistryRepository and BindingReviewRepository ports frozen under modules/experts/src/ports/. Actor must equal the constructor's ready-generation identity for all calls; Person must equal actual Vault person. Every operation checks actual Vault access before and after I/O. No raw keys, credentials, service locator, or business callbacks.

## Owner helpers

The binding service cloud lane produces pure Experts helpers with exact signatures:

- validate_binding_review_descriptor(&BindingReviewDescriptor)->Result<(),AgentFailure>
- binding_review_digest(&BindingReviewDescriptor)->Result<[u8;32],AgentFailure>
- project_binding_review(&BindingReviewDescriptor,now_unix_ms:i64)->Result<BindingReview,AgentFailure>
- project_expert_directory(&RegistrySnapshot,PersonId)->Result<ExpertDirectorySnapshot,AgentFailure>
- project_binding_mutation_receipt(&BindingReplacementReceipt,&BindingReviewDescriptor)->Result<BindingMutationReceipt,AgentFailure>

Native has wired these exports. Use descriptor and mutation-receipt validation; do not reproduce semantic digest/choice/projection logic in Vault. The mutation helper verifies the exact reviewed assignment, package/definition, resulting selection and command digest, with the stored commit timestamp strictly before expiry. Available local Tasks/Memory candidates legitimately have no Connections source expectation; validate supplied expectations without inventing a source authority for them. Use AgentRegistry::restore to validate complete owner-produced registry state and preserve every registry/private-state invariant. Storage must not discover candidates, reselect packages, alter bindings, grant source access or call providers.

## Physical storage and transactions

Use encrypted tables beside agent_expert_registry. Immutable prepare descriptors are keyed by review ID and unique (Person,command ID), with exact Person/device, assignment, requirement, digest and payload. Replacement receipts are keyed by actual command ID and uniquely by consumed review ID. Registry command receipts retain immutable request digest and original acknowledged snapshot. Bound total Experts command/review admissions to 4096, with explicit BudgetExceeded before admission; never silently evict receipts or truncate recovery evidence.

A command UUID reused across Experts prepare/replacement/configuration families conflicts. Exact replay is resolved before fresh revision/expiry checks. It returns the original receipt/descriptor even if later registry state changed; no new review ID, timestamp, candidate map or revision is allocated.

New Vault creation calls initialize_expert_binding_reviews(); existing Vault open calls validate_expert_binding_reviews(), which never creates missing receipt tables. All mutation paths validate the current schema. Missing or partially present command/review tables are unavailable evidence, never fresh state.

RegistryRepository::read returns actual stored state. If this is a valid fresh Vault with no registry yet, return the owner-created empty AgentRegistry::new(actual registry_instance_id).snapshot() without writing. Registry commit of the first owner-produced bundle must atomically initialize the registry and existing Vault identity version transition exactly as the current initialization code does. Never infer missing state from corruption or a failed read, overwrite uncertain rows, or reset a Vault.

RegistryRepository::commit runs one short immediate transaction: exact existing command replay or cross-family conflict, expected registry revision check, owner snapshot validation (same actual registry instance/Person), physical revision CAS, command receipt write, access check and commit. The caller already ran the pure configuration/install policy. Preserve existing aggregate revision and private state CAS guarantees.

BindingReviewRepository::prepare uses one transaction to rejoin identical immutable BindingPrepareIdentity or create the validated descriptor. Compare exact immutable descriptor on same identity when no original prepare receipt is available; changed input conflicts. get/find_prepare read only stored descriptors after actor and digest equality. Expiry does not prevent inspection of historical immutable evidence.

commit_replacement uses one immediate transaction for all of: exact command replay, exact actor/review descriptor/ref match, review unconsumed or identical consumed receipt, expected binding revision, expected registry revision, owner-produced next RegistrySnapshot CAS, consumed-review linkage and immutable BindingReplacementReceipt. Validate that the next assignment/binding and the stored reviewed candidate mappings agree with the owner-supplied selected refs. Use owner validation functions for semantic checks. A selected unavailable candidate cannot gain new authority. Same successful command returns its original receipt after lost acknowledgement without reapplying the binding.

The receipt's committed_at_unix_ms is the owner-supplied actual commit observation retained with the first commit. Require creation <= commit time < review expiry on first application, and preserve that original timestamp on replay. BindingMutationReceipt is a safe projection of this actual receipt: command_id from receipt.registry.command_id; review_ref unchanged; assignment_ref from the stored descriptor identity.assignment_id; binding_revision from the receipt's acknowledged assignment.binding.revision; registry_revision from receipt.registry.snapshot.revision; committed_at_unix_ms unchanged. Validate the matching assignment exists and belongs to the exact stored Person/installation/package/definition. Never guess an assignment or infer completion from pending state.

Context candidate/source expectations are read and checked by Experts before this encrypted transaction. There is no atomic source+registry transaction across Turso metadata and encrypted Vault. Do not perform source/provider I/O under SQL or claim such atomicity. Any later source drift is reauthorized at actual source read/model dispatch; binding configuration never grants Observe.

## Conversation transaction read helpers

In the Vault impl expose these pub(crate) methods, accepting &turso::Connection so existing Transaction deref works:

read_binding_review_on(&self,connection:&turso::Connection,device_id:&str,reference:&BindingReviewRef)->Result<BindingReviewDescriptor,AgentFailure>

read_binding_review_receipt_on(&self,connection:&turso::Connection,device_id:&str,reference:&BindingReviewRef)->Result<Option<BindingReplacementReceipt>,AgentFailure>

They perform same-transaction immutable reads and exact actual Vault Person/device/ref/digest validation. The receipt helper follows unique consumed-review linkage to the actual command receipt. An unconsumed review returns None. A consumed review with missing/mismatched receipt is corruption, not None. They do not apply reviews, advance state, refresh catalogs, or infer permission. Conversation uses them to authenticate atomic blocked audits and historical owner resolution.

Report useful results promptly, then exact files/export/visibility needs. No checks or commits.
