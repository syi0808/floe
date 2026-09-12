# Connection Authorization — Acceptance Ledger

This ledger records implemented paths, not just available types. Last reviewed 2026-09-12.
P7 is incomplete while required production paths below remain pending. Live OS/provider checks
require separate explicit consent and disposable resources; synthetic tests do not replace them.

| Production path | Reviewed implementation | Outstanding acceptance gate |
| --- | --- | --- |
| Native Calendar consent | Exact reviewed source stamp, finite subset, durable consumer epoch and scoped commit tests | Broader non-Calendar policy adoption |
| Native Calendar admission | Encrypted grant and atomic registry mapping; lazy source admission | Immutable invocation/query-bound lease; cleanup worker |
| macOS native read | Bounded direct EventKit adapter wired at tool invocation, cancellation and generation fixtures | Pinned immutable lease and real OS validation |
| iOS/Android native read | Existing platform publication remains | Tool-time acquisition broker, disposal/late-callback protocol and lifecycle tests |
| General conversation | Governed store CAS/projection; monotonic unknown; restored-turn filtering | Positive production source resolvers; opaque replay optimization disabled |
| Calendar history/private state | Conservative history filtering remains | Real consumed-view dependencies in result/session/private-state transaction |
| Compaction and learning | Transactional coverage union and independent evidence gates; positive/negative tests | Source-learning consent/retention policy; no implicit Read-to-Learning permission |
| Remote issuer enrollment | Durable console approval, proof of possession, revocation and quarantine | Producer pinning, encrypted owner signer and explicit client review flow |
| Remote Calendar route | Existing exact connection and mirror revision checks | Online owner admission/release, authoritative provider subject and source epoch adoption |
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
