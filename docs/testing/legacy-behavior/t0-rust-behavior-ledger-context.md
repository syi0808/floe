> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# Floe Context test behavior ledger

## Baseline and method

- Product baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Execution branch: `refactor/architecture-20261002`; audit HEAD `a1cee05c98d0fc503469dccbe686e526b52fce71`. Documentation work is concurrent; this was not a clean detached audit checkout.
- 150 actual registered tests: 96 inline + 54 integration, across24 test-bearing files. Stable CTX IDs and source spans retained.
- All150 registered bodies and their helper/fixture semantics manually reviewed. Exact source/assertion sites and per-assertion target dispositions: [native audit](artifact-index.md#unpublished-artifacts).
- Static evidence only. No tests, formatter, compiler, build, architecture checker, source removal, runtime probes or external actions. Removal is a later coordinated gate.
- Source-file and registration-span hash mismatches: 0 / 0.
- 194 named scenario entries are documentation, not registrations or a universal literal-table count.

## Architecture and evidence boundaries

ADR0034 retires exact model-recipient consent and accepts Access-owned DeviceOnly/GatewayAllowed source processing. Health needs a separate source-owned local semantic transform before local or Gateway reasoning, keeps HighlySensitive, and fails closed. These are target requirements, not baseline runtime claims.

Route-free projection and catalog derivation are independently durable; absence of obsolete consent/route keys is not itself obsolete. Transport ContextTransferClass::DeviceOnly is a different concept from source-processing DeviceOnly. Synthetic signed-preview, native-driver, grant and repository fakes do not establish real-provider, cryptographic or durable-storage behavior.

## Test/support boundaries

- Cargo autotests=false and explicit integration harness includes5 suites plus shared support.
- TestTimelineRepository is shared only by calendar_timeline/native_context integration suites. No external fixture files or production imports of that test helper were found.
- Later removal must reconcile the integration target, test-only floe-kernel dev dependency and tokio test-util feature without removing production tokio.
- calendar_lease.rs cfg(test) import9-10 and remote_sources.rs cfg(test) import28-29 are separate test-only items. coverage.rs production message_coverage442-468, observations.rs valid_native_subject_fingerprint747-752 and source_view.rs production trait impls469-485 lie AFTER test modules and must survive any removal.

## Per-file registration totals

- `crates/modules/context/src/application/archive.rs`: 3; SHA-256 `59e328b6f64e64f9106e8a7c3170d67db65d479673dd1237886cf7d0171267cc`
- `crates/modules/context/src/application/calendar_connector.rs`: 3; SHA-256 `030eddcbe6610f897d37869b9b8f292197ffdb431a3d1de8e23db4e2dbbddf96`
- `crates/modules/context/src/application/calendar_lease.rs`: 2; SHA-256 `5b22d996c19039c032ccba030c85ff2793edb0f734c186fac16dfd74b2fb17d0`
- `crates/modules/context/src/application/consumed.rs`: 3; SHA-256 `165cb2cf1feefbd8b322644f3e5f4ec93fe17ab5ea2ecabcd252c025ced76307`
- `crates/modules/context/src/application/coverage.rs`: 5; SHA-256 `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`
- `crates/modules/context/src/application/expert_sources.rs`: 4; SHA-256 `b4a1ee060e2f4a7bef4c3a7453b74027a9e1e9cdd6932541e4c6943a6d8b6eb7`
- `crates/modules/context/src/application/history.rs`: 5; SHA-256 `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`
- `crates/modules/context/src/application/leases.rs`: 6; SHA-256 `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`
- `crates/modules/context/src/application/model_coverage.rs`: 3; SHA-256 `a18e44ab93ce424b457c88da1d44f75f3eb282804eeb4a4b2b1d006c112fe215`
- `crates/modules/context/src/application/model_projection.rs`: 16; SHA-256 `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`
- `crates/modules/context/src/application/observations.rs`: 2; SHA-256 `ef20c51b06bfc43a3aadf3575debbad02610fc90aa3ccdea93819c9bc22b1675`
- `crates/modules/context/src/application/personal_lineage.rs`: 1; SHA-256 `87e282ca4700529d8ee9aa9e9e02a332c9262ab7e7538675f942fab8fb421bc7`
- `crates/modules/context/src/application/personal_sources.rs`: 3; SHA-256 `8df0675cd9f2f15bdb32650e27d443b1b034e9674bc3aa54742071a79595e65a`
- `crates/modules/context/src/application/projection.rs`: 4; SHA-256 `a644ee98c0d5839affb698ab9ef9c886c70090569e92a20b77b5a55736a40ce3`
- `crates/modules/context/src/application/remote_sources.rs`: 14; SHA-256 `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`
- `crates/modules/context/src/application/remote_views.rs`: 2; SHA-256 `4607244824ad3e727b1bfdb62ba2607ef092a5d2e9e70913f26ace378a135b3b`
- `crates/modules/context/src/application/service.rs`: 5; SHA-256 `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`
- `crates/modules/context/src/application/source_candidates.rs`: 7; SHA-256 `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`
- `crates/modules/context/src/application/source_view.rs`: 8; SHA-256 `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`
- `crates/modules/context/tests/calendar_timeline.rs`: 23; SHA-256 `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`
- `crates/modules/context/tests/native_calendar_read.rs`: 13; SHA-256 `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`
- `crates/modules/context/tests/native_context.rs`: 3; SHA-256 `a1aeeefc29d0a7f318432a7a57689ce738fac5731f47d6c0faee750744196977`
- `crates/modules/context/tests/policy.rs`: 6; SHA-256 `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`
- `crates/modules/context/tests/routing.rs`: 9; SHA-256 `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`

## Registration-level ledger

### CTX-001 — `projects_whole_turns_and_excludes_unknown`

- Baseline: `crates/modules/context/src/application/archive.rs:151-165`; `#[tokio::test]`; unit module.
- Span SHA-256: `ce6eb48a8c688346a9924598f343b6e496a0f2fa3635fd8bc2fd73d8d1f65fbe`; file SHA-256: `59e328b6f64e64f9106e8a7c3170d67db65d479673dd1237886cf7d0171267cc`.
- Preconditions: The older/first turn contains `question` with Independent coverage and `private` with Unknown coverage; the later/second turn contains `later` with Independent coverage.
- Inputs/actions: Read bounded archive under an allow-all callback; Independent/Unknown coverage does not require calling that authorizer.
- Exact behavior and limits: Two first-turn messages (Independent question, Unknown private) followed by one Independent second-turn later message; archive read with allow-all returns exactly one message from the second turn. No assertion that first is newest.
- Named scenarios:
  - Older first turn: Independent `question` + Unknown `private` => omit the entire turn.
  - Later second turn: Independent `later` => retain this turn and message.
- Helpers/fixtures: Reader fake; fixture(messages); message(turn_id, text, coverage).
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-001`.

### CTX-002 — `rechecks_dependent_authority_before_returning_text`

- Baseline: `crates/modules/context/src/application/archive.rs:167-213`; `#[tokio::test]`; unit module.
- Span SHA-256: `710d5cc0e4b23b7e4c09dfaf1715ccd157f95c53bdd8b4636cf09c416bb90d80`; file SHA-256: `59e328b6f64e64f9106e8a7c3170d67db65d479673dd1237886cf7d0171267cc`.
- Preconditions: Archive contains dependent text and the authority callback reports false for its dependency.
- Inputs/actions: Read the bounded archive and run the dependency recheck immediately before returning.
- Exact behavior and limits: One dependent derived-summary message, exact constructed source dependency, authorization callback false; successful archive snapshot has no messages. Source authority must be rechecked before exposing derived text.
- Helpers/fixtures: Reader fake; fixture; dependency callback.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-002`.

### CTX-003 — `byte_bound_never_splits_latest_turn`

- Baseline: `crates/modules/context/src/application/archive.rs:215-241`; `#[tokio::test]`; unit module.
- Span SHA-256: `cbe9fe058881d4e0fb1715ffe82fc636ccebf3fa53d31c01965d738f9f99dfa7`; file SHA-256: `59e328b6f64e64f9106e8a7c3170d67db65d479673dd1237886cf7d0171267cc`.
- Preconditions: The older turn has `old`; the newest turn has the two messages `한글` and `pair`.
- Inputs/actions: Set the archive byte bound to the serialized size of messages `[1..]` (the newest turn) and read the tail.
- Exact behavior and limits: Older old message then same newest turn containing 한글 and pair; max_bytes equals JSON message-array cost 2 + individual serialized message lengths + 1 comma; returns exactly two messages, both newest turn. This is one Korean message and one ASCII message, not two Korean messages.
- Named scenarios:
  - Newest turn contains `한글` and `pair`; serialized bound for that complete turn => both messages retained.
- Helpers/fixtures: Reader fake; fixture; message; serialized byte accounting.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-003`.

### CTX-004 — `source_identity_projects_without_a_mirror`

- Baseline: `crates/modules/context/src/application/calendar_connector.rs:302-330`; `#[test]`; unit module.
- Span SHA-256: `62b7a5e18dc54a06432c8887c00495eaab20aa91452ee6409938010a22358d82`; file SHA-256: `030eddcbe6610f897d37869b9b8f292197ffdb431a3d1de8e23db4e2dbbddf96`.
- Preconditions: Established EventKit selected Home source bound to the person and device; no mirror is supplied. No test assertion claims that source state is Ready before projection.
- Inputs/actions: Build the calendar connector projection from connection identity and current mirror status.
- Exact behavior and limits: Established device-owned EventKit selected Home source, no mirror; connector projection is Pending, connection_id calendar-source, person_id exact person, empty views, and SourceConnection equals clone made before projection.
- Helpers/fixtures: In-file connection, mirror-status, and connector projection fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-004`.

### CTX-005 — `foreign_device_cannot_project_calendar_source`

- Baseline: `crates/modules/context/src/application/calendar_connector.rs:332-350`; `#[test]`; unit module.
- Span SHA-256: `60aa2767bd39f3cdc147a55555cd12962aa4e9e51aae4fc68b779295bd7de295`; file SHA-256: `030eddcbe6610f897d37869b9b8f292197ffdb431a3d1de8e23db4e2dbbddf96`.
- Preconditions: EventKit SourceConnection is established for device-a; request is projected from device-b.
- Inputs/actions: Attempt calendar source projection for the foreign device.
- Exact behavior and limits: Source owned by device-a projected from device-b is InvalidObservation.
- Helpers/fixtures: In-file SourceConnection and projection request builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-005`.

### CTX-006 — `stale_mirror_status_cannot_expand_source_resources`

- Baseline: `crates/modules/context/src/application/calendar_connector.rs:352-396`; `#[test]`; unit module.
- Span SHA-256: `63d3739298e2d33ef079006f404ddcf3f8cb93f58dfe56c07e5db0dfa5033e42`; file SHA-256: `030eddcbe6610f897d37869b9b8f292197ffdb431a3d1de8e23db4e2dbbddf96`.
- Preconditions: Current physical source resource set is [home]; source has reviewed subject a*64. Mirror sync statuses contain home and work.
- Inputs/actions: Project current source and mirror status.
- Exact behavior and limits: Current source has only Home, current subject fingerprint, mirror status contains Home plus stale Work; projection is Ready with exactly one view. Test does not explicitly inspect the emitted view resource identity.
- Helpers/fixtures: In-file connection and mirror status fixtures; source-resource mapping.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-006`.

### CTX-007 — `retained_observation_requires_exact_dependency_identity`

- Baseline: `crates/modules/context/src/application/calendar_lease.rs:140-187`; `#[test]`; unit module.
- Span SHA-256: `fda3269233108a48d036b3ae7dedfe63d531b8c71b52676e2419d93099211f24`; file SHA-256: `5b22d996c19039c032ccba030c85ff2793edb0f734c186fac16dfd74b2fb17d0`.
- Preconditions: A retained observation is bound to a particular registry, dependency, person/process, and query fingerprint.
- Inputs/actions: Retain and fetch exact dependency; retry duplicate key; request with another registry and altered query fingerprint.
- Exact behavior and limits: Retained observation is readable; retaining same dependency again conflicts; a fresh registry cannot observe it (StaleContext); altered query fingerprint with other dependency identity unchanged is StaleContext.
- Helpers/fixtures: In-memory calendar observation registry and dependency/query fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-007`.

### CTX-008 — `retained_observation_expires_without_reopen_repair`

- Baseline: `crates/modules/context/src/application/calendar_lease.rs:189-205`; `#[tokio::test(start_paused = true)]`; unit module.
- Span SHA-256: `553df77969e9a9dadf5b513d8aad36c6924ccbfc30d7e958be99a8ee75c93b06`; file SHA-256: `5b22d996c19039c032ccba030c85ff2793edb0f734c186fac16dfd74b2fb17d0`.
- Preconditions: Observation TTL is 10 ms and Tokio time is paused.
- Inputs/actions: Retain, advance monotonic time 20 ms, then fetch without reopening.
- Exact behavior and limits: Retained observation deadline 10 ms, paused clock advanced 20 ms; read is StaleContext, without reopen repair.
- Named scenarios:
  - TTL=10 ms; advance=20 ms; fetch=>StaleContext.
- Helpers/fixtures: Paused Tokio clock; retained observation registry.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-008`.

### CTX-009 — `either_clock_expiry_or_changed_authority_blocks_without_erasing_lineage`

- Baseline: `crates/modules/context/src/application/consumed.rs:152-181`; `#[test]`; unit module.
- Span SHA-256: `908dcaaa25af10be48db5b507165c2658fb2ede23b910f034655146a8304e96f`; file SHA-256: `165cb2cf1feefbd8b322644f3e5f4ec93fe17ab5ea2ecabcd252c025ced76307`.
- Preconditions: Consumed context has dependency lineage, wall-clock expiry, monotonic deadline, and an authorization callback.
- Inputs/actions: Validate while live; then independently expire monotonic deadline, expire wall clock, or deny current authority.
- Exact behavior and limits: Recorded exact dependency and scope validate before expiry; monotonic deadline equality, wall-clock expires_at equality, or failed authority callback each yields StaleContext; dependency lineage remains present after failures.
- Named scenarios:
  - Before expiry and authority true => Ok.
  - At exact monotonic deadline => StaleContext.
  - At exact wall expires_at => StaleContext.
  - Authority false => StaleContext; lineage remains.
- Helpers/fixtures: ConsumedContext registry; dependency and GrantScope fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-009`.

### CTX-010 — `mismatched_scope_is_rejected_without_recording_lineage`

- Baseline: `crates/modules/context/src/application/consumed.rs:183-205`; `#[test]`; unit module.
- Span SHA-256: `f7600412780a95b7be2647f20ba29631df5a008eafa7b8452f68ab7ae0f050a5`; file SHA-256: `165cb2cf1feefbd8b322644f3e5f4ec93fe17ab5ea2ecabcd252c025ced76307`.
- Preconditions: GrantScope consumer differs from consumed-context consumer.
- Inputs/actions: Record the consumed dependency under mismatched scope.
- Exact behavior and limits: Scope with different-reader consumer cannot record dependency: InvalidInput, dependencies remain empty.
- Helpers/fixtures: ConsumedContext registry; dependency and scope fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-010`.

### CTX-011 — `repeated_observation_cannot_extend_the_consumed_deadline`

- Baseline: `crates/modules/context/src/application/consumed.rs:207-238`; `#[test]`; unit module.
- Span SHA-256: `2cc8a64a8420f9793426abfe3209e808e428844371320a87ea2d6c1d8cbfcbe5`; file SHA-256: `165cb2cf1feefbd8b322644f3e5f4ec93fe17ab5ea2ecabcd252c025ced76307`.
- Preconditions: Same observation key and lineage are recorded once with an original deadline.
- Inputs/actions: Repeat identical record, then attempt same observation with a later deadline; validate after original deadline; validate empty lineage.
- Exact behavior and limits: Same dependency/scope/deadline records idempotently; extending deadline by 20 seconds conflicts; one dependency remains and is stale at original deadline. Empty lineage validates without invoking authority callback.
- Named scenarios:
  - Identical dependency/scope/deadline => idempotent.
  - Same observation with deadline extended20s => Conflict.
  - At original deadline => StaleContext.
  - Empty lineage => Ok without callback.
- Helpers/fixtures: ConsumedContext registry; dependency/scope fixture; controllable clocks.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-011`.

### CTX-012 — `fresh_accumulator_establishes_coverage`

- Baseline: `crates/modules/context/src/application/coverage.rs:308-325`; `#[test]`; unit module.
- Span SHA-256: `a5682f26229fe4a8bf4eac71fa1f515ce0d784ed6c6c77f63324e6f8c4c91644`; file SHA-256: `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`.
- Preconditions: Two separate fresh CoverageAccumulator instances are used: one for Independent/Unknown transitions and one for the first dependent observation.
- Inputs/actions: Establish Independent on first accumulator and Dependent on separate accumulator; mark first Unknown and then record Independent again.
- Exact behavior and limits: Fresh accumulator starts Unknown; first host-independent fact establishes Independent; first host dependency establishes Dependent; after mark_unknown a later independent fact cannot erase Unknown.
- Helpers/fixtures: CoverageAccumulator; test-only dependency/registry fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-012`.

### CTX-013 — `rejected_merge_preserves_accumulated_coverage`

- Baseline: `crates/modules/context/src/application/coverage.rs:327-346`; `#[test]`; unit module.
- Span SHA-256: `5e5a670e10184632a3af967b07b04eeb73316246ba405022ad5fe07230041a3c`; file SHA-256: `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`.
- Preconditions: Accumulator holds one dependency. A conflicting copy changes query_fingerprint while keeping observation identity unchanged; this is not a turn record.
- Inputs/actions: Merge conflicting observation.
- Exact behavior and limits: Same observation with changed query fingerprint conflicts (ContextDependencyError::Conflict); prior Dependent coverage remains exactly unchanged.
- Helpers/fixtures: CoverageAccumulator and turn/dependency helper.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-013`.

### CTX-014 — `registry_rejected_result_merge_does_not_mutate_existing_result`

- Baseline: `crates/modules/context/src/application/coverage.rs:348-374`; `#[test]`; unit module.
- Span SHA-256: `f832a9d1556cacec2e59de62eab5321a7b4159065d783c8dc2e761c908373440`; file SHA-256: `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`.
- Preconditions: Registry holds one result dependency under exact turn/result IDs; incoming dependency uses the same observation ID but changed query_fingerprint.
- Inputs/actions: Attempt registry result merge.
- Exact behavior and limits: Registry result dependency merge with same observation/different fingerprint is InvalidInput; prior result remains Some(Dependent) and byte-equivalent semantic coverage.
- Helpers/fixtures: Test-only coverage registry and result constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-014`.

### CTX-015 — `registry_folds_multiple_messages_for_existing_turn`

- Baseline: `crates/modules/context/src/application/coverage.rs:376-400`; `#[test]`; unit module.
- Span SHA-256: `d1f9905bd54e7eeff56d571c6feeecd7d270e3dad661ac6932cac285f6458260`; file SHA-256: `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`.
- Preconditions: A turn already has Independent classification and receives more messages.
- Inputs/actions: Fold multiple message observations for that existing turn.
- Exact behavior and limits: Two non-user facts for same existing turn, initial Independent classification, fold to one Independent turn entry.
- Helpers/fixtures: Coverage registry; test-only turn/result builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-015`.

### CTX-016 — `failed_fold_does_not_publish_a_partially_classified_turn`

- Baseline: `crates/modules/context/src/application/coverage.rs:402-439`; `#[test]`; unit module.
- Span SHA-256: `038aafa485b9fcf9c693c743d22cbe02eb91a01b45a5c71877d8732beaa20243`; file SHA-256: `b54b15682015174aaa6795e11916806347c0db6d786a7977ab006de0c16223ae`.
- Preconditions: A fresh user-turn fact and an existing-turn fact lacking initial stored coverage form the first batch. Nil result and nil turn are separate later calls, not part of that first batch.
- Inputs/actions: Fold fresh+missing-existing-coverage batch; then separately record nil result ID and fold nil turn ID.
- Exact behavior and limits: Fresh user turn followed by existing turn with absent stored coverage returns VaultUnavailable and publishes no partial fresh-turn classification; nil result ID and nil turn ID separately return InvalidInput.
- Named scenarios:
  - Fresh fact + existing turn absent from initial coverage => VaultUnavailable; fresh not published.
  - record_result_independent with nil RESULT ID => InvalidInput.
  - fold_messages with nil TURN ID => InvalidInput.
- Helpers/fixtures: Coverage registry; test-only user-fact/turn builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-016`.

### CTX-017 — `duplicate_requirement_keys_keep_distinct_selected_refs`

- Baseline: `crates/modules/context/src/application/expert_sources.rs:306-345`; `#[tokio::test]`; unit module.
- Span SHA-256: `7956ab537842c788894d22d5137b36d13168b18cdbdb0d7f60c463aec08cb57a`; file SHA-256: `b4a1ee060e2f4a7bef4c3a7453b74027a9e1e9cdd6932541e4c6943a6d8b6eb7`.
- Preconditions: Two declared requirements share capability calendar.timeline but have distinct keys and selected resource refs calendar-a/calendar-b.
- Inputs/actions: Read once under each exact key using identical query contract.
- Exact behavior and limits: Requirements calendar_a/calendar_b share capability calendar.timeline but retain distinct calendar-a/calendar-b selected references; each read returns expected resource and floe.source.calendar access ID. Function name says duplicate keys, but keys actually differ.
- Named scenarios:
  - calendar_a => calendar-a.
  - calendar_b => calendar-b.
- Helpers/fixtures: SelectedProbeDriver; selected_reference; DeclaredSourceRequirement.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-017`.

### CTX-018 — `selected_remote_port_receives_exact_requirement_refs`

- Baseline: `crates/modules/context/src/application/expert_sources.rs:367-404`; `#[tokio::test]`; unit module.
- Span SHA-256: `d07dcff1d9e1fed047e57329ecb0ff19f3d893b8fd4c6bc477b6bd3360a0604c`; file SHA-256: `b4a1ee060e2f4a7bef4c3a7453b74027a9e1e9cdd6932541e4c6943a6d8b6eb7`.
- Preconditions: Two work.context requirements have distinct keys work_a/work_b and refs work-a/work-b; fake reader returns unavailable.
- Inputs/actions: Invoke each requirement through selected remote reader.
- Exact behavior and limits: work_a/work_b requirements share work.context but carry distinct work-a/work-b refs; remote fake returns Unavailable for each and records exactly [work-a, work-b].
- Named scenarios:
  - work_a forwards work-a.
  - work_b forwards work-b.
- Helpers/fixtures: SelectedProbeReader; SelectedProbeDriver; selected_reference.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-018`.

### CTX-019 — `declared_local_capability_selects_only_its_driver`

- Baseline: `crates/modules/context/src/application/expert_sources.rs:424-468`; `#[tokio::test]`; unit module.
- Span SHA-256: `6ce3bbbba301e77d07a1f85adedec947d38bdf0cea37051df291fed5e13c658b`; file SHA-256: `b4a1ee060e2f4a7bef4c3a7453b74027a9e1e9cdd6932541e4c6943a6d8b6eb7`.
- Preconditions: Local capability declaration maps calendar.timeline to one local expert driver; another key is undeclared.
- Inputs/actions: Read declared calendar key then request undeclared key.
- Exact behavior and limits: Declared calendar requirement selects Calendar driver once and returns Ready; undeclared other returns CapabilityDenied without second driver call.
- Helpers/fixtures: ProbeDriver; local source capability mapping.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-019`.

### CTX-020 — `undeclared_key_and_unbounded_query_fail_before_source_selection`

- Baseline: `crates/modules/context/src/application/expert_sources.rs:470-509`; `#[tokio::test]`; unit module.
- Span SHA-256: `1e8810641c025f9127cf651adae5e4426d1456fcc3d9e27a26d4440fde65cae2`; file SHA-256: `b4a1ee060e2f4a7bef4c3a7453b74027a9e1e9cdd6932541e4c6943a6d8b6eb7`.
- Preconditions: Requirement key is not declared; another request carries an excessive limit (100000).
- Inputs/actions: Submit both requests through source selection.
- Exact behavior and limits: Undeclared work under mail declaration yields CapabilityDenied; remote mail query limit 100000 yields InvalidInput before source selection. No real remote reader provided.
- Named scenarios:
  - Unknown key => CapabilityDenied before selection.
  - limit=100000 => InvalidInput before selection.
- Helpers/fixtures: SelectedProbeDriver; requirement/query fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-020`.

### CTX-021 — `reads_distinct_turns_in_input_order`

- Baseline: `crates/modules/context/src/application/history.rs:69-84`; `#[tokio::test]`; unit module.
- Span SHA-256: `54a2df0c17d07e3d64cbcff7cf11cebdb8430df4af089e4450ed81c7346ef2c7`; file SHA-256: `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`.
- Preconditions: Valid session; turn list is [first, second, first]; reader returns Independent.
- Inputs/actions: Read history coverage and inspect calls/map.
- Exact behavior and limits: History turn sequence first,second,first issues reads exactly [first,second] and returns two Independent entries.
- Named scenarios:
  - Input IDs [first, second, first] => calls [first, second].
- Helpers/fixtures: Reader fake with call log; UUID turn/session fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-021`.

### CTX-022 — `rejects_invalid_request_before_reading`

- Baseline: `crates/modules/context/src/application/history.rs:86-99`; `#[tokio::test]`; unit module.
- Span SHA-256: `f54f15f7a12753cdfbf2bd644e99ef39fc5a77f50be5247a545581de1dd09bba`; file SHA-256: `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`.
- Preconditions: Requests include nil session with valid turn and valid session with nil turn.
- Inputs/actions: Call history coverage for each malformed identity.
- Exact behavior and limits: Nil session with valid turn, and valid session with nil turn, each InvalidInput; zero reader calls across both rows.
- Named scenarios:
  - nil session + valid turn => InvalidInput.
  - valid session + nil turn => InvalidInput.
- Helpers/fixtures: Reader fake; UUID fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-022`.

### CTX-023 — `empty_history_does_not_read_evidence`

- Baseline: `crates/modules/context/src/application/history.rs:101-109`; `#[tokio::test]`; unit module.
- Span SHA-256: `389c6f308130c8be7f70cdc42c05b7247f8ac6037e52d113abd27ddfb3357da4`; file SHA-256: `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`.
- Preconditions: Valid session and empty turn list; reader configured to fail if called.
- Inputs/actions: Request empty history coverage.
- Exact behavior and limits: Empty history succeeds with empty BTreeMap and zero reader calls even when reader is configured VaultUnavailable.
- Helpers/fixtures: Failing reader fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-023`.

### CTX-024 — `malformed_coverage_is_storage_failure_without_partial_result`

- Baseline: `crates/modules/context/src/application/history.rs:111-120`; `#[tokio::test]`; unit module.
- Span SHA-256: `6b3ef00edaa5ba9d4920eb9042c6cc75009e8df8e14bb2b2f877aac122b21a19`; file SHA-256: `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`.
- Preconditions: One requested turn; reader returns malformed Dependent with empty dependencies.
- Inputs/actions: Read coverage for exactly one turn and validate returned coverage.
- Exact behavior and limits: Stored Dependent with empty dependencies is malformed and maps to StorageUnavailable, with no successful partial result.
- Helpers/fixtures: Reader fake returning malformed DependencyCoverage.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-024`.

### CTX-025 — `reader_errors_are_fatal`

- Baseline: `crates/modules/context/src/application/history.rs:122-129`; `#[tokio::test]`; unit module.
- Span SHA-256: `420ffc20a2ff6b47be55d9fcee7c3f12d23d2a166be068b31ccecc858ca0ea9d`; file SHA-256: `04653532a45ac8e2eccf158c15670e081d396eeb18cefa6f444abe52652135cd`.
- Preconditions: Reader returns VaultUnavailable.
- Inputs/actions: Read history coverage.
- Exact behavior and limits: Reader VaultUnavailable is propagated for nonempty two-turn request; test does not assert count of reads after the first failure.
- Helpers/fixtures: Reader fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-025`.

### CTX-026 — `reservation_quota_is_per_person_and_raii_releases`

- Baseline: `crates/modules/context/src/application/leases.rs:270-283`; `#[test]`; unit module.
- Span SHA-256: `a61877124f512a1acb3591369d6b6abf3482a26ff16443cb353f338e9524b862`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: One person consumes MAX_LIVE_LEASES; another reservation exceeds that person quota.
- Inputs/actions: Reserve MAX_LIVE_LEASES one-byte leases, attempt one extra, drop all reservations, reserve again; no direct numeric count is asserted.
- Exact behavior and limits: MAX_LIVE_LEASES one-byte reservations for one person exhaust count; next one-byte reserve BudgetExceeded; dropping all RAII reservations frees capacity.
- Helpers/fixtures: LeaseRegistry; reservation fixture; RAII Drop.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-026`.

### CTX-027 — `reservation_bytes_are_isolated_per_person`

- Baseline: `crates/modules/context/src/application/leases.rs:285-298`; `#[test]`; unit module.
- Span SHA-256: `3dc7da85258d25179089b59c789ce1a29fd89bce70eb65cfd5d53e2036d4d7c2`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: Person A reserves full byte quota; Person B is independent.
- Inputs/actions: Reserve full quota for A, try A plus one byte, reserve full quota for B, then drop.
- Exact behavior and limits: One person reserves MAX_LEASE_BYTES then next byte BudgetExceeded; second person can independently reserve MAX_LEASE_BYTES; dropping first reservation frees first person's capacity.
- Helpers/fixtures: LeaseRegistry; person-scoped quota fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-027`.

### CTX-028 — `retained_evidence_uses_stored_bytes_and_isolates_people`

- Baseline: `crates/modules/context/src/application/leases.rs:300-374`; `#[test]`; unit module.
- Span SHA-256: `7edd8c3e76ae824d57c85917cb6d8bf6851b2c087947491c426b0b9f0f2e6fa0`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: Registry tracks serialized observation bytes and per-person accounting; test adjusts internal accounting near cap.
- Inputs/actions: Retain evidence, challenge quota using actual stored bytes, retain for another person, and expire first.
- Exact behavior and limits: Retained evidence accounting equals serialized dependency bytes + 64 fingerprint bytes; forced stored byte count MAX_LEASE_BYTES-1 blocks further same-person evidence, another person succeeds, expiry of held evidence then frees same-person capacity.
- Helpers/fixtures: LeaseRegistry internals; serialized payload/fingerprint fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-028`.

### CTX-029 — `retained_evidence_count_is_per_person_and_expiry_frees_capacity`

- Baseline: `crates/modules/context/src/application/leases.rs:376-426`; `#[test]`; unit module.
- Span SHA-256: `15fd2f681841bdc7a8dcb73d1e14a9b5588a3b6fad9bdbe66e066eab2cad3a79`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: One person is at retained-observation count limit; a second person has separate capacity.
- Inputs/actions: Attempt excess for first, retain for second, expire first observation, retry first.
- Exact behavior and limits: MAX_LIVE_LEASES retained observations exhaust one person's count; another person succeeds; expiring one first-person observation permits a replacement.
- Helpers/fixtures: LeaseRegistry; controllable clock; per-person evidence fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-029`.

### CTX-030 — `observation_requires_exact_identity_and_process`

- Baseline: `crates/modules/context/src/application/leases.rs:428-485`; `#[test]`; unit module.
- Span SHA-256: `0beb7ebd68e712d0a6e80c626d4ccbdd8711bc7da3419f6f9c21d9ca3a9ab040`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: Observation includes canonical dependency, query fingerprint, and process incarnation.
- Inputs/actions: Fetch exact identity, retry duplicate, alter query, and change process incarnation.
- Exact behavior and limits: Exact observation returns same dependency and a*64 fingerprint; duplicate retention Conflict; changed query fingerprint StaleContext; retaining first registry's dependency in second process registry StaleContext.
- Helpers/fixtures: LeaseRegistry; dependency/query/process fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-030`.

### CTX-031 — `observation_expires_monotonically`

- Baseline: `crates/modules/context/src/application/leases.rs:487-510`; `#[tokio::test(start_paused = true)]`; unit module.
- Span SHA-256: `61c81116d560ebcaa62dc4da2e604497b53b36f1a509d3c48d7da03c25f2fb55`; file SHA-256: `942928b308da4eff2f950b4bb6b53bcffb52f10cb338f46b3c7ef3edc007c1cf`.
- Preconditions: Paused clock; one exact dependency retained with10ms deadline and later re-retained after expiry.
- Inputs/actions: Retain10ms, advance20ms, observe stale, then re-retain the SAME dependency with5-second deadline.
- Exact behavior and limits: Monotonic 10 ms lease becomes StaleContext after 20 ms; test then successfully retains same dependency with new 5-second deadline. It does not assert permanent ban on re-retention.
- Named scenarios:
  - 10ms lease +20ms advance => StaleContext; SAME dependency can then be retained with new5s deadline.
- Helpers/fixtures: Paused Tokio time; LeaseRegistry.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-031`.

### CTX-032 — `one_route_neutral_authorization_reauthorizes_personal_and_remote`

- Baseline: `crates/modules/context/src/application/model_coverage.rs:227-250`; `#[tokio::test]`; unit module.
- Span SHA-256: `32ffebe46fb9e45896406ab8d82ceb8267e5246106f1f0cef1e3c4429d4d64cd`; file SHA-256: `a18e44ab93ce424b457c88da1d44f75f3eb282804eeb4a4b2b1d006c112fe215`.
- Preconditions: History turn merges Personal-source LocalOnly and remote-source ApprovedRecipient fixture dependencies. AcceptAll is a fake that ignores dependency/request and returns Ok(()), not a real grant-store integration.
- Inputs/actions: Project one route-neutral turn and reauthorize both dependencies.
- Exact behavior and limits: History turn merges one personal and one remote dependency; one route-neutral authorization with AcceptAll retains derived text and exactly both dependencies.
- Helpers/fixtures: StaticReader; personal_dependency; remote_dependency uses historical ApprovedRecipient; AcceptAll fake ignores inputs.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Retain exact live source/grant/category/purpose/consumer admission and fail-closed dependency retention. Replace LocalOnly/ApprovedRecipient with Access-owned DeviceOnly/GatewayAllowed; the approved-recipient fixture/row is historical evidence, not an authority requirement.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-032`.

### CTX-033 — `revoked_dependency_denies_its_turn_without_failing_projection`

- Baseline: `crates/modules/context/src/application/model_coverage.rs:252-269`; `#[tokio::test]`; unit module.
- Span SHA-256: `8f6b3328d7367635cb6a4bcc77d97080d60d410cb0694e77d931302b922f6f13`; file SHA-256: `a18e44ab93ce424b457c88da1d44f75f3eb282804eeb4a4b2b1d006c112fe215`.
- Preconditions: Turn depends on a source whose authorization callback now denies it.
- Inputs/actions: Project and reauthorize through DenyAll.
- Exact behavior and limits: DenyAll for personal dependency yields a successful projection decision with retain_derived=false and empty authorized dependencies, rather than failing entire projection.
- Helpers/fixtures: Coverage registry; DenyAll authorizer.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-033`.

### CTX-034 — `review_required_denies_its_turn_without_failing_projection`

- Baseline: `crates/modules/context/src/application/model_coverage.rs:271-288`; `#[tokio::test]`; unit module.
- Span SHA-256: `521aa77057ce4836ae529f5447e22a4390e0560c3286440032d298a75633b9d2`; file SHA-256: `a18e44ab93ce424b457c88da1d44f75f3eb282804eeb4a4b2b1d006c112fe215`.
- Preconditions: Turn dependency is under access review.
- Inputs/actions: Project and invoke authorizer returning AccessReviewRequired.
- Exact behavior and limits: NeedsReview for personal dependency yields successful denied turn decision with empty authorized dependencies, rather than failing entire projection.
- Helpers/fixtures: Coverage registry; authorizer returning AccessReviewRequired.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-034`.

### CTX-035 — `manager_projection_carries_purpose_contract_and_correction`

- Baseline: `crates/modules/context/src/application/model_projection.rs:472-508`; `#[test]`; unit module.
- Span SHA-256: `9405009040d1c687e8143f63306d910c307f6ac886dd6043fbb318b6ecc24eb4`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Manager projection fixture has purpose `everyday_assistance`, response contract `User-facing text.`, one memory, one evidence item, 3 prompt components, one Personal input class, and output limit 4096; correction is `try again`.
- Inputs/actions: Assemble the Manager projection and inspect purpose, response contract, correction, contextual memory/evidence counts, output limit, input classes, and revision.
- Exact behavior and limits: Manager input everyday_assistance/User-facing text., correction try again, one memory and one evidence; output preserves purpose, contract, correction, counts 1/1, max_output_bytes 4096, [Personal], and projection_revision 1.
- Named scenarios:
  - The stated purpose, response contract, correction, counts, output byte limit, input class, and projection revision are all asserted.
- Helpers/fixtures: input, prompt, conversation, agent_context, catalog fixtures.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-035`.

### CTX-036 — `expert_projection_carries_full_live_context`

- Baseline: `crates/modules/context/src/application/model_projection.rs:510-527`; `#[test]`; unit module.
- Span SHA-256: `212d9f08ce10667b736695d289ffff2fa4721144acdffa0efb680848f7a9b91d`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Expert projection uses the shared fixture containing one memory and one evidence item, with no history dependencies.
- Inputs/actions: Assemble the Expert projection.
- Exact behavior and limits: Expert role with same live fixture and no history dependencies retains one memory and one evidence; coverage Independent.
- Helpers/fixtures: input, prompt, conversation, agent_context, catalog fixtures.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-036`.

### CTX-037 — `declared_personal_cannot_downgrade_actual_sensitive_evidence`

- Baseline: `crates/modules/context/src/application/model_projection.rs:529-562`; `#[test]`; unit module.
- Span SHA-256: `07347d0179a7f3129ce6fb3703a4833e27de740927d2f17d13a8600313532216`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Evidence is HighlySensitive; run once without extra Personal evidence and once with mixed Personal evidence.
- Inputs/actions: Assemble Expert projections for `mixed=false` and `mixed=true`.
- Exact behavior and limits: For HighlySensitive evidence alone and mixed with Personal calendar evidence, caller-declared Personal cannot downgrade: output classes exactly [Personal,HighlySensitive], evidence equals full input vector, coverage Independent, projection validation succeeds.
- Named scenarios:
  - mixed=false: only original HighlySensitive evidence => [Personal, HighlySensitive].
  - mixed=true: add Personal calendar evidence => same class union and both evidence items retained.
- Helpers/fixtures: agent_context and projection fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-037`.

### CTX-038 — `declared_classes_normalize_and_preserve_stricter_or_forbidden_values`

- Baseline: `crates/modules/context/src/application/model_projection.rs:564-593`; `#[test]`; unit module.
- Span SHA-256: `6a7a9f7462f4e1ca55607a2423de42083b2e5b0840d9ca04ac58d3a8cf6f2f12`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Manager declares HighlySensitive, Personal, HighlySensitive; actual context has Personal evidence.
- Inputs/actions: Assemble; separately ask effective class computation for Credential and DeviceOnlyRaw.
- Exact behavior and limits: Declared [HighlySensitive,Personal,HighlySensitive] normalizes to [Personal,HighlySensitive] and validates; helper unions Credential or DeviceOnlyRaw with Personal without discarding stricter/forbidden class. These helper calls do not authorize generic reasoning for forbidden classes.
- Named scenarios:
  - Declared classes [HighlySensitive, Personal, HighlySensitive] => [Personal, HighlySensitive].
  - Credential => [Personal, Credential].
  - DeviceOnlyRaw => [Personal, DeviceOnlyRaw].
- Helpers/fixtures: agent_context, projection input fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-038`.

### CTX-039 — `only_projected_persona_memory_and_evidence_contribute_classes`

- Baseline: `crates/modules/context/src/application/model_projection.rs:595-637`; `#[test]`; unit module.
- Span SHA-256: `e7635b765c3ba85c374cad3d4e11f2f606cb8124226eb8bad82669ee714d7640`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Fixture evidence is cleared; input class is HighlySensitive. Iterate persona absent/present and Manager, Expert, Learner, Finalization roles. For persona-present, fixture memory is cleared and a default persona is installed. A separate Finalization case uses HighlySensitive evidence.
- Inputs/actions: Assemble all 8 persona × role combinations, validate each projection, then assemble Finalization with HighlySensitive evidence.
- Exact behavior and limits: Cross-product persona=false/true and Manager/Expert/Learner/Finalization: memory or Persona contributes Personal for non-finalization alongside declared HighlySensitive; Finalization yields only declared HighlySensitive. With actual HighlySensitive evidence and declared Personal, Finalization removes evidence and reports [Personal].
- Named scenarios:
  - Persona absent × Manager => [Personal, HighlySensitive].
  - Persona absent × Expert => [Personal, HighlySensitive].
  - Persona absent × Learner => [Personal, HighlySensitive].
  - Persona absent × Finalization => [HighlySensitive].
  - Persona present × Manager => [Personal, HighlySensitive].
  - Persona present × Expert => [Personal, HighlySensitive].
  - Persona present × Learner => [Personal, HighlySensitive].
  - Persona present × Finalization => [HighlySensitive].
  - Finalization with actual HighlySensitive evidence => [Personal], evidence empty.
- Helpers/fixtures: agent_context; projection role/input fixtures.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-039`.

### CTX-040 — `learner_projection_carries_memories_under_review`

- Baseline: `crates/modules/context/src/application/model_projection.rs:639-656`; `#[test]`; unit module.
- Span SHA-256: `0d882dde8ce05df0da11061c0eeffcec5db03b79b1a8e26f917077d269868c70`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Learner common fixture has one Preference/Fact memory and one evidence item. There is no asserted under-review status or memory promotion.
- Inputs/actions: Assemble Learner projection and inspect only the asserted counts and coverage.
- Exact behavior and limits: Learner with no history dependencies retains one fixture Preference/Fact memory and one evidence item, coverage Independent. The assertions do not inspect an under-review status or memory promotion.
- Helpers/fixtures: input, agent_context memory fixture.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-040`.

### CTX-041 — `finalization_empties_live_context_but_keeps_settled_observations`

- Baseline: `crates/modules/context/src/application/model_projection.rs:658-683`; `#[test]`; unit module.
- Span SHA-256: `7c2d8745d6efe10ab14409e3a362057fe8b2e55cb4c49390dac5ada3ad20c933`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Finalization fixture contains live memory/evidence and a current user message; an Independent tool exchange is appended.
- Inputs/actions: Assemble Finalization projection and inspect context, manifest, conversation length, and coverage.
- Exact behavior and limits: Finalization removes live memories/evidence and their manifest entries while preserving current-turn user plus settled tool exchange (length 2); coverage Independent.
- Helpers/fixtures: input, agent_context, settled observation fixture.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.
- Native assertion sites: 6; exact source and per-site disposition in native audit JSON under `CTX-041`.

### CTX-042 — `coverage_folds_history_and_current_exchanges_exactly`

- Baseline: `crates/modules/context/src/application/model_projection.rs:685-786`; `#[test]`; unit module.
- Span SHA-256: `e0c86d937fb93907f07874a69ebfc58cd94e2b08db6646ddb65129443407cc47`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Four distinct dependencies are placed in history, a ToolResult, a nested text artifact, and a completed TaskSnapshot delegation receipt.
- Inputs/actions: Assemble Manager projection with those four dependency-bearing inputs and compare resulting coverage with Independent merged with each dependency.
- Exact behavior and limits: Coverage equals exact merge of history dependency, tool-result dependency, artifact dependency, and delegation receipt dependency; conversation includes all four distinct sources. No current live evidence is itself asserted as a coverage dependency.
- Named scenarios:
  - History list carries `history_dep`.
  - ToolExchange result coverage carries `tool_dep`.
  - Nested note Artifact coverage carries `artifact_dep`.
  - Completed schedule TaskSnapshot receipt coverage carries `delegation_dep`.
  - Expected coverage is Independent merged with all four dependencies.
- Helpers/fixtures: dependency, tool_exchange, input, conversation fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-042`.

### CTX-043 — `identical_authorized_input_projects_identically`

- Baseline: `crates/modules/context/src/application/model_projection.rs:788-822`; `#[test]`; unit module.
- Span SHA-256: `fd7b3ba4657221dbd78bfc7a364f38e917bbb4356418bbb31962e524da8c7656`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Two assemblies use equal authorized input values and the same conversation value; assembly creates distinct projection references.
- Inputs/actions: Assemble twice; compare envelope, coverage, input classes, and projection UUID.
- Exact behavior and limits: Identical authorized input reused twice yields exactly equal envelopes, coverage, and data classes but distinct projection_ref UUIDs.
- Helpers/fixtures: input and context fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-043`.

### CTX-044 — `forbidden_route_and_secret_fields_are_absent`

- Baseline: `crates/modules/context/src/application/model_projection.rs:841-883`; `#[test]`; unit module.
- Span SHA-256: `22879911b892da94b655c2fcbf85c20795b55126b13241da695fe411b855d202`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Serialized Manager projection includes prompt prose and catalog/card data; static card `supported_placements` may be present.
- Inputs/actions: Recursively collect serialized object keys and assert absence of the exact forbidden key list.
- Exact behavior and limits: Recursive serialized key walk excludes exact keys recipient, endpoint, base_url, bearer, credential, credentials, consent, external_transfer_consent, allowed_placements, placement, route, remote_route, profile, profile_id, token. Static supported_placements on cards is deliberately allowed; prose is not checked for these words.
- Named scenarios:
  - Exact serialized object key recipient absent.
  - Exact serialized object key endpoint absent.
  - Exact serialized object key base_url absent.
  - Exact serialized object key bearer absent.
  - Exact serialized object key credential absent.
  - Exact serialized object key credentials absent.
  - Exact serialized object key consent absent.
  - Exact serialized object key external_transfer_consent absent.
  - Exact serialized object key allowed_placements absent.
  - Exact serialized object key placement absent.
  - Exact serialized object key route absent.
  - Exact serialized object key remote_route absent.
  - Exact serialized object key profile absent.
  - Exact serialized object key profile_id absent.
  - Exact serialized object key token absent.
- Helpers/fixtures: object_keys; agent_context/catalog/input fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Preserve route-free Context input and secret/credential exclusion under the accepted architecture; retire obsolete consent machinery outside this projection. The absence assertions are not themselves obsolete. Re-prove the structural boundary rather than fossilizing a blacklist of spellings.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-044`.

### CTX-045 — `capability_summary_derives_from_catalog_tools`

- Baseline: `crates/modules/context/src/application/model_projection.rs:885-909`; `#[test]`; unit module.
- Span SHA-256: `32537d0e9c265505571affea7d9749c64843b7f313b2dd722e75d39b73c20d10`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Catalog has communication-evidence and identity-evidence tool cards; the shared catalog fixture is used.
- Inputs/actions: Assemble Manager projection and inspect available capability rows.
- Exact behavior and limits: Catalog tools produce two sorted capabilities: communication first, identity second; identity version 3, read_only=true, output class Personal, schema Some({}). No route consulted.
- Helpers/fixtures: tool_descriptor/catalog fixture.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Preserve derivation from the admitted tool catalog, correct class/schema/revision and absence of model-route authority. Reassess literal fixture IDs and presentation ordering; this is not a requirement to keep obsolete recipient consent.
- Native assertion sites: 7; exact source and per-site disposition in native audit JSON under `CTX-045`.

### CTX-046 — `unknown_tool_output_class_fails_closed`

- Baseline: `crates/modules/context/src/application/model_projection.rs:911-929`; `#[test]`; unit module.
- Span SHA-256: `ac9d5d987d40dd73588b5d87073607bb9ac3023c02036fc563022f3e3d6faf42`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: First catalog tool output data class is the unrecognized string `mystery`.
- Inputs/actions: Assemble Manager projection.
- Exact behavior and limits: Tool output_data_class mystery makes assembly InvalidInput.
- Helpers/fixtures: tool_exchange and catalog fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-046`.

### CTX-047 — `catalog_discovery_sorting_is_deterministic_and_duplicates_fail_closed`

- Baseline: `crates/modules/context/src/application/model_projection.rs:931-973`; `#[test]`; unit module.
- Span SHA-256: `dd2aff147b2fb79a913fe7475ac2a459298938272d25b8dbaa2834d9f1e2c1b0`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Catalog contains tools and cards; a second input reverses both vectors, then a third adds a duplicate tool.
- Inputs/actions: Assemble each catalog projection and compare discovery and run-frame hash; attempt duplicate catalog.
- Exact behavior and limits: Reversing cards/tools leaves discovery and run_frame_sha256 identical; duplicate tool descriptor makes assembly InvalidInput. Fixture contains one card, so reversal does not prove multi-card ordering by itself.
- Named scenarios:
  - Original tool/card order vs reversed => deterministic equal result/hash.
  - Duplicate tool descriptor => InvalidInput.
- Helpers/fixtures: agent_card, tool_descriptor, catalog fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-047`.

### CTX-048 — `experts_are_derived_only_from_catalog`

- Baseline: `crates/modules/context/src/application/model_projection.rs:975-994`; `#[test]`; unit module.
- Span SHA-256: `88340880be8169069dbf52f92be863bfb5663954ebb49ce7cf9abeffbf75644c`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Catalog contains one schedule card.
- Inputs/actions: Assemble Manager projection and inspect active experts and manifest agent cards.
- Exact behavior and limits: Catalog fixture produces exactly one active expert schedule and exactly one manifest card schedule.
- Helpers/fixtures: catalog and agent-card fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-048`.

### CTX-049 — `output_bytes_are_bounded_and_manifest_mirrors_inputs`

- Baseline: `crates/modules/context/src/application/model_projection.rs:996-1028`; `#[test]`; unit module.
- Span SHA-256: `d9e0485e036b7dcdb05df60b44b555773cb79690c20be49c6a3e5cb652ec1aa5`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Common projection input has output limit 4096, 3 prompt components, one evidence item, and one memory.
- Inputs/actions: First set `max_output_bytes = usize::MAX`; then set it to `32 * 1024` and assemble.
- Exact behavior and limits: usize::MAX max_output_bytes rejected InvalidInput; 32*1024 accepted but advertised attempt max_output_bytes capped to 16384; manifest contains 3 prompt components, 1 evidence, 1 memory. Not a test of total serialized projection byte size.
- Named scenarios:
  - usize::MAX => InvalidInput.
  - 32 * 1024 => output max 16384; prompt components=3, evidence=1, memories=1.
- Helpers/fixtures: projection input/context/catalog fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-049`.

### CTX-050 — `empty_session_data_classes_are_rejected`

- Baseline: `crates/modules/context/src/application/model_projection.rs:1030-1048`; `#[test]`; unit module.
- Span SHA-256: `daef7e4dc816c6f19e8e3008301a64effcf8a1a88f550621e076a18fe171e950`; file SHA-256: `ce556d5529c3d87a65fd2e9f1c5fb5da78778982f9e2c46ef58f53db688360e9`.
- Preconditions: Manager projection input has its input data classes cleared.
- Inputs/actions: Assemble the projection.
- Exact behavior and limits: Empty caller input_data_classes rejected InvalidInput even though live context has Personal evidence/memory.
- Helpers/fixtures: projection input fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-050`.

### CTX-051 — `published_calendar_observation_is_fenced_by_connection_source_truth`

- Baseline: `crates/modules/context/src/application/observations.rs:640-699`; `#[test]`; unit module.
- Span SHA-256: `8f86ac728c6a6829120d0ae43de5cb543bef1dcee51195ee3d629cc67b6a792f`; file SHA-256: `ef20c51b06bfc43a3aadf3575debbad02610fc90aa3ccdea93819c9bc22b1675`.
- Preconditions: EventKit fixture has person `person`, connection `calendar-primary`, owner `mac-local`, resource `home`, native subject fingerprint 64 `a` characters; observation uses observed time now−1 and expiry now+60000, ±60000 range, and no records.
- Inputs/actions: Publish, read through authorized access, configure the connection to resource `work` at revision 2, then retry the old observation.
- Exact behavior and limits: Publish fresh complete Home calendar observation against exact source authority/revision/device; authorization succeeds; reconfigure source to Work at revision 2 and old Home observation becomes CapabilityUnavailable.
- Helpers/fixtures: Observation registry; connection/source fixture; clock and native subject.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-051`.

### CTX-052 — `foreign_device_cannot_publish_against_a_source`

- Baseline: `crates/modules/context/src/application/observations.rs:701-744`; `#[test]`; unit module.
- Span SHA-256: `d58640c82fe7d0df234afbac14bbf838e1fe587de25ff4301f1a2d261c2570e9`; file SHA-256: `ef20c51b06bfc43a3aadf3575debbad02610fc90aa3ccdea93819c9bc22b1675`.
- Preconditions: Source owner is `mac-local`; request names `foreign-device` while the exact observation otherwise carries the same source/resource identity.
- Inputs/actions: Attempt publication for the foreign device.
- Exact behavior and limits: Publishing same valid observation as foreign-device against mac-local source is StaleContext.
- Helpers/fixtures: Observation registry; SourceConnection and subject fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-052`.

### CTX-053 — `the_subject_is_stable_and_every_observation_is_its_own_query`

- Baseline: `crates/modules/context/src/application/personal_lineage.rs:111-127`; `#[test]`; unit module.
- Span SHA-256: `7f57e3550c2688de016a628b7ebf8a9750cbd668b874445aaf101fc789e54284`; file SHA-256: `87e282ca4700529d8ee9aa9e9e02a332c9262ab7e7538675f942fab8fb421bc7`.
- Preconditions: Subject fingerprint is computed for same person/device/view and for another device; query fingerprints use two random observation IDs and process IDs, then a second pair.
- Inputs/actions: Compare same-device subject, other-device subject, and query fingerprint pair.
- Exact behavior and limits: Attention subject fingerprint deterministic for same person/device/view, differs for other device; query fingerprints differ when observation/process UUIDs differ.
- Helpers/fixtures: Lineage subject/query constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-053`.

### CTX-054 — `selected_contacts_read_uses_current_connection_handles_and_logical_grant`

- Baseline: `crates/modules/context/src/application/personal_sources.rs:971-1107`; `#[tokio::test]`; unit module.
- Span SHA-256: `5b1cc8dad24ee366bd7c1aa7434812d2ad005b6c7aaa621118e28f8f551d1931`; file SHA-256: `8df0675cd9f2f15bdb32650e27d443b1b034e9674bc3aa54742071a79595e65a`.
- Preconditions: Reviewed Apple Contacts source begins revision1 with person.identity:a and subject A; configure_reviewed_native(expected_revision1) adds b and changes subject B, producing current revision2. Logical grant created afterward remains stable through later source change.
- Inputs/actions: Read selected current Contacts using fake connections/grants/driver; afterward reconfigure subject to A at current revision and reauthorize old dependency.
- Exact behavior and limits: Reviewed Apple Contacts source grows from handle a/subject A to a+b/subject B before read; driver gets exact current [a,b], dependency permission resources contain one logical People view, observed source_resources [a,b], new source authority but unchanged grant ID/authority. Later subject A change makes reauthorization PolicyDenied.
- Helpers/fixtures: FixtureConnections; SwappingRecords; EchoingDriver; selected contact candidate/read builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-054`.

### CTX-055 — `selected_contacts_subject_drift_fails_without_adopting_source`

- Baseline: `crates/modules/context/src/application/personal_sources.rs:1109-1194`; `#[tokio::test]`; unit module.
- Span SHA-256: `2910ab2e58fc54d01596a4b3becceec805f7eb21294fa9daa1de808d0baf57f5`; file SHA-256: `8df0675cd9f2f15bdb32650e27d443b1b034e9674bc3aa54742071a79595e65a`.
- Preconditions: Apple connection subject is A with an active grant; Contacts driver returns subject B.
- Inputs/actions: Read selected Contacts.
- Exact behavior and limits: Apple Contacts driver reports subject B after read while connection is reviewed for A; result AccessReviewRequired, source fingerprint remains A, source authority unchanged, grant remains Active. No source adoption on observation drift.
- Helpers/fixtures: FixtureConnections; SwappingRecords; driver that returns mismatching subject.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-055`.

### CTX-056 — `selected_apple_contacts_ignore_live_android_grant`

- Baseline: `crates/modules/context/src/application/personal_sources.rs:1196-1278`; `#[tokio::test]`; unit module.
- Span SHA-256: `0b75d6a1420ed5bef68109cd0b18034f2285a0902d577ef6759d786cebf65b15`; file SHA-256: `8df0675cd9f2f15bdb32650e27d443b1b034e9674bc3aa54742071a79595e65a`.
- Preconditions: Live grant is Android `contacts.android`; request selects Apple source with physical `person.identity:a`, owner `apple:device`, and logical people resource.
- Inputs/actions: Read selected Apple Contacts.
- Exact behavior and limits: Only live Android grant exists, selected source is Apple Contacts; returns one NeedsUserAction blocker targeting contacts.apple. Unselected platform grant cannot authorize selected Apple source; Android runtime support is not a target requirement.
- Helpers/fixtures: FixtureConnections; platform grants; CapturingPeopleDriver.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-056`.

### CTX-057 — `independent_and_unknown_never_invoke_authorizer`

- Baseline: `crates/modules/context/src/application/projection.rs:109-141`; `#[tokio::test]`; unit module.
- Span SHA-256: `a1ff1ac7d4eea71e768c9c962a385d69c20e1e0579d46f2c0c8ce5c3cc126d07`; file SHA-256: `a644ee98c0d5839affb698ab9ef9c886c70090569e92a20b77b5a55736a40ce3`.
- Preconditions: Project one Independent and one Unknown coverage through a counted authorizer.
- Inputs/actions: Run each projection path.
- Exact behavior and limits: Independent retains derived text, Unknown does not; both return no authorized dependencies and authorizer call count remains zero.
- Named scenarios:
  - Independent => retained, authorized dependencies empty, 0 callback calls.
  - Unknown => not retained, authorized dependencies empty, 0 callback calls.
- Helpers/fixtures: Coverage/dependency fixtures; callback counter.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-057`.

### CTX-058 — `all_dependencies_must_authorize_before_projection_retains_them`

- Baseline: `crates/modules/context/src/application/projection.rs:143-180`; `#[tokio::test]`; unit module.
- Span SHA-256: `daff846f7238a02246baf1597138091c64dd084d8b720b9f390e83fa56aad817`; file SHA-256: `a644ee98c0d5839affb698ab9ef9c886c70090569e92a20b77b5a55736a40ce3`.
- Preconditions: Coverage has two dependency IDs, first and second.
- Inputs/actions: Authorize once with all-true callback; once with callback true for first ID and false for the other.
- Exact behavior and limits: Two dependencies with allow-all retain exact full dependency vector; allowing only first denies whole projection, clears authorized dependencies, yet authorizer sees both IDs exactly once.
- Named scenarios:
  - Allow-all => retained with exact dependencies.
  - Allow dependency matching first_id and deny other (callback order unasserted) => no retained text/dependencies; both IDs seen in2 calls.
- Helpers/fixtures: Dependency fixtures; authorizer callback log.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 7; exact source and per-site disposition in native audit JSON under `CTX-058`.

### CTX-059 — `fatal_authorization_error_after_denial_is_returned`

- Baseline: `crates/modules/context/src/application/projection.rs:182-202`; `#[tokio::test]`; unit module.
- Span SHA-256: `ffe8136407a384ff2f718ce02b419f1f6b8068cbfe98446f0fcf1c8350741167`; file SHA-256: `a644ee98c0d5839affb698ab9ef9c886c70090569e92a20b77b5a55736a40ce3`.
- Preconditions: First dependency callback returns false; second returns `VaultUnavailable`.
- Inputs/actions: Project dependent turn with those callback results.
- Exact behavior and limits: First dependency denied and second authorization fails VaultUnavailable: fatal error propagated, both callbacks invoked. Denial does not short-circuit away later fatal error.
- Named scenarios:
  - First false, then VaultUnavailable => VaultUnavailable and callback count 2.
- Helpers/fixtures: Dependency fixtures; callback log.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-059`.

### CTX-060 — `invalid_coverage_is_rejected_before_authorization`

- Baseline: `crates/modules/context/src/application/projection.rs:204-213`; `#[tokio::test]`; unit module.
- Span SHA-256: `1a86187156ac2fd632c39d3a8f844d3273d4c333829b36b0b1775bd59583cedb`; file SHA-256: `a644ee98c0d5839affb698ab9ef9c886c70090569e92a20b77b5a55736a40ce3`.
- Preconditions: Input coverage is malformed empty-dependent coverage.
- Inputs/actions: Project with an authorizer callback that panics if invoked.
- Exact behavior and limits: Empty Dependent coverage yields InvalidInput before authorizer invocation.
- Helpers/fixtures: Malformed coverage and callback panic/counter fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-060`.

### CTX-061 — `hosted_calendar_selected_view_records_exact_signed_leaves_and_stales_on_change`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1021-1098`; `#[tokio::test]`; unit module.
- Span SHA-256: `c9c028921cdaf82dcd70beaa238373e86055a4e985ca19b149334fea885cb768`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Hosted calendar-account source uses a synthetic SignedSourcePreview and fake verify_view_source_preview that returns fixture reference, with physical source_resources A,B and logical calendar.timeline:calendar-account. Query1000..2000/null/25. This case does not verify signatures cryptographically.
- Inputs/actions: Read selected view, add source C, and reauthorize the old dependency.
- Exact behavior and limits: Hosted logical Calendar source uses fake verified preview with physical leaves A,B; read returns Calendar view and one binding, logical calendar.timeline:calendar-account permission resource, exact A/B observed source resources, original grant ID/authority. Adding C stales dependency (PolicyDenied) without changing grant identity/authority.
- Helpers/fixtures: ViewFixture/SourceFixture; synthetic signed-preview transport; fake verify_view_source_preview returns a reference, not cryptographic verification.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 9; exact source and per-site disposition in native audit JSON under `CTX-061`.

### CTX-062 — `selected_a_ignores_unselected_b_even_if_b_is_blocked`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1100-1117`; `#[tokio::test]`; unit module.
- Span SHA-256: `de724ba2ae23ae836463027cc75ac96ccccfe056c58844f261befd519e7cf352`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Source A is selected and ready; source B is present but configured unready via `add_source("b", false)`.
- Inputs/actions: Read only selected A.
- Exact behavior and limits: Selected active A with unselected paused B returns one A binding and exactly one payload read.
- Named scenarios:
  - A ready + B unready, requested A => Ready, binding A, 1 read.
- Helpers/fixtures: ViewFixture; selected_mail; read_selected_mail.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-062`.

### CTX-063 — `selected_missing_a_does_not_adopt_live_b`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1119-1135`; `#[tokio::test]`; unit module.
- Span SHA-256: `fd3b43f9e4990de14d77660f7ac38d194fdc9db9a890c0b2edd12d8e351aa8f3`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Only source B is available but explicit selection names source A.
- Inputs/actions: Read selected A.
- Exact behavior and limits: Selected missing A with live B returns one blocker naming A and zero payload reads; no fallback source adoption.
- Helpers/fixtures: ViewFixture; selected_mail; read_selected_mail.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-063`.

### CTX-064 — `same_connection_mail_and_logistics_grants_are_not_duplicate_authority`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1137-1151`; `#[tokio::test]`; unit module.
- Span SHA-256: `bd6a2f702941e158a20592f18756241c87699228f251fc482023d80282aeb175`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: One shared connection has ready MAIL_VIEW and LOGISTICS_VIEW grants scoped to their distinct logical resources.
- Inputs/actions: Read Mail and Logistics views separately.
- Exact behavior and limits: Same connection has distinct Mail and Logistics logical grants; both views Ready, total two reads; these are not duplicate exact authority.
- Named scenarios:
  - MAIL_VIEW succeeds.
  - LOGISTICS_VIEW succeeds; total payload reads2.
- Helpers/fixtures: ViewFixture::add_view_source, read_mail, read_view(LOGISTICS_VIEW).
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-064`.

### CTX-065 — `work_view_ignores_unrelated_mail_grants`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1153-1166`; `#[tokio::test]`; unit module.
- Span SHA-256: `52d211672d997b5d4ad1154466d92015eb2eadef8e10ac26c020063680a148ef`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Only a mail grant exists; request is WORK_VIEW.
- Inputs/actions: Read work view.
- Exact behavior and limits: Request Work when only Mail grant exists yields SelectResource and zero reads.
- Helpers/fixtures: ViewFixture; read_view.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-065`.

### CTX-066 — `duplicate_exact_target_fails_before_payload_io`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1168-1178`; `#[tokio::test]`; unit module.
- Span SHA-256: `a06001c15d8d339731a463f90265dea45cf8867e95e0d469ee8d2829297e160b`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Two grants claim exact target `read_mail` for the same source.
- Inputs/actions: Attempt exact-target read.
- Exact behavior and limits: Duplicate exact Mail target yields Conflict and zero payload reads.
- Helpers/fixtures: ViewFixture; duplicate source fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-066`.

### CTX-067 — `exact_target_with_wrong_consumer_is_a_review_blocker`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1180-1207`; `#[tokio::test]`; unit module.
- Span SHA-256: `627e6f4c4be2f6cd3a9293e74408456cb7a85ff5faffcc22d7e0292f6d492d37`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Exact target exists but the current consumer differs from the requested consumer.
- Inputs/actions: Read mail target.
- Exact behavior and limits: Exact Mail grant with consumer schedule while caller assistant yields ReviewChangedSource and zero reads.
- Helpers/fixtures: ViewFixture; wrong-consumer GrantScope fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-067`.

### CTX-068 — `exact_target_with_wrong_scope_is_never_ready`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1209-1267`; `#[tokio::test]`; unit module.
- Span SHA-256: `61b3e7ea2d8faee2be080047ea7b6e49ed8bf132f65c66bd4c991a22f098976a`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Four exact-target grants each mismatch a requested scope component.
- Inputs/actions: Read once per scope row.
- Exact behavior and limits: Four mismatched-scope rows each NeedsUserAction/ReviewChangedSource and zero reads: Derived vs Content category; Scheduling vs Assistant purpose; approved-recipient(model,Content) vs LocalOnly processing; Suggestion vs Read operation. Category/purpose/operation and fail-closed processing admission are durable; exact approved-recipient representation is obsolete under ADR0034.
- Named scenarios:
  - Derived + Read + Assistant + LocalOnly => ReviewChangedSource, 0 reads.
  - Content + Read + Scheduling + LocalOnly => ReviewChangedSource, 0 reads.
  - Content + Read + Assistant + approved_recipient("model", [Content]) => ReviewChangedSource, 0 reads (legacy recipient row).
  - Content + Suggestion + Assistant + LocalOnly => ReviewChangedSource, 0 reads.
- Helpers/fixtures: ViewFixture; grant-scope constructor; fake payload reader.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Retain exact live source/grant/category/purpose/consumer admission and fail-closed dependency retention. Replace LocalOnly/ApprovedRecipient with Access-owned DeviceOnly/GatewayAllowed; the approved-recipient fixture/row is historical evidence, not an authority requirement.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-068`.

### CTX-069 — `changed_source_incarnation_stales_existing_dependency_without_changing_grant`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1269-1306`; `#[tokio::test]`; unit module.
- Span SHA-256: `c82416b98a7f428f16e53a80ffe11d3504da0769830a396a5414d8ea7cc5f47f`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: A read succeeds; source authority incarnation rotates while the grant stays the same.
- Inputs/actions: Reauthorize the earlier dependency after source rotation.
- Exact behavior and limits: After successful remote read, source incarnation rotates; dependency reauthorization PolicyDenied, grant authority unchanged, payload read count remains 1.
- Helpers/fixtures: ViewFixture; rotated source authority hook; consumed dependency validator.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-069`.

### CTX-070 — `foreign_person_grant_is_never_adopted`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1308-1322`; `#[tokio::test]`; unit module.
- Span SHA-256: `a6b6a061218509e9b2f5af162a7a2ed3eb3b15c3ec3d8e1ab42663a6f9f40ecd`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Grant is owned by another person than the current request.
- Inputs/actions: Read current person’s mail view.
- Exact behavior and limits: Fixture person changes after grant creation; foreign-person grant ignored, SelectResource blocker, zero reads.
- Helpers/fixtures: ViewFixture; PersonId-switched source fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-070`.

### CTX-071 — `paused_source_blocks_without_a_truncated_aggregate`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1324-1343`; `#[tokio::test]`; unit module.
- Span SHA-256: `3c2a2d3cc62a36ac5125776571f3f216740189c8144343d9e0b760b08c8ce088`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Aggregate needs source A ready and source B unready/paused.
- Inputs/actions: Read the aggregate view.
- Exact behavior and limits: Unselected aggregate has active A and paused B: one inline EnableObserve blocker for B, source_id floe.source.mail, and zero payload reads. Whole aggregate classified before I/O; no truncated success.
- Helpers/fixtures: ViewFixture; paused source fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 6; exact source and per-site disposition in native audit JSON under `CTX-071`.

### CTX-072 — `unknown_target_is_navigation_only_selection`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1345-1361`; `#[tokio::test]`; unit module.
- Span SHA-256: `2329385a0830031bd7a3c6e0afe38d01234a6e6fac43f9348a4398f73857bfa8`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Empty ViewFixture has no source for requested mail target.
- Inputs/actions: Read the target.
- Exact behavior and limits: No source target yields one SelectResource blocker without connection ID, inline_resolution=false, zero reads.
- Helpers/fixtures: Empty ViewFixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-072`.

### CTX-073 — `admitted_sources_keep_their_own_bindings`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1363-1388`; `#[tokio::test]`; unit module.
- Span SHA-256: `89a6bf7551876601debaf1ffb50bbbaf8d207074fa076eb8fd01cf882e3e563d`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Sources a and b are both admitted for the aggregate.
- Inputs/actions: Read aggregate.
- Exact behavior and limits: Two admitted sources produce exactly two bindings whose connection IDs are A and B, with two payload reads.
- Helpers/fixtures: ViewFixture; two SourceFixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-073`.

### CTX-074 — `expired_credential_mid_read_becomes_a_reconnect_blocker`

- Baseline: `crates/modules/context/src/application/remote_sources.rs:1390-1410`; `#[tokio::test]`; unit module.
- Span SHA-256: `4c8a14b65b239de5706d054c1fbf9b0ce81a05db3ebb0c98f12ef339895dac5a`; file SHA-256: `f91a7d0078e30c2405453d7a7a00ca4a5f596a67db106e226351849a13dbc847`.
- Preconditions: Sources a and b begin ready; b credential is marked expired before read.
- Inputs/actions: Read aggregate.
- Exact behavior and limits: Two admitted sources, B credential expires during read; result is one Reconnect blocker targeting B, never truncated Ready aggregate. Test does not assert payload read count/order.
- Helpers/fixtures: ViewFixture; per-source credential-expiry injection.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-074`.

### CTX-075 — `communication_merge_is_bounded_deterministic_and_partial`

- Baseline: `crates/modules/context/src/application/remote_views.rs:399-418`; `#[test]`; unit module.
- Span SHA-256: `c8d311a15add57a14be15a764cb759a473b7e634359cbb3b98679dd491c266c1`; file SHA-256: `4607244824ad3e727b1bfdb62ba2607ef092a5d2e9e70913f26ace378a135b3b`.
- Preconditions: Gmail contributes one item at timestamp 20 with completeness true; Microsoft contributes one at timestamp 30 with completeness false. Query schema version is AGENT_VERSION, empty query, cursor 0, limit 8; now=2000, max count 8, max bytes MAX.
- Inputs/actions: Merge both source views.
- Exact behavior and limits: Merge Gmail timestamp20 complete and Microsoft timestamp30 incomplete; result two items ordered with Microsoft first, coverage_complete=false, source_handle starts multi:mail.communication:. Query limit8/budget constants are supplied; no overflow rejection exercised in this case.
- Helpers/fixtures: Communication view/item constructors.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-075`.

### CTX-076 — `calendar_contract_requires_exact_categories_query_and_result`

- Baseline: `crates/modules/context/src/application/remote_views.rs:425-476`; `#[test]`; unit module.
- Span SHA-256: `c85e577805b478249297b4fc4f81e4cdc49dca6b3278aa02e8f319121397eb02`; file SHA-256: `4607244824ad3e727b1bfdb62ba2607ef092a5d2e9e70913f26ace378a135b3b`.
- Preconditions: Remote Calendar categories Metadata and Content; query1000..2000/null/limit1, matching empty complete view observed1000/expires2000 and source calendar:source.
- Inputs/actions: Validate query and matching empty view at1500 with max_items1 and byte budget constant; then change requested end to3000.
- Exact behavior and limits: Calendar is remote view with exactly Metadata+Content categories; valid range1000..2000/null cursor/limit1 validates to (1,MAX_CALENDAR_CONTEXT_BYTES); matching empty view valid at1500; query end3000 against view end2000 yields StaleContext.
- Helpers/fixtures: Calendar remote view and query constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-076`.

### CTX-077 — `preparation_is_lazy_and_read_validates_the_observation`

- Baseline: `crates/modules/context/src/application/service.rs:338-361`; `#[tokio::test]`; unit module.
- Span SHA-256: `533915d1b46729a85f6c85afb5529a4b90fe6eac98f5dbafaf6a2afd27b3ba41`; file SHA-256: `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`.
- Preconditions: Fixture request has matching person, process incarnation, source, query and consumer binding.
- Inputs/actions: Prepare without read, then call read_source once.
- Exact behavior and limits: ContextService.prepare performs zero reads; first read returns Ready payload {items:[]} and increments reader count to1.
- Helpers/fixtures: FixtureReader; dependency/scope/request builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-077`.

### CTX-078 — `concurrent_small_views_reserve_their_measured_bytes`

- Baseline: `crates/modules/context/src/application/service.rs:363-389`; `#[tokio::test]`; unit module.
- Span SHA-256: `4f381e51b8ce856155a8c57a950814206ac4de9aed9faddfb910bf43a0276b53`; file SHA-256: `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`.
- Preconditions: Two small views are requested for the same person through one fixture reader.
- Inputs/actions: Await first read and then await second read sequentially; both requests are separate.
- Exact behavior and limits: Two small views retained simultaneously are both Ready and have equal payload. Calls are awaited sequentially; case does not prove execution concurrency or assert exact charged bytes directly.
- Helpers/fixtures: FixtureReader; separate request/dependency fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-078`.

### CTX-079 — `cancellation_and_person_mismatch_do_not_reach_the_reader`

- Baseline: `crates/modules/context/src/application/service.rs:391-420`; `#[tokio::test]`; unit module.
- Span SHA-256: `b183b4f6afc555e8265f04f9a52b56ed5e47b9dc336fd1be7d696bc5441350de`; file SHA-256: `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`.
- Preconditions: One request is cancelled; another request is created from a PreparedContext for a different PersonId.
- Inputs/actions: Read both requests through the original prepared context.
- Exact behavior and limits: Already cancelled request returns Cancelled; request prepared for another person returns PolicyDenied; both together invoke reader zero times.
- Named scenarios:
  - Cancelled request => Cancelled, 0 calls.
  - Other prepared PersonId => PolicyDenied, 0 calls.
- Helpers/fixtures: FixtureReader; PreparedContext and request fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-079`.

### CTX-080 — `mismatched_process_and_query_binding_is_rejected`

- Baseline: `crates/modules/context/src/application/service.rs:422-440`; `#[tokio::test]`; unit module.
- Span SHA-256: `6ceec5bb6978de85b424db3e71424b58d59498b960dac64997c230611927d6e8`; file SHA-256: `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`.
- Preconditions: FixtureReader wrong_binding=true changes source key to other.view, query fingerprint to32 bytes of9 and process incarnation to a new UUID together.
- Inputs/actions: Attempt source read.
- Exact behavior and limits: FixtureReader wrong_binding=true produces mismatched source key other.view, process incarnation and query fingerprint together; read returns PolicyDenied. This combined mutation does not isolate each fence independently.
- Helpers/fixtures: FixtureReader; dependency/process/query fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-080`.

### CTX-081 — `typed_blockers_pass_through_but_never_for_another_consumer`

- Baseline: `crates/modules/context/src/application/service.rs:461-499`; `#[tokio::test]`; unit module.
- Span SHA-256: `8d97b35abd0d42a87303e23f7836bfc3f7ce61a96ddebd77267452597af38bdb`; file SHA-256: `f5847cbc613caacd0bf4aa13e824851dbfc056251ad5a6397917f0e7e0697b5a`.
- Preconditions: BlockedReader returns a valid SelectResource blocker for consumer `fixture.expert`, then for `fixture.other`; requirement source ID is `floe.source.mail`, operation Read, purpose Assistant, inline resolution false.
- Inputs/actions: Read each blocker result as `fixture.expert`.
- Exact behavior and limits: NeedsUserAction blocker for exact requested consumer passes through unchanged; otherwise identical blocker for fixture.other is PolicyDenied.
- Named scenarios:
  - Correct consumer => typed blockers preserved.
  - Other consumer => PolicyDenied.
- Helpers/fixtures: BlockedReader; blocked_fixture; consumer/scope builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-081`.

### CTX-082 — `intrinsic_selection_is_exactly_device_pinned`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:335-349`; `#[test]`; unit module.
- Span SHA-256: `21aff1935ee600a39aef44d5ea5c04dc0393a07484a2a8154b155eea97d01882`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Intrinsic local capabilities floe.tasks and memory.confirmed are requested on mac-local.
- Inputs/actions: Discover then validate on mac-local and other-device.
- Exact behavior and limits: For floe.tasks and memory.confirmed candidates, selection validates on mac-local and is StaleContext on other-device.
- Named scenarios:
  - floe.tasks on mac-local valid / other-device stale.
  - memory.confirmed on mac-local valid / other-device stale.
- Helpers/fixtures: SourceCandidateRequest; local selection validator.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-082`.

### CTX-083 — `intrinsic_reference_is_not_an_access_grant_and_has_stable_id`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:351-375`; `#[test]`; unit module.
- Span SHA-256: `b664149be02b485577eed423643bd9581ca9ba429da45c5b506850dbc95c9ed5`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Intrinsic local candidate is for `floe.tasks`, device owner `device:mac-local`, and has its own candidate ID.
- Inputs/actions: Build candidates and inspect candidate binding and confirmed-interaction discovery.
- Exact behavior and limits: Intrinsic floe.tasks returns one candidate with connector+connection LOCAL_CONTEXT_CONNECTOR, owner device:mac-local, resource floe.tasks; candidate ID equals source_candidate_id and length64. relationships.confirmed_interactions returns none. No grant construction or source read occurs.
- Helpers/fixtures: request builder; local candidate ID hashing.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-083`.

### CTX-084 — `native_calendar_resource_change_preserves_single_connection_view_candidate`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:377-472`; `#[test]`; unit module.
- Span SHA-256: `1be37f4f11076db71a9e120a04fe8e408b11844251fcad10bdbbc5300c106209`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Native EventKit source owned by mac-local initially contains physical calendar-a labeled Personal, subject a*64; later adds calendar-b labeled Work, renames calendar-a, then disconnects.
- Inputs/actions: Discover matching/foreign candidates; add calendar-b, rename calendar-a to Renamed, disconnect.
- Exact behavior and limits: Native Calendar initially yields one logical calendar.timeline:calendar-account candidate; foreign person and device yield none. Adding calendar-b and then renaming calendar-a leaves candidate exactly equal; disconnect yields none.
- Named scenarios:
  - calendar-a => one logical candidate.
  - Wrong person => none.
  - Wrong device => none.
  - Add calendar-b => unchanged candidate.
  - Rename calendar-a => entire candidate vector unchanged.
  - Disconnect => none.
- Helpers/fixtures: SourceConnection constructors; candidate request builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-084`.

### CTX-085 — `hosted_calendar_resource_change_preserves_one_logical_candidate`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:474-526`; `#[test]`; unit module.
- Span SHA-256: `fc01d22bfacd9d618a7328e7ab2ae525c9ea7b436a6b8d933354b2d5afb885df`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Google and Microsoft server calendar connections are for same person and serve Calendar A; then Calendar B is added.
- Inputs/actions: Build logical candidates before and after each provider/resource change.
- Exact behavior and limits: Both calendar.google and calendar.microsoft hosted sources under server-owner yield one logical calendar.timeline:calendar-account candidate; growing physical calendars leaves candidate exactly unchanged.
- Named scenarios:
  - calendar.google => one logical calendar candidate stable after adding calendar-b.
  - calendar.microsoft => same.
- Helpers/fixtures: SourceConnection; candidate request builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-085`.

### CTX-086 — `personal_candidates_require_serving_connections_and_survive_source_edits`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:528-581`; `#[test]`; unit module.
- Span SHA-256: `52027c9f16ec496bb8d57ccb1ff2d62b1ace61707988d12629f2cff0fb0e0fc4`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Apple contacts initially has one reviewed native resource `a`, label A; then resource `b` is added and native subject changes from 64 `a` to 64 `b`.
- Inputs/actions: Build candidate before/after source edit and after disconnect.
- Exact behavior and limits: No Contacts source yields none; reviewed Apple source yields one logical people.identity:contacts.apple.local candidate; adding b and changing subject leaves candidate exactly unchanged; disconnect yields none.
- Helpers/fixtures: SourceConnection and source candidate request fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-086`.

### CTX-087 — `personal_candidate_modes_and_singletons_are_checked_per_connection`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:583-647`; `#[test]`; unit module.
- Span SHA-256: `bc981c129b05b0c6c3c44f8a406845f0157b6f78a4cc616f178a25dea98f4c8a`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Attention/macOS and Health/Apple AllAvailable sources each contain their required singleton physical view resource under exact device owner.
- Inputs/actions: Build candidate; change subject; switch connection mode from AllAvailable to Selected.
- Exact behavior and limits: Attention macOS and Health Apple AllAvailable singleton sources each yield one logical view:connection candidate; subject change preserves candidate; switching resource mode to Selected yields none. Does not test Health transform evidence or model routing.
- Named scenarios:
  - attention.macos / ATTENTION_VIEW_ID / owner macos:mac-local / AllAvailable.
  - health.apple / Wellbeing view / owner apple:mac-local / AllAvailable.
  - For each fixture, changed subject preserves candidate; Selected mode yields no candidate.
- Helpers/fixtures: SourceConnection; reviewed native candidate builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-087`.

### CTX-088 — `remote_accounts_require_pinned_producer_and_keep_exact_distinct_targets`

- Baseline: `crates/modules/context/src/application/source_candidates.rs:649-703`; `#[test]`; unit module.
- Span SHA-256: `c3a4a9e81192dc219b7f67df0ecd70138c36d7bcd7e12349735a3ffacdfdf466`; file SHA-256: `a84061c188b49823694b923ddaccbe64aadf604a52acaf3f7bb2adddcd415b50`.
- Preconditions: Two Gmail accounts mail-a/mail-b belong to current person; a third belongs to another person.
- Inputs/actions: Discover with no execution owner, then with `server:paired` owner.
- Exact behavior and limits: Two own Ready Gmail snapshots plus foreign snapshot: absent pinned remote_execution_owner yields no candidates; server:paired yields exactly mail-a/mail-b logical resources, distinct candidate IDs; foreign person ignored.
- Named scenarios:
  - mail-a + requested person; mail-b + requested person; foreign + other person.
  - No remote execution owner => none.
  - server:paired pinned => mail-a and mail-b candidates with distinct IDs.
- Helpers/fixtures: ConnectorSnapshot builders; SourceCandidateRequest.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-088`.

### CTX-089 — `accepts_canonical_dependency_and_selected_scope`

- Baseline: `crates/modules/context/src/application/source_view.rs:283-299`; `#[test]`; unit module.
- Span SHA-256: `6d638b9397e4b5ca26bc0d81d435aba5df16231d609a4674b7b9994cc97d446a`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: Canonical dependency and selected GrantScope cover the requested source and bounded payload.
- Inputs/actions: Construct bounded SourceView and validate.
- Exact behavior and limits: Canonical dependency, covering scope, 32-byte reservation, 5-second deadline create fresh payload view; first binding exactly preserves dependency/scope and payload equals payload.
- Helpers/fixtures: SourceView fixture; LeaseRegistry; dependency/scope builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-089`.

### CTX-090 — `rejects_scope_that_does_not_cover_dependency`

- Baseline: `crates/modules/context/src/application/source_view.rs:301-324`; `#[test]`; unit module.
- Span SHA-256: `822fe43eae88f1e85ae46dbe12eaa4e324820fed69e8efc70892a48b17407c58`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: View dependency names a resource missing from selected scope.
- Inputs/actions: Construct/validate view.
- Exact behavior and limits: Scope resource other/item does not cover dependency fixture/item: SourceView creation InvalidInput.
- Helpers/fixtures: SourceView fixture; mismatched GrantScope.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-090`.

### CTX-091 — `shared_view_keeps_its_reservation_until_the_last_owner_drops`

- Baseline: `crates/modules/context/src/application/source_view.rs:326-350`; `#[test]`; unit module.
- Span SHA-256: `268c441e94fb3c95e51b97a276794b83d9ed557c06597b17118dedb594094fb2`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: SourceView is shared through multiple Arc owners and holds byte reservation.
- Inputs/actions: Clone shared ownership, drop first owner, then final owner.
- Exact behavior and limits: Arc-shared view holds MAX_LEASE_BYTES reservation after first Arc drops; next byte BudgetExceeded; remaining payload accessible; dropping last Arc frees full capacity.
- Helpers/fixtures: SourceView fixture; Arc ownership; LeaseRegistry.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-091`.

### CTX-092 — `rejects_stale_deadline_and_unbound_reservation`

- Baseline: `crates/modules/context/src/application/source_view.rs:352-391`; `#[test]`; unit module.
- Span SHA-256: `404cfdb6e11b1b12d5a85e684cbd2ca1b2dbc5c68aea6751ecd219b01af074f5`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: Live reserved quota used with expired view deadline, another-person reservation, or different-registry reservation in three separate constructions.
- Inputs/actions: Validate each unbound/stale view construction.
- Exact behavior and limits: Past deadline, other-person reservation, and same-person reservation from other process registry each yield StaleContext.
- Named scenarios:
  - Expired deadline => StaleContext.
  - Other person reservation => StaleContext.
  - Other registry reservation => StaleContext.
- Helpers/fixtures: SourceView fixture; LeaseRegistry and clock.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-092`.

### CTX-093 — `rejects_payload_larger_than_reservation`

- Baseline: `crates/modules/context/src/application/source_view.rs:393-407`; `#[test]`; unit module.
- Span SHA-256: `143bd97b7d72a28ea0f8834b3aacb7c879f41ca032961f01e6555d916fee5777`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: Payload serializes beyond its reserved byte count.
- Inputs/actions: Build and validate view.
- Exact behavior and limits: Payload string with one-byte reservation yields BudgetExceeded.
- Helpers/fixtures: SourceView fixture; payload builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-093`.

### CTX-094 — `quota_stops_counting_serializer_without_output_buffer`

- Baseline: `crates/modules/context/src/application/source_view.rs:409-428`; `#[test]`; unit module.
- Span SHA-256: `188620e44482f91c05c441333f79e590cd252d747e5d5778eedd2b6d6612ca67`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: RepeatingPayload serializes one million items while reservation is one byte.
- Inputs/actions: Request SourceView serialization under bounded quota.
- Exact behavior and limits: RepeatingPayload with million-element serializer and one-byte reservation yields BudgetExceeded before million attempts. Counting writer avoids producing full output buffer; source/helper establishes implementation detail, assertion establishes early stop.
- Helpers/fixtures: RepeatingPayload; bounded serializer; LeaseRegistry.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-094`.

### CTX-095 — `malformed_serialization_releases_reservation`

- Baseline: `crates/modules/context/src/application/source_view.rs:430-446`; `#[test]`; unit module.
- Span SHA-256: `fb1d3c2a7b4341a4b986b3cd83dacc45df1b325cbbeab7aa7ecdc3a01b49fb54`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: Payload serialization fails after reservation.
- Inputs/actions: Attempt SourceView creation/serialization and inspect quota.
- Exact behavior and limits: FailingPayload serialization yields InvalidInput and frees reservation, proven by immediately reserving MAX_LEASE_BYTES.
- Helpers/fixtures: FailingPayload; SourceView fixture; LeaseRegistry.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-095`.

### CTX-096 — `serialization_crossing_deadline_is_stale_and_releases_reservation`

- Baseline: `crates/modules/context/src/application/source_view.rs:448-466`; `#[test]`; unit module.
- Span SHA-256: `c9dceadc7097a798b8423512b4646984e09f68aa50dce3afce12dab2a9cc997a`; file SHA-256: `f1cad68a50968fb45d10d68a8d52936f93e8ed707f3ad328be83124e3da052e3`.
- Preconditions: Slow serializer sleeps 120 ms while lease deadline is 100 ms.
- Inputs/actions: Serialize once under deadline and inspect attempt count/quota.
- Exact behavior and limits: SlowPayload crosses 100ms deadline: StaleContext after one serialization attempt, and full capacity can be reserved again.
- Named scenarios:
  - Lease=100 ms; serialization=120 ms => StaleContext, one attempt, reservation freed.
- Helpers/fixtures: SlowPayload; bounded serializer; LeaseRegistry.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-096`.

### CTX-097 — `projection_is_scoped_clipped_bounded_and_contains_no_provider_native_metadata`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:380-414`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `f1be172fd3a8a0e193d17f719199060afbc50df114a8a671d077a65da540bfee`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Timeline fixture contains a Home appointment and private/secret provider metadata; range is the authorized source range.
- Inputs/actions: Read same bounded timeline twice and inspect returned item and serialized JSON.
- Exact behavior and limits: Fixture grant selects Home only; timeline has one Home appointment, clipped start at grant starts_at and end now+90min, Synthetic class. Serialized payload excludes secret-id, Private home, Hidden work, private-provider, external_revision, can_modify, UTC. First read performs2 authority checks; repeated read equals prior view.
- Named scenarios:
  - Expected item fields and Synthetic source as stated.
  - Serialized JSON excludes all seven listed provider/native strings/fields.
  - Second authorized read equals first.
- Helpers/fixtures: Fixture; TestTimelineRepository; Access fake; grant/request helpers.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-097`.

### CTX-098 — `mirror_provenance_cannot_override_access_source_authority`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:416-446`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `fddb23a0488d35b3fcf1a84f74686ea44e05ddb090b999907e869952288fe978`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Mirror row has source_connection_id tampered to `other-source` and revision incremented.
- Inputs/actions: Read timeline once with current authority, then make Access deny.
- Exact behavior and limits: Changing mirror source_connection_id to other-source does not itself defeat current Access authority: still one projected item. When Access denies, read is CapabilityDenied. Mirror provenance cannot override Access.
- Helpers/fixtures: Fixture; TestTimelineRepository; Access fake; grant/request builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-098`.

### CTX-099 — `read_range_is_request_scoped_within_the_authorized_source`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:448-497`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `0d707c246ad27283fc56cf0ba5fe769dedb18d6907cdc6cd263f5229b3f420c9`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Grant covers source range; requested narrow interval is now+75m to now+105m.
- Inputs/actions: Read narrow interval, then expanded range from grant.starts_at−1m through grant.ends_at.
- Exact behavior and limits: Explicit request now+75..105min is used and item start clipped to75; a separate request starting one minute before grant.starts_at succeeds and preserves expanded start. Despite test title, grant time window is not asserted as an immutable request-time authorization boundary.
- Named scenarios:
  - Narrow range: [now+75m, now+105m], one Home appointment with start at range start.
  - Expanded request: range starts grant.starts_at−1m and ends grant.ends_at; test checks expanded range_start.
- Helpers/fixtures: Fixture; Access fake; request/grant helpers.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 6; exact source and per-site disposition in native audit JSON under `CTX-099`.

### CTX-100 — `request_scoped_observation_does_not_depend_on_page_mirror_coverage`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:499-533`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `52263a99a0a2704783a5bacee5c726da3d4a6df52624be7e45d866d7ffefdb50`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Requested_start=now()+7 days; requested_end=requested_start+7 days, therefore now()+14 days. Native ObservationAccess returns Home event at now()+8 days for1 hour, titled Next week review.
- Inputs/actions: Read native request-scoped observation for now+7d..now+14d.
- Exact behavior and limits: Native observation answers requested next-week 7-day window beyond page mirror coverage; range preserved, coverage_complete=true, title Next week review.
- Named scenarios:
  - Requested range now+7d..now+14d; event at now+8d for1h; complete view echoes exact request and title.
- Helpers/fixtures: Fixture; projected observation access fake; source observation.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-100`.

### CTX-101 — `native_subject_change_during_read_denies_projection`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:535-556`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `1e854601cfdd9c74596fe35285675de922f2912ca663bef3c389e0dde75ad832`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Native source subject changes during an authorized read.
- Inputs/actions: Read timeline while subject check drifts.
- Exact behavior and limits: Observation reports changed native subject during read: StaleContext.
- Helpers/fixtures: Fixture; subject-changing Access fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-101`.

### CTX-102 — `projected_observation_preserves_server_coverage_and_opaque_provenance`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:558-592`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `7eb1c3e035c1d3bb4f4fbe56e4cb3733d270f8647f38da6b3568cf6a68462a20`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Server cursor sequence starts at None then `next-page`; first page is incomplete with one `Server review` event; second page is complete.
- Inputs/actions: Read both pages, inspect opaque evidence handles and cursors.
- Exact behavior and limits: Server-projected first page preserves source_handle calendar.timeline:server, coverage_complete=false, next-page cursor, title, and namespaced UUIDv5 from opaque evidence ID; source_was_observed=true. Second page complete/no cursor, different evidence handle; cursor calls exactly [None,Some(next-page)].
- Named scenarios:
  - First page incomplete; cursor next-page; asserted opaque UUIDv5 evidence handle.
  - Second page complete; cursor absent; evidence handle differs.
  - Access cursor calls are [None, Some("next-page")].
- Helpers/fixtures: Fixture; projected observation access; page/cursor builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 10; exact source and per-site disposition in native audit JSON under `CTX-102`.

### CTX-103 — `cursor_read_never_falls_back_to_an_unpaged_mirror`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:594-607`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `92ec935858aa31f534502830e16b923ce8c414e833742a07376ba0282c5f0e21`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Read request carries a cursor while only unpaged mirror source is available.
- Inputs/actions: Attempt cursor read.
- Exact behavior and limits: Cursor next-page with only unpaged mirror and no observation returns CapabilityUnavailable, not mirror fallback.
- Helpers/fixtures: Fixture; Access fake; cursor request.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-103`.

### CTX-104 — `projection_reads_events_across_a_bounded_multi_day_range`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:609-698`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `f4fa5b974ce9a48ee35c5fb5fed265ef00051811dbf0594d2440424e690e1067`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Fixture-provider stored mirror, not a live native source, contains timed first/fifth-day events and a third-day all-day event over a7-day range.
- Inputs/actions: Read the bounded seven-day timeline.
- Exact behavior and limits: Seven-day range projects ordered First day, Third day all day, Fifth day; middle all-day interval length86400000ms; view range exactly grant day bounds.
- Named scenarios:
  - Seven-day window: first event, day-three all-day item, day-five item.
- Helpers/fixtures: Fixture; timeline repository; grant/range builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-104`.

### CTX-105 — `timeline_grant_accepts_more_than_four_exact_calendars`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:700-709`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `5546080180d502f26387a6bd55b5b18e6711619b98e972fa93710a1ddce4c56d`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Grant calendar IDs contain 11 distinct strings `calendar-0` through `calendar-10`.
- Inputs/actions: Validate grant, append duplicate `calendar-0`, validate again.
- Exact behavior and limits: Eleven exact unique calendar IDs validate; appending duplicate calendar-0 fails validation. No four-calendar cap requirement.
- Named scenarios:
  - 11 distinct calendar IDs => is_ok.
  - Duplicate calendar-0 => is_err; exact kind unasserted.
- Helpers/fixtures: Fixture; grant builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-105`.

### CTX-106 — `unknown_scope_identity_budgets_and_permission_are_denied_before_projection`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:711-749`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `3c7f496f99a197d63908927d7fb9cca46c00039afac4f7b8a410926f7f22e966`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Seven mode values independently vary foreign person, wrong handle, unselected calendar, zero item budget, one-byte budget, foreign stamp, and Access denial.
- Inputs/actions: Loop mode 0 through 6, mutate the corresponding request/grant, and call timeline.
- Exact behavior and limits: Seven rows: wrong person, wrong handle, unselected calendar, max_items0, max_bytes1, foreign authority snapshot, denied permission. Zero/below-min budgets return BudgetExceeded; other rows CapabilityDenied. Zero authority calls asserted only for wrong person, wrong handle, and max_items0.
- Named scenarios:
  - mode 0: wrong PersonId => CapabilityDenied; 0 Access calls.
  - mode 1: wrong handle UUID => CapabilityDenied; 0 Access calls.
  - mode 2: unselected calendar => CapabilityDenied.
  - mode 3: max_items=0 => BudgetExceeded; 0 Access calls.
  - mode 4: max_bytes=1 => BudgetExceeded.
  - mode 5: foreign person stamp => CapabilityDenied.
  - mode 6: Access denied => CapabilityDenied.
- Helpers/fixtures: Fixture; Access fake; grant/request builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-106`.

### CTX-107 — `failed_other_calendar_does_not_poison_a_healthy_explicit_subset`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:751-797`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `7c850a3a5151b90326764aa8deecf4e6aa679dcba4d69d5254aec356ef877a56`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Home calendar is healthy; work calendar permission/read fails.
- Inputs/actions: Read Home-only scope, then add failed Work to the grant and read the combined selected set.
- Exact behavior and limits: Healthy selected Home empty batch plus failed Work PermissionDenied: Home-only grant succeeds empty; adding failed Work makes CapabilityDenied. Partial source failure does not poison healthy explicit subset or allow requested failed source.
- Helpers/fixtures: Fixture; per-calendar batch/permission fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-107`.

### CTX-108 — `stale_cache_uncovered_ranges_and_access_revocation_never_return_empty_success`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:799-836`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `fa5c24ed05b40d7193e8c39336adc8380ca32c4e7e5677b8ae6b9429adf199dc`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: First row advances read clock6min while extending grant expiry; second shifts grant.day and grant starts/ends1day into uncovered range without changing the stored mirror; third Access denies.
- Inputs/actions: Try each changed clock/grant-range/Access state against the original mirror.
- Exact behavior and limits: Stale mirror clock+6min with still-live grant and uncovered tomorrow range each StaleContext; Access denied is CapabilityDenied. None becomes empty success.
- Named scenarios:
  - Clock now+6m, grant expiry new-clock+1m => StaleContext for cache.
  - Shift GRANT date/range1day while mirror unchanged => StaleContext.
  - Access denied => CapabilityDenied.
- Helpers/fixtures: Fixture; controllable clock; Access fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-108`.

### CTX-109 — `access_generation_change_and_later_source_denial_invalidate_a_read_lease`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:838-873`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `41b3ab310d03956a48230bbcf973e1db3d074e20626cfd468222bb68c382a630`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Access generation changes between authorization checks; another case denies on revalidation after successful read.
- Inputs/actions: Read and revalidate under mutation.
- Exact behavior and limits: Authority generation change on second check makes read StaleContext; separate read and revalidate initially succeed, then Access denial causes CapabilityDenied at revalidate.
- Helpers/fixtures: Fixture; generation-mutating Access fake; timeline lease.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-109`.

### CTX-110 — `native_subject_change_invalidates_cached_read_before_egress`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:875-890`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `b091929dfcab82919c8334b7b276ef9747ad07e9f1bf954dac59460817bec51a`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Initial cached read succeeds; second subject fingerprint differs.
- Inputs/actions: Read then attempt cached egress under changed subject.
- Exact behavior and limits: Access subject changes on call index2: first read succeeds, cached second read StaleContext before egress.
- Helpers/fixtures: Fixture; subject-drifting Access fake.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-110`.

### CTX-111 — `missing_native_subject_fingerprint_denies_calendar_read`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:892-906`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `6a317216e60dc077ec6f58914c31f3c74f2c3c7bf96f3dfdee1ec82fb66fc352`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Native calendar source has no subject fingerprint.
- Inputs/actions: Attempt timeline read.
- Exact behavior and limits: Missing/invalid native subject fingerprint from Access yields CapabilityDenied.
- Helpers/fixtures: Fixture with missing subject fingerprint.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-111`.

### CTX-112 — `all_day_and_long_events_block_the_entire_requested_window_without_false_focus`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:908-940`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `8cfa1903be0f0af67ae33c7397e782d4e10b2811a8f1c1dd02f5412c92894070`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: One event is all-day and another spans the full requested interval.
- Inputs/actions: Read one timeline containing both all-day and long events and inspect both clipped intervals.
- Exact behavior and limits: All-day event and long event -60..720min both clip to exact full requested window, with2 items; no false free interval inferred.
- Named scenarios:
  - All-day event covers complete requested window.
  - Long event crossing window covers complete requested window.
- Helpers/fixtures: Fixture; all-day and long-event builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-112`.

### CTX-113 — `oversized_titles_are_unicode_safe_but_overfull_calendars_are_not_truncated`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:942-993`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `0fb2c01f905e67397cfb6123f7dbf32e9de9add47ce1c73bec200a22c7ad30b1`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: One Korean title is "개인 제목".repeat(100); separate MAX_TIMELINE_VIEW_ITEMS+1 Busy records exceed the item bound.
- Inputs/actions: Project Unicode title, then project overfull calendar.
- Exact behavior and limits: Korean title repeated100 is UTF-8-safe bounded to<=256 bytes and ends ellipsis; MAX_TIMELINE_VIEW_ITEMS+1 overlapping records yields BudgetExceeded, not partial truncated calendar.
- Named scenarios:
  - Unicode title: `.len() <= 256` bytes and `ends_with(…)`.
  - MAX_TIMELINE_VIEW_ITEMS+1 Busy records => BudgetExceeded.
- Helpers/fixtures: Fixture; event and grant builders.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-113`.

### CTX-114 — `input_window_honors_dst_day_bounds_without_assuming_twenty_four_hours`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:995-1011`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `37f9c5a1f24564d3f575c175bb0199c6bceca10fbd69678bc29aa80fb1ad259f`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Day-bound offset pairs represent spring-forward and fall-back transitions.
- Inputs/actions: Compute day bounds and validate full day and 8–16 hour subrange for each row.
- Exact behavior and limits: DST offsets -18000→-14400 and reverse produce23h and25h days; full-day and8h..16h subwindow grants validate in each row.
- Named scenarios:
  - offset -18000 -> -14400 => 23-hour day.
  - offset -14400 -> -18000 => 25-hour day.
- Helpers/fixtures: Fixture; range_bounds and CalendarGrant validation.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-114`.

### CTX-115 — `grants_accept_bounded_multi_day_and_historical_ranges`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:1013-1030`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `36f5de8037225587b14a10f71d30a851eb7a711f3938a9bdcbed14d3545da15e`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Seven-day current date range; same range shifted30days backward; third range length MAX_TIMELINE_VIEW_DAYS+1.
- Inputs/actions: Validate current7-day range, same-duration30-day historical shift, and oversized MAX_TIMELINE_VIEW_DAYS+1 range.
- Exact behavior and limits: Seven-day current and30-day historical shifted ranges validate; MAX_TIMELINE_VIEW_DAYS+1 range InvalidInput.
- Named scenarios:
  - 7-day current range accepted.
  - Same7-day range shifted30days earlier accepted.
  - MAX_TIMELINE_VIEW_DAYS+1 => InvalidInput.
- Helpers/fixtures: Fixture; grant/range helpers.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-115`.

### CTX-116 — `cancelled_and_expired_reads_do_not_start_authority_work`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:1032-1049`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `f8843dfe6c65546915c51d41a8c6f052f62feba5e299e1b4c6e7096a32ae1889`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Request is already cancelled or deadline expired before call.
- Inputs/actions: Attempt timeline for each early-stop state.
- Exact behavior and limits: Precancelled read Cancelled; deadline equal Instant::now DeadlineExceeded; both cause zero Access calls.
- Named scenarios:
  - Already cancelled => Cancelled, zero authority calls.
  - Deadline now => DeadlineExceeded, zero authority calls.
- Helpers/fixtures: Fixture; cancellation token and deadline clock.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-116`.

### CTX-117 — `pending_access_is_cancelled_on_stop_deadline_and_dropped_view_future`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:1051-1093`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `6ef98d526dff8b31ab3a7064da08cb6fdeb6b95852869185997c62fefdeab32e`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Access fake blocks and observes child cancellation; cases stop explicitly, hit deadline, or drop future.
- Inputs/actions: Start pending view and trigger each lifecycle event.
- Exact behavior and limits: Pending authority acquisition: explicit request stop Cancelled;50ms deadline DeadlineExceeded; dropping unresolved view future cancels operation child. All3 child tokens cancelled; request token is cancelled only for explicit stop. This preserves downward cancellation and does not authorize cancelling a Run on observer disposal.
- Named scenarios:
  - Explicit stop => Cancelled; child and request token cancelled.
  - 50ms deadline => DeadlineExceeded; child cancelled, request token not cancelled.
  - Drop unresolved view future => child cancelled, request token not cancelled.
- Helpers/fixtures: Fixture; cancellable Access fake; futures/cancellation token.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-117`.

### CTX-118 — `mirror_size_and_invalid_grants_fail_without_partial_projection`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:1095-1135`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `641c574b85ba0c204d48901d0dec7ee1a6b2d2a5c4c9e1074280a51c5f6f6d67`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: Invalid variants include duplicate ID, 513-character ID, expiry now+6m, and ends==starts. Separate valid one-item mirror contains title of 4,194,304 `x` bytes.
- Inputs/actions: Validate each grant row, then project oversized-title record.
- Exact behavior and limits: Four invalid grants: duplicate calendar,513-char ID, expiry now+6min, zero-duration range, all InvalidInput.4MiB raw mirror title yields BudgetExceeded before partial projection.
- Named scenarios:
  - Duplicate calendar ID => InvalidInput.
  - Calendar ID length 513 => InvalidInput.
  - Expiry now+6m => InvalidInput.
  - ends_at == starts_at => InvalidInput.
  - One 4,194,304-byte title => BudgetExceeded.
- Helpers/fixtures: Fixture; mirror/grant builders.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-118`.

### CTX-119 — `native_eventkit_without_observation_does_not_use_mirror_payload`

- Baseline: `crates/modules/context/tests/calendar_timeline.rs:1137-1168`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `df17c9873a1a6b1a66e2caa595cd9465192398cce8202be8faad4cac16208f5d`; file SHA-256: `a465c3636297ce1c0975eaf20fddf362500293846fb7a11da423c31304c4fd0b`.
- Preconditions: EventKit connector has persisted mirror data but native source observation is absent.
- Inputs/actions: Attempt timeline read.
- Exact behavior and limits: EventKit mirror with fictional Personal-class title but no fresh native observation yields CapabilityUnavailable; no native mirror payload fallback.
- Helpers/fixtures: Fixture; EventKit connection; missing observation provider.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-119`.

### CTX-120 — `native_admission_checks_subject_grant_and_current_connection`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:258-282`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `699cdc6548e2e304ad92a96e29b008f98cda3fb27ce993c624f442ab04f219d0`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Native stamp has subject fingerprint 64 `a` characters; fixture has a serving current connection and Read grant.
- Inputs/actions: Admit native calendar read for `floe.builtin.schedule`.
- Exact behavior and limits: Native admission returns subject a*64 and one logical native-calendar resource for exact connection; Connections read twice, device check once, grant admission once.
- Helpers/fixtures: Connections fake; Device fake; Grants fake; fixture(); window().
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-120`.

### CTX-121 — `native_admission_serves_two_canonical_consumers_under_one_grant`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:284-318`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `e8857de778a226bd9392d92b280f0476650f37e674173450eb92c8ece221e4b8`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Same native source and logical grant can admit schedule and focus-attention consumers.
- Inputs/actions: Admit both canonical consumers.
- Exact behavior and limits: Schedule and Focus-Attention admissions share same grant ID; each returned scope has exactly its one requested consumer. Fixture creates consumer-specific scopes under shared grant ID, not a real multi-consumer grant-store integration.
- Named scenarios:
  - floe.builtin.schedule => one-consumer scope.
  - floe.builtin.focus-attention => one-consumer scope.
- Helpers/fixtures: Connections/Device/Grants fakes; fixture; window.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-121`.

### CTX-122 — `native_admission_reads_more_than_128_current_calendars_under_one_logical_grant`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:320-350`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `fcd58bec87642c198a76eaf69c6789e5f24400124ba5c8157049dd3473437f2e`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Selected native source is configured with 129 resources named `calendar-{index}` and display labels `Calendar {index}`.
- Inputs/actions: Admit current native calendar read.
- Exact behavior and limits: Current source configured with129 calendars admits all129 physical IDs under one logical permission resource. No128-calendar cap.
- Named scenarios:
  - Physical calendar IDs 0..128 (129 total); logical GrantScope resources length=1.
- Helpers/fixtures: Connections fake; fixture; resource(index) constructor.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-122`.

### CTX-123 — `native_admission_rejects_missing_or_changed_authority`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:352-387`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `1742b22a0093aea125a3660c622778bb2b159b876fabfd0ae910b93d52540a60`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Case one removes current connection; case two changes connection while Grants reads it.
- Inputs/actions: Attempt admission for each case.
- Exact behavior and limits: Missing connection AccessReviewRequired with zero device checks; connection change during grant admit StaleContext with exactly one grant call.
- Helpers/fixtures: Connections fake with mutation; Device and Grants counters.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-123`.

### CTX-124 — `native_admission_rejects_unbound_stamp_and_grant`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:389-439`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `2616f390cf32c4d4f14f1d6d1338862fee2ed97edceea554b9096fff416805b7`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Three variants: device fingerprint `invalid`; Grants returns wrong source binding; Grants returns wrong consumer.
- Inputs/actions: Attempt native admission per variant.
- Exact behavior and limits: Invalid native fingerprint CapabilityDenied before any grant call; wrong execution-owner source and wrong consumer grant independently CapabilityDenied.
- Named scenarios:
  - Device fingerprint `invalid` => CapabilityDenied; grants.calls=0.
  - wrong_source=true => CapabilityDenied.
  - wrong_consumer=true => CapabilityDenied.
- Helpers/fixtures: Connections/Device/Grants fakes; stamp and grant mutation helpers.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-124`.

### CTX-125 — `native_view_records_complete_empty_coverage_and_exact_dependency`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:441-476`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `de400f2ea707ff555d1d2fcca64359eee58f97310c6908651e3662620110af63`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Query covers now−60s to now+60s, cursor None, limit 8; source returns no events.
- Inputs/actions: Read native view and inspect dependency plus counted calls.
- Exact behavior and limits: Empty complete native batch produces coverage_complete=true, empty items, exact query range, person and schedule consumer, current source authority;2 device checks/1 observation/2 grant calls and retained lease observation readable.
- Helpers/fixtures: Connections/Device/Grants fakes; fixture; retained lease.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 11; exact source and per-site disposition in native audit JSON under `CTX-125`.

### CTX-126 — `current_resource_growth_reads_all_calendars_and_stales_old_dependency`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:478-616`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `343b80d4a93f010d5bde913d42326de697965cec890ab0711248feaad9836b71`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Current source begins with 11 selected IDs `calendar-00`…`calendar-10`, then grows to 12 IDs through `calendar-11`.
- Inputs/actions: Read under 11 IDs, change source, reauthorize old dependency, then read/authorize current dependency.
- Exact behavior and limits: Read11 current physical calendars under1 logical grant; grow to12 rotates SourceAuthority, old dependency StaleContext. New read empty valid view, last check+observe each receive exact sorted12 IDs, dependency has one logical resource plus exact12 physical resources/current authority, and new dependency reauthorizes.
- Named scenarios:
  - 11 leaves initially; 12 after change; grant remains one logical resource.
- Helpers/fixtures: Connections/Device/Grants fakes; fixture; mutable resource set.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 10; exact source and per-site disposition in native audit JSON under `CTX-126`.

### CTX-127 — `native_view_rejects_partial_batch_and_unpageable_cursor`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:627-671`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `ca70d09b7a20b26b19d09994c6b7123048e894eb9a3241e067178fc4453d8b18`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Device can return partial batch; cursor variant requests `next`.
- Inputs/actions: Read each query.
- Exact behavior and limits: Missing/partial calendar batches CapabilityUnavailable; cursor next unsupported by native read also CapabilityUnavailable without an additional device check.
- Named scenarios:
  - Partial batch => CapabilityUnavailable.
  - Cursor => rejected before additional authorization.
- Helpers/fixtures: Connections/Device/Grants fakes; batch/cursor injection.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-127`.

### CTX-128 — `native_permission_denial_requires_review_without_issuing_a_view`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:673-698`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `096f576db4a194a26261d1e40126d3dec0f17fe9285b4f775c0ee13166f3f056`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Device read fails with PermissionDenied.
- Inputs/actions: Read native view.
- Exact behavior and limits: Native primary batch PermissionDenied maps to AccessReviewRequired, no view, exactly1 grant admission call.
- Helpers/fixtures: Connections/Device/Grants fakes; Device PermissionDenied response.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-128`.

### CTX-129 — `native_view_preserves_all_day_calendar_evidence`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:700-744`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `56d7059d6c7cb03e14a653ea0e148cb0999da27159150adbfde18eb892d6945f`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Local calendar has event `event-one`, title `Day off`, calendar `primary`, all-day schedule from local date start to next local date start; query spans ±1h around those boundaries.
- Inputs/actions: Read native view.
- Exact behavior and limits: Local-time all-day event Day off produces exactly1 item with local midnight start/end (not assumed24h), all_day=true and title preserved, within wider query.
- Helpers/fixtures: Connections/Device/Grants fakes; all-day observation builder.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-129`.

### CTX-130 — `native_dependency_rechecks_the_current_grant_source`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:746-802`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `1b0077709ac693188b4f8e075f9910db10a76cef14c4f3ffb94f9265dcb5cfa8`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Initial native read creates dependency over initial source resources; dependency is authorized once before source configuration changes.
- Inputs/actions: Add `secondary` to selected resources and reauthorize old dependency.
- Exact behavior and limits: Native dependency initially reauthorizes; adding secondary calendar to current source makes old dependency StaleContext.
- Helpers/fixtures: Connections/Device/Grants fakes; dependency validator.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-130`.

### CTX-131 — `native_view_rejects_connection_change_after_observation`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:804-829`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `62bcca864066f0bd9d3379af060f40f67eac05724f5de7d317b74c205f8052f9`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Grant fixture changes connection on its second call during native read.
- Inputs/actions: Attempt native view read.
- Exact behavior and limits: Source connection changes during second grant admission after observation: StaleContext with1 device observation and2 grant calls.
- Helpers/fixtures: Connections mutation hook; Device/Grants fakes.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-131`.

### CTX-132 — `native_view_rejects_device_generation_drift`

- Baseline: `crates/modules/context/tests/native_calendar_read.rs:831-855`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `a063efd9f22396361a5cb22317833f96ee1b8e8b9a49af50b2822e6dfd146f7d`; file SHA-256: `f61c4b0326879f889a87aac95686297d75712df2617e1981e043341830c39ace`.
- Preconditions: Device fixture reports generation drift during observation.
- Inputs/actions: Attempt native view read.
- Exact behavior and limits: Observation generation2 after check generation1 yields StaleContext with exactly1 observation.
- Helpers/fixtures: Device fake with generation mutation; Connections/Grants fakes.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-132`.

### CTX-133 — `optional_context_keeps_budget_issues_distinct_from_unreadable_storage`

- Baseline: `crates/modules/context/tests/native_context.rs:15-138`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `c5320857b0e53c91cad3994f1cb5b07849d87eca8b88f79c75bcf4c307a5e993`; file SHA-256: `a1aeeefc29d0a7f318432a7a57689ce738fac5731f47d6c0faee750744196977`.
- Preconditions: Two tasks and two notes exist; one-item limit cannot fit either source. Separate corrupt task/note rows point to another person; final fixture makes store reads fail.
- Inputs/actions: Acquire optional task and note source at item limit 1; inject foreign-person rows and read at limit 8; then fail repository reads.
- Exact behavior and limits: Two tasks/notes exceed1-item budgets: optional acquisition returns no value plus BudgetExceeded issue. Foreign-person payload filed under requested owner's index makes raw task/note views StorageUnavailable; unreadable storage also propagates StorageUnavailable through optional acquisition. No partial or healthy-empty downgrade.
- Named scenarios:
  - Tasks, count 2, item limit 1 => no value + BudgetExceeded issue.
  - Notes, count 2, item limit 1 => no value + BudgetExceeded issue.
  - Foreign-person indexed task => StorageUnavailable.
  - Foreign-person indexed note => StorageUnavailable.
  - Repository read failure for Tasks/Notes => StorageUnavailable.
- Helpers/fixtures: floe-kernel in-memory repository; task/note fixture builders; support repository.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 8; exact source and per-site disposition in native audit JSON under `CTX-133`.

### CTX-134 — `projects_bounded_floe_native_task_and_note_views`

- Baseline: `crates/modules/context/tests/native_context.rs:140-239`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `fe738f8df3aec0c3500051823126df3f4ad29268df7ed04cc05de65aed85d71d`; file SHA-256: `a1aeeefc29d0a7f318432a7a57689ce738fac5731f47d6c0faee750744196977`.
- Preconditions: Fixture contains own active task `Ship the proposal` (High, deadline 17:00), own completed `Already done` task, another person’s private task, and note `Remember the launch constraint`.
- Inputs/actions: Build and validate task view; build note view; read task view again.
- Exact behavior and limits: Own incomplete High task Ship the proposal with17:00 deadline projects exact UUID/title/deadline/priority; completed task and foreign-person task excluded; native view validates; note exact UUID/excerpt/update timestamp preserved; repeated task projection with same handle/time identical.
- Helpers/fixtures: floe-kernel in-memory repository; support TestTimelineRepository-compatible patterns.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 6; exact source and per-site disposition in native audit JSON under `CTX-134`.

### CTX-135 — `enforces_item_and_byte_budgets_before_exposure`

- Baseline: `crates/modules/context/tests/native_context.rs:241-271`; `#[tokio::test]`; integration module included by tests/integration.rs.
- Span SHA-256: `5beab90fc4bc5f3694e1a2ae7dfc3d440225518fa3fb232c0b64ceb5441c19ce`; file SHA-256: `a1aeeefc29d0a7f318432a7a57689ce738fac5731f47d6c0faee750744196977`.
- Preconditions: Two native tasks exist; one request uses item limit 1, another uses item limit 8 and byte limit 64.
- Inputs/actions: Build task view under each limit.
- Exact behavior and limits: Two-task view with max_items1 and with max_bytes64 each fails; test asserts is_err only, not exact failure variant.
- Named scenarios:
  - Two tasks with item limit 1 => is_err.
  - Two tasks with item limit 8 and 64-byte cap => is_err.
- Helpers/fixtures: floe-kernel in-memory repository; task fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-135`.

### CTX-136 — `highly_sensitive_requires_both_bounded_projection_and_separate_remote_consent`

- Baseline: `crates/modules/context/tests/policy.rs:54-106`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `d0617f37e9bb1245875bc452a962530455e910918063b6863a3fcc52a03fbacd`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: HighlySensitive evidence, encrypted session, policy purpose `bounded-test-projection`, placements DeviceLocal/Remote; bounded flag and consent are toggled.
- Inputs/actions: Authorize DeviceLocal and Remote through the exact field states.
- Exact behavior and limits: Historical policy: HighlySensitive encrypted local succeeds without bounded flag; Remote lacks consent→ConsentRequired; bounded=true still ConsentRequired; then Granted→success; bounded=false with Granted→PolicyDenied. Retire local transform bypass and model-recipient consent expectations; replace with source-owned Health transform proof plus unchanged HighlySensitive and DeviceOnly/GatewayAllowed processing admission, separate from model selection.
- Named scenarios:
  - DeviceLocal + encrypted + unbounded/no consent => Ok.
  - Remote + encrypted + unbounded/no consent => ConsentRequired.
  - Remote + bounded/no consent => ConsentRequired.
  - Remote + bounded + Granted consent => Ok.
  - Remote + unbounded + Granted consent => PolicyDenied.
- Helpers/fixtures: policy() and context() test constructors; policy fields.
- Classification: **obsolete representation/compatibility assertion to retire**
- Target disposition: Retire the exact model-recipient/TransferConsent and local-unbounded HighlySensitive outcomes. Re-prove Health source-owned local semantic transform evidence before either local or Gateway reasoning, unchanged HighlySensitive classification, and Access-owned DeviceOnly/GatewayAllowed category admission; transform success never grants permission.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-136`.

### CTX-137 — `raw_sources_and_credentials_stay_outside_agent_context_even_with_consent`

- Baseline: `crates/modules/context/tests/policy.rs:108-121`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `6ea5268c76ce6bb8f9695dc674df6048045ae354463d07aeb390f898b5125bb8`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: For each of DeviceOnlyRaw and Credential, session is encrypted, bounded projection true, transfer consent Granted.
- Inputs/actions: Try DeviceLocal and Remote placements.
- Exact behavior and limits: Cross-product DeviceOnlyRaw/Credential × DeviceLocal/Remote, even bounded=true+consent Granted, each PolicyDenied. Durable generic reasoning exclusion remains; consent/placement plumbing is obsolete scaffolding.
- Named scenarios:
  - DeviceOnlyRaw + DeviceLocal => PolicyDenied.
  - DeviceOnlyRaw + Remote => PolicyDenied.
  - Credential + DeviceLocal => PolicyDenied.
  - Credential + Remote => PolicyDenied.
- Helpers/fixtures: policy()/context() test constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Preserve rejection of Credential and DeviceOnlyRaw before either generic local or Gateway reasoning; remove historical TransferConsent/placement scaffolding without removing the data-class prohibition.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-137`.

### CTX-138 — `expired_and_empty_projection_metadata_fail_closed`

- Baseline: `crates/modules/context/tests/policy.rs:123-157`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `bbeda11726296e4d8d10409677027c355be539ccb81a46991eeeceff7eefed53`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: Evidence expires at unix ms 100; separate policy variants use empty purpose and projection version 0.
- Inputs/actions: Authorize at now=100; clear purpose and authorize at now=1; then set purpose `test`, version 0 and authorize.
- Exact behavior and limits: Evidence expires at100: authorize at100 StaleContext; empty purpose PolicyDenied; restored test purpose with projection_version0 PolicyDenied.
- Named scenarios:
  - now=100 equals expiry => StaleContext.
  - Empty purpose => PolicyDenied.
  - projection_version=0 => PolicyDenied.
- Helpers/fixtures: policy()/context() constructors; controllable expiry.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-138`.

### CTX-139 — `confirmed_memory_requires_personal_scope_and_current_bounded_metadata`

- Baseline: `crates/modules/context/tests/policy.rs:159-203`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `f79ab38c35a5139e83d79ee1c61c3f4fcdf33d11f7389c34de70afa24282750a`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: Memory fixture is Preference/Fact, confidence 1000, valid_until=200, with one source ref; memory added to Personal context with evidence removed.
- Inputs/actions: Authorize at now=100 under Personal/Encrypted; try Synthetic/SyntheticOnly; set valid_until=100; then clear valid_until and source refs.
- Exact behavior and limits: Personal confirmed memory with valid metadata and evidence cleared succeeds at100; Synthetic policy+SyntheticOnly protection denies Personal memory; valid_until100 at100 StaleContext; clearing source_refs after removing expiry yields PolicyDenied.
- Named scenarios:
  - Personal + Encrypted, current valid memory => Ok.
  - Synthetic + SyntheticOnly => PolicyDenied.
  - valid_until=100 at now=100 => StaleContext.
  - No source refs => PolicyDenied.
- Helpers/fixtures: policy/context constructors; memory metadata fixture.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-139`.

### CTX-140 — `context_evidence_is_bounded_and_source_handles_are_unique`

- Baseline: `crates/modules/context/tests/policy.rs:205-232`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `0fe56e0aee1140ad63cbba7fd5ff72f9189c36e633a4cef53321ed14e4f5c418`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: Evidence text can be MAX_CONTEXT_EVIDENCE_BYTES+1; duplicate case repeats same ContextEvidence/source_handle.
- Inputs/actions: Authorize both contexts under Personal/Encrypted at now=1.
- Exact behavior and limits: Evidence text MAX_CONTEXT_EVIDENCE_BYTES+1 yields BudgetExceeded; duplicate identical source_handle yields PolicyDenied.
- Named scenarios:
  - Text bytes MAX_CONTEXT_EVIDENCE_BYTES+1 => BudgetExceeded.
  - Duplicate evidence source handle => PolicyDenied.
- Helpers/fixtures: policy/context/evidence constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-140`.

### CTX-141 — `optional_context_issues_are_explicit_and_memory_issues_cannot_hide_data`

- Baseline: `crates/modules/context/tests/policy.rs:234-276`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `6984317a387276bb705b7790fc48508ca7e19b2bb69e5406ac1b72d6b0161cc2`; file SHA-256: `a374686add636a3680e57d81342a6a17c611e34605d1062756aee01c09836e3c`.
- Preconditions: Initially optional_context_issues=[Memory/Unavailable]. Add memory for second check. Remove memory and append Memory/Denied without clearing first issue, producing duplicate Memory issue sources for third check.
- Inputs/actions: Authorize issue-only, issue+memory, then TWO same-source issues with memory removed.
- Exact behavior and limits: One optional Memory/Unavailable issue with no memories succeeds; adding memory while issue remains PolicyDenied; removing memory then adding duplicate Memory/Denied issue also PolicyDenied.
- Named scenarios:
  - Memory/Unavailable, no memory => Ok.
  - Memory/Unavailable plus memory => PolicyDenied.
  - Memory/Unavailable + Memory/Denied, no memory => PolicyDenied (duplicate issue sources); lone Denied is not tested.
- Helpers/fixtures: optional context and memory constructors.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-141`.

### CTX-142 — `route_arbitration_prefers_health_then_freshness_then_provider_without_prompting`

- Baseline: `crates/modules/context/tests/routing.rs:145-187`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `8220e9dc2ffd71ce4249e3a56a7e2e245bb527050f1a4a82a4711b7317704fd9`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: First comparison: Ready Microsoft timestamp NOW−1000 and Degraded Gmail NOW−100. Second comparison marks both Degraded, Gmail NOW−100 and Microsoft NOW−1000.
- Inputs/actions: Route the same work-mail logical source with no prompt-selection input.
- Exact behavior and limits: Ready older Microsoft beats fresher Degraded Google; deduplicated_candidates=1. When both Degraded, fresher Google wins. No isolated provider-preference tie case in this registration.
- Named scenarios:
  - Ready Microsoft vs newer Degraded Gmail => Microsoft selected.
  - Both Degraded, Gmail fresher => Gmail selected.
- Helpers/fixtures: snapshot() and descriptor/view helpers; route_logical_views; fixed NOW.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess source arbitration/ranking and provider tie-break policy; retain current-authority, freshness and source-scope checks. This is source-view arbitration, not Gateway-primary reasoning fallback.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-142`.

### CTX-143 — `nonconforming_and_duplicate_physical_views_are_not_promoted`

- Baseline: `crates/modules/context/tests/routing.rs:189-225`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `0b17084bbd7e28d374320652b65f1438edef0976df342dd3d4db8c6398719601`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Android and Google candidates share physical source handle `mail:shared`; health-connect view exceeds descriptor max_bytes (100000 > 65536).
- Inputs/actions: Route logical sources a-personal-mail, b-duplicate-mail, and health.
- Exact behavior and limits: Two distinct logical routes point to same physical mail:shared evidence; earlier a-personal-mail retained, duplicate b-duplicate-mail unavailable; oversized100000-byte Health-named fake also unavailable. Assertions prove conformity and physical dedup, not Android/Health production semantics.
- Named scenarios:
  - Duplicate physical handle behind later logical source is not selected.
  - Oversized Health-named generic view not selected.
  - Selected only a-personal-mail; unavailable exactly b-duplicate-mail and health.
- Helpers/fixtures: snapshot/health builders; route_logical_views.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-143`.

### CTX-144 — `calendar_parity_routes_share_one_logical_source_without_prompt_selection`

- Baseline: `crates/modules/context/tests/routing.rs:227-274`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `aee1075bc9fdf74a3f034b1e37807e3da4ffa516a9697baf43bd9750b7b98a20`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Google and Microsoft calendar aliases expose the same personal-calendar logical source, same capability/view and equal health/freshness.
- Inputs/actions: Route snapshots supplied in order [Microsoft, Google] with both provider names configured.
- Exact behavior and limits: Equal-fresh Ready Google/Microsoft Calendar logical candidates with provider preference Google first: one Google selected, deduplicated_candidates1, independent of reversed input snapshot order.
- Named scenarios:
  - Same logical calendar source for both providers.
  - Equal health/freshness snapshots ordered [Microsoft, Google] => Google selected; one deduplicated candidate.
- Helpers/fixtures: snapshot/route builders; route_logical_views.
- Classification: **product hypothesis requiring reassessment**
- Target disposition: Reassess source arbitration/ranking and provider tie-break policy; retain current-authority, freshness and source-scope checks. This is source-view arbitration, not Gateway-primary reasoning fallback.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-144`.

### CTX-145 — `location_uses_the_present_invoking_mobile_instead_of_the_newest_device`

- Baseline: `crates/modules/context/tests/routing.rs:276-330`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `943953db4714701a6c731604393957778a18181f93e1d77aef95fe118ac3faab`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Phone is a present mobile device observed NOW−5000; tablet is absent mobile designated carried observed NOW−100; desktop is present observed NOW−10. Scope names phone as invoking mobile.
- Inputs/actions: Route current-location candidates supplied [tablet, desktop, phone].
- Exact behavior and limits: Location policy invoking present phone vs absent designated tablet and present desktop selects phone despite newer other snapshots; consumer_device_id phone.
- Helpers/fixtures: Runtime device and route candidate fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 1; exact source and per-site disposition in native audit JSON under `CTX-145`.

### CTX-146 — `attention_stays_on_the_interaction_device_instead_of_becoming_person_global`

- Baseline: `crates/modules/context/tests/routing.rs:332-377`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `85450ee56feb453c364d47d0a1a01d6cd5a8119f464eed60b630532a51298cd0`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Phone candidate is newer (NOW−10); desktop candidate is NOW−1000. Interaction device and active desktop are both `desktop`.
- Inputs/actions: Route attention candidates.
- Exact behavior and limits: Attention policy interaction device desktop chooses one desktop observation despite much newer phone; consumer desktop. Device-scoped attention must not become person-global by freshness.
- Helpers/fixtures: Runtime device and interaction fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 2; exact source and per-site disposition in native audit JSON under `CTX-146`.

### CTX-147 — `stale_and_device_only_offline_context_fail_closed_but_relay_cache_is_degraded`

- Baseline: `crates/modules/context/tests/routing.rs:379-441`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `c5bbbd4aa7e0a4e5353f20e72806a9fd21676687172966a8b1390678c6039d5b`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Offline tablet has stale attention snapshot NOW−121000 and fresh snapshot NOW−1000; max age 120000; interaction device tablet. First runtime declares DeviceOnly; second declares OpaqueRelay for both sources.
- Inputs/actions: Route stale/fresh candidates under each source transfer policy.
- Exact behavior and limits: Tablet offline: stale121s and fresh1s DeviceOnly observations produce no route under max_age120s; with declared+allowed OpaqueRelay, one fresh offline cached source selected as DegradedOfflineCache, stale source still excluded. ContextTransferClass::DeviceOnly here is transport policy, distinct from ADR0034 source processing DeviceOnly.
- Named scenarios:
  - Stale + fresh DeviceOnly while offline => selected empty; current-attention unavailable.
  - Stale + fresh OpaqueRelay while offline => one DegradedOfflineCache selection with `attention:offline`.
- Helpers/fixtures: snapshot and offline cache fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Preserve fail-closed freshness and declared/allowed cross-device transport checks. Reassess offline-cache product eligibility and truthful degraded labeling. ContextTransferClass::DeviceOnly is distinct from ADR0034 source-processing DeviceOnly; neither grants reasoning or Action authority.
- Native assertion sites: 5; exact source and per-site disposition in native audit JSON under `CTX-147`.

### CTX-148 — `any_scope_labels_offline_device_cache_without_degrading_server_sources`

- Baseline: `crates/modules/context/tests/routing.rs:443-497`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `dc96f250e77392c8af2e38ba5cc980fdcc69d10ca82708242f4679f2aa44d661`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Any-scope offline tablet calendar.timeline cache and Ready server snapshot. The calendar.server-named server fixture actually retains mail.communication descriptor/view from snapshot(); device transfer EncryptedDerivedSync is declared/allowed.
- Inputs/actions: Route device alone, then device plus server.
- Exact behavior and limits: Any-scope offline tablet with allowed EncryptedDerivedSync alone selects DegradedOfflineCache; adding older Ready server source selects server as Fresh. Server fixture retains mail.communication view despite calendar-named connector/logical label.
- Named scenarios:
  - Offline device cache alone => DegradedOfflineCache.
  - Offline cache plus Ready server => server connector selected, Fresh.
- Helpers/fixtures: snapshot and route fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Preserve fail-closed freshness and declared/allowed cross-device transport checks. Reassess offline-cache product eligibility and truthful degraded labeling. ContextTransferClass::DeviceOnly is distinct from ADR0034 source-processing DeviceOnly; neither grants reasoning or Action authority.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-148`.

### CTX-149 — `cross_device_route_requires_declared_transfer_and_allowlist`

- Baseline: `crates/modules/context/tests/routing.rs:499-551`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `9960b5746a3f027d69d2ea06563d33bd745a0950eeeafccb789159e12beb6a6d`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Tablet attention route is cross-device; cases have no logical view policy, no declared transfer, or DeviceOnly while policy allows only OpaqueRelay.
- Inputs/actions: Route each runtime.
- Exact behavior and limits: Cross-device source denied in3 rows: missing logical policy, missing source transfer declaration, and DeviceOnly transfer disallowed by OpaqueRelay allowlist. No selected route in each.
- Named scenarios:
  - Missing policy => no selected route.
  - Policy but no transfer declaration => no selected route.
  - DeviceOnly despite OpaqueRelay allowlist => no selected route.
- Helpers/fixtures: route policy and transfer allowlist fixtures.
- Classification: **durable safety/property to re-prove after S2**
- Target disposition: Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.
- Native assertion sites: 3; exact source and per-site disposition in native audit JSON under `CTX-149`.

### CTX-150 — `explicit_source_disagreement_keeps_all_distinct_evidence`

- Baseline: `crates/modules/context/tests/routing.rs:553-593`; `#[test]`; integration module included by tests/integration.rs.
- Span SHA-256: `7b8d9543777b809b5e525e1040d0729db518c34ae1b9137b4f5447394a57b284`; file SHA-256: `22b561b76317f117baa98e575cf7007b28a5f8b1a125a171193cc34b1eaefc48`.
- Preconditions: Health-like connector/provider/evidence names wrap generic Personal mail.communication snapshots. Ready primary and Degraded secondary differ in source handle; secondary failure is SourceDisagreement. No Health transform evidence is exercised.
- Inputs/actions: Route both candidates and inspect selection and disagreement record.
- Exact behavior and limits: Ready primary and degraded secondary marked SourceDisagreement retain primary selection plus1 disagreement containing2 distinct evidence handles wellbeing:healthkit and wellbeing:health-connect. Fixtures still use mail.communication Personal descriptors; not a Health transform or HighlySensitive admission test.
- Named scenarios:
  - Primary remains selected.
  - One disagreement record with two distinct evidence handles; exact set asserted.
- Helpers/fixtures: snapshot, source disagreement, and health fixtures.
- Classification: **mixed durable property and obsolete/implementation representation; split by assertion**
- Target disposition: Preserve distinct-source disagreement provenance without treating a preferred source as all-clear. Reassess source ranking; these are generic Personal/mail fixtures with Health-like labels, not evidence of the mandatory Health transform.
- Native assertion sites: 4; exact source and per-site disposition in native audit JSON under `CTX-150`.
