# S5.5 Canonical Acceptance Status Matrix

> Date: 2026-09-11  
> Baseline: `d97160d` plus ADR 0024
> Acceptance: **0/14** (`S5.5-C1`–`C6`, `S5.5-E1`–`E8` all pending)

## Purpose

This document is the current reconciliation of S5.5 implementation and acceptance evidence. The
other `s5-5-*.md` validation records are immutable checkpoint evidence for individual increments;
when their remaining-work text differs from this matrix, this matrix takes precedence for current
status. Update this matrix when a gate gains new code or evidence rather than rewriting a historical
checkpoint.

The 2026-09-11 progress statement that implementation increments 1–7 were complete originally meant
that the Work Context and Life Logistics code paths had reached their increment endpoints. Since
that checkpoint, the Apple/Android personal-context publication path, multi-View Expert composition
and durable built-in Expert assignments have also landed. This still does **not** complete an
acceptance criterion: qualifying live provider/physical-device scenarios and the integrated
failure/privacy corpus remain absent.

The columns below are deliberately separate:

- **Implemented code** records production or contract paths that exist in the repository.
- **Automated evidence** records fixture, conformance, integration or build checks; it is not live
  provider evidence.
- **Live evidence** requires a real supported provider/OS host and the acceptance scenario. `None`
  means no qualifying evidence is recorded, not that the implementation was not tested.
- **Remaining gate** is the smallest honest statement of what still blocks the criterion.

## Product Wiring Baseline

Acceptance evidence alone does not answer whether an implementation can be reached from the
shipping client. The table below records four independent states at the baseline commit:

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

| Gate | Implemented | Runtime connected | Product UI exposed | Physical/live verified |
| --- | --- | --- | --- | --- |
| **C1 — Common Observe/Act lifecycle** | **Partial.** Common descriptors, snapshots, failures and validation exist, but iOS Calendar does not participate in the native lifecycle. | **Partial.** Existing runtimes report status; the paired Flutter client cannot create, reconnect or disconnect server connections. | **Partial.** Data & privacy renders joined snapshots, while the main Connections page only operates the Calendar card. | **No.** No complete real-adapter lifecycle matrix is recorded. |
| **C2 — Personal context cohort** | **Partial.** Gmail and the Apple Contacts/Feasibility/Health/Screen Time packages exist; Calendar's EventKit bridge is macOS-only. | **Partial.** iOS personal Views publish to local FFI, but iOS Calendar calls an unregistered channel and cannot feed the Schedule path. | **Partial.** Data & privacy exposes Apple context controls; Connections is a hard-coded macOS Calendar experience. | **No.** No combined signed Apple-device Expert scenario is recorded. |
| **C3 — Provider parity** | **Partial.** Google/Microsoft and Android adapters exist, but production Calendar consumers do not support all declared providers. | **Partial.** Go and Android read paths exist; Rust Schedule rejects Google/Microsoft/Android access and the Rust remote Calendar request does not satisfy the Go range contract. | **Partial.** Android permission/scope controls and configured-server status appear in Data & privacy; SaaS connection setup does not appear in Flutter. | **No.** No live OAuth or Android physical-device cohort is recorded. |
| **C4 — Work context cohort** | **Yes.** Bounded Slack, Teams, Drive and GitHub adapters, merged View and Work Context Expert exist. | **Partial.** A server configured outside Flutter can serve `work.context` to the Expert; connection ownership is not Person-scoped. | **Partial.** Go management UI can configure sources and Flutter can show configured status, but Flutter has no connection flow. | **No.** No combined live Work Context scenario is recorded. |
| **C5 — Life logistics cohort** | **Yes.** Gmail logistics classification, Home Assistant adapter, merged View and Life Logistics Expert exist. | **Partial.** A preconfigured server can serve `life.logistics`; Flutter cannot establish the connection and server state is not Person-scoped. | **Partial.** Go management UI can configure Home Assistant and Flutter can show configured status, but Flutter has no connection flow. | **No.** No live mail plus logistics-source scenario is recorded. |
| **C6 — Failure and privacy matrix** | **Partial.** Typed failure, freshness, revoke and routing-policy code exists, without the complete integrated matrix. | **Partial.** Individual boundaries propagate failures, but the disconnected connection and Calendar paths prevent a full production recovery flow. | **Partial.** Data & privacy renders several typed states, but cannot recover server connections from Flutter. | **No.** No real-source recovery and outbound-capture corpus is recorded. |
| **E1 — Schedule & Feasibility** | **Partial.** Schedule composition and proposal boundaries exist, but the provider/device Calendar path is incomplete. | **Partial.** macOS EventKit and local Feasibility/Wellbeing paths exist; iOS, Android and server Calendar parity do not reach the same production execution path. | **Partial.** The Agent panel and Calendar access UI exist, but required source readiness cannot be completed across supported providers. | **No.** No live ETA/weather/capacity scenario is recorded. |
| **E2 — Commitments** | **Yes.** Mail, Calendar, Task and confirmed-Memory composition exists. | **Partial.** Mail reaches the Expert from a configured server, while its optional server Calendar read currently fails the Go request contract. | **Partial.** Results can render in the generic Agent panel; Flutter has no Mail connection/setup journey. | **No.** No live mailbox or cross-source quality run is recorded. |
| **E3 — Communication** | **Yes.** Mail assessment and draft-for-review output exist without send authority. | **Partial.** A configured server Mail View reaches the Expert; connection setup remains outside the client and channel-choice evidence is incomplete. | **Partial.** Results can render in the generic Agent panel; Mail and channel connection UX is absent. | **No.** No live mailbox, channel-choice or Review/Act scenario is recorded. |
| **E4 — Relationships** | **Yes.** People plus confirmed-interaction composition and guarded output exist. | **Partial.** Local Apple/Android People publication and optional server interaction reads exist, but supported-device coverage is not complete. | **Partial.** Results render generically and device permission controls appear in Data & privacy; there is no Relationships-specific product state. | **No.** No live Contacts/interaction scenario is recorded. |
| **E5 — Focus & Attention** | **Yes.** Attention, Calendar and Work Context composition exists. | **Partial.** The macOS heuristic can publish locally; iOS Screen Time has no entitled observation and Calendar/Work optional paths are incomplete. | **Partial.** Results render generically and source state appears in Data & privacy; there is no complete focus-source setup path. | **No.** No live focus-quality scenario is recorded. |
| **E6 — Wellbeing** | **Yes.** Apple Health and Health Connect reducers plus Wellbeing/Calendar composition exist. | **Partial.** Derived device Views can publish locally, but physical permission/read coverage and the optional Calendar path remain incomplete. | **Partial.** Results render generically and Health controls appear in Data & privacy; end-to-end readiness is not represented. | **No.** No physical-device Health read and quality scenario is recorded. |
| **E7 — Work Context** | **Yes.** The merged bounded View, durable source-gated assignment and Expert dispatch exist. | **Partial.** Preconfigured Go sources reach the server-model Expert; client-led connection and Person ownership are absent. | **Partial.** Results render generically and configured status is visible; source setup exists only in the Go management page. | **No.** No combined live source scenario is recorded. |
| **E8 — Life Logistics** | **Yes.** The merged bounded View, durable source-gated assignment and Expert dispatch exist. | **Partial.** Preconfigured Go sources reach the server-model Expert; client-led connection and Person ownership are absent. | **Partial.** Results render generically and configured status is visible; source setup exists only in the Go management page. | **No.** No combined live source scenario is recorded. |

### Confirmed Product Disconnects

The following are code-observed gaps, not conclusions inferred from missing tests:

1. `apps/client/lib/features/day_canvas/presentation/connector_screen.dart` renders exactly one
   hard-coded macOS Calendar card. It is not backed by a connector catalog.
2. `apps/client/lib/features/server/local_server_client.dart` only reads `/v1/connections`.
   `server/internal/console/console.go` returns snapshots for already-configured runtimes, while
   connect/disconnect operations live behind the management-cookie, same-Origin and CSRF-protected
   `/manage/api/*` surface. The paired-client API has no connector mutation operation, catalog or
   Person binding.
3. `apps/client/lib/main.dart` selects `EventKitCalendarAdapter` on iOS, but
   `apps/client/ios/Runner/AppDelegate.swift` registers only `floe/apple_context`; the
   `floe/calendar` channel is registered only by the macOS Runner. The iOS Info.plist also has no
   Calendar usage description.
4. `crates/floe-ffi/src/vault_host/schedule_conversation.rs` maps Google, Microsoft and Android
   Calendar providers to `Unsupported`, and `crates/floe-infra/src/native_calendar.rs` only loads
   the native bridge on macOS.
5. `crates/floe-infra/src/remote_model.rs` sends only `schema_version` when reading
   `calendar.timeline`, while `server/internal/console/console.go` requires range, cursor and limit.
   Therefore the real server Calendar View cannot currently satisfy the Rust request.
6. Built-in domain Expert results can appear through the generic Agent panel, but Flutter has no
   domain-Expert catalog or source-readiness journey. This is acceptable for hidden runtime
   vocabulary; it is not evidence that Connections and Data & privacy can establish the required
   source path.

## Connector Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **C1 — Common Observe/Act lifecycle** | Common versioned descriptors, connection snapshots, typed failures and strict View validation cover server connectors, Android sources, Apple personal-context sources and macOS EventKit. Native personal Views publish through a person/device-bound expiring FFI store; revoke removes them. iOS Calendar is not registered into this lifecycle. Observe-only grants remain separate from action authority. | Rust conformance/cache/revoke tests; Calendar reopen/disconnect/staleness tests; Go connector lifecycle tests; strict Swift/Dart/native snapshot and publication tests. | None qualifying for the complete adapter cohort. Earlier EventKit PoCs were not rerun as this common-lifecycle matrix. | Connect iOS Calendar and the paired-client server lifecycle, then on real signed/provider hosts capture permission, ready, stale/no-data, revoke/disconnect and reconnect/recovery for every in-scope adapter and verify common discovery remains correct across restart. |
| **C2 — Personal context cohort** | Read-only Apple Contacts, Core Location → MapKit ETA → WeatherKit Feasibility, HealthKit-derived Wellbeing and public Screen Time feasibility-gate packages exist. The iOS Runner integrates those four sources, exposes lifecycle/settings operations and publishes fresh available Views into local FFI context. Gmail and Floe-native Task/Note paths exist. Apple Calendar/EventKit is integrated only by the macOS Runner; the iOS Runner does not register `floe/calendar`. Schedule, Relationships, Focus and Wellbeing consume available multi-View inputs, not the missing iOS Calendar path. | Swift package fixtures cross Rust validators; Flutter gateway/publication tests; FFI person/device/freshness/revoke tests; Schedule and personal Expert multi-source artifact tests. HealthKit/WeatherKit signing configuration is checked in; Family Controls is deliberately absent. | No combined signed Apple physical-device Expert scenario. The Screen Time gate intentionally reports `entitlement_unavailable`/unknown until its separate approved capability and extension exist. | Register and permission iOS Calendar, connect its observation to the local Expert context, then run the combined cohort on eligible signed hosts with real Contacts/location/MapKit/WeatherKit/HealthKit data and attribution. On a physical device, record the public Screen Time entitlement/authorization/region gate; if approved capability is available, add and validate the report extension and coarse Attention reduction, otherwise preserve the evidenced typed unsupported result. The macOS heuristic is a separate Attention source, not Screen Time evidence. |
| **C3 — Provider parity** | Google Calendar, Microsoft Calendar/Mail and Android Calendar/Contacts/Health Connect adapters and lifecycle routes exist. Android Contacts and derived Wellbeing publish into the strict local FFI context after explicit settings actions. Provider-neutral calendar arbitration/deduplication and ADR 0024 routing policy exist. However, Schedule maps Google/Microsoft/Android Calendar to unsupported, and the Rust remote Calendar request omits fields required by the Go endpoint. | Go adapter/OAuth/console race tests and vet; Android debug build plus Dart/native/publication/Rust fixtures; Rust provider and runtime-policy arbitration tests. These checks do not cover the mismatched real Rust → Go Calendar request. | No recorded live Google/Microsoft OAuth provider run or Android physical-device permission/read run for this gate. | Repair the production Calendar request/provider dispatch, then run the full live provider cohort, capture conforming snapshots and demonstrate same-logical-source arbitration/deduplication with real data, physical Android lifecycle transitions and revoked/unsupported states. |
| **C4 — Work context cohort** | Selected-scope Slack, Teams, Drive and GitHub reads merge into bounded `work.context`. The Work Context Expert is provisioned as an enabled durable, person-scoped assignment and advertised only while its mandatory source grant is available. | Go connector/privacy tests, cross-language fixtures, registry reopen/source-refresh tests and synthetic paired HTTP/FFI delegation tests. | No qualifying combined live Work Context scenario; Teams tenant/admin-consent evidence is explicitly absent. | Run selected live communication, file and project sources together and prove bounded scope/provenance, partial-source behavior and grounded output through the durable assignment. |
| **C5 — Life logistics cohort** | Gmail produces bounded logistics candidates and a selected Home Assistant adapter merges with them into `life.logistics`. The Life Logistics Expert is provisioned as an enabled durable, source-gated assignment. | Go classification/adapter/privacy tests, cross-language fixtures, registry reopen/source-refresh tests and synthetic paired HTTP/FFI delegation tests. | No live selected Home Assistant or other travel/delivery/home source scenario. | Run live mail plus one permitted live logistics source and prove raw webhook/security/payment data remains excluded through the durable assignment. Home Assistant is sufficient; another travel adapter is not required. |
| **C6 — Failure and privacy matrix** | Typed failures exist across adapters; optional-source degradation preserves a Manager turn; strict local publication rejects stale/malformed Views and revokes cached context. Runtime policy enforces device presence/scope, transfer allowlists, device-only offline behavior and explicit disagreement preservation. | Unit/fixture coverage includes credential, rate-limit, partial, stale, no-data, unsupported/revoked shapes, cross-device denial, clock/freshness bounds, disagreement and malformed sensitive/authority-bearing fields. | No integrated real-source failure/recovery matrix or outbound capture corpus. | Exercise every required failure and material disagreement end to end with real sources; prove the remaining Manager turn and capture outbound traffic showing zero credential, raw Health/Attention, precise-location-history or unapproved-content leaks. |

## Expert Acceptance

| Gate | Implemented code | Automated evidence | Live evidence | Remaining gate |
| --- | --- | --- | --- | --- |
| **E1 — Schedule & Feasibility** | Schedule combines its available Calendar path, Floe-native Task/Note, an optional strict local Feasibility View containing ETA/leave-by/coarse weather impact and optional derived Wellbeing capacity. Its production Calendar access remains macOS EventKit-only; iOS lacks the channel and Google/Microsoft/Android are rejected by Schedule dispatch. | Calendar/Schedule/native-context tests, cross-language Apple Feasibility/Wellbeing fixtures and an observed Schedule model-request test. | No real ETA/Weather/capacity scenario. | Complete provider/device Calendar dispatch, then on signed hosts evaluate conflict/free-window/leave-by/plan realism with live provider data and any allowed capacity projection; prove every mutation still returns only an S3 proposal. |
| **E2 — Commitments** | The durable source-gated Commitments Expert combines fresh Mail with available Calendar, Task and confirmed Memory context and preserves per-source epistemic/provenance distinctions. The optional remote Calendar call is wired in Rust, but its request currently fails Go validation. | Deterministic corpus plus synthetic Mail/Calendar/Task/Memory → typed A2A artifact integration and registry grant tests. | No live mailbox or cross-source quality run. | Repair the real server Calendar contract, then evaluate real Mail/Message/Calendar/Task/confirmed Memory cases for deadline, expected reply and follow-up quality, including missing-source behavior. |
| **E3 — Communication** | The isolated Communication Expert is durably assigned and source-gated over fresh Mail; drafts carry no send authority. | Deterministic corpus, synthetic View → typed artifact integration and registry grant tests. | No live mailbox, channel-choice or Review/Act scenario. | Add/verify People/relationship and available-channel evidence needed for channel choice, evaluate a live corpus and route a draft through transaction-bound Review plus a separate Act grant. |
| **E4 — Relationships** | The durable source-gated Relationships Expert combines local People with optional confirmed-interaction evidence; confirmed Memory remains governed in the shared Agent context. | Multi-View contract/artifact tests reject invented identity, unsupported inference and cross-source evidence; registry reopen/grant tests pass. | No live Contacts/interaction scenario. | Evaluate real Contacts identity ambiguity and confirmed interaction/Memory follow-up without address-book replication or unsupported relationship inference. |
| **E5 — Focus & Attention** | The durable source-gated Focus Expert combines device-scoped Attention with optional Calendar and active Work Context. macOS publishes an opt-in, public-event-only coarse Attention View while the app runs, with presence and warmup requirements and no raw activity history. Its optional remote Calendar call currently fails the Go request contract. | Swift/Dart macOS source tests, FFI publication tests, multi-View Expert artifacts and registry grant tests. | No live focus-quality scenario; iPhone/iPad Screen Time has no approved live View yet. | Repair the Calendar path and evaluate interruption cost/focus protection on a supported host with schedule/work/preference. Keep macOS evidence labeled as the ADR 0024 heuristic, record the separate public Screen Time gate outcome, and evaluate its coarse signal only when entitled. |
| **E6 — Wellbeing** | The durable source-gated Wellbeing Expert combines a local derived Wellbeing View with optional Calendar. Both Apple HealthKit and Android Health Connect have device-local reducers and publish through the strict local context boundary. Its optional remote Calendar call currently fails the Go request contract. | Swift/Android projection fixtures, Dart publication tests, Rust cross-language/multi-View artifact tests and registry grant tests. | No physical-device Health read or live schedule-quality scenario. | Repair the Calendar path, run eligible signed Apple and physical Android reads, then evaluate non-diagnostic capacity/recovery effects with schedule while proving no raw samples or remote silent fallback. |
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

The new local FFI context store is not an S8 implementation or server cache. It is process-local,
person/device keyed, accepts only the four strict Personal View IDs, removes expired/revoked entries
and makes local native observations available to the local Agent. It has no Go transport, heartbeat,
query lease, encrypted relay or durable cross-device persistence. The runtime routing policy and
tests define how future candidates must be selected, but networking and two-device convergence
remain S8-only work.

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
