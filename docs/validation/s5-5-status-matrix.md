# S5.5 Canonical Acceptance Status Matrix

> Date: 2026-09-11  
> Current code baseline: `bed1fba` plus ADR 0024 and ADR 0025
> Initial product-wiring audit: `d97160d`
> Acceptance: **0/14** (`S5.5-C1`–`C6`, `S5.5-E1`–`E8` all pending)

## Purpose

This document is the current reconciliation of S5.5 implementation and acceptance evidence. The
other `s5-5-*.md` validation records are immutable checkpoint evidence for individual increments;
when their remaining-work text differs from this matrix, this matrix takes precedence for current
status. Update this matrix when a gate gains new code or evidence rather than rewriting a historical
checkpoint.

At the initial 2026-09-11 audit, the statement that implementation increments 1–7 were complete
described isolated code endpoints rather than product wiring. The current baseline now connects
those endpoints through Person-owned server connections, client-driven lifecycle UI, iOS/Android
Calendar publication and provider-neutral Schedule dispatch. This still does **not** complete an
acceptance criterion: implementation increment 8, qualifying live provider/physical-device
scenarios and the integrated failure/privacy corpus remain absent.

The columns below are deliberately separate:

- **Implemented code** records production or contract paths that exist in the repository.
- **Automated evidence** records fixture, conformance, integration or build checks; it is not live
  provider evidence.
- **Live evidence** requires a real supported provider/OS host and the acceptance scenario. `None`
  means no qualifying evidence is recorded, not that the implementation was not tested.
- **Remaining gate** is the smallest honest statement of what still blocks the criterion.

## Product Wiring Baseline

Acceptance evidence alone does not answer whether an implementation can be reached from the
shipping client. The table below records four independent states at the current code baseline:

- **Implemented** means the required adapter, View or Expert code exists. It does not imply that a
  production request can reach it.
- **Runtime connected** means the production Flutter/native/FFI/Go path can supply the criterion's
  required input to its consumer. A fixture-only path is not connected.
- **Product UI exposed** means a person can discover and operate the path in Flutter. The Go
  management page and a read-only diagnostic snapshot do not by themselves satisfy this column.
- **Physical/live verified** means the repository records evidence from the real OS host or service
  provider required by the criterion. Automated fixtures and builds do not satisfy this column.

`Yes` covers the criterion as written. `Partial` identifies a real path with a material missing
provider, source or product transition. `No` means the required end-to-end state is absent. These
states do not change the canonical **0/14** acceptance result.

The initial `d97160d` audit found a read-only server inventory, a hard-coded macOS Calendar card,
no iOS Calendar channel, no Person-owned server connection mutations and disconnected Schedule
Calendar contracts. Those findings are retained here as dated history rather than current-state
claims. Implementation increments 1–7 have since closed those wiring gaps; increment 8 and every
live/physical acceptance run remain outstanding.

| Gate | Implemented | Runtime connected | Product UI exposed | Physical/live verified |
| --- | --- | --- | --- | --- |
| **C1 — Common Observe/Act lifecycle** | **Yes.** Common descriptors, Person-owned snapshots, typed failures, revision preconditions and Observe/Act separation cover the in-scope server and device adapters. | **Yes.** Paired-client catalog, connect/poll/cancel/scope/disconnect, credential cleanup and device Calendar publication are connected to their owning runtimes. | **Yes.** Connections exposes server and device catalogs, lifecycle status, scope changes, provider selection and recovery actions. | **No.** No complete real-adapter lifecycle matrix is recorded. |
| **C2 — Personal context cohort** | **Yes.** Gmail, Apple Calendar/Contacts/Feasibility/Health/Screen Time gate and Floe-native inputs have production adapters and strict Views. | **Yes.** iOS EventKit and other Apple observations publish through Person/device-bound FFI context; Gmail uses the Person-owned server runtime. | **Yes.** Connections owns Calendar/Gmail setup and Data & privacy exposes Apple context grants and typed unsupported states. | **No.** No combined signed Apple-device Expert scenario is recorded. |
| **C3 — Provider parity** | **Yes.** Google/Microsoft and Android adapters share strict identity, range, scope and revision contracts. | **Yes.** Selected Google/Microsoft Calendar connections use the bounded server View; iOS/Android EventKit/provider batches publish to the local Schedule observation port. | **Yes.** Flutter catalogs Google/Microsoft connectors, operates Android Calendar scope and selects the active Calendar provider. | **No.** No live OAuth or Android physical-device cohort is recorded. |
| **C4 — Work context cohort** | **Yes.** Bounded Slack, Teams, Drive and GitHub adapters, merged View and Work Context Expert exist. | **Yes.** Person-owned configured runtimes serve `work.context` only to the matching paired-client/Expert route. | **Yes.** Flutter starts OAuth or one-shot-secret connections, edits bounded scope and disconnects every cohort source. | **No.** No combined live Work Context scenario is recorded. |
| **C5 — Life logistics cohort** | **Yes.** Gmail logistics classification, Home Assistant adapter, merged View and Life Logistics Expert exist. | **Yes.** Person-owned Gmail/Home Assistant runtimes feed `life.logistics` without an unscoped fallback. | **Yes.** Flutter connects, scopes and disconnects Gmail and Home Assistant from the common Connections catalog. | **No.** No live mail plus logistics-source scenario is recorded. |
| **C6 — Failure and privacy matrix** | **Partial.** Typed failures, freshness, exact-identity conflicts, cleanup/revoke recovery and routing policy exist; the increment-8 integrated corpus does not. | **Partial.** Production boundaries fail closed and surface stale/revoked/changed connection state, but the full real-source disagreement and recovery matrix is unverified. | **Partial.** Connections and Data & privacy surface actionable states; outbound-capture and cross-source disagreement evidence has no product presentation. | **No.** No real-source recovery and outbound-capture corpus is recorded. |
| **E1 — Schedule & Feasibility** | **Yes.** Schedule composition, exact Calendar connection routing and proposal boundaries cover server and device providers. | **Yes.** Google/Microsoft use the selected Person-owned server connection; EventKit/Android use fresh Person/device/provider/scope/revision-bound observations. | **Yes.** Connections selects the Calendar used by Floe and Schedule; Agent and Calendar access surfaces expose the resulting path. | **No.** No live ETA/weather/capacity scenario is recorded. |
| **E2 — Commitments** | **Yes.** Mail, selected Calendar, Task and confirmed-Memory composition exists. | **Yes.** Person-owned Mail and bounded Calendar requests reach the durable Expert dispatch with exact connection preconditions. | **Yes.** Flutter establishes the required sources and renders results in the generic Agent product surface. | **No.** No live mailbox or cross-source quality run is recorded. |
| **E3 — Communication** | **Partial.** Mail assessment and draft-for-review output exist without send authority; relationship and available-channel evidence for channel choice remains incomplete. | **Partial.** Person-owned Mail reaches the Expert, but the full channel-choice and Review/Act scenario is not connected. | **Partial.** Mail setup and generic results are exposed; supported-channel choice is not yet a complete product journey. | **No.** No live mailbox, channel-choice or Review/Act scenario is recorded. |
| **E4 — Relationships** | **Yes.** People plus confirmed-interaction composition and guarded output exist. | **Partial.** Apple/Android People publication reaches the durable Expert route, but no production Go source currently serves `relationships.confirmed_interactions`. | **Partial.** Device grants and generic results are exposed; confirmed-interaction source readiness has no product path. | **No.** No live Contacts/interaction scenario is recorded. |
| **E5 — Focus & Attention** | **Yes.** Attention, selected Calendar and active Work Context composition exists with a typed Screen Time capability gate. | **Yes.** Supported coarse Attention sources plus exact Calendar/Work routes feed the Expert; unsupported Screen Time remains explicit rather than fabricated. | **Yes.** Source state and setup are exposed across Connections/Data & privacy and results render in Agent. | **No.** No live focus-quality scenario is recorded. |
| **E6 — Wellbeing** | **Yes.** Apple Health and Health Connect reducers plus selected-Calendar composition exist. | **Yes.** Person/device-bound derived Views and exact Calendar routing reach the Wellbeing Expert without raw-sample transfer. | **Yes.** Health grants and Calendar selection are exposed and results render through the generic Agent surface. | **No.** No physical-device Health read and quality scenario is recorded. |
| **E7 — Work Context** | **Yes.** The merged bounded View, durable source-gated assignment and Expert dispatch exist. | **Yes.** Person-owned server runtimes feed the matching Work Context Expert without global connector fallback. | **Yes.** Flutter manages all cohort connections/scopes and renders the Expert result in Agent. | **No.** No combined live source scenario is recorded. |
| **E8 — Life Logistics** | **Yes.** The merged bounded View, durable source-gated assignment and Expert dispatch exist. | **Yes.** Person-owned Gmail/Home Assistant runtimes feed the matching Expert without global connector fallback. | **Yes.** Flutter manages both source types and renders the Expert result in Agent. | **No.** No combined live source scenario is recorded. |

### Implemented Wiring Contract

The current implementation closes the initial audit gaps along these code-owned boundaries:

1. `apps/client/lib/features/day_canvas/presentation/connector_screen.dart` now joins a
   platform-specific device Calendar with the server catalog. `server_connector_panel.dart` owns
   connect, OAuth launch/poll/cancel, one-shot secret submission, scope update and disconnect UX.
2. `server/internal/console/client_connectors.go` derives Person/device ownership from the paired
   bearer credential. Connection records and credential names bind `person_id` plus a stable
   `connection_id`; provider runtimes and `/v1/views/*` are selected by that owned record.
3. `apps/client/ios/Runner/CalendarChannel.swift`, the iOS AppDelegate and Info.plist provide the
   EventKit channel and permission declaration. Android and Apple Calendar reads share the Flutter
   adapter and publication boundary rather than introducing provider-specific Expert code.
4. `calendar_observation_publisher.dart` and `calendar_observation_refresh.dart` publish and refresh
   device batches before Agent invocation. The FFI store and Schedule dispatch require the exact
   Person, device, provider, calendar scope, connection revision and covered time range.
5. `crates/floe-infra/src/remote_model.rs` and `server/internal/console/console.go` share the bounded
   server Calendar request contract, including connector and connection identity. A stale revision,
   ambiguous selection or selected-provider failure fails closed without provider fallthrough.
6. Built-in Expert implementation is consolidated under `crates/floe-agent/src/experts/`, with IDs,
   required sources and metadata centralized in `experts/catalog.rs`; FFI dispatch is grouped under
   `vault_host/conversation_turn/expert_dispatch/`.

This boundary deliberately has **no implicit compatibility migration**. Unscoped pairings,
connections and credential names are rejected, and the removed global runtime fallback is not
consulted. Existing pre-ADR-0025 installations must explicitly re-pair and reconnect so ownership
is established instead of silently assigning private data to a Person.

## Connector Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **C1 — Common Observe/Act lifecycle** | Common versioned descriptors, Person-owned connection snapshots, typed failures, strict View validation and revision preconditions cover the in-scope server and device sources. Native Views publish through a person/device-bound expiring FFI store; revoke removes them. The paired-client lifecycle supports catalog, connect, poll, cancel, scope update and disconnect without granting Act authority. | Rust conformance/cache/revoke and exact-identity tests; Flutter Calendar publication/catalog/lifecycle tests; Go owner binding, credential cleanup, attempt persistence and concurrent lifecycle tests. | None qualifying for the complete adapter cohort. Earlier EventKit PoCs were not rerun as this common-lifecycle matrix. | On real signed/provider hosts, capture permission, ready, stale/no-data, revoke/disconnect and reconnect/recovery for every in-scope adapter and verify common discovery remains correct across restart. |
| **C2 — Personal context cohort** | Read-only Apple Calendar/Contacts, Core Location → MapKit ETA → WeatherKit Feasibility, HealthKit-derived Wellbeing and public Screen Time feasibility-gate packages exist. The iOS Runner registers both Calendar and Apple-context channels and publishes fresh available Views into Person/device-bound FFI context. Gmail uses a Person-owned server runtime; Floe-native Task/Note paths remain distinct. | Swift package fixtures cross Rust validators; Flutter gateway/Calendar refresh/publication tests; FFI person/device/freshness/revoke tests; Schedule and personal Expert multi-source artifact tests. HealthKit/WeatherKit signing configuration and Calendar usage descriptions are checked in; Family Controls remains deliberately absent. | No combined signed Apple physical-device Expert scenario. The Screen Time gate intentionally reports `entitlement_unavailable`/unknown until its separate approved capability and extension exist. | Run the combined cohort on eligible signed hosts with real Calendar/Contacts/location/MapKit/WeatherKit/HealthKit data and attribution. On a physical device, record the public Screen Time entitlement/authorization/region gate; if approved capability is available, add and validate the report extension and coarse Attention reduction, otherwise preserve the evidenced typed unsupported result. The macOS heuristic is a separate Attention source, not Screen Time evidence. |
| **C3 — Provider parity** | Google Calendar, Microsoft Calendar/Mail and Android Calendar/Contacts/Health Connect adapters and lifecycle routes exist. Schedule selects the exact Google/Microsoft Person connection through the bounded server View contract and consumes EventKit/Android through a Person/device/provider/scope/revision-bound local observation. Flutter publishes and refreshes device Calendar batches. | Go selected-connector and owner-binding tests; Rust request-contract, projected-observation and device-publication tests; Flutter iOS/Android Calendar binding and refresh tests; Android debug build plus native fixtures. | No recorded live Google/Microsoft OAuth provider run or Android physical-device permission/read run for this gate. | Run the full live provider cohort, capture conforming snapshots and demonstrate same-logical-source arbitration/deduplication with real data, physical Android lifecycle transitions and revoked/unsupported states. |
| **C4 — Work context cohort** | Selected-scope Slack, Teams, Drive and GitHub reads merge into bounded `work.context`. Connections are Person-owned, provider runtimes are rebuilt from their exact record, and the Work Context Expert is advertised only while its mandatory source grant is available. | Go connector/privacy, paired-client lifecycle/ownership and runtime-isolation tests; cross-language fixtures; registry reopen/source-refresh tests and synthetic paired HTTP/FFI delegation tests. | No qualifying combined live Work Context scenario; Teams tenant/admin-consent evidence is explicitly absent. | Run selected live communication, file and project sources together and prove bounded scope/provenance, partial-source behavior and grounded output through the durable assignment. |
| **C5 — Life logistics cohort** | Person-owned Gmail produces bounded logistics candidates and a selected Person-owned Home Assistant adapter merges with them into `life.logistics`. The Life Logistics Expert is provisioned as an enabled durable, source-gated assignment. | Go classification/adapter/privacy, paired-client lifecycle/ownership and cleanup tests; cross-language fixtures; registry reopen/source-refresh tests and synthetic paired HTTP/FFI delegation tests. | No live selected Home Assistant or other travel/delivery/home source scenario. | Run live mail plus one permitted live logistics source and prove raw webhook/security/payment data remains excluded through the durable assignment. Home Assistant is sufficient; another travel adapter is not required. |
| **C6 — Failure and privacy matrix** | Typed failures exist across adapters; optional-source degradation preserves a Manager turn; strict local publication rejects stale/malformed Views and revokes cached context. Runtime policy enforces device presence/scope, transfer allowlists, device-only offline behavior and explicit disagreement preservation. | Unit/fixture coverage includes credential, rate-limit, partial, stale, no-data, unsupported/revoked shapes, cross-device denial, clock/freshness bounds, disagreement and malformed sensitive/authority-bearing fields. | No integrated real-source failure/recovery matrix or outbound capture corpus. | Exercise every required failure and material disagreement end to end with real sources; prove the remaining Manager turn and capture outbound traffic showing zero credential, raw Health/Attention, precise-location-history or unapproved-content leaks. |

## Expert Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **E1 — Schedule & Feasibility** | Schedule combines an exact selected Calendar connection, Floe-native Task/Note, an optional strict local Feasibility View containing ETA/leave-by/coarse weather impact and optional derived Wellbeing capacity. Google/Microsoft execute through their owned server connection; EventKit/Android use the local observation port. Mutations remain S3 proposals. | Calendar/Schedule/native-context and exact connection/revision tests, Flutter device publication/refresh tests, server selected-connector tests, cross-language Apple Feasibility/Wellbeing fixtures and observed Schedule model-request tests. | No real ETA/Weather/capacity scenario. | On signed hosts, evaluate conflict/free-window/leave-by/plan realism with live provider data and any allowed capacity projection; prove every mutation still returns only an S3 proposal. |
| **E2 — Commitments** | The durable source-gated Commitments Expert combines fresh Person-owned Mail with the selected bounded Calendar request, Task and confirmed Memory context and preserves per-source epistemic/provenance distinctions. | Deterministic corpus plus synthetic Mail/Calendar/Task/Memory → typed A2A artifact integration, exact Calendar request and registry grant tests. | No live mailbox or cross-source quality run. | Evaluate real Mail/Message/Calendar/Task/confirmed Memory cases for deadline, expected reply and follow-up quality, including missing-source behavior. |
| **E3 — Communication** | The isolated Communication Expert is durably assigned and source-gated over fresh Mail; drafts carry no send authority. | Deterministic corpus, synthetic View → typed artifact integration and registry grant tests. | No live mailbox, channel-choice or Review/Act scenario. | Add/verify People/relationship and available-channel evidence needed for channel choice, evaluate a live corpus and route a draft through transaction-bound Review plus a separate Act grant. |
| **E4 — Relationships** | The durable source-gated Relationships Expert combines local People with optional confirmed-interaction evidence; confirmed Memory remains governed in the shared Agent context. The strict remote View client exists, but the Go server has no production `relationships.confirmed_interactions` source endpoint. | Multi-View contract/artifact tests reject invented identity, unsupported inference and cross-source evidence; registry reopen/grant tests pass. | No live Contacts/interaction scenario. | Connect a production confirmed-interaction source, then evaluate real Contacts identity ambiguity and confirmed interaction/Memory follow-up without address-book replication or unsupported relationship inference. |
| **E5 — Focus & Attention** | The durable source-gated Focus Expert combines device-scoped Attention with the selected exact Calendar connection and active Person-owned Work Context. macOS publishes an opt-in, public-event-only coarse Attention View while the app runs, with presence and warmup requirements and no raw activity history. | Swift/Dart macOS source tests, FFI publication and exact Calendar request tests, multi-View Expert artifacts, Person-owned work-context tests and registry grant tests. | No live focus-quality scenario; iPhone/iPad Screen Time has no approved live View yet. | Evaluate interruption cost/focus protection on a supported host with schedule/work/preference. Keep macOS evidence labeled as the ADR 0024 heuristic, record the separate public Screen Time gate outcome, and evaluate its coarse signal only when entitled. |
| **E6 — Wellbeing** | The durable source-gated Wellbeing Expert combines a Person/device-bound derived Wellbeing View with the selected exact Calendar connection. Both Apple HealthKit and Android Health Connect have device-local reducers and publish through the strict local context boundary. | Swift/Android projection fixtures, Dart publication tests, Rust exact Calendar request and cross-language/multi-View artifact tests and registry grant tests. | No physical-device Health read or live schedule-quality scenario. | Run eligible signed Apple and physical Android reads, then evaluate non-diagnostic capacity/recovery effects with schedule while proving no raw samples or remote silent fallback. |
| **E7 — Work Context** | Bounded multi-provider Work Context and the isolated capability-free Expert are connected through an enabled durable, person/source-scoped assignment. | Expert contract/privacy corpus, synthetic fresh-View delegation, registry idempotency/reopen and unavailable-source gating tests. | No combined live source scenario. | Run a live selected-scope corpus that grounds blocker, preparation context and next action and proves scope containment/partial-source behavior. |
| **E8 — Life Logistics** | Bounded Gmail/Home Assistant Logistics context and the isolated capability-free Expert are connected through an enabled durable, person/source-scoped assignment. | Expert contract/privacy corpus, synthetic fresh-View delegation, registry idempotency/reopen and unavailable-source gating tests. | No combined live source scenario. | Run live mail plus Home Assistant (or another accepted source), evaluate preparation/change candidates and prove payment, entry/security and high-authority home actions never bypass proposal/policy. |

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

The local FFI context store is not an S8 implementation or server cache. It is process-local,
person/device keyed, accepts strict Personal Views plus a separately validated Calendar observation,
removes expired/revoked entries and makes local native observations available to the local Agent.
It has no Go transport, heartbeat, query lease, encrypted relay or durable cross-device persistence.
The runtime routing policy and tests define how future candidates must be selected, but networking
and two-device convergence remain S8-only work.

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
- Person-owned connector lifecycle: ADR 0025,
  `../decisions/0025-person-owned-connections.md`,
  `../planning/05-integrations/paired-client-connector-api.md`
