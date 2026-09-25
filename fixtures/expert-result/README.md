# Expert result interoperability sample

`schedule-v1.json` comes from `VaultStatefulExpertSettlement::settle` in the
Schedule settlement test. The test parses the production `BuiltinExpertOutput.data`,
normalizes only UUID/person/source-handle and expiry identity/clock values, then
serializes the result once and compares it byte-for-byte with this file.

To inspect a regeneration candidate from the production path:

```sh
FLOE_PRINT_EXPERT_RESULT_FIXTURE=1 cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency --lib -- --nocapture
```

Copy only the `EXPERT_RESULT_FIXTURE=` value into `schedule-v1.json` after
reviewing semantic changes. The ordinary read-only drift check is:

```sh
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency --lib
```

Flutter tests in `apps/client` load the same tracked bytes via
`test/support/expert_result.dart`.
