# Connection Authorization — Acceptance Ledger

This ledger records implemented paths, not just available types. Last reviewed 2026-09-12.
P7 is incomplete while required production paths below remain pending. Live OS/provider checks
require separate explicit consent and disposable resources; synthetic tests do not replace them.

## Active development checkpoint

The feature-first workflow in `AGENTS.md` supersedes the exhaustive test matrices in older planning
notes. Complete a usable path, exercise it directly, and keep focused regressions for actual bugs
and essential authorization invariants. Counts below older checkpoints are historical evidence,
not a requirement to recreate every test before implementing the next feature.

- Calendar history now revalidates revoked dependencies at dependent commit while retaining prior
  user-visible history. The current Calendar integration group passes 37 tests.
- Agent action approval, policy, admission and uncertain-result recovery are owned by the encrypted
  vault. The Core row is a projection. The native EventKit adapter checks the reviewed subject at
  preflight and again before saving; ordinary mirror revision changes do not invalidate consent.
- A connected `/focus` command requests a proposal against exactly one reviewed EventKit calendar;
  it does not dispatch writes. Inspect/review/execute first consult the unlocked vault, not the
  potentially stale Core projection. Native source matching was exercised through the existing
  dylib fixture; live provider writes have not been performed.
- Attention uses a trusted host broker and final-save liveness validation. Contacts now has native
  selected-identity inspection, explicit review and encrypted selected-handle persistence; the
  generic display publication remains insufficient to authorize a model read.
- Wellbeing and Feasibility remain explicitly unavailable in governed native acquisition. Mobile
  encrypted-vault key storage is also not implemented by the current macOS-only keyring adapter.
  These are implementation gaps, not tests that can be waived by using a synthetic provider.
- Contacts capability and Relationships expert now resolve the unique active reviewed grant,
  reload the saved selection, acquire through the broker and record the dependency for model/CAS
  validation. Mobile secure-key support still gates a real device run.
- Mail/Work/Logistics now have settings Inspect → Review → Pause controls and Vault worker
  handlers. Review binds the frozen signed source, provider identity, revision and producer;
  model authorization checks fresh source and consumer policy. The approved processing recipient
  is the pinned producer audience, not an arbitrary external inference provider.
- Direct macOS smoke check: rebuilt and launched the app with fresh disposable Floe person data
  after the old schema failed to decode. Calendar startup and Settings navigation work. Locked
  Vault action permissions show explicit unlock guidance rather than a save/provider error.
- The direct check exposed a Keychain read blocking the main thread. Credential work now runs
  off-main, and reads time out without deleting credentials. Remote server settings visibly
  exit loading with a credential-store error on this machine; actual pairing remains unverified.
- Focused UI regressions pass: 13 action-review and 17 settings/server checks. The action-review
  scrollbar-only golden was removed; narrow-layout readability and overflow checks remain.
- The stable Core integration checkpoint passes 169 tests and FFI passes 66. Positive remote
  expert artifact checks use an injected reader, not a claim of end-to-end signed authorization.
  The signed Calendar fixture uses a bounded 10-second socket timeout to tolerate suite contention.
  The Go server suite passes;
  native source fixture checks and signed Calendar exchange are synthetic checks, separate
  from the direct macOS startup/settings exercise and unperformed external provider writes.
- Next implementation order: mobile secure-key provider, governed Feasibility acquisition,
  then governed Wellbeing acquisition. These are usable-path work, not additional test matrices.

## Historical checkpoints

The following table and audit notes describe earlier reviews; the active checkpoint above records
newer integration work. Old pending items are not automatically current blockers.

| Production path | Reviewed implementation | Outstanding acceptance gate |
| --- | --- | --- |
| Native Calendar consent | Durable OS subject mapping and selected-subset preview-before-confirm implemented; root Flutter 26/26 | Final combined-tree review and native platform validation |
| Native Calendar admission | Encrypted grant/mapping; invocation/query leases and mobile hook implemented under review | Final FFI positive broker integration and combined-tree regression |
| macOS native read | Native source boundary accepted in `5417858`; actual Swift warnings-as-errors typecheck and 25 native assertions pass; synthetic adapter 13/13 | Final integrated host commit and separately consented real OS validation |
| iOS/Android native read | Native bounded readers, application-lifetime broker and Calendar tool/review hook implemented; root broker 12/12, Android Kotlin build passed | Final tool-through-broker positive integration; iOS SDK build unavailable |
| General conversation | Governed store CAS/projection; monotonic unknown; restored-turn filtering | Positive production source resolvers; opaque replay optimization disabled |
| Calendar history/private state | Bounded live evidence and positive two-turn history implemented; root Calendar 36/36; private state is metadata-only | Historical dependency expiry/revocation during model and final-commit fence |
| Revocation cleanup | Durable bounded scan/outbox and merge-before-sanitize accepted in `afaa9fb`; root cleanup 6/6, integration 18/18 | Worker idle integration and final combined-tree regression suite |
| Compaction and learning | Transactional coverage union and independent evidence gates; positive/negative tests | Source-learning consent/retention policy; no implicit Read-to-Learning permission |
| Remote issuer enrollment | Durable console approval; producer identity `2214b16`, encrypted owner signer `9450d72`; FFI/UI implemented and focused tests passed | Integrated host/client commit and final route adoption |
| Remote Calendar route | Signed producer source preview, explicit grants and local Foundation processing implemented under review | Original-request challenge binding and actual host-to-producer positive circuit |
| Remote Mail route | Existing Person ownership checks | Exact source selection; remove Gmail-to-Microsoft fallback; owner admission/release |
| Remote Work route | Existing Person-owned runtime aggregation | One exact authorized source per request, explicit Core aggregation and coverage |
| Remote Logistics route | Existing owned runtime/Gmail aggregation | Exact selected source, authorization and bounded partial-coverage parity |
| Contacts/Wellbeing/Attention/Feasibility | Attention atomic encrypted grant foundation has six lifecycle tests; other source adapters remain pending | Trusted native Attention broker, final-egress fence, all domain positive and negative fixtures |
| Action proposals and execution | Existing approval, durable attempts and uncertain-outcome reconciliation | Current dependency/action-policy admission and revoke/dispatch race tests |
| Recovery UI | Strict stage-aware failure envelope, session-preserving source review and generic stale context guidance | Exact affected-source attribution, eligible read-retry producers, final route/action matrix |

## Validation approach

- Build the affected application and exercise the changed user flow before expanding tests.
- Run the relevant existing regression group once the path is stable. Use broader suites at a
  coherent integration checkpoint, not repeatedly during each agent's partial edits.
- Preserve regressions for authorization bypass, revoked data release, data loss and duplicate
  effects. Consolidate redundant serialization/implementation-detail tests rather than growing
  a complete scenario matrix for each new helper.
- Report direct checks separately from synthetic fixtures and unavailable platform/provider paths.
  Enrollment helpers and signed fixtures alone are not remote-route adoption.

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

The trusted Attention broker and frozen review are now implemented, but encrypted end-to-end
positive admission and the final dependent-save fence are still pending. The root full FFI run
found a failed legacy personal-delegation fixture and a hung unavailable-provider fixture; the hung
test process was terminated and the fixtures were assigned for correction. That run is not a pass
and those failures are not classified as baseline. Test compilation must exercise the production
authorization path rather than excluding it with `cfg(not(test))`.

Existing native projection packages pass root checks: Apple Contacts 9, Apple Health 8 and
Feasibility 5 tests. These are projection tests, not proof of adoption by the new grant boundary.
