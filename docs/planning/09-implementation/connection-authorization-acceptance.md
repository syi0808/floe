# Connection Authorization — Acceptance Ledger

This ledger records implemented paths, not just available types. Last reviewed 2026-09-12.
P7 is incomplete while required production paths below remain pending. Live OS/provider checks
require separate explicit consent and disposable resources; synthetic tests do not replace them.

| Production path | Reviewed implementation | Outstanding acceptance gate |
| --- | --- | --- |
| Native Calendar consent | Durable OS subject mapping and selected-subset preview-before-confirm implemented; root Flutter 26/26 | Final combined-tree review and native platform validation |
| Native Calendar admission | Encrypted grant/mapping; invocation/query leases and mobile hook implemented under review | Final FFI positive broker integration and combined-tree regression |
| macOS native read | Bounded direct EventKit adapter; expected subject checked before event query; root synthetic suite 13/13 | Complete consent preview integration and real OS validation |
| iOS/Android native read | Native bounded readers, application-lifetime broker and Calendar tool/review hook implemented; root broker 12/12, Android Kotlin build passed | Final tool-through-broker positive integration; iOS SDK build unavailable |
| General conversation | Governed store CAS/projection; monotonic unknown; restored-turn filtering | Positive production source resolvers; opaque replay optimization disabled |
| Calendar history/private state | Consumed lease coverage implemented in scoped result transaction; permission regression passes; private state is metadata-only | Positive live-evidence persisted-history resolver |
| Revocation cleanup | Durable bounded scan/outbox and merge-before-sanitize accepted in `afaa9fb`; root cleanup 6/6, integration 18/18 | Worker idle integration and final combined-tree regression suite |
| Compaction and learning | Transactional coverage union and independent evidence gates; positive/negative tests | Source-learning consent/retention policy; no implicit Read-to-Learning permission |
| Remote issuer enrollment | Durable console approval; producer identity `2214b16`, encrypted owner signer `9450d72`; FFI/UI implemented and focused tests passed | Integrated host/client commit and final route adoption |
| Remote Calendar route | Identity foundation `5488c8e`; admission/read/release, encrypted signer and consent UI implemented under review | Authenticated producer source preview, actual model-recipient binding and production positive circuit |
| Remote Mail route | Existing Person ownership checks | Exact source selection; remove Gmail-to-Microsoft fallback; owner admission/release |
| Remote Work route | Existing Person-owned runtime aggregation | One exact authorized source per request, explicit Core aggregation and coverage |
| Remote Logistics route | Existing owned runtime/Gmail aggregation | Exact selected source, authorization and bounded partial-coverage parity |
| Contacts/Wellbeing/Attention/Feasibility | Attention atomic encrypted grant foundation has six lifecycle tests; other source adapters remain pending | Trusted native Attention broker, final-egress fence, all domain positive and negative fixtures |
| Action proposals and execution | Existing approval, durable attempts and uncertain-outcome reconciliation | Current dependency/action-policy admission and revoke/dispatch race tests |
| Recovery UI | Strict stage-aware failure envelope, session-preserving source review and generic stale context guidance | Exact affected-source attribution, eligible read-retry producers, final route/action matrix |

## Required final evidence

- Rust workspace tests, focused encrypted rollback/reopen tests, and rebuilt C ABI tests.
- Go full suites and authorization/console race tests, plus shared signed protocol vectors.
- Flutter analyze and tests, separating independently reproduced baseline failures from regressions.
- Native package and synthetic acquisition/action suites, including default-parallel execution.
- Every protected route has an exact-source positive test and a pre-read denial test; unavailable
  stubs are explicitly pending, never counted as adopted providers.
- Source A revocation preserves independent conversation and source B; revoked data cannot re-enter
  through summaries, provider replay, accepted memory, private state, archives or proposals.
- Existing remote routes are not claimed to use the new owner engine until wired. New enrollment
  APIs or signed fixtures alone do not protect the pre-existing read endpoints.

See [delivery plan](connection-authorization-delivery.md) for reviewed commits and validation counts,
and [runtime design](connection-authorization-runtime.md) for the complete scenario matrix.

## Audited storage boundaries

`ExpertPrivateState` currently contains only schema/revision, invocation count and last invocation
identity. It does not retain a source-derived text/blob or feed such a blob to the model. Do not add
a new private-state payload mechanism merely to implement a resolver for it. Current lineage work
must instead cover the actual session delegation/result/proposal bytes and their receipt transaction.
The metadata-only private state remains a CAS/deduplication mechanism, never an authorization proof.
Any future source-bearing private state requires dependency coverage before reuse.

The native subject audit found a remaining cross-conversation gap: a lease's EventKit subject
fingerprint is invocation-local, while the durable grant currently records only Core's mirror source
authority. A native account/source replacement preserving the selected calendar IDs can therefore
escape that durable comparison. Required correction: persist the explicitly reviewed host fingerprint
in the encrypted grant mapping and compare fresh host evidence before acquisition. That backend
correction and selected-subset preview-before-confirm are now implemented, including frozen retry,
cosmetic revision drift and A-preview to B-confirmation tests. First-read pinning is not consent.
Final native/FFI integration remains distinct from this UI checkpoint. The P2 milestone does not
close this OS identity gate by itself.

Provider identity checkpoint `5488c8e` verifies Google subject and Microsoft signed tenant/subject
evidence rather than email labels. Source consume holds a cache-only fence, not a token-network lock;
credential binding/generation, logout/persistence failure and late-load replacement are tested.
Persisted verified identities hydrate as unverified audit state and require fresh provider preflight.
Root full Go and Google/Microsoft/authorization/console race suites passed. This foundation does not
protect the pre-existing Calendar/Mail/Work/Logistics read endpoints by itself.

iOS Calendar source passes Swift syntax parsing. The installed Xcode lacks the required iOS 26.2
SDK/destination, so this is not a successful iOS build. Android Kotlin compilation succeeded after
removing only regenerable Rust incremental build artifacts to recover disk space.

Root's current Core library checkpoint passes 161 tests (`/tmp/floe-p7-root-core-next.log`). This
does not establish whole-workspace or P7 acceptance: agents continue changing the tree. The remote
signer fixture exercises both Google and Microsoft, but cannot substitute for a real host-to-producer
positive test. Attention's current generic publication and caller-provided subject cannot establish
native authority; its trusted broker correction remains in progress.
