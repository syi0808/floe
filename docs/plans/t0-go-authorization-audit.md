# T0 Go authorization semantic audit and correction

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Static reading only; no execution. This addendum corrects the imported Go ledger and must be merged into its machine-readable case coverage before deletion. It is not approval of all other Go suites.

## Scope and finding

Fully read `server/internal/authorization/authorization_test.go` (585 lines, including helpers), `producer_identity_test.go` (44), `source_service_test.go` (35), and root `fixtures/remote-authorization/v1.json`. These contain 16 test registrations. The imported ledger's missing-fixture finding is false: both `v1.json` and `producer-v1.json` exist. `../../../fixtures` from the authorization package resolves to the repository-root fixtures directory. Do not replace the referenced vector with the producer fixture.

The import counts declarations, but some cases have null setup/actions or collapse materially different table rows. In particular producer UUID cases and the shared negative vector require the expansions below. This audit records assertions rather than claiming they pass or automatically become target requirements.

## Shared setup/dependencies

`testEngine` constructs a fake wall/monotonic clock at Unix 1700000000, an in-memory issuer store, a deterministic Ed25519 key, an authenticated client/person/device, and a completed proof-of-possession/admin-approved issuer. `signProof` signs SignatureDomain concatenated with the exact challenge bytes. `testRequest` binds audience local-producer, purpose everyday_assistance, consumer day-canvas, a Gmail source incarnation/epoch7, grant incarnation/epoch3, mail.read resource, exact query digest, max10 items and4096 bytes. These literal fixture values are examples; exact signature/source/grant/query binding is the durable property.

`testStore` counts durable activation/revocation and can fail commits or block activation with channels. `testAuthority` can report inactive or a substituted source; `blockingAuthority` pauses during source validation; `failingAuthority` returns a callback error. These are test-only helpers. Production authority/service APIs are not removal candidates merely because these helpers use them.

## authorization_test.go registration audit

- Lines146–176, `TestEnrollmentRequiresPoPAndAdminAndDurableCommit`: the shared successful setup calls durable activation exactly once. A separate store failing activation with disk-full permits enrollment/proof completion but approval returns ErrUnavailable; subsequent admission is ErrDenied. The title does not establish independently tested invalid-proof/admin-denial cases; do not invent them.
- Lines179–229, `TestAdmissionReleaseExactSignatureSourceAndReplay`: valid issue/claim preserves the complete request; a repeated claim returns ErrReplay. Stage private result and valid release returns those exact bytes; repeated release returns ErrReplay. Independently an inactive source at claim and a current source with epoch incremented each return ErrDenied. Preserve all five outcome checks, not just the happy path.
- Lines232–259, `TestReleaseCancelDuringAuthorityCheckDenies`: after valid admission and staged secret, block release inside authority validation, cancel the release, then unblock. Pending claim returns ErrReplay; cancellation must win before data release.
- Lines262–290, `TestConcurrentReleaseHasOneWinner`: hold first signed release claim in authority callback; second identical claim returns ErrReplay; release first callback and it succeeds. No second successful consumption.
- Lines293–325, `TestAuthorityFailureCleansCheckingState`: failed callback during admission yields an error and consumes that challenge (later valid authority still ErrReplay). Repeat independently for staged release: callback fails and retry with valid authority is ErrReplay. A fresh challenge between the cases still admits successfully.
- Lines328–349, `TestEnrollmentQuotaSurvivesLocalConfirmation`: create and proof-complete MaxPendingPerClient distinct enrollments without admin activation. One additional BeginEnrollment returns ErrDenied. Local confirmation does not free pending quota. Record the loop bound by its source constant rather than fabricate a literal count.
- Lines352–381, `TestAdmittedActivationFinishesAfterTTL`: start admin approval and block durable activation; advance fake clock by ChallengeTTL+1ms; release commit. Approval succeeds, revocation count remains0, and new admission succeeds. TTL expiry does not roll back already admitted durable activation.
- Lines384–419, `TestRejectionLosesToAdmittedActivation`: block activation after approval starts, concurrently reject the same enrollment. Rejection must not return during the10ms observation; after releasing store, approval succeeds, rejection is ErrConflict, and issuer remains usable. The10ms timing observation is test machinery, not a new product latency requirement.
- Lines422–438, `TestReturnedBytesAndRequestsAreCopies`: copy original issued challenge bytes, mutate returned bytes and caller request resource from mail.read to calendar.read, then sign original bytes and claim. Claim succeeds and returned stored resource stays mail.read. The test checks defensive-copy semantics, not arbitrary mutation acceptance.
- Lines441–465, `TestStrictBytesAndModifiedFields`: issued canonical challenge parses. Flip final byte, sign the modified byte sequence, then claim against original challenge; ErrDenied is required even though the test does not require the modified JSON to parse. Append duplicate operation key to original object; ParseChallengeBytes returns ErrInvalid.
- Lines468–491, `TestExpiryAndStageCaps`: advance clock past ChallengeTTL by1ms and claim; ErrExpired. Restore clock, issue/claim a new challenge, then stage MaxStageBytesPerResult+1 bytes; ErrDenied. No assertion of allowed exact-cap behavior here.
- Lines494–513, `TestRevokeDurabilityAndOrdering`: revoke commit failing disk-full returns ErrUnavailable and existing issuer still admits. Successful subsequent revocation makes admission ErrDenied and re-enrollment of the same revoked key ErrDenied.
- Lines516–574, `TestSharedPositiveVector`: exact root fixture cases enumerated separately below. `mustHex` at577–585 fails the test on malformed fixture key; it is test-only parsing support.

## Shared positive/negative vector cases

Source: `fixtures/remote-authorization/v1.json`; test loops its `negative` entries, not `producer-v1.json`. All cases start from the same positive canonical admission challenge and fixture public key/signature unless specified. Preserve a static fixture copy or complete values in evidence before any fixture deletion, and audit Rust/other consumers separately.

1. `AUTH-VECTOR-positive`: ParseChallengeBytes accepts the positive bytes; raw-unpadded-base64 signature decodes; Ed25519 Verify(public key, SignatureDomain || exact challenge, signature) is true.
2. `AUTH-VECTOR-signature_modified`: decode the fixture's altered signature; decoding must succeed and Verify against original bytes must be false. The branch also fails if decoding fails.
3. `AUTH-VECTOR-challenge_modified`: remove final brace and append an unknown field suffix; parser must reject.
4. `AUTH-VECTOR-duplicate_key`: append a second operation field with release value; parser must reject.
5. `AUTH-VECTOR-padded_base64`: call strict decodeB64 with the signature plus '='; it must return an error.
6. `AUTH-VECTOR-changed_source`: replace person-runtime with other-runtime in challenge. Parser must accept the changed structure; original signature must no longer verify.
7. `AUTH-VECTOR-changed_grant`: replace grant ID6666… with9999…; parser must accept; original signature must fail.
8. `AUTH-VECTOR-changed_query`: replace exact query SHA256 with64'a' characters; parser must accept; original signature must fail.
9. `AUTH-VECTOR-changed_audience`: replace local-producer with other-producer; parser must accept; original signature must fail.
10. `AUTH-VECTOR-changed_operation`: replace admission with enrollment; parsing is allowed to reject (unlike the four preceding rows), but original signature must fail.
11. `AUTH-VECTOR-obsolete_policy`: append obsolete policy incarnation/epoch object; parser must reject.

Classification: exact signature binding, strict parsing and no-extra-authority properties are durable safety requirements. The fixture's concrete old request vocabulary must be assessed against the target signed protocol; it is not an excuse to weaken signature checking or preserve obsolete product consent. No missing-fixture uncertainty remains.

## producer_identity_test.go: seven explicit rows

Source8–42. Generate a valid ProducerIdentity record, then independently replace KeyID with each candidate, serialize the record and call DecodeProducerIdentity. Both its success boolean and `validConnectionID(candidate)` must equal the row's valid flag:

| Stable case | Candidate | Expected |
|---|---|---|
| AUTH-UUID-01 | aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa | accept both |
| AUTH-UUID-02 | AAAAAAAA-AAAA-4AAA-Baaa-AAAAAAAAAAAA | accept mixed case/valid variant both |
| AUTH-UUID-03 | aaaaaaaa-aaaa-1aaa-8aaa-aaaaaaaaaaaa | reject version1 both |
| AUTH-UUID-04 | aaaaaaaa-aaaa-4aaa-7aaa-aaaaaaaaaaaa | reject variant7 both |
| AUTH-UUID-05 | aaaaaaaa-aaaa-4aaa-caaa-aaaaaaaaaaaa | reject variantc both |
| AUTH-UUID-06 | aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaz | reject invalid hex both |
| AUTH-UUID-07 | empty string | reject both |

Classification: durable identifier validation consistency; preserve the exact allowed representation policy only where accepted by new wire contract. Do not replace this with a one-line claim that all UUIDs behave correctly.

## source_service_test.go: explicit envelope/view cases

`TestCalendarAuthorityRejectsCaseAliasesAndNestedUnknownFields`8–23:
- AUTH-CAL-01: schema_version1 and query(range_start1,range_end2,cursor empty,limit1) -> ValidateCalendarCaseExact true.
- AUTH-CAL-02: top-level SCHEMA_VERSION alias -> false.
- AUTH-CAL-03: valid schema_version but nested LIMIT alias with otherwise specified range/cursor -> false.
- AUTH-CAL-04: validateCalendarObject on range1..2,cursor empty,limit1 plus unknown:true using the exact allowed-key set -> nonnil error.
`TestBoundedCalendarViewCountsOnlyValidatedItems`25–34:
- AUTH-VIEW-01: map items=[{id:one},{id:two}] -> no error, count2, returned bytes valid JSON. It does not prove every semantic item field is validated.
- AUTH-VIEW-02: map items='not-an-array' -> nonnil error.
Classification: strict authority fields and bounded schema safety are durable; current function placement/name is legacy architecture. No real calendar or external provider was invoked by this audit.

## Deletion gate

This addendum closes the content review of these16 registrations only. Integrator must merge stable row IDs into the imported JSON and Markdown, remove false fixture ambiguity, ensure all relevant fixture consumers are represented, and rerun static registration/case reconciliation. Other Go suites still need separate completeness review. No source/test deletion has been performed by this audit.
