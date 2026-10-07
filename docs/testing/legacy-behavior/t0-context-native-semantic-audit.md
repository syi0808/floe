> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 Context native semantic audit

All150 registrations now have source-grounded behavior, exact assertion evidence and target dispositions. Canonical [Context ledger](t0-rust-behavior-ledger-context.md) has been corrected from imported v2. No source/test removal or runtime validation occurred.

- Baseline `3f4b407f8079d611224cd7adbef121f9e7e75e8e`;96 inline +54 integration in24 files.
- Explicit assertion macro sites: 475. A macro in a loop is1 source site; rows are preserved in exact test source and named scenarios.
- File hash mismatches: 0; span hash mismatches: 0.
- [Full JSON evidence](artifact-index.md#unpublished-artifacts) includes complete registered source, per-assertion source spans/hashes/classification/disposition and relevant helper blocks.

## Material corrections

- CTX-001, CTX-003: Older/newest turn setup corrected; actual newest messages are Korean 한글 and ASCII pair; exact serialized array-byte calculation preserved.
- CTX-006, CTX-012, CTX-016, CTX-024, CTX-031: Restrict to actual assertions: one view, separate accumulators, nil result/turn (not session), one history turn, same dependency re-retention.
- CTX-035, CTX-036, CTX-038, CTX-039, CTX-040, CTX-041, CTX-043, CTX-045, CTX-047, CTX-048, CTX-049: All purpose/contract/correction/role/count/class/catalog/hash/output-cap assertions recorded, not generic context preservation.32KiB is capped16384; usize::MAX invalid. One-card reversal is not multi-card ordering proof; forbidden-class union is not permission.
- CTX-032, CTX-061, CTX-121: Fake authorizer, synthetic signed preview and per-consumer fake grant scopes do not prove live grant-store or cryptographic-signature behavior.
- CTX-064: Distinct same-connection views are Mail and Logistics, not Mail and Work.
- CTX-078, CTX-080: Sequential awaited small reads; combined source/process/query mutation. No concurrency proof or isolated fence proof.
- CTX-084, CTX-099, CTX-100, CTX-108, CTX-113, CTX-115: Exact physical calendar-a/b names; accepted time request may precede grant.starts_at; request+7d..+14d; grant range shifts not mirror; Korean 개인 제목; actual constant MAX_TIMELINE_VIEW_DAYS.
- CTX-117: Child cancellation for stop/deadline/drop; parent request cancelled only for explicit stop. No implicit Run cancellation authorization.
- CTX-136, CTX-137, CTX-044, CTX-045: Retire recipient-consent/local Health bypass assertions; preserve raw/credential exclusion and route-free projection/catalog boundary. Health transform mandatory on both reasoning routes; not implemented by this audit.
- CTX-141: Final denial is duplicate Memory/Unavailable+Memory/Denied issues. It does not test lone Denied issue without memory.
- CTX-143, CTX-147, CTX-148, CTX-150: Source transfer policy differs from source-processing policy; Health-named fixtures may still be generic mail/Personal data; no Health transform or real provider conformance proof.

## Target policy vs baseline

- `docs/decisions/0034-gateway-reasoning-and-source-processing-authority.md`: Accepted target: Access-owned DeviceOnly/GatewayAllowed; insufficient source processing is SourceAccess review, never routing/fallback bypass. Health-specific local semantic transform is mandatory before either reasoning route; failure closed; HighlySensitive retained. Exact model-recipient consent is obsolete.
- `crates/contracts/context/src/lib.rs`:232-259: Observed baseline still declares LocalOnly and ApprovedRecipient; do not claim target cutover or reinterpret stored LocalOnly as GatewayAllowed.
- `crates/contracts/agent/src/context.rs`:24-165: Observed baseline InferencePolicyDecision still uses placements, TransferConsent and bounded_sensitive_projection; AgentContext forbids duplicate optional issue sources and memory plus memory issue.
- `crates/modules/context/src/application/personal_sources.rs`:461-601: Baseline selected Wellbeing read checks source/grant/subject and validates returned view, creates LocalOnly dependency; no source-owned transform-proof contract is established by the scoped tests.
- `crates/modules/context/src/application/routing.rs`:9-17: ContextTransferClass::DeviceOnly/OpaqueRelay/EncryptedDerivedSync/DeclaredServerProcessing is transport/source-view arbitration, not the accepted processing-policy type or reasoning selector.
- `docs/architecture/invariants.md`: Preserve one authority/path, provenance, CAS, pre-dispatch intent, cancellation direction, uncertain-write recovery.
- `docs/plans/2026-10-02-architecture-refactor.md`:401-437: T0 is static behavior preservation before deletion; no test rewrite/runtime validation until full structural closure; removal still requires coordinated gate.

## Safe removal boundary

calendar_lease.rs cfg(test) import9-10 and remote_sources.rs cfg(test) import28-29 are separate test-only items. coverage.rs production message_coverage442-468, observations.rs valid_native_subject_fingerprint747-752 and source_view.rs production trait impls469-485 lie AFTER test modules and must survive any removal.

The audit records behavior even where baseline policy is wrong. It does not approve deletion, certify tests passed, or authorize rewriting tests before structural closure. Exact assertion and row targets in JSON distinguish durable safety, product hypotheses, obsolete policy, and representation detail.

## Registration review notes

### CTX-001

`crates/modules/context/src/application/archive.rs:151-165`

Two first-turn messages (Independent question, Unknown private) followed by one Independent second-turn later message; archive read with allow-all returns exactly one message from the second turn. No assertion that first is newest.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-002

`crates/modules/context/src/application/archive.rs:167-213`

One dependent derived-summary message, exact constructed source dependency, authorization callback false; successful archive snapshot has no messages. Source authority must be rechecked before exposing derived text.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-003

`crates/modules/context/src/application/archive.rs:215-241`

Older old message then same newest turn containing 한글 and pair; max_bytes equals JSON message-array cost 2 + individual serialized message lengths + 1 comma; returns exactly two messages, both newest turn. This is one Korean message and one ASCII message, not two Korean messages.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-004

`crates/modules/context/src/application/calendar_connector.rs:302-330`

Established device-owned EventKit selected Home source, no mirror; connector projection is Pending, connection_id calendar-source, person_id exact person, empty views, and SourceConnection equals clone made before projection.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-005

`crates/modules/context/src/application/calendar_connector.rs:332-350`

Source owned by device-a projected from device-b is InvalidObservation.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-006

`crates/modules/context/src/application/calendar_connector.rs:352-396`

Current source has only Home, current subject fingerprint, mirror status contains Home plus stale Work; projection is Ready with exactly one view. Test does not explicitly inspect the emitted view resource identity.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-007

`crates/modules/context/src/application/calendar_lease.rs:140-187`

Retained observation is readable; retaining same dependency again conflicts; a fresh registry cannot observe it (StaleContext); altered query fingerprint with other dependency identity unchanged is StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-008

`crates/modules/context/src/application/calendar_lease.rs:189-205`

Retained observation deadline 10 ms, paused clock advanced 20 ms; read is StaleContext, without reopen repair.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-009

`crates/modules/context/src/application/consumed.rs:152-181`

Recorded exact dependency and scope validate before expiry; monotonic deadline equality, wall-clock expires_at equality, or failed authority callback each yields StaleContext; dependency lineage remains present after failures.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-010

`crates/modules/context/src/application/consumed.rs:183-205`

Scope with different-reader consumer cannot record dependency: InvalidInput, dependencies remain empty.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-011

`crates/modules/context/src/application/consumed.rs:207-238`

Same dependency/scope/deadline records idempotently; extending deadline by 20 seconds conflicts; one dependency remains and is stale at original deadline. Empty lineage validates without invoking authority callback.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-012

`crates/modules/context/src/application/coverage.rs:308-325`

Fresh accumulator starts Unknown; first host-independent fact establishes Independent; first host dependency establishes Dependent; after mark_unknown a later independent fact cannot erase Unknown.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-013

`crates/modules/context/src/application/coverage.rs:327-346`

Same observation with changed query fingerprint conflicts (ContextDependencyError::Conflict); prior Dependent coverage remains exactly unchanged.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-014

`crates/modules/context/src/application/coverage.rs:348-374`

Registry result dependency merge with same observation/different fingerprint is InvalidInput; prior result remains Some(Dependent) and byte-equivalent semantic coverage.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-015

`crates/modules/context/src/application/coverage.rs:376-400`

Two non-user facts for same existing turn, initial Independent classification, fold to one Independent turn entry.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-016

`crates/modules/context/src/application/coverage.rs:402-439`

Fresh user turn followed by existing turn with absent stored coverage returns VaultUnavailable and publishes no partial fresh-turn classification; nil result ID and nil turn ID separately return InvalidInput.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-017

`crates/modules/context/src/application/expert_sources.rs:306-345`

Requirements calendar_a/calendar_b share capability calendar.timeline but retain distinct calendar-a/calendar-b selected references; each read returns expected resource and floe.source.calendar access ID. Function name says duplicate keys, but keys actually differ.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-018

`crates/modules/context/src/application/expert_sources.rs:367-404`

work_a/work_b requirements share work.context but carry distinct work-a/work-b refs; remote fake returns Unavailable for each and records exactly [work-a, work-b].

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-019

`crates/modules/context/src/application/expert_sources.rs:424-468`

Declared calendar requirement selects Calendar driver once and returns Ready; undeclared other returns CapabilityDenied without second driver call.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-020

`crates/modules/context/src/application/expert_sources.rs:470-509`

Undeclared work under mail declaration yields CapabilityDenied; remote mail query limit 100000 yields InvalidInput before source selection. No real remote reader provided.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-021

`crates/modules/context/src/application/history.rs:69-84`

History turn sequence first,second,first issues reads exactly [first,second] and returns two Independent entries.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-022

`crates/modules/context/src/application/history.rs:86-99`

Nil session with valid turn, and valid session with nil turn, each InvalidInput; zero reader calls across both rows.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-023

`crates/modules/context/src/application/history.rs:101-109`

Empty history succeeds with empty BTreeMap and zero reader calls even when reader is configured VaultUnavailable.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-024

`crates/modules/context/src/application/history.rs:111-120`

Stored Dependent with empty dependencies is malformed and maps to StorageUnavailable, with no successful partial result.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-025

`crates/modules/context/src/application/history.rs:122-129`

Reader VaultUnavailable is propagated for nonempty two-turn request; test does not assert count of reads after the first failure.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-026

`crates/modules/context/src/application/leases.rs:270-283`

MAX_LIVE_LEASES one-byte reservations for one person exhaust count; next one-byte reserve BudgetExceeded; dropping all RAII reservations frees capacity.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-027

`crates/modules/context/src/application/leases.rs:285-298`

One person reserves MAX_LEASE_BYTES then next byte BudgetExceeded; second person can independently reserve MAX_LEASE_BYTES; dropping first reservation frees first person's capacity.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-028

`crates/modules/context/src/application/leases.rs:300-374`

Retained evidence accounting equals serialized dependency bytes + 64 fingerprint bytes; forced stored byte count MAX_LEASE_BYTES-1 blocks further same-person evidence, another person succeeds, expiry of held evidence then frees same-person capacity.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-029

`crates/modules/context/src/application/leases.rs:376-426`

MAX_LIVE_LEASES retained observations exhaust one person's count; another person succeeds; expiring one first-person observation permits a replacement.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-030

`crates/modules/context/src/application/leases.rs:428-485`

Exact observation returns same dependency and a*64 fingerprint; duplicate retention Conflict; changed query fingerprint StaleContext; retaining first registry's dependency in second process registry StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-031

`crates/modules/context/src/application/leases.rs:487-510`

Monotonic 10 ms lease becomes StaleContext after 20 ms; test then successfully retains same dependency with new 5-second deadline. It does not assert permanent ban on re-retention.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-032

`crates/modules/context/src/application/model_coverage.rs:227-250`

History turn merges one personal and one remote dependency; one route-neutral authorization with AcceptAll retains derived text and exactly both dependencies.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Retain exact live source/grant/category/purpose/consumer admission and fail-closed dependency retention. Replace LocalOnly/ApprovedRecipient with Access-owned DeviceOnly/GatewayAllowed; the approved-recipient fixture/row is historical evidence, not an authority requirement.

### CTX-033

`crates/modules/context/src/application/model_coverage.rs:252-269`

DenyAll for personal dependency yields a successful projection decision with retain_derived=false and empty authorized dependencies, rather than failing entire projection.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-034

`crates/modules/context/src/application/model_coverage.rs:271-288`

NeedsReview for personal dependency yields successful denied turn decision with empty authorized dependencies, rather than failing entire projection.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-035

`crates/modules/context/src/application/model_projection.rs:472-508`

Manager input everyday_assistance/User-facing text., correction try again, one memory and one evidence; output preserves purpose, contract, correction, counts 1/1, max_output_bytes 4096, [Personal], and projection_revision 1.

Classification: product hypothesis requiring reassessment. Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.

### CTX-036

`crates/modules/context/src/application/model_projection.rs:510-527`

Expert role with same live fixture and no history dependencies retains one memory and one evidence; coverage Independent.

Classification: product hypothesis requiring reassessment. Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.

### CTX-037

`crates/modules/context/src/application/model_projection.rs:529-562`

For HighlySensitive evidence alone and mixed with Personal calendar evidence, caller-declared Personal cannot downgrade: output classes exactly [Personal,HighlySensitive], evidence equals full input vector, coverage Independent, projection validation succeeds.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-038

`crates/modules/context/src/application/model_projection.rs:564-593`

Declared [HighlySensitive,Personal,HighlySensitive] normalizes to [Personal,HighlySensitive] and validates; helper unions Credential or DeviceOnlyRaw with Personal without discarding stricter/forbidden class. These helper calls do not authorize generic reasoning for forbidden classes.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-039

`crates/modules/context/src/application/model_projection.rs:595-637`

Cross-product persona=false/true and Manager/Expert/Learner/Finalization: memory or Persona contributes Personal for non-finalization alongside declared HighlySensitive; Finalization yields only declared HighlySensitive. With actual HighlySensitive evidence and declared Personal, Finalization removes evidence and reports [Personal].

Classification: product hypothesis requiring reassessment. Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.

### CTX-040

`crates/modules/context/src/application/model_projection.rs:639-656`

Learner with no history dependencies retains one fixture Preference/Fact memory and one evidence item, coverage Independent. The assertions do not inspect an under-review status or memory promotion.

Classification: product hypothesis requiring reassessment. Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.

### CTX-041

`crates/modules/context/src/application/model_projection.rs:658-683`

Finalization removes live memories/evidence and their manifest entries while preserving current-turn user plus settled tool exchange (length 2); coverage Independent.

Classification: product hypothesis requiring reassessment. Reassess role-owned live-context policy against the final model-input contract after S2. Preserve no class downgrade, faithful admitted input and truthful manifests. Do not promote fixture counts, projection revision 1, or the test name into an unreviewed product requirement.

### CTX-042

`crates/modules/context/src/application/model_projection.rs:685-786`

Coverage equals exact merge of history dependency, tool-result dependency, artifact dependency, and delegation receipt dependency; conversation includes all four distinct sources. No current live evidence is itself asserted as a coverage dependency.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-043

`crates/modules/context/src/application/model_projection.rs:788-822`

Identical authorized input reused twice yields exactly equal envelopes, coverage, and data classes but distinct projection_ref UUIDs.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-044

`crates/modules/context/src/application/model_projection.rs:841-883`

Recursive serialized key walk excludes exact keys recipient, endpoint, base_url, bearer, credential, credentials, consent, external_transfer_consent, allowed_placements, placement, route, remote_route, profile, profile_id, token. Static supported_placements on cards is deliberately allowed; prose is not checked for these words.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Preserve route-free Context input and secret/credential exclusion under the accepted architecture; retire obsolete consent machinery outside this projection. The absence assertions are not themselves obsolete. Re-prove the structural boundary rather than fossilizing a blacklist of spellings.

### CTX-045

`crates/modules/context/src/application/model_projection.rs:885-909`

Catalog tools produce two sorted capabilities: communication first, identity second; identity version 3, read_only=true, output class Personal, schema Some({}). No route consulted.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Preserve derivation from the admitted tool catalog, correct class/schema/revision and absence of model-route authority. Reassess literal fixture IDs and presentation ordering; this is not a requirement to keep obsolete recipient consent.

### CTX-046

`crates/modules/context/src/application/model_projection.rs:911-929`

Tool output_data_class mystery makes assembly InvalidInput.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-047

`crates/modules/context/src/application/model_projection.rs:931-973`

Reversing cards/tools leaves discovery and run_frame_sha256 identical; duplicate tool descriptor makes assembly InvalidInput. Fixture contains one card, so reversal does not prove multi-card ordering by itself.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-048

`crates/modules/context/src/application/model_projection.rs:975-994`

Catalog fixture produces exactly one active expert schedule and exactly one manifest card schedule.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-049

`crates/modules/context/src/application/model_projection.rs:996-1028`

usize::MAX max_output_bytes rejected InvalidInput; 32*1024 accepted but advertised attempt max_output_bytes capped to 16384; manifest contains 3 prompt components, 1 evidence, 1 memory. Not a test of total serialized projection byte size.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-050

`crates/modules/context/src/application/model_projection.rs:1030-1048`

Empty caller input_data_classes rejected InvalidInput even though live context has Personal evidence/memory.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-051

`crates/modules/context/src/application/observations.rs:640-699`

Publish fresh complete Home calendar observation against exact source authority/revision/device; authorization succeeds; reconfigure source to Work at revision 2 and old Home observation becomes CapabilityUnavailable.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-052

`crates/modules/context/src/application/observations.rs:701-744`

Publishing same valid observation as foreign-device against mac-local source is StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-053

`crates/modules/context/src/application/personal_lineage.rs:111-127`

Attention subject fingerprint deterministic for same person/device/view, differs for other device; query fingerprints differ when observation/process UUIDs differ.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-054

`crates/modules/context/src/application/personal_sources.rs:971-1107`

Reviewed Apple Contacts source grows from handle a/subject A to a+b/subject B before read; driver gets exact current [a,b], dependency permission resources contain one logical People view, observed source_resources [a,b], new source authority but unchanged grant ID/authority. Later subject A change makes reauthorization PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-055

`crates/modules/context/src/application/personal_sources.rs:1109-1194`

Apple Contacts driver reports subject B after read while connection is reviewed for A; result AccessReviewRequired, source fingerprint remains A, source authority unchanged, grant remains Active. No source adoption on observation drift.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-056

`crates/modules/context/src/application/personal_sources.rs:1196-1278`

Only live Android grant exists, selected source is Apple Contacts; returns one NeedsUserAction blocker targeting contacts.apple. Unselected platform grant cannot authorize selected Apple source; Android runtime support is not a target requirement.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-057

`crates/modules/context/src/application/projection.rs:109-141`

Independent retains derived text, Unknown does not; both return no authorized dependencies and authorizer call count remains zero.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-058

`crates/modules/context/src/application/projection.rs:143-180`

Two dependencies with allow-all retain exact full dependency vector; allowing only first denies whole projection, clears authorized dependencies, yet authorizer sees both IDs exactly once.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-059

`crates/modules/context/src/application/projection.rs:182-202`

First dependency denied and second authorization fails VaultUnavailable: fatal error propagated, both callbacks invoked. Denial does not short-circuit away later fatal error.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-060

`crates/modules/context/src/application/projection.rs:204-213`

Empty Dependent coverage yields InvalidInput before authorizer invocation.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-061

`crates/modules/context/src/application/remote_sources.rs:1021-1098`

Hosted logical Calendar source uses fake verified preview with physical leaves A,B; read returns Calendar view and one binding, logical calendar.timeline:calendar-account permission resource, exact A/B observed source resources, original grant ID/authority. Adding C stales dependency (PolicyDenied) without changing grant identity/authority.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-062

`crates/modules/context/src/application/remote_sources.rs:1100-1117`

Selected active A with unselected paused B returns one A binding and exactly one payload read.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-063

`crates/modules/context/src/application/remote_sources.rs:1119-1135`

Selected missing A with live B returns one blocker naming A and zero payload reads; no fallback source adoption.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-064

`crates/modules/context/src/application/remote_sources.rs:1137-1151`

Same connection has distinct Mail and Logistics logical grants; both views Ready, total two reads; these are not duplicate exact authority.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-065

`crates/modules/context/src/application/remote_sources.rs:1153-1166`

Request Work when only Mail grant exists yields SelectResource and zero reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-066

`crates/modules/context/src/application/remote_sources.rs:1168-1178`

Duplicate exact Mail target yields Conflict and zero payload reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-067

`crates/modules/context/src/application/remote_sources.rs:1180-1207`

Exact Mail grant with consumer schedule while caller assistant yields ReviewChangedSource and zero reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-068

`crates/modules/context/src/application/remote_sources.rs:1209-1267`

Four mismatched-scope rows each NeedsUserAction/ReviewChangedSource and zero reads: Derived vs Content category; Scheduling vs Assistant purpose; approved-recipient(model,Content) vs LocalOnly processing; Suggestion vs Read operation. Category/purpose/operation and fail-closed processing admission are durable; exact approved-recipient representation is obsolete under ADR0034.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Retain exact live source/grant/category/purpose/consumer admission and fail-closed dependency retention. Replace LocalOnly/ApprovedRecipient with Access-owned DeviceOnly/GatewayAllowed; the approved-recipient fixture/row is historical evidence, not an authority requirement.

### CTX-069

`crates/modules/context/src/application/remote_sources.rs:1269-1306`

After successful remote read, source incarnation rotates; dependency reauthorization PolicyDenied, grant authority unchanged, payload read count remains 1.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-070

`crates/modules/context/src/application/remote_sources.rs:1308-1322`

Fixture person changes after grant creation; foreign-person grant ignored, SelectResource blocker, zero reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-071

`crates/modules/context/src/application/remote_sources.rs:1324-1343`

Unselected aggregate has active A and paused B: one inline EnableObserve blocker for B, source_id floe.source.mail, and zero payload reads. Whole aggregate classified before I/O; no truncated success.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-072

`crates/modules/context/src/application/remote_sources.rs:1345-1361`

No source target yields one SelectResource blocker without connection ID, inline_resolution=false, zero reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-073

`crates/modules/context/src/application/remote_sources.rs:1363-1388`

Two admitted sources produce exactly two bindings whose connection IDs are A and B, with two payload reads.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-074

`crates/modules/context/src/application/remote_sources.rs:1390-1410`

Two admitted sources, B credential expires during read; result is one Reconnect blocker targeting B, never truncated Ready aggregate. Test does not assert payload read count/order.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-075

`crates/modules/context/src/application/remote_views.rs:399-418`

Merge Gmail timestamp20 complete and Microsoft timestamp30 incomplete; result two items ordered with Microsoft first, coverage_complete=false, source_handle starts multi:mail.communication:. Query limit8/budget constants are supplied; no overflow rejection exercised in this case.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-076

`crates/modules/context/src/application/remote_views.rs:425-476`

Calendar is remote view with exactly Metadata+Content categories; valid range1000..2000/null cursor/limit1 validates to (1,MAX_CALENDAR_CONTEXT_BYTES); matching empty view valid at1500; query end3000 against view end2000 yields StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-077

`crates/modules/context/src/application/service.rs:338-361`

ContextService.prepare performs zero reads; first read returns Ready payload {items:[]} and increments reader count to1.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-078

`crates/modules/context/src/application/service.rs:363-389`

Two small views retained simultaneously are both Ready and have equal payload. Calls are awaited sequentially; case does not prove execution concurrency or assert exact charged bytes directly.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-079

`crates/modules/context/src/application/service.rs:391-420`

Already cancelled request returns Cancelled; request prepared for another person returns PolicyDenied; both together invoke reader zero times.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-080

`crates/modules/context/src/application/service.rs:422-440`

FixtureReader wrong_binding=true produces mismatched source key other.view, process incarnation and query fingerprint together; read returns PolicyDenied. This combined mutation does not isolate each fence independently.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-081

`crates/modules/context/src/application/service.rs:461-499`

NeedsUserAction blocker for exact requested consumer passes through unchanged; otherwise identical blocker for fixture.other is PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-082

`crates/modules/context/src/application/source_candidates.rs:335-349`

For floe.tasks and memory.confirmed candidates, selection validates on mac-local and is StaleContext on other-device.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-083

`crates/modules/context/src/application/source_candidates.rs:351-375`

Intrinsic floe.tasks returns one candidate with connector+connection LOCAL_CONTEXT_CONNECTOR, owner device:mac-local, resource floe.tasks; candidate ID equals source_candidate_id and length64. relationships.confirmed_interactions returns none. No grant construction or source read occurs.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-084

`crates/modules/context/src/application/source_candidates.rs:377-472`

Native Calendar initially yields one logical calendar.timeline:calendar-account candidate; foreign person and device yield none. Adding calendar-b and then renaming calendar-a leaves candidate exactly equal; disconnect yields none.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-085

`crates/modules/context/src/application/source_candidates.rs:474-526`

Both calendar.google and calendar.microsoft hosted sources under server-owner yield one logical calendar.timeline:calendar-account candidate; growing physical calendars leaves candidate exactly unchanged.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-086

`crates/modules/context/src/application/source_candidates.rs:528-581`

No Contacts source yields none; reviewed Apple source yields one logical people.identity:contacts.apple.local candidate; adding b and changing subject leaves candidate exactly unchanged; disconnect yields none.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-087

`crates/modules/context/src/application/source_candidates.rs:583-647`

Attention macOS and Health Apple AllAvailable singleton sources each yield one logical view:connection candidate; subject change preserves candidate; switching resource mode to Selected yields none. Does not test Health transform evidence or model routing.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-088

`crates/modules/context/src/application/source_candidates.rs:649-703`

Two own Ready Gmail snapshots plus foreign snapshot: absent pinned remote_execution_owner yields no candidates; server:paired yields exactly mail-a/mail-b logical resources, distinct candidate IDs; foreign person ignored.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-089

`crates/modules/context/src/application/source_view.rs:283-299`

Canonical dependency, covering scope, 32-byte reservation, 5-second deadline create fresh payload view; first binding exactly preserves dependency/scope and payload equals payload.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-090

`crates/modules/context/src/application/source_view.rs:301-324`

Scope resource other/item does not cover dependency fixture/item: SourceView creation InvalidInput.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-091

`crates/modules/context/src/application/source_view.rs:326-350`

Arc-shared view holds MAX_LEASE_BYTES reservation after first Arc drops; next byte BudgetExceeded; remaining payload accessible; dropping last Arc frees full capacity.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-092

`crates/modules/context/src/application/source_view.rs:352-391`

Past deadline, other-person reservation, and same-person reservation from other process registry each yield StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-093

`crates/modules/context/src/application/source_view.rs:393-407`

Payload string with one-byte reservation yields BudgetExceeded.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-094

`crates/modules/context/src/application/source_view.rs:409-428`

RepeatingPayload with million-element serializer and one-byte reservation yields BudgetExceeded before million attempts. Counting writer avoids producing full output buffer; source/helper establishes implementation detail, assertion establishes early stop.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-095

`crates/modules/context/src/application/source_view.rs:430-446`

FailingPayload serialization yields InvalidInput and frees reservation, proven by immediately reserving MAX_LEASE_BYTES.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-096

`crates/modules/context/src/application/source_view.rs:448-466`

SlowPayload crosses 100ms deadline: StaleContext after one serialization attempt, and full capacity can be reserved again.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-097

`crates/modules/context/tests/calendar_timeline.rs:380-414`

Fixture grant selects Home only; timeline has one Home appointment, clipped start at grant starts_at and end now+90min, Synthetic class. Serialized payload excludes secret-id, Private home, Hidden work, private-provider, external_revision, can_modify, UTC. First read performs2 authority checks; repeated read equals prior view.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-098

`crates/modules/context/tests/calendar_timeline.rs:416-446`

Changing mirror source_connection_id to other-source does not itself defeat current Access authority: still one projected item. When Access denies, read is CapabilityDenied. Mirror provenance cannot override Access.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-099

`crates/modules/context/tests/calendar_timeline.rs:448-497`

Explicit request now+75..105min is used and item start clipped to75; a separate request starting one minute before grant.starts_at succeeds and preserves expanded start. Despite test title, grant time window is not asserted as an immutable request-time authorization boundary.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-100

`crates/modules/context/tests/calendar_timeline.rs:499-533`

Native observation answers requested next-week 7-day window beyond page mirror coverage; range preserved, coverage_complete=true, title Next week review.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-101

`crates/modules/context/tests/calendar_timeline.rs:535-556`

Observation reports changed native subject during read: StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-102

`crates/modules/context/tests/calendar_timeline.rs:558-592`

Server-projected first page preserves source_handle calendar.timeline:server, coverage_complete=false, next-page cursor, title, and namespaced UUIDv5 from opaque evidence ID; source_was_observed=true. Second page complete/no cursor, different evidence handle; cursor calls exactly [None,Some(next-page)].

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-103

`crates/modules/context/tests/calendar_timeline.rs:594-607`

Cursor next-page with only unpaged mirror and no observation returns CapabilityUnavailable, not mirror fallback.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-104

`crates/modules/context/tests/calendar_timeline.rs:609-698`

Seven-day range projects ordered First day, Third day all day, Fifth day; middle all-day interval length86400000ms; view range exactly grant day bounds.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-105

`crates/modules/context/tests/calendar_timeline.rs:700-709`

Eleven exact unique calendar IDs validate; appending duplicate calendar-0 fails validation. No four-calendar cap requirement.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-106

`crates/modules/context/tests/calendar_timeline.rs:711-749`

Seven rows: wrong person, wrong handle, unselected calendar, max_items0, max_bytes1, foreign authority snapshot, denied permission. Zero/below-min budgets return BudgetExceeded; other rows CapabilityDenied. Zero authority calls asserted only for wrong person, wrong handle, and max_items0.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-107

`crates/modules/context/tests/calendar_timeline.rs:751-797`

Healthy selected Home empty batch plus failed Work PermissionDenied: Home-only grant succeeds empty; adding failed Work makes CapabilityDenied. Partial source failure does not poison healthy explicit subset or allow requested failed source.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-108

`crates/modules/context/tests/calendar_timeline.rs:799-836`

Stale mirror clock+6min with still-live grant and uncovered tomorrow range each StaleContext; Access denied is CapabilityDenied. None becomes empty success.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-109

`crates/modules/context/tests/calendar_timeline.rs:838-873`

Authority generation change on second check makes read StaleContext; separate read and revalidate initially succeed, then Access denial causes CapabilityDenied at revalidate.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-110

`crates/modules/context/tests/calendar_timeline.rs:875-890`

Access subject changes on call index2: first read succeeds, cached second read StaleContext before egress.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-111

`crates/modules/context/tests/calendar_timeline.rs:892-906`

Missing/invalid native subject fingerprint from Access yields CapabilityDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-112

`crates/modules/context/tests/calendar_timeline.rs:908-940`

All-day event and long event -60..720min both clip to exact full requested window, with2 items; no false free interval inferred.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-113

`crates/modules/context/tests/calendar_timeline.rs:942-993`

Korean title repeated100 is UTF-8-safe bounded to<=256 bytes and ends ellipsis; MAX_TIMELINE_VIEW_ITEMS+1 overlapping records yields BudgetExceeded, not partial truncated calendar.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-114

`crates/modules/context/tests/calendar_timeline.rs:995-1011`

DST offsets -18000→-14400 and reverse produce23h and25h days; full-day and8h..16h subwindow grants validate in each row.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-115

`crates/modules/context/tests/calendar_timeline.rs:1013-1030`

Seven-day current and30-day historical shifted ranges validate; MAX_TIMELINE_VIEW_DAYS+1 range InvalidInput.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-116

`crates/modules/context/tests/calendar_timeline.rs:1032-1049`

Precancelled read Cancelled; deadline equal Instant::now DeadlineExceeded; both cause zero Access calls.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-117

`crates/modules/context/tests/calendar_timeline.rs:1051-1093`

Pending authority acquisition: explicit request stop Cancelled;50ms deadline DeadlineExceeded; dropping unresolved view future cancels operation child. All3 child tokens cancelled; request token is cancelled only for explicit stop. This preserves downward cancellation and does not authorize cancelling a Run on observer disposal.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-118

`crates/modules/context/tests/calendar_timeline.rs:1095-1135`

Four invalid grants: duplicate calendar,513-char ID, expiry now+6min, zero-duration range, all InvalidInput.4MiB raw mirror title yields BudgetExceeded before partial projection.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-119

`crates/modules/context/tests/calendar_timeline.rs:1137-1168`

EventKit mirror with fictional Personal-class title but no fresh native observation yields CapabilityUnavailable; no native mirror payload fallback.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-120

`crates/modules/context/tests/native_calendar_read.rs:258-282`

Native admission returns subject a*64 and one logical native-calendar resource for exact connection; Connections read twice, device check once, grant admission once.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-121

`crates/modules/context/tests/native_calendar_read.rs:284-318`

Schedule and Focus-Attention admissions share same grant ID; each returned scope has exactly its one requested consumer. Fixture creates consumer-specific scopes under shared grant ID, not a real multi-consumer grant-store integration.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-122

`crates/modules/context/tests/native_calendar_read.rs:320-350`

Current source configured with129 calendars admits all129 physical IDs under one logical permission resource. No128-calendar cap.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-123

`crates/modules/context/tests/native_calendar_read.rs:352-387`

Missing connection AccessReviewRequired with zero device checks; connection change during grant admit StaleContext with exactly one grant call.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-124

`crates/modules/context/tests/native_calendar_read.rs:389-439`

Invalid native fingerprint CapabilityDenied before any grant call; wrong execution-owner source and wrong consumer grant independently CapabilityDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-125

`crates/modules/context/tests/native_calendar_read.rs:441-476`

Empty complete native batch produces coverage_complete=true, empty items, exact query range, person and schedule consumer, current source authority;2 device checks/1 observation/2 grant calls and retained lease observation readable.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-126

`crates/modules/context/tests/native_calendar_read.rs:478-616`

Read11 current physical calendars under1 logical grant; grow to12 rotates SourceAuthority, old dependency StaleContext. New read empty valid view, last check+observe each receive exact sorted12 IDs, dependency has one logical resource plus exact12 physical resources/current authority, and new dependency reauthorizes.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-127

`crates/modules/context/tests/native_calendar_read.rs:627-671`

Missing/partial calendar batches CapabilityUnavailable; cursor next unsupported by native read also CapabilityUnavailable without an additional device check.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-128

`crates/modules/context/tests/native_calendar_read.rs:673-698`

Native primary batch PermissionDenied maps to AccessReviewRequired, no view, exactly1 grant admission call.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-129

`crates/modules/context/tests/native_calendar_read.rs:700-744`

Local-time all-day event Day off produces exactly1 item with local midnight start/end (not assumed24h), all_day=true and title preserved, within wider query.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-130

`crates/modules/context/tests/native_calendar_read.rs:746-802`

Native dependency initially reauthorizes; adding secondary calendar to current source makes old dependency StaleContext.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-131

`crates/modules/context/tests/native_calendar_read.rs:804-829`

Source connection changes during second grant admission after observation: StaleContext with1 device observation and2 grant calls.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-132

`crates/modules/context/tests/native_calendar_read.rs:831-855`

Observation generation2 after check generation1 yields StaleContext with exactly1 observation.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-133

`crates/modules/context/tests/native_context.rs:15-138`

Two tasks/notes exceed1-item budgets: optional acquisition returns no value plus BudgetExceeded issue. Foreign-person payload filed under requested owner's index makes raw task/note views StorageUnavailable; unreadable storage also propagates StorageUnavailable through optional acquisition. No partial or healthy-empty downgrade.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-134

`crates/modules/context/tests/native_context.rs:140-239`

Own incomplete High task Ship the proposal with17:00 deadline projects exact UUID/title/deadline/priority; completed task and foreign-person task excluded; native view validates; note exact UUID/excerpt/update timestamp preserved; repeated task projection with same handle/time identical.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-135

`crates/modules/context/tests/native_context.rs:241-271`

Two-task view with max_items1 and with max_bytes64 each fails; test asserts is_err only, not exact failure variant.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-136

`crates/modules/context/tests/policy.rs:54-106`

Historical policy: HighlySensitive encrypted local succeeds without bounded flag; Remote lacks consent→ConsentRequired; bounded=true still ConsentRequired; then Granted→success; bounded=false with Granted→PolicyDenied. Retire local transform bypass and model-recipient consent expectations; replace with source-owned Health transform proof plus unchanged HighlySensitive and DeviceOnly/GatewayAllowed processing admission, separate from model selection.

Classification: obsolete representation/compatibility assertion to retire. Retire the exact model-recipient/TransferConsent and local-unbounded HighlySensitive outcomes. Re-prove Health source-owned local semantic transform evidence before either local or Gateway reasoning, unchanged HighlySensitive classification, and Access-owned DeviceOnly/GatewayAllowed category admission; transform success never grants permission.

### CTX-137

`crates/modules/context/tests/policy.rs:108-121`

Cross-product DeviceOnlyRaw/Credential × DeviceLocal/Remote, even bounded=true+consent Granted, each PolicyDenied. Durable generic reasoning exclusion remains; consent/placement plumbing is obsolete scaffolding.

Classification: durable safety/property to re-prove after S2. Preserve rejection of Credential and DeviceOnlyRaw before either generic local or Gateway reasoning; remove historical TransferConsent/placement scaffolding without removing the data-class prohibition.

### CTX-138

`crates/modules/context/tests/policy.rs:123-157`

Evidence expires at100: authorize at100 StaleContext; empty purpose PolicyDenied; restored test purpose with projection_version0 PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-139

`crates/modules/context/tests/policy.rs:159-203`

Personal confirmed memory with valid metadata and evidence cleared succeeds at100; Synthetic policy+SyntheticOnly protection denies Personal memory; valid_until100 at100 StaleContext; clearing source_refs after removing expiry yields PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-140

`crates/modules/context/tests/policy.rs:205-232`

Evidence text MAX_CONTEXT_EVIDENCE_BYTES+1 yields BudgetExceeded; duplicate identical source_handle yields PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-141

`crates/modules/context/tests/policy.rs:234-276`

One optional Memory/Unavailable issue with no memories succeeds; adding memory while issue remains PolicyDenied; removing memory then adding duplicate Memory/Denied issue also PolicyDenied.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-142

`crates/modules/context/tests/routing.rs:145-187`

Ready older Microsoft beats fresher Degraded Google; deduplicated_candidates=1. When both Degraded, fresher Google wins. No isolated provider-preference tie case in this registration.

Classification: product hypothesis requiring reassessment. Reassess source arbitration/ranking and provider tie-break policy; retain current-authority, freshness and source-scope checks. This is source-view arbitration, not Gateway-primary reasoning fallback.

### CTX-143

`crates/modules/context/tests/routing.rs:189-225`

Two distinct logical routes point to same physical mail:shared evidence; earlier a-personal-mail retained, duplicate b-duplicate-mail unavailable; oversized100000-byte Health-named fake also unavailable. Assertions prove conformity and physical dedup, not Android/Health production semantics.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Re-prove the boundedness, identity, deterministic admitted projection, failure or resource-lifetime property actually asserted. Exact IDs, sort tie-breaks, numeric fixture thresholds and implementation mechanisms remain baseline evidence; reassess those details rather than retaining them mechanically.

### CTX-144

`crates/modules/context/tests/routing.rs:227-274`

Equal-fresh Ready Google/Microsoft Calendar logical candidates with provider preference Google first: one Google selected, deduplicated_candidates1, independent of reversed input snapshot order.

Classification: product hypothesis requiring reassessment. Reassess source arbitration/ranking and provider tie-break policy; retain current-authority, freshness and source-scope checks. This is source-view arbitration, not Gateway-primary reasoning fallback.

### CTX-145

`crates/modules/context/tests/routing.rs:276-330`

Location policy invoking present phone vs absent designated tablet and present desktop selects phone despite newer other snapshots; consumer_device_id phone.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-146

`crates/modules/context/tests/routing.rs:332-377`

Attention policy interaction device desktop chooses one desktop observation despite much newer phone; consumer desktop. Device-scoped attention must not become person-global by freshness.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-147

`crates/modules/context/tests/routing.rs:379-441`

Tablet offline: stale121s and fresh1s DeviceOnly observations produce no route under max_age120s; with declared+allowed OpaqueRelay, one fresh offline cached source selected as DegradedOfflineCache, stale source still excluded. ContextTransferClass::DeviceOnly here is transport policy, distinct from ADR0034 source processing DeviceOnly.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Preserve fail-closed freshness and declared/allowed cross-device transport checks. Reassess offline-cache product eligibility and truthful degraded labeling. ContextTransferClass::DeviceOnly is distinct from ADR0034 source-processing DeviceOnly; neither grants reasoning or Action authority.

### CTX-148

`crates/modules/context/tests/routing.rs:443-497`

Any-scope offline tablet with allowed EncryptedDerivedSync alone selects DegradedOfflineCache; adding older Ready server source selects server as Fresh. Server fixture retains mail.communication view despite calendar-named connector/logical label.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Preserve fail-closed freshness and declared/allowed cross-device transport checks. Reassess offline-cache product eligibility and truthful degraded labeling. ContextTransferClass::DeviceOnly is distinct from ADR0034 source-processing DeviceOnly; neither grants reasoning or Action authority.

### CTX-149

`crates/modules/context/tests/routing.rs:499-551`

Cross-device source denied in3 rows: missing logical policy, missing source transfer declaration, and DeviceOnly transfer disallowed by OpaqueRelay allowlist. No selected route in each.

Classification: durable safety/property to re-prove after S2. Re-prove the actual asserted safety/property at its canonical owner after S2. Preserve identity, source-owned authority, boundedness, provenance, freshness and cancellation direction as applicable; fixture literals and helper layout are evidence, not mandatory implementation shape.

### CTX-150

`crates/modules/context/tests/routing.rs:553-593`

Ready primary and degraded secondary marked SourceDisagreement retain primary selection plus1 disagreement containing2 distinct evidence handles wellbeing:healthkit and wellbeing:health-connect. Fixtures still use mail.communication Personal descriptors; not a Health transform or HighlySensitive admission test.

Classification: mixed durable property and obsolete/implementation representation; split by assertion. Preserve distinct-source disagreement provenance without treating a preferred source as all-clear. Reassess source ranking; these are generic Personal/mail fixtures with Health-like labels, not evidence of the mandatory Health transform.

