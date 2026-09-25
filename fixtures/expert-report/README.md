# Delegation product-boundary fixture

`delegation-v1.json` is the completed Schedule delegation produced by the Rust settlement test. The test runs the Schedule-owned assessment and Actions-owned proposal through `VaultStatefulExpertSettlement`, a canonical `ExpertReport`, `TaskCoordinator` with the durable Vault Task repository, and `task_receipt_to_a2a` before serializing the session delegation message. Only UUIDs and proposal timestamps are normalized; no product fields are reconstructed in Dart.

The ordinary test compares the projected JSON byte-for-byte with the tracked fixture:

```sh
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency --lib
```

To print a regeneration candidate after reviewing a contract change:

```sh
FLOE_PRINT_EXPERT_REPORT_FIXTURE=1 cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency --lib -- --nocapture
```

Copy only the `DELEGATION_FIXTURE=` JSON value into `delegation-v1.json` and rerun both the Rust test and the Flutter fixture test. The Flutter parser reads these exact tracked bytes.
