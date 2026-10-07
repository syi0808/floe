> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 provider/native/diagnostics behavior ledger

Baseline `3f4b407f8079d611224cd7adbef121f9e7e75e8e`, observed branch `refactor/architecture-20261002`. This partition captures legacy evidence before any authorized removal; it neither claims execution success nor approves a deletion range. Only these new documents were written. No source, test, manifest, fixture, credentials or data changed; no build/compiler/formatter/test/checker ran.

## Completed evidence

- [Native/diagnostics](artifact-index.md#unpublished-artifacts): 10 registered cases.
- [Provider control/pairing](artifact-index.md#unpublished-artifacts): 28 registered cases.
- [Model adapters/rendering](artifact-index.md#unpublished-artifacts): 36 registered cases.
- [Sources and live diagnostics](artifact-index.md#unpublished-artifacts): 22 registered cases.
- [Read/registration/manifest/consumer audit](artifact-index.md#unpublished-artifacts): all 39 owned files, 11,475 lines, 423,534 bytes read in full; all hashes match the frozen prior ledger. 96 actual test attributes reconcile one-to-one with case narratives: zero missing, duplicate or extra case records. 78 inline and 18 integration cases; 15 whole-file macOS cases, one additionally ignored real-model case, and one Unix-only identity case. Helpers add zero to counts. The 84 helper anchors supplement, rather than replace, their narrative behavior descriptions.

Every case records original setup/input/actions, asserted outcome, explicit table/sequential subcases, failure/lifecycle caveats, dependencies, exact source symbol/span/hash and target classification. These remain baseline assertions, not automatically accepted target requirements. No source execution or Cargo-derived discovery is claimed.

## Review-critical distinctions

1. Exact recipient consent, root-profile defaults, server-local-as-device recipient semantics, direct delegate wire variants, schema-1 model envelopes, transfer flags and fabricated accounting are obsolete or reassessment candidates. Keep independent source-processing authority, verified Gateway binding and dispatch/release fences.
2. The test named `external_model_response_is_suppressed_after_recipient_consent_revocation` actually removes the saved pairing after two reads. The ledger captures that concrete mutation.
3. `canonical_provider_public_surface_contains_no_secret` only checks a hand-written list of field-name strings. It does not inspect the API or prove secrecy. Use actual visibility/contracts/redaction proof in S3.
4. Native `changed_subject` fixture first returns subject `a`, then rejects the events observation for subject `b`. The corresponding test establishes closed failure, not independently measured absence of that events call. The malformed-and-oversized named case only exercises a malformed stamp; other registered cases cover bounds.
5. The native fixture's side-effect-before-timeout path is shared recovery evidence. `NativeCalendarFixture.swift` is consumed by provider native tests, App native-actions tests and Flutter native-calendar integration through the shared builder. Keep it and the builder until all consumers are coordinated. Gmail `ready_snapshot.json` is also shared with Connections and Flutter.
6. `CurrentSavedConnectionStore::fixed` is used by the active opt-in `local_model_smoke/manager_guidance.rs` diagnostic, as well as tests. It is not exclusively test support. Keep until the diagnostic caller cutover.
7. The ignored real OAuth model case remains an explicitly opt-in diagnostic with separate credential/model approval. Its exact-recipient assertions need replacement; T0 does not authorize running it, configuring/refreshing credentials or deleting live support.
8. Production definitions continue after inline test modules in `control/authorization.rs` and `sources/server.rs`. Never remove the remaining file tail when deleting tests. The native broker, keychain, panic hook, redaction and dormant Android file are product/platform code, not test infrastructure.

## Manifest closure and next gate

The three packages use implicit Cargo automatic test discovery, no explicit `[[test]]`, no custom feature tables/required-features and no executable rustdoc code fences. Provider `tests/support/live_model.rs` is an external module of `live_server_access`; it is not another integration binary. Unit test imports and helper methods outside test modules are identified in the audit. Provider dev dependencies still support retained live diagnostics; `floe-kernel` is deliberately promoted by the accepted S1 contract plan. No manifest should be pruned solely from this count.

The parent must review these narratives and preservation edges before authorizing exact recoverable test-only deletions. S3 replacements follow G2 structural closure and accepted invariants, not copied legacy expectations. This is the provider partition of the existing central T0 gate, not a separate execution plan.
