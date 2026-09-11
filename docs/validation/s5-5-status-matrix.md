# S5.5 Canonical Acceptance Status Matrix

> Date: 2026-09-11  
> Baseline: `dee1f96` plus ADR 0024  
> Acceptance: **0/14** (`S5.5-C1`–`C6`, `S5.5-E1`–`E8` all pending)

## Purpose

This document is the current reconciliation of S5.5 implementation and acceptance evidence. The
other `s5-5-*.md` validation records are immutable checkpoint evidence for individual increments;
when their remaining-work text differs from this matrix, this matrix takes precedence for current
status. Update this matrix when a gate gains new code or evidence rather than rewriting a historical
checkpoint.

The 2026-09-11 progress statement that implementation increments 1–7 were complete meant that the
Work Context and Life Logistics code paths had reached their increment endpoints. It did **not**
mean that every earlier increment or its acceptance criterion was complete. In particular, the
Apple personal-context production path, production delegation for three personal Experts, broad
cross-source evaluation, live provider evidence and the integrated failure/privacy corpus remain.

The columns below are deliberately separate:

- **Implemented code** records production or contract paths that exist in the repository.
- **Automated evidence** records fixture, conformance, integration or build checks; it is not live
  provider evidence.
- **Live evidence** requires a real supported provider/OS host and the acceptance scenario. `None`
  means no qualifying evidence is recorded, not that the implementation was not tested.
- **Remaining gate** is the smallest honest statement of what still blocks the criterion.

## Connector Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **C1 — Common Observe/Act lifecycle** | Common versioned capability/View descriptors, lifecycle snapshots, typed failures, provenance/freshness/retention validation and shared connection inspection exist. Calendar, mail, Android and work/life adapters use substantial parts of the contract. | Rust conformance and Calendar reopen/disconnect/staleness tests; Go connector lifecycle tests; FFI/Flutter connection parsing tests. | None qualifying for the complete adapter cohort. Earlier EventKit PoCs were not rerun as the common-lifecycle acceptance matrix. | Make the common snapshot/discovery path authoritative for every in-scope adapter, then capture real disconnect, revoke, reconnect, stale and recovery transitions without deriving Act from Observe. |
| **C2 — Personal context cohort** | Apple Calendar/EventKit and Gmail paths exist. Provider-neutral People, Feasibility, Attention and Wellbeing contracts exist; Floe-native Task/Note reaches Schedule. Android Contacts/Health is separate parity evidence, not the required Apple cohort. | Personal View privacy/conformance tests and native Task/Note-to-Schedule tests. | No combined Apple personal-context Expert scenario. | Implement Apple Contacts, Core Location → MapKit ETA → WeatherKit, HealthKit-derived state and the supported Screen Time public-API gate on eligible native hosts; wire all required Views into production Experts and run the combined scenario. The macOS activity heuristic from ADR 0024 is a separate `attention.coarse` source and must not be labeled Screen Time. |
| **C3 — Provider parity** | Google Calendar, Microsoft Calendar/Mail and Android Calendar/Contacts/Health Connect adapters and lifecycle routes exist. Calendar route arbitration and duplicate-source suppression exist without model/provider selection. | Go adapter/OAuth/console race tests and vet; Android debug build plus Dart/native/Rust fixtures; Rust arbitration/conformance tests. | No recorded live Google/Microsoft OAuth provider run or Android physical-device permission/read run for this gate. | Run the full live provider cohort, capture conforming snapshots and demonstrate same-logical-source arbitration/deduplication with real data and revoked/unsupported states. |
| **C4 — Work context cohort** | Selected-scope Slack, Teams, Drive and GitHub read adapters merge into bounded `work.context`; the paired Agent performs fresh View → isolated Work Context Expert delegation. | Go connector/privacy tests, cross-language fixtures and synthetic paired HTTP/FFI delegation tests. | No qualifying combined live Work Context scenario; Teams tenant/admin-consent evidence is explicitly absent. | Run selected live communication, file and project sources together, prove bounded scope/provenance and partial-source behavior, and complete the product assignment path rather than relying only on stateless built-in delegation. |
| **C5 — Life logistics cohort** | Gmail produces bounded reservation/travel/delivery/errand candidates; a selected Home Assistant adapter merges with them into `life.logistics`; the paired Agent delegates to the isolated Expert. | Go classification/adapter/privacy tests, cross-language fixtures and synthetic paired HTTP/FFI delegation tests. | No live selected Home Assistant or other travel/delivery/home source scenario. | Run live mail plus one permitted live logistics source, prove raw webhook/security/payment data remains excluded, and complete the product assignment path. Home Assistant is sufficient; an additional travel adapter is not required. |
| **C6 — Failure and privacy matrix** | Typed failures exist across the common contract and individual adapters; optional-source degradation can preserve a Manager turn; strict View validators reject several sensitive or authority-bearing fields. | Unit/fixture coverage includes credential, rate-limit, partial, stale, no-data, unsupported/revoked-shaped cases and malformed outbound projections. | No integrated real-source failure/recovery matrix or outbound capture corpus. | Exercise every required failure class, material source disagreement and multi-source continuation end to end; capture outbound traffic proving zero credential, raw Health/Attention, precise-location-history or unapproved-content leaks. |

## Expert Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **E1 — Schedule & Feasibility** | Calendar plus Floe-native Task/Note reaches the production Schedule Expert. A strict Feasibility View contract exists. | Calendar/Schedule and native-context integration tests plus Feasibility contract fixtures. | No scenario combining Calendar, Task, ETA, Weather and allowed capacity. | Produce live Feasibility and capacity Views, consume the complete source set in Schedule, evaluate conflict/free-window/leave-by/plan realism and keep mutation behind an S3 proposal. |
| **E2 — Commitments** | Isolated Commitments role/result contract and stateless paired-server delegation over a fresh Gmail Communication View exist. | Deterministic positive/negative corpus and synthetic View → Expert → typed artifact integration. | No live mailbox or cross-source quality run. | Combine Mail/Message with Calendar, Task and confirmed Memory; distinguish observed commitments from inference, bind source references, install/assign the production Expert durably and evaluate a live corpus. |
| **E3 — Communication** | Isolated Communication role/result contract and stateless paired-server delegation over Gmail exist; draft output carries no send authority. | Deterministic corpus and synthetic View → Expert → typed artifact integration. | No live mailbox, channel-choice or Review/Act scenario. | Add People/relationship and available-channel context, demonstrate reply/tone/channel quality, route a draft through transaction-bound Review and a separate Act grant, and install/assign the production Expert durably. |
| **E4 — Relationships** | Strict People View plus isolated Relationships prompt/result contract exist. | Contract corpus rejects invented identities, unsupported inference and invalid evidence. | None. | Implement a live Contacts identity projection, combine it only with confirmed interaction/Memory, register/delegate the Expert in production and evaluate identity ambiguity and follow-up without address-book replication. |
| **E5 — Focus & Attention** | Strict coarse Attention View plus isolated Focus prompt/result contract exist. | Contract corpus rejects raw app/domain evidence and authority leakage. | None. | Produce a supported device-scoped Attention View, combine schedule/active work/preference, register/delegate the Expert and evaluate interruption cost. On macOS use the ADR 0024 public-event heuristic; raw/private Screen Time stores are prohibited. |
| **E6 — Wellbeing** | Strict derived Wellbeing View and isolated Expert contract exist; Android Health Connect can produce a local derived View. | Personal Expert corpus plus Android adapter/build/cross-language validation. | No physical-device Health run or production Expert scenario. | Add the eligible Apple HealthKit path for C2, register/delegate the Expert, combine derived capacity/recovery with schedule and demonstrate non-diagnostic behavior with no raw samples or remote silent fallback. |
| **E7 — Work Context** | Bounded multi-provider Work Context and an isolated capability-free Expert are connected through stateless paired-server delegation. | Expert contract/privacy corpus and synthetic fresh-View delegation integration. | No combined live source scenario. | Run a live selected-scope corpus that grounds blocker, preparation context and next action; prove scope containment and complete durable product assignment. |
| **E8 — Life Logistics** | Bounded Gmail/Home Assistant Logistics context and an isolated capability-free Expert are connected through stateless paired-server delegation. | Expert contract/privacy corpus and synthetic fresh-View delegation integration. | No combined live source scenario. | Run live mail plus Home Assistant (or another accepted source), evaluate preparation/change candidates and prove payment, entry/security and high-authority home actions never bypass proposal/policy. Complete durable product assignment. |

## ADR 0024 Boundary

S5.5 validates each native source and Expert scenario on an execution host that can actually own
that source. It does not claim cross-device delivery. The following remain S8 work: device identity
and capability heartbeats, the Go Device Gateway, bounded query leases, end-to-end encrypted opaque
relay, device revocation propagation and two-real-device convergence.

S5.5 implementations must nevertheless follow ADR 0024 now:

- current location, ETA and Attention are ephemeral/on-demand and are not server content records;
- raw Health and Screen Time/activity never leave their producing device;
- only policy-approved derived snapshots may be synchronized, with expiry and no history by default;
- unavailable sources return typed state rather than fabricated empty Views; and
- cross-device permission, server-readable processing and remote-model transfer are separate grants.

Consequently, absence of the S8 relay does not block native-host S5.5 evidence, and a synthetic
cross-device shortcut cannot satisfy it.

## Evidence Index

- Common lifecycle and conformance: `s5-5-connected-context-conformance.md`,
  `s5-5-calendar-connector-snapshot.md`
- Personal Views and Experts: `s5-5-native-context-expert.md`,
  `s5-5-personal-context-views.md`, `s5-5-personal-domain-experts.md`
- Mail and provider parity: `s5-5-gmail-observe-adapter.md`, `s5-5-mail-experts.md`,
  `s5-5-context-routing.md`, `s5-5-provider-parity-foundation.md`,
  `s5-5-android-context-foundation.md`
- Work and life domains: `s5-5-work-logistics-foundation.md`
- Device placement, collection and convergence policy: ADR 0024,
  `../decisions/0024-device-context-collection-and-convergence.md`

