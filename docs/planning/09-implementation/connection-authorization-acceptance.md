# Connection Authorization — Acceptance Ledger

This ledger records implemented paths, not just available types. Last reviewed 2026-09-12.
P7 is incomplete while required production paths below remain pending. Live OS/provider checks
require separate explicit consent and disposable resources; synthetic tests do not replace them.

| Production path | Reviewed implementation | Outstanding acceptance gate |
| --- | --- | --- |
| Native Calendar consent | Core source stamp, finite subset, consumer epoch; durable OS subject mapping implemented under review | Actual selected-subset host preview and Flutter confirmation; mobile review binding |
| Native Calendar admission | Encrypted grant/mapping; invocation/query leases implemented under review | Result-transaction permission regression; production mobile hook |
| macOS native read | Bounded direct EventKit adapter; expected subject checked before event query; root synthetic suite 13/13 | Complete consent preview integration and real OS validation |
| iOS/Android native read | Native bounded readers and application-lifetime broker implemented; root broker 12/12, Android Kotlin build passed | Actual Calendar tool/review hook; iOS SDK build unavailable |
| General conversation | Governed store CAS/projection; monotonic unknown; restored-turn filtering | Positive production source resolvers; opaque replay optimization disabled |
| Calendar history/private state | Consumed lease coverage implemented in scoped result transaction; private state is metadata-only | Positive persisted-history resolver; result-transaction permission regression |
| Revocation cleanup | Durable bounded scan/outbox and merge-before-sanitize accepted in `afaa9fb`; root cleanup 6/6, integration 18/18 | Worker idle integration and final combined-tree regression suite |
| Compaction and learning | Transactional coverage union and independent evidence gates; positive/negative tests | Source-learning consent/retention policy; no implicit Read-to-Learning permission |
| Remote issuer enrollment | Durable console approval; producer identity `2214b16`, encrypted owner signer `9450d72`; FFI/UI implemented and focused tests passed | Integrated host/client commit and final route adoption |
| Remote Calendar route | Provider-authoritative Google/Microsoft subjects and cached source fence accepted in `5488c8e` | Existing read route still needs explicit remote consent and online owner admission/read/release |
| Remote Mail route | Existing Person ownership checks | Exact source selection; remove Gmail-to-Microsoft fallback; owner admission/release |
| Remote Work route | Existing Person-owned runtime aggregation | One exact authorized source per request, explicit Core aggregation and coverage |
| Remote Logistics route | Existing owned runtime/Gmail aggregation | Exact selected source, authorization and bounded partial-coverage parity |
| Contacts/Wellbeing/Attention/Feasibility | Existing domain-specific projection and retention paths | Shared grant/lease/dependency boundary with positive and negative source fixtures |
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
correction is now implemented, but the actual selected-subset preview and user confirmation path is
still pending. First-read pinning is not consent. The P2 Core source-authority milestone does not
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
