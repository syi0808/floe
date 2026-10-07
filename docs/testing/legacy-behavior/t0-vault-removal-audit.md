> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 Vault surgical removal audit

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Every checked Vault source/manifest byte matches this baseline. This is a static removal plan only; no test/source/manifest/fixture deletion or execution gate was performed. The coordinator retains ownership of the frozen main plan and the document-before-delete decision.

## Result

- 142 actual registrations, 142 indexed V IDs and 142 unique named ledger rows. No missing/duplicate cases or source-hash drift.
- 18 `cfg(test)` items: eight inline modules, seven external-module registrations and three standalone helpers. Eight whole test/support files and one dev-dependency section are separately identified: 27 removal operations.
- The JSON companion contains every V case's exact item start/end, first/last anchors, ledger line, owning removal operation, all contained helper declarations, per-reference dev-dependency evidence and a hash freeze of all Vault files.
- This review checks complete case registration and deletion/support boundaries. It does not rerun or replace the coordinator's deep semantic behavior review and claims no test pass.

## Concrete findings

- **VA-F01 (coordinator_status_reconciliation):** The ledger header still says "In progress; no test removal permitted from this partial ledger", while its coverage footer says all 142 registrations have named rows. The independent path/symbol/ID audit confirms complete registration coverage, not permission to override that stated hold. Coordinator should reconcile ledger status before actual deletion. This audit does not alter the ledger or authorize deletion.
- **VA-F02 (preservation_hazard):** remote_authority.rs test block ends at 2504. Keep live RemoteAuthorizationKeys documentation and implementation at 2506–2547; never delete from cfg(test) through EOF.
- **VA-F03 (preservation_hazard):** Remove agent_actions.rs forwarding-only cfg(test) helpers 463–477 and 479–495 as exact items. Keep live cancellation/fence dispatch at 497–572, its imports and all following production code; repositories/expert_actions.rs calls the live method at 59.
- **VA-F04 (shared_fixture_retention):** Keep producer-v1.json. The literal consumer graph is one Vault Rust case plus one Go test, not proof of additional Rust consumers. Vault-only T0 cannot delete the shared fixture.

## Exact removal operations

All spans are inclusive baseline line numbers. Remove complete items, not surrounding production ranges. First/last anchors below are literal source; JSON includes wider three-line anchors and adjacent preserved lines. A final `}` anchor is meaningful only together with its frozen path/hash/span.

| ID | Path within `crates/adapters/vault/` | Inclusive span | Kind | First / last source anchor |
|---|---|---:|---|---|
| VR01 | `src/repositories/connections.rs` | 169–320 | inline_test_module | `#[cfg(test)]` / `}` |
| VR02 | `src/vault/access_grants.rs` | 1094–1890 | inline_test_module | `#[cfg(test)]` / `}` |
| VR03 | `src/vault/calendar_grants.rs` | 337–1522 | inline_test_module | `#[cfg(test)]` / `}` |
| VR04 | `src/vault/context_cleanup.rs` | 676–1398 | inline_test_module | `#[cfg(test)]` / `}` |
| VR05 | `src/vault/context_dependencies.rs` | 313–1427 | inline_test_module | `#[cfg(test)]` / `}` |
| VR06 | `src/vault/keyring.rs` | 83–138 | inline_test_module | `#[cfg(test)]` / `}` |
| VR07 | `src/vault/recipient_consents.rs` | 336–492 | inline_test_module | `#[cfg(test)]` / `}` |
| VR08 | `src/vault/remote_authority.rs` | 1466–2504 | inline_test_module | `#[cfg(test)]` / `}` |
| VR09 | `src/lib.rs` | 12–13 | test_module_registration | `#[cfg(all(test, unix))]` / `mod test_expert_registry;` |
| VR10 | `src/repositories/conversation.rs` | 959–960 | test_module_registration | `#[cfg(test)]` / `mod tests;` |
| VR11 | `src/repositories/task.rs` | 153–154 | test_module_registration | `#[cfg(test)]` / `mod tests;` |
| VR12 | `src/vault/conversation_interactions.rs` | 706–707 | test_module_registration | `#[cfg(test)]` / `mod tests;` |
| VR13 | `src/vault/conversations.rs` | 1555–1556 | test_module_registration | `#[cfg(test)]` / `mod tests;` |
| VR14 | `src/vault/tasks.rs` | 626–627 | test_module_registration | `#[cfg(test)]` / `mod tests;` |
| VR15 | `src/vault.rs` | 799–800 | test_module_registration | `#[cfg(test)]` / `mod synthetic_tests;` |
| VR16 | `src/vault.rs` | 404–409 | test_only_helper | `#[cfg(test)]` / `}` |
| VR17 | `src/vault/agent_actions.rs` | 463–477 | test_only_helper | `#[cfg(test)]` / `}` |
| VR18 | `src/vault/agent_actions.rs` | 479–495 | test_only_helper | `#[cfg(test)]` / `}` |
| VR19 | `src/repositories/conversation/tests.rs` | 1–3089 | whole_test_file | `use std::{` / `}` |
| VR20 | `src/repositories/task/tests.rs` | 1–143 | whole_test_file | `use std::{` / `}` |
| VR21 | `src/test_expert_registry.rs` | 1–73 | whole_test_support_file | `use floe_agent_contract::{` / `}` |
| VR22 | `src/vault/conversation_interactions/tests.rs` | 1–835 | whole_test_file | `use std::{` / `}` |
| VR23 | `src/vault/conversations/tests.rs` | 1–719 | whole_test_file | `use std::{` / `}` |
| VR24 | `src/vault/synthetic_tests.rs` | 1–81 | whole_test_file | `use super::*;` / `}` |
| VR25 | `src/vault/tasks/tests.rs` | 1–326 | whole_test_file | `use std::{` / `}` |
| VR26 | `tests/agent_vault.rs` | 1–1810 | whole_test_file | `#![cfg(unix)]` / `}` |
| VR27 | `Cargo.toml` | 44–48 | dev_dependency_section | `[dev-dependencies]` / `tokio.workspace = true` |

## Production boundaries that must remain

- `remote_authority.rs`: preserve lines 1–1465 and 2505–2547. In particular, docs 2506–2508 and `impl<Keys: VaultKeyProvider> floe_access::RemoteAuthorizationKeys for EncryptedAgentVault<Keys>` at 2509 implement four live Access methods. The test module is not the file tail.
- `agent_actions.rs`: remove only 463–477 and 479–495. Preserve `admit_agent_action_dispatch_with_cancellation_and_fence` at 497–572, every later production item and required imports. This path checks identity/digest/state/approval/expiry, automatic-action policy, task and dependency authority, cancellation and the caller fence inside its transaction.
- `repositories/expert_actions.rs` lines 51/59, Actions port line 154 and Actions application line 272 are live _and_fence consumers. The App test caller at `crates/app/src/vault_host/tests/expert_actions.rs:704` belongs to a different audit and does not make the live method removable.
- `vault.rs`: retain live `resume_session` through 402, `check_access` from 411 and `insert_session` from 415; remove only the sample-session test helper and the final synthetic-test registration.
- Retain all other production source, production/target-specific dependency sections, platform gates, package membership, security protocol constants, crypto/keyring implementations and source-owned storage checks. Test deletion does not authorize an S1/S2 production deletion.

## Harness and support closure

- Cargo implicitly discovers `tests/agent_vault.rs`; there is no explicit `[[test]]`, `autotests` override, example, bench, test feature or build script in this crate. Removing that file removes the implicit target. Do not introduce policy changes such as `autotests = false` or `doctest = false`.
- V141 invokes the same test binary with `--exact vault_lock_child`; V142 is an actual registration, not an extra standalone target. It returns early without `FLOE_TEST_VAULT_ROOT`; when launched by V141 it uses that temporary root and `FLOE_TEST_VAULT_PERSON`. Both disappear with the same whole file. No subprocess was run.
- The six external test-suite source files and `test_expert_registry.rs` are owned only by the seven listed module registrations. Their imports, mocks, key providers, fixture constructors, model/tool ports and helper methods disappear with their containing files.
- `test_expert_registry::install` consumers are repository Task line 82 and Vault Task lines 77, 196, 236. The helper builds a synthetic Schedule registry bundle. No production consumer exists.
- `create_sample_session` consumers are repository Conversation line 529 and synthetic-memory line 52. The two action helper declarations form only a forwarding chain; the first has no callers in the scanned source, the second is called only by the first.
- All inline test helpers are contained by the exact balanced modules. Calendar authorization/fresh-grant helpers deliberately ignore supplied source/resource/fingerprint arguments; unconditional Context/Conversation resolvers are fixture support. These limitations remain in the behavior ledger and are not promoted into target policy.
- No Vault Rust doc code fences/doctest examples or other test-registration mechanisms were found. General production documentation remains unchanged.

## Dependency and fixture decisions

| Vault dev dependency | Manifest line | Source references | Retained production uses after proposed removal |
|---|---:|---:|---:|
| `floe-agent-runtime` | 45 | 6 | 0 |
| `floe-inference` | 46 | 2 | 0 |
| `tempfile` | 47 | 54 | 0 |
| `tokio` | 48 | 134 | 0 |

All four entries and the `[dev-dependencies]` header are lines 44–48 and can be removed only with all consuming suites. Keep production lines 1–42 unchanged. Do not edit Cargo.lock manually or invoke Cargo resolution at T0.

`fixtures/remote-authorization/producer-v1.json` remains unchanged. Exact tracked source consumers:

- Vault `src/vault/remote_authority.rs:1814–1815`, case V111, removed only as part of the inline test module.
- `server/internal/application/producer_identity_test.go:107`, `TestSharedProducerFixtureRejectsNegativeCases`; its `../../..` path resolves to the same root fixture. This Go consumer is outside Vault scope.

No other Vault `include_str!`/`include_bytes!`/`include!` source fixture consumer was found. No Vault-only orphan decision can authorize deleting this shared security fixture.

## Coverage by test-containing/support source

| Source within Vault | Registered cases | Owning removal |
|---|---|---|
| `src/repositories/connections.rs` | V001–V003 (3) | VR01 |
| `src/repositories/conversation/tests.rs` | V004–V023 (20) | VR19 |
| `src/repositories/task/tests.rs` | V024 (1) | VR20 |
| `src/test_expert_registry.rs` | support only (0) | VR21 |
| `src/vault/access_grants.rs` | V025–V035 (11) | VR02 |
| `src/vault/calendar_grants.rs` | V036–V055 (20) | VR03 |
| `src/vault/context_cleanup.rs` | V056–V061 (6) | VR04 |
| `src/vault/context_dependencies.rs` | V062–V080 (19) | VR05 |
| `src/vault/conversation_interactions/tests.rs` | V081–V090 (10) | VR22 |
| `src/vault/conversations/tests.rs` | V091–V099 (9) | VR23 |
| `src/vault/keyring.rs` | V100–V102 (3) | VR06 |
| `src/vault/recipient_consents.rs` | V103–V105 (3) | VR07 |
| `src/vault/remote_authority.rs` | V106–V118 (13) | VR08 |
| `src/vault/synthetic_tests.rs` | V119 (1) | VR24 |
| `src/vault/tasks/tests.rs` | V120–V122 (3) | VR25 |
| `tests/agent_vault.rs` | V123–V142 (20) | VR26 |

## Static checks and execution handoff

- Reconciled all path/symbol/attribute/declaration tuples, V IDs and named ledger rows; checked every Vault file against the frozen baseline and every indexed file hash/line count.
- Balanced all eight inline modules, three standalone helpers and all 142 test functions with comment/string-aware lexical scanning, then inspected removal boundaries. The exact JSON spans are evidence, not a claim that a compiler/parser ran.
- Scanned dependency uses and shared source consumers. An in-memory-only deletion simulation leaves zero Rust test registrations, cfg(test) declarations, deleted helper names or Vault dev-dependency references; no source output was written.
- Before actual removal, coordinator must resolve the ledger header hold, recheck hashes, remove exact items bottom-up and pair source-file deletions with module registrations and manifest changes. Preserve the remote-authority production tail and live fenced dispatch.
- Only the audit JSON and this Markdown were written. No compiler, build, formatter, test, fixture generator, architecture checker, Cargo resolution, database/keychain access, user-data reset or source deletion was performed.
