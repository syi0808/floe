# Floe Progress

> Last updated: 2026-09-11
>
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/` 및 ADR을 따른다.

### S5.5 bounded Google Calendar adapter foundation — 2026-09-11

- Added a selected-calendar, GET-only Google Calendar adapter with a strict `calendar.timeline`
  View. It bounds range, pagination, payload and event count while excluding descriptions,
  locations, attendees, provider IDs and all action authority.
- Added an isolated Calendar OAuth credential with the exact read-only scope, typed failure/cache
  snapshots, and Go→Rust View/connector conformance fixtures.
- Focused Go race tests/vet and Rust contract tests pass. Startup/dashboard routing and live Google
  Calendar evidence remain, so S5.5-C3 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-provider-parity-foundation.md).

### S5.5 calendar parity contract path — 2026-09-11

- Extended the durable calendar domain and Schedule Expert setup contract with explicit Google,
  Microsoft and Android providers. Each provider receives an isolated provider-pinned package and
  personal-data binding; unsupported runtime access fails typed instead of falling through.
- All three providers now project the same conforming `calendar.timeline` connector snapshot with
  stable provider IDs, and Google/Microsoft candidates exercise deterministic logical-source routing.
- Focused Agent/Core/FFI tests pass. The remote and Android source adapters themselves remain, so
  S5.5-C3 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-context-routing.md).

### S5.5 Microsoft Mail OAuth and product route — 2026-09-11

- Added a dedicated Microsoft PKCE OAuth runtime with exact `Mail.Read` and `offline_access`
  consent, Keychain credential isolation, refresh rotation, client-ID binding and local disconnect.
- Wired the Microsoft adapter into startup, connection inventory, dashboard lifecycle and the paired
  Communication View route. Gmail remains first priority and Microsoft is the deterministic fallback.
- Focused Go race tests/vet and dashboard JavaScript validation pass. Live Microsoft evidence and
  Calendar/Android/Health Connect parity remain, so S5.5-C3 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-provider-parity-foundation.md).

### S5.5 bounded Microsoft Mail adapter foundation — 2026-09-11

- Added a Microsoft Graph Mail adapter that reads only bounded inbox metadata through `Mail.Read`.
  It hashes message/conversation IDs, omits provider URLs and headers, and emits the existing strict
  provider-neutral Communication View with no action authority.
- Added typed credential, permission, rate-limit and partial-fetch snapshots with fresh-cache
  degradation, plus Go→Rust View and connector conformance fixtures.
- Focused Go race tests/vet and Rust contract tests pass. Microsoft OAuth/product routing, Microsoft
  Calendar, Google Calendar, Android Calendar/Contacts, Health Connect and live parity evidence
  remain, so S5.5-C3 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-provider-parity-foundation.md).

### S5.5 mail and home Life Logistics composition — 2026-09-11

- Gmail metadata now emits bounded reservation/travel/delivery/errand candidates only for explicit
  phrases. Candidates remain epistemically marked `mail_candidate` and expose no body, provider ID,
  payment, access code or action authority.
- Added provider-neutral Logistics merging so Gmail evidence and Home Assistant state reach one Life
  Logistics Expert route with duplicate rejection, earliest expiry and partial-source tolerance.
  The Gmail descriptor and Go→Rust fixtures include the new Observe-only Logistics View.
- Focused Go race tests/vet and Rust contract tests pass. Live Gmail plus Home Assistant/travel or
  delivery evidence remains required, so S5.5-C5/E8 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 bounded Google Drive file adapter — 2026-09-11

- Added an ephemeral Google Drive adapter for one selected folder. It lists at most eight recent
  entries, reads only supported text/Google Document content, and projects at most 2 KiB per file
  without provider IDs, URLs, MIME details or binary content.
- Added an isolated PKCE Drive OAuth credential with the exact read-only scope, separate from Gmail,
  plus dashboard login/folder configuration and Go→Rust conformance fixtures. Drive, Slack and
  GitHub now merge behind the same Work Context route.
- Focused Go race tests/vet and Rust contract tests pass. Valid-host live evidence is still required,
  so S5.5-C4/E7 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 bounded Slack work communication adapter — 2026-09-11

- Added a server-native GET-only Slack adapter for one selected channel or thread. Bounded message
  text becomes provider-neutral Work Context communication evidence while channel/message IDs,
  files, reactions and profiles stay outside the View.
- Added Keychain-backed console setup, typed failure snapshots and Go→Rust conformance fixtures.
  GitHub and Slack Views merge behind the same Agent route with duplicate rejection and partial
  source tolerance.
- Focused Go race tests/vet and Rust contract tests pass. A file adapter and live Slack/GitHub
  evidence remain, so S5.5-C4/E7 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 managed Work and Life connector startup — 2026-09-11

- Added loopback-console configuration for one selected GitHub repository and a bounded Home
  Assistant entity allowlist. Tokens remain in macOS Keychain, private state contains configuration
  only, restart restores runtimes and disconnect removes the credential.
- Added dashboard setup controls and typed connector failure snapshots. A provider failure leaves a
  recent prior View visible as degraded only until expiry instead of failing the whole connection
  inventory.
- Added paired HTTP-to-Expert integration evidence for both Work Context and Life Logistics,
  including strict View parsing and evidence-linked typed A2A artifacts.
- Full Go race tests/vet, Rust workspace tests and dashboard JavaScript syntax validation pass. Live provider evidence,
  work communication/file adapters and travel/delivery coverage remain, so S5.5-C4/C5/E7/E8 and
  the slice stay **0/14**. [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 Work and Life Logistics conversation delegation — 2026-09-10

- Paired general conversations now advertise strict read-only Work Context and Life Logistics
  capabilities plus stateless Work Context and Life Logistics A2A Expert cards.
- Each delegation reads a fresh bounded server View, validates it in Rust and runs the isolated
  Expert without provider actions. Fixed-route authentication, payload rejection and the full
  `floe-ffi` suite pass.
- Durable registry assignment, startup connector configuration, cross-source scenarios and live
  provider evidence remain, so S5.5-C4/C5/E7/E8 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 work/logistics paired View transport — 2026-09-10

- Added configured-scope GitHub and Home Assistant service boundaries plus authenticated paired
  routes for Work Context and Life Logistics. Repository/entity selection stays server-owned and
  route payloads reject all fields except the schema version.
- Installed runtimes join the common connection inventory without exposing provider configuration.
  Focused race tests and vet pass for both adapters and the console transport.
- Startup credential/configuration wiring, client Agent consumption and live provider evidence
  remain, so S5.5-C4/C5/E7/E8 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 bounded Home Assistant logistics adapter — 2026-09-10

- Added a server-native GET-only Home Assistant adapter for up to 16 explicitly selected benign
  state entities. It rejects security-control domains, redirects and unsafe endpoints and exports
  only bounded provider-neutral state summaries with opaque provenance.
- The connector emits a common server descriptor with one Observe capability and no Act authority.
  Go adapter tests and Go→Rust Life Logistics/connector conformance fixtures pass.
- Credential lifecycle, scheduling, client/Agent transport, live Home Assistant evidence and
  travel/delivery adapters remain, so S5.5-C5/E8 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 bounded GitHub work adapter — 2026-09-10

- Added a server-native GET-only GitHub Issues adapter for one selected repository. Direct TLS
  networking, redirect denial, bounded response/count/text and hashed repository/issue handles keep
  credentials, provider URLs and unrestricted organization search outside Work Context.
- The connector emits a common server descriptor with one Observe capability and no Act authority.
  Go adapter tests and Go→Rust Work View/connector conformance fixtures pass.
- Credential lifecycle, scheduling, client/Agent transport, live GitHub evidence and additional
  communication/file adapters remain, so S5.5-C4/E7 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 Work Context and Life Logistics contracts — 2026-09-10

- Added bounded selected-scope Work Context and Life Logistics Views. Strict contracts exclude
  absolute paths, organization-wide search, full documents, payments, access codes, unlock controls
  and raw webhook payloads.
- Added isolated Work Context and Life Logistics Expert roles with evidence-linked blockers,
  next actions and preparation recommendations. Neither receives capabilities; runtime owns scope,
  source and expiry metadata and rejects attempted execution fields.
- Focused View/privacy and Expert tests pass 4/4. Live work/file/project/travel/delivery/home adapters
  and product delegation remain, so S5.5-C4/C5/E7/E8 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-work-logistics-foundation.md).

### S5.5 provider-neutral route arbitration — 2026-09-10

- Added explicit logical-source routing over common connector snapshots. Selection rejects stale or
  nonconforming candidates and ranks health, freshness, configured provider priority and stable ID
  without asking the model to choose a provider.
- One View is selected per logical source and duplicate physical source handles are suppressed;
  missing logical sources remain typed instead of becoming fake empty context.
- Google/Microsoft/Android/Health Connect-shaped fixture tests pass 2/2. Actual parity adapters and
  live routing remain, so S5.5-C3 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-context-routing.md).

### S5.5 Relationships, Focus and Wellbeing contracts — 2026-09-10

- Added isolated prompt roles and typed results for Relationships, Focus & Attention and Wellbeing.
  Results are bound to existing identity/provenance handles and runtime-owned source/expiry metadata.
- Relationship inference cannot invent identities; Focus has no raw app or notification authority;
  Wellbeing consumes only derived capacity/recovery and rejects diagnostic fields. Actionable
  judgments require evidence while no-conclusion requires none.
- Focused deterministic tests pass 2/2. Product delegation, live source adapters and cross-source
  Schedule/Memory evaluation remain, so S5.5-E4/E5/E6 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-personal-domain-experts.md).

### S5.5 bounded personal context Views — 2026-09-10

- Added strict provider-neutral People identity, next-event Feasibility, coarse Attention and derived
  Wellbeing Views with five-minute freshness, byte/count bounds and opaque provenance handles.
- The contracts exclude contact notes, precise coordinates/history, app/domain activity, raw health
  samples and diagnosis. Unknown derived states require zero confidence; non-unknown states require
  explicit evidence.
- Focused contract/privacy tests pass 2/2. Live Apple adapters and Relationships/Focus/Wellbeing
  Expert consumption remain, so S5.5-C2/E1/E4/E5/E6 and the slice stay **0/14**.
  [Evidence and limits](docs/validation/s5-5-personal-context-views.md).

### S5.5 Commitments and Communication contracts — 2026-09-10

- Added separate Commitments and Communication prompt roles and typed result contracts over the
  common bounded Communication View. Model text cannot invent evidence handles or control
  runtime-owned source, expiry and invocation metadata.
- Commitments preserves observed versus inferred status; Communication returns reply need, rationale,
  tone and optional email draft while granting no send/archive capability. Malformed, duplicate,
  oversized and authority-blurring results fail closed.
- A deterministic positive/negative mail corpus passes 3/3 and the Rust workspace passes. Product
  conversations with a paired server now advertise both stateless Experts through the existing
  in-process A2A boundary; the Manager chooses whether to delegate and each delegation reads a fresh
  bounded View before isolated reasoning.
- Durable registry assignment, combined Calendar/mail routing, cross-source evaluation and live
  mailbox quality remain, so
  S5.5-E2/E3 and the slice remain **0/14**.
  [Evidence and limits](docs/validation/s5-5-mail-experts.md).

### S5.5 bounded Gmail context capability — 2026-09-10

- Added a paired, read-only Communication View route over the private Gmail metadata index.
  Requests are POST-only with bounded query/cursor/limit fields; disconnected or revoked sources
  publish no content, and no body, credential, provider ID or mail action is exposed.
- Added a strict Rust Communication View projection and cross-language fixture. The conversational
  Agent advertises `mail.communication.read` only when its paired server route exists, validates
  every returned View and treats provider text as untrusted capability data.
- Focused Go and Rust tests pass, including authentication, malformed/authority escalation,
  stale/duplicate/oversized View rejection and an authenticated end-to-end HTTP read. Dedicated
  Commitments/Communication Expert evaluation remains, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 shared Gmail connection inspection — 2026-09-10

- Added paired-client `GET /v1/connections` transport for server-owned connector snapshots.
  Authentication remains separate from management sessions; the route is read-only, rejects
  browser origins and redacts connector failures.
- The native client validates the bounded v1 envelope, applies the existing strict common
  connection parser and merges server Gmail health with device Calendar health in one Connections
  section. Partial failure leaves healthy sources visible and refreshes both execution locations.
- Focused Go and Flutter tests plus Flutter analyzer pass. Live Google credentials and
  Commitments/Communication Expert consumption remain, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 Gmail service lifecycle — 2026-09-10

- Composed Google OAuth, checkpointed sync and the private metadata index into one server-owned
  Gmail service. The authenticated console now offers inspect, manual sync and revoke controls;
  connected metadata refreshes every five minutes with a bounded purpose query.
- Ready/degraded/revoked lifecycle and sync failures persist across restart. Successful disconnect
  revokes the upstream grant and clears indexed metadata; cached Views are never published while
  disconnected or revoked.
- Full Go race tests and vet pass. Live Google credentials, client snapshot transport and
  Commitments/Communication Expert consumption remain, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 Gmail desktop OAuth boundary — 2026-09-10

- Added Google Desktop OAuth with random loopback callback, PKCE S256, five-minute state,
  offline access and the sole `gmail.readonly` scope. Access/refresh tokens remain in macOS
  Keychain and serialized refresh clears rejected grants without exposing provider responses.
- Added authenticated local-console connect/status/cancel/revoke controls. Changed client IDs do
  not inherit old credentials; disconnect revokes Google before deleting local tokens.
- Race-enabled OAuth and console tests pass. A Google project/client, scheduled connector
  registration, live mailbox evidence and Agent consumption remain, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 Gmail checkpointed sync — 2026-09-10

- Added bounded Gmail bootstrap and incremental synchronization over the private metadata index.
  Bootstrap records a profile checkpoint before purpose-scoped search and catches up afterward;
  incremental runs atomically merge added, label-changed and permanently deleted messages.
- Expired history checkpoints now trigger bounded full-sync recovery. Provider or metadata failure
  leaves the previous durable checkpoint intact, so retries cannot publish partial state.
- Race-enabled sync tests pass for catch-up, changes, deletion and `404` recovery. OAuth, console
  registration, scheduled triggering, live mailbox evidence and Agent consumption remain, so
  S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 durable Gmail metadata index — 2026-09-10

- Added a private, connection-scoped Gmail metadata index with atomic full replacement and
  checkpoint-CAS delta commits. It persists headers, labels, snippets and provenance identifiers,
  but has no field for message bodies or credentials.
- Added bounded Communication View projection with hashed message/thread handles, deterministic
  newest-first paging and five-minute freshness. Public/symlinked storage, corrupt state,
  cross-connection files, stale checkpoints and oversized indexes fail closed.
- Race-enabled reopen/update/delete/privacy tests pass. OAuth, sync orchestration, console wiring,
  live mailbox evidence and Expert consumption remain, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 Gmail Observe adapter foundation — 2026-09-10

- Added a server-native Gmail REST adapter for bounded search, metadata, separately authorized
  on-demand body reads and history-based changes. Remote endpoints require TLS; redirects and
  environment proxies are disabled; identifiers, cursors, pages and envelopes are bounded.
- Added typed credential/rate-limit/checkpoint/provider failures and a common read-only connector
  descriptor for `mail.communication` plus ephemeral `mail.body`. No mail mutation authority or
  provider-native ID enters its snapshot.
- Race-enabled fixtures cover the HTTP and authority boundary, and the emitted descriptor passes
  the shared Rust conformance validator. Google OAuth, durable indexing,
  local-console registration, live mailbox evidence and Commitments/Communication Expert
  consumption remain, so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-gmail-observe-adapter.md).

### S5.5 Floe-native context reaches Schedule Expert — 2026-09-10

- Added bounded, versioned Personal Views for durable Floe-native Tasks and Notes. Projections
  exclude completed/deleted and cross-person records, preserve opaque evidence handles, truncate
  untrusted text safely and omit domain source metadata.
- Personal Calendar Agent turns now load these Views into the production Schedule Expert model
  context. Synthetic turns receive no Personal evidence, while existing model placement and
  transfer-consent policy remains authoritative.
- Added aggregate evidence bounds to inference policy and focused coverage for durable reopen,
  person isolation, item/byte limits and observed Expert requests. The broader personal context
  cohort is still incomplete, so S5.5 remains **0/14**.
  [Evidence and limits](docs/validation/s5-5-native-context-expert.md).

### S5.5 shared Calendar connection inspection — 2026-09-10

- Added a read-only `connections` protocol/FFI operation over the common connector snapshot.
  It works without creating or unlocking the Agent vault and rejects attempted mutation or
  credential fields.
- Added strict Dart projections for descriptors, capabilities, lifecycle, freshness, bounds
  and provenance. Data & privacy now shows a shared Connections section with provider,
  execution location, available Views, last success, degraded reason and granted read scope.
- The presentation explicitly keeps source read access separate from action approval and does
  not expose provider-native identifiers. Focused protocol and FFI tests pass; Flutter gateway,
  malformed escalation and settings tests pass 3/3, with analyzer clean.
- Only Calendar uses the production snapshot and signed live EventKit evidence was not rerun,
  so S5.5 remains **0/14**.
  [Evidence and limits](docs/validation/s5-5-calendar-connector-snapshot.md).

### S5.5 durable Calendar connector snapshot — 2026-09-10

- Projected the existing durable Calendar mirror into the common versioned connector
  contract with provider-neutral `calendar.timeline` View metadata, device execution,
  mirror retention, five-minute freshness and bounded provenance requirements.
- Calendar Observe and Act descriptors remain separate and the connection snapshot grants
  read only. Provider-native calendar identifiers are hashed in source handles; names,
  external IDs and credentials do not enter the common snapshot.
- Persisted aggregate and per-source Calendar failure observation times. Ready, pending,
  degraded, unavailable and disconnected states now survive reopen, while successful
  imports clear prior failures.
- New Core integration tests pass 4/4, including partial-source degraded recovery and
  disconnect/reconnect without stale View resurrection. Full Rust workspace check and
  tests pass. Shared Connections UI, other adapters and signed live EventKit evidence remain,
  so S5.5 stays **0/14**.
  [Evidence and limits](docs/validation/s5-5-calendar-connector-snapshot.md).

### S5.5 connected context conformance foundation — 2026-09-10

- Added strict versioned descriptors for connector execution location, Observe/Act/Interact
  authority, provider-neutral Views and explicit-foreground Situations. View descriptors
  carry data class, retention, freshness, item/byte bounds and provenance requirements.
- Added typed connection snapshots for scopes, last success/failure and
  ready/degraded/disconnected/revoked/unsupported states without exposing credentials or
  provider-native objects.
- Added a provider-neutral fixture harness that rejects authority mixing, missing grants,
  credential projections, stale/oversized Views and incomplete provenance. Cross-source
  evaluation keeps a required View usable while reporting a degraded optional source.
- Focused `floe-agent` conformance tests pass 5/5; full Rust workspace check and tests
  pass. No live connector, Expert production path or durable disconnect/reconnect
  lifecycle is connected yet, so S5.5 remains Planned at **0/14**.
  [Evidence and limits](docs/validation/s5-5-connected-context-conformance.md).

### S4 persistent conversation lifecycle — 2026-09-10

- Removed UI-driven vault locking. Focus changes, app backgrounding, destination
  changes and closing the assistant surface now preserve the loaded conversation and
  allow an active turn to continue.
- Controller disposal stops active work without clearing the conversation or issuing a
  vault lock; native gateway shutdown remains responsible for releasing storage and
  key resources when the app exits.
- Focused assistant widget tests pass across inactive, hidden and resumed lifecycle
  transitions, and Flutter analysis reports no issues.

### S4 page-independent conversation redesign — 2026-09-10

- Recorded the corrected ownership in ADR 0023: one Person-scoped assistant
  conversation is mounted by a page but is not scoped by it; the Manager and delegated
  Expert, not Day Canvas, determine which evidence interval the request needs.
- Added request interval and cursor fields to the internal Timeline read contract.
  Calendar projection now returns and clips to the requested subrange only when it is
  inside the currently authorized View, and rejects an attempted range expansion.
- Focused `floe-agent` and `floe-core` suites pass, including new request-scoped range
  and out-of-grant coverage.
- Generic conversation Expert discovery, on-demand connector refresh, pagination and
  removal of Calendar-specific client/session paths remain. The existing Day Canvas
  path is therefore migration-only and weekly retrieval is not yet end-to-end fixed.

### S4 bounded multi-day Calendar query foundation — 2026-09-10

- Generalized Calendar turn and Timeline View validation from a fixed 24-hour day to
  a caller-selected continuous range of up to 31 civil days, including 23/25-hour DST
  boundaries and historical ranges backed by a fresh source observation.
- Raised the internal bounded View capacity to 128 events and 64 KiB while preserving
  the separate model/tool output limits, exact Person/calendar grants, expiry and
  fail-closed behavior for uncovered or overfull sources.
- Added a typed Flutter `AgentCalendarQueryRange` transport override while retaining
  the selected Day Canvas day as the current default. Rust coverage now exercises a
  seven-day mirror projection and complete Calendar Manager/Expert turn; Flutter
  coverage verifies multi-day wire serialization.
- Natural-language range resolution and connector refresh are not connected yet, so
  live free-text turns still default to the selected day. This increment removes the
  fixed-day transport/Core blocker without claiming end-to-end weekly retrieval.
  [Evidence and limits](docs/validation/s4-calendar-timeline.md).

### S5 user-facing saved Memory view — 2026-09-10

- Added a read-only, Person-scoped Memory overview contract over active confirmed
  revisions. It returns an exact saved/pending count and at most 100 newest saved
  summaries without reusing the model retrieval projection or exposing raw sessions.
- Data & privacy now shows a Memory summary card that opens a dedicated Memory settings
  page. The page combines pending Review with a user-language Saved memories list,
  friendly category/origin labels and empty/loading/error states; confidence and IDs
  remain out of the primary presentation.
- Dart projections reject malformed versions, duplicate IDs, invalid kinds, confidence,
  sources and temporal ranges. Controller state clears on vault lock/unavailability and
  approval refreshes committed saved Memory rather than applying optimistic UI state.
- Full Rust workspace tests pass. Focused Flutter gateway/widget/settings tests pass
  14/14; analyzer reports only the existing unused test import. Edit, Forget, details,
  controls and pagination remain, so S5 stays **1/6**.
  [Evidence and limits](docs/validation/s5-memory-settings-view.md).

### S5 idle Learner scheduling and foreground preemption — 2026-09-10

- The unlocked Person-vault worker now discovers, claims and runs one device-local
  Learner review only after foreground work becomes idle, then durably settles the
  job as completed, deferred or failed. Empty queues back off for 30 seconds and
  transient worker failures retry after 5 seconds instead of polling continuously.
- An accepted foreground submission marks itself pending before enqueue and cancels
  an in-flight Learner token. Cancelled/deadline/unavailable reviews are deferred for
  a later idle window; a final transient attempt becomes terminal without another run.
- Foreground completion remains independent from discovery and Learner inference.
  Candidate staging retains source-revision CAS, and a candidate committed at the
  cancellation boundary is completed rather than falsely reported as deferred.
- Full Rust workspace tests pass, including explicit foreground-preemption and durable
  final-attempt settlement coverage. This completes **S5-A2**; S5 is now **1/6**.
  [Evidence and limits](docs/validation/s5-idle-learner-worker.md).

### S5 device-local structured Learner adapter — 2026-09-10

- Added a production `ModelRunner` adapter that converts immutable Learner review
  input into a dedicated local-model request with a background policy, Personal-only
  data classification, no external-transfer consent and no capabilities or experts.
- The Learner has a separate role/protocol prompt, receives confirmed Memory only as
  contextual data, and cannot receive persona or Playbook instructions. Its only valid
  response is one strict, versioned JSON answer containing zero or one Memory proposal.
- Model placement is checked before dispatch. Generic model response validation,
  token/cost/output/deadline accounting and cancellation remain active, and runtime-owned
  provenance still replaces model-supplied observation time before candidate staging.
- Full Rust workspace check and test suites pass. Idle worker invocation and foreground
  preemption are now implemented separately; this adapter checkpoint itself claimed no criterion.
  [Evidence and limits](docs/validation/s5-local-learner-adapter.md).

### S5 explicit conversation learning discovery — 2026-09-10

- Added a narrow deterministic trigger for explicit remember/forget and correction
  phrases in Korean and English. It only schedules Review work; it never interprets
  the statement into Memory or activates knowledge.
- The encrypted vault now discovers eligible completed Personal conversations in a
  bounded scan, requires the latest user turn to have a matching assistant outcome,
  builds a Unicode-safe bounded digest and snapshots only confirmed Memory context.
- Ordinary conversation, synthetic/scoped sessions, halted turns and incomplete
  exchanges are excluded. Repeated discovery is content-idempotent and source revision
  validation remains enforced by the durable queue before model work.
- Signal tests and the complete 22-test encrypted-vault suite pass. Production local
  Learner inference and idle worker invocation remain, so discovery creates no Memory
  candidate by itself and S5 stays 0/6.
  [Evidence and limits](docs/validation/s5-learning-discovery.md).

### S5 durable Learner review queue — 2026-09-10

- Added an encrypted Person-scoped queue for immutable Learner inputs. Enqueue is
  content-idempotent, assigns the trusted run ID internally and replays the original
  queued/terminal record rather than creating duplicate background work.
- Claim uses a 30-second lease with attempt identity. Explicit defer records a typed
  transient failure and future availability; abandoned leases recover after restart,
  while a third abandoned attempt becomes terminal instead of remaining stuck.
- Every claim revalidates the exact completed Personal source session, revision and
  evidence turns before model work. A newer foreground revision fails the queued job
  as stale. Completion can reference only a candidate created by that Learner run over
  the same source turns.
- Focused encrypted-vault queue tests pass 2/2 alongside Learner runtime tests 4/4.
  Idle worker scheduling, foreground-triggered cancellation and a production local
  Learner adapter remain; S5 stays at 0/6.
  [Evidence and limits](docs/validation/s5-learner-queue.md).

### S5 isolated Learner runtime boundary — 2026-09-10

- Added a typed, single-proposal Learner runtime over immutable completed-session
  digests, evidence turn IDs, current confirmed Memory projections and explicit source
  revision. It has a separate local-only model port, token/cost/input/output/deadline
  budgets and cancellation token, with no Capability, Expert, network or mutation port.
- The model can return no proposal as a normal result. For a Memory proposal, the
  runtime—not model output—owns extractor/prompt versions, Learner run actor, evidence
  references and observation time before using the candidate-only sink.
- Candidate staging now applies source-session revision CAS. A newer foreground turn
  makes an old review conflict instead of allowing a stale digest to write, while a
  committed candidate is never hidden by a later cancellation result.
- Focused Learner tests pass 4/4 and encrypted-vault tests pass 19/19. Durable job
  persistence is now implemented separately; idle scheduling and a production learner
  model adapter remain, so this is not yet a running background service and S5 stays 0/6.
  [Evidence and limits](docs/validation/s5-learner-runtime.md).

### S5 Memory Review boundary and settings surface — 2026-09-10

- Added a typed `memory_review` protocol/FFI operation that lists only pending Memory
  candidates from an unlocked Person vault and accepts only approve/reject decisions.
  The host supplies the `User` actor and decision time; callers cannot inject payloads,
  actors, evidence, or direct revision writes.
- Approval and rejection reuse the encrypted atomic candidate/decision/revision ledger.
  A committed decision is not reported as cancelled afterward, and Playbook candidates
  cannot cross the Memory-specific review endpoint.
- Added strict Dart projections, controller lifecycle clearing, and a Data & privacy
  settings card with source count plus explicit Approve/Reject controls.
- Full Rust workspace tests and build pass. Focused Flutter tests pass 2/2; analyzer
  reports only a pre-existing unused test import. The full Flutter suite still has
  unrelated baseline fixture timing, stale Registry expectations, and golden diffs.
- Conversation extraction, background Learner, rollback UI, curation and deletion
  propagation remain; therefore S5 stays at 0/6.
  [Evidence and limits](docs/validation/s5-memory-review.md).

### S5 confirmed Memory context retrieval — 2026-09-10

- General encrypted Personal conversations now load active confirmed Memory into a
  typed `ContextMemory` section and record target revision plus source turn references
  in `ContextManifest`. Memory stays contextual data and never enters instructions.
- Temporal filtering and policy revalidation reject future, expired, source-free,
  duplicate, non-Personal and over-budget projections. Synthetic sessions and Calendar
  Experts receive no ambient Personal Memory.
- Focused Agent policy/runtime tests pass 44/44 and encrypted-vault tests pass 19/19;
  full Rust workspace tests/check and formatting pass.
- Relevance ranking, durable per-attempt manifests, conversation extraction and
  background Learner remain. Review FFI/UI is now implemented separately; S5-A1–A6
  stay pending.
  [Evidence and limits](docs/validation/s5-memory-context-retrieval.md).

### S5 governed Memory persistence foundation — 2026-09-10

- Accepted ADR 0019 after reviewing Hermes Agent's background review, staged-write,
  origin, pin and Curator implementation. Floe adapts the mechanism so a Learner can
  propose but never activate authoritative knowledge.
- Added typed LearningObservation, Memory candidate, decision, immutable revision and
  mutation-ledger contracts in the encrypted Person vault. Duplicate extraction is
  idempotent across retries, and revisions retain base hashes and rollback material.
- Only completed Personal sessions can stage evidence. Synthetic sources and non-user
  decisions fail closed; pending/rejected candidates never appear as active Memory.
- Focused encrypted-vault tests now pass 19/19. Full Rust workspace tests/check and
  formatting pass, including create/revise/reject/reopen coverage.
- Conversation extraction, shared Review UI/FFI, ContextEnvelope retrieval, background
  Learner, rollback execution, curation and deletion propagation remain. This is a
  foundation checkpoint and does not satisfy S5-A1–A6.
  [Evidence and limits](docs/validation/s5-governed-memory-foundation.md).

### S5 encrypted Session Archive foundation — 2026-09-10

- Added Person-vault-local conversation search across live and compacted sessions.
  Search indexes remain inside the existing encrypted Turso database and reject empty,
  oversized and over-limit queries.
- Added turn-boundary compaction with optimistic revision checks. The active session
  keeps a typed summary and recovery pointer while the exact pre-compaction session,
  including ordered messages and capability/delegation records, remains immutable.
- Recovery and search continue after vault reopen; active turns, stale revisions and
  missing cutoffs fail without partially changing the session or archive.
- Focused `floe-agent`, encrypted-vault and full workspace tests pass. Workspace
  `cargo check` and formatting pass; warnings-denied Clippy remains blocked by
  pre-existing warnings in the Agent/Core runtime and journal.
- This is foundation evidence only. Automatic/model-generated compaction, ranked FTS,
  archive UI/FFI, deletion propagation and Memory/Playbook learning remain. S5 stays
  Planned at 0/5 because S4 is not Accepted.
  [Evidence and limits](docs/validation/s5-session-archive-foundation.md).

### Schedule time context and compact Markdown — 2026-09-09

- Schedule Expert tasks now carry invocation-time local clock context after the stable
  cached instructions. Calendar Tool output pairs raw instants with adaptive local
  display values (`HH:mm`, date, year and seconds only as needed), while timezone
  metadata remains internal and is omitted from Manager-facing prose.
- Assistant Markdown block spacing is reduced from 12 to 4 logical pixels, tightening
  list rows without changing the underlying Markdown content.
- Focused Rust coverage includes same-day and cross-year formatting; Flutter coverage
  asserts the compact renderer style.

### Agent context and Schedule Expert generalization — 2026-09-09

- Split monolithic prompts into typed Behavior Kernel, Role, optional Persona and
  capability-protocol components, then added one Core-owned `ContextEnvelope` and
  provenance manifest shared by local and remote model adapters.
- Added validated `PersonaProfile` composition for Manager calls while keeping Persona
  out of Schedule Expert prompts and contextual evidence.
- Replaced implicit focus-time handling for free-text Calendar requests with general
  Schedule analysis. The Expert can choose Calendar read/search or an explicitly
  relevant free-window Tool and no longer has a mandatory Tool sequence.
- Added the bounded nested Playbook registry/discovery contract: roots are visible
  first and loading a parent reveals only direct child summaries.
- Workspace Rust formatting, checks and tests pass. Durable Persona/Memory/Playbook
  storage, `SOUL.md` UI and model-visible Playbook loading remain S5 work.
  [Evidence and limits](docs/validation/s4-agent-context-generalization.md). S4 remains
  0/14.

### Managed Agent instructions and Markdown chat — 2026-09-08

The instruction layout in this checkpoint is superseded by the 2026-09-09 typed
prompt assembly above; the Markdown presentation evidence remains current.

- Moved Manager/Schedule Expert instructions and product-owned preset turn templates
  from Rust literals into compile-time text resources, with explicit formal-register
  and no-emoji rules for generated output.
- Assistant messages now render selectable GitHub-Flavored Markdown; user messages
  remain literal text. Model-authored images are reduced to alt text and links remain
  inert, so presentation cannot initiate external reads or bypass action authority.
- Added focused Rust and Flutter regression coverage and updated the S4 prompt/chat
  design contract. [Evidence and limits](docs/validation/s4-agent-prompts-and-markdown.md).
  Live instruction-following and model/source/privacy gates remain open, so S4 stays
  0/14.

### Lightweight Schedule Expert subagent — 2026-09-08

The mandatory free-window loop in this checkpoint is superseded by the 2026-09-09
general Calendar capability selection above; its isolation and authority evidence
remains current.

- Reframed the built-in Schedule Expert as a bounded domain subagent while keeping
  Floe's Manager as the only user-facing conversation owner.
- Calendar turns now give the Schedule Expert a fresh, isolated loop of up to ten
  model calls. It can invoke the deterministic `schedule.find_free_windows` Tool up
  to nine times over different bounded ranges before producing a concise
  Manager-facing summary. Expert-internal messages are not added to Manager history.
- Existing exact interval analysis and proposal creation remain deterministic. The
  Expert derives a `fast` schedule-summary policy from the active placement, data and
  consent boundary and has separate call/tool/token/cost/output/deadline limits;
  declarative fixtures remain model-free.
- Structured results record only the bounded summary and model-call count alongside
  existing source-backed insights and proposals. Calendar mutation still routes
  through the existing S3 authority/review/action boundary.
- [Design](docs/planning/03-intelligence/manager-and-experts.md) and
  [validation](docs/validation/s4-schedule-subagent.md). Live model/source/privacy
  gates remain open, so S4 stays 0/14.

### Unified review and action authority — 2026-09-06

- Reframed Calendar Proposal as an internal action intent projected into shared
  Review requests and Activity surfaces.
- Main Review shows only actionable items; terminal Calendar results move to the
  separate Activity destination.
- Added durable Person-scoped Calendar create authority (`allow` / `ask` / `deny`)
  and an Action permissions settings surface. The default remains `ask`.
- All app builds include Calendar writing. Runtime policy and fresh provider
  checks still gate every execution.
- [Product and domain contract](docs/planning/01-experience/review-authority-and-activity.md).
- Began Apple Calendar-familiar direct manipulation: Calendar creation now starts
  from the toolbar `+` or an empty 15-minute-snapped double-click rather than a
  labeled proposal control in the contextual rail.
- Empty days retain the full interactive time grid. `A little breathing room` is
  now a compact, non-blocking banner in the existing Calendar tools row.
- Drag-to-move and capability-aware context menus remain the next interaction slice;
  they will use the same authority/review/activity boundary.

## Delivery Board

Delivery follows [ADR 0006](docs/decisions/0006-slice-driven-delivery.md) and the
[slice acceptance plan](docs/planning/08-engineering/vertical-slice-delivery.md).
Phase order is no longer an implementation gate. Acceptance counts below report
verified criteria, not estimated implementation percentages.

| Slice | Status | Integration evidence | Acceptance | Blocker / prerequisite | Next demo |
| --- | --- | --- | --- | --- | --- |
| S1 — Calendar read | Implementing | Live timed-event PoC; dual scope, partial recovery, disconnect and DST automated checks | 0/4 | Controlled permission/DST/recurrence/lifecycle gates | Finish controlled live matrix |
| S3 — Approved action | Integrated; validating | Signed-app approval/create/collection/restart; real response-loss recovery and exact cleanup | 2/5 | S1 Verified; live rejection/blocking/failure matrix; dogfood | Complete remaining acceptance matrix |
| S4 — Connected Agent/Experts | Implementing | Encrypted sample and Calendar-scoped panels; Calendar consent/sessions/Core turns; saved proposal cards/S3 review | 0/14 | S3 Accepted; live key/model/source and privacy gates | Validate a live on-device Calendar conversation |
| S5 — Memory/self-improvement | Planned; foundations started | Encrypted Session Archive plus staged Memory candidate/revision ledger fixtures | 0/6 | S4 Accepted; P0-D corpus; P0-F local vault/key | Review, reuse and roll back one Memory and Playbook change |
| S5.5 — Connected domains | Planned; foundation started | Common conformance harness plus durable Calendar connector snapshot and degraded-source fixture | 0/14 | S5 Accepted; live source/provider host access; connector/Expert corpora | Complete one cross-domain briefing with degraded-source recovery |
| S6 — Transcription/voice | Planned | None | 0/5 | S5.5 Accepted; streaming/recording STT/TTS PoC | Continue Agent chat by voice and review one source-linked transcript |
| S7 — Local wake-up | Planned | None | 0/4 | S6 Accepted; resident wake lifecycle | Wake phrase opens a visible local voice session |
| S8 — Cross-device/server | Planned | None | 0/4 | S7 Accepted; sync/security PoCs | Same result on two devices |
| S9 — Intervention | Planned | None | 0/4 | S8 Accepted; intervention policy | Calendar change triggers controlled suggestion |

S1 implementation now connects a native EventKit adapter to Rust-owned mirror
storage and Day Canvas. No live acceptance criterion is marked verified yet.

### S4 Calendar conversation app integration — 2026-09-08

- The assistant controller now discovers an explicitly enabled Calendar Expert scope,
  resumes its isolated encrypted session and dispatches the current Day Canvas range
  through the distinct Calendar turn transport. With no eligible scope it continues
  to present the visibly labeled sample conversation rather than adopting a source.
- The panel distinguishes sample and Calendar conversations and offers bounded briefing
  and focus-proposal requests. Streaming progress, stop, retry, recovery, saved source
  evidence and existing S3 proposal review use the shared presentation path.
- EventKit turns explicitly select Foundation Models; fixture turns explicitly select
  the deterministic model. The local adapter now permits Personal-class requests only
  when invoked with the encrypted session boundary, while Synthetic-only use continues
  to reject them and no remote fallback exists.
- Focused Flutter controller/UI, transport/session and Rust local-model/Calendar-turn
  tests pass; analysis passes. The refreshed bundled availability smoke reports
  `AppleIntelligenceNotEnabled`; live generation and live EventKit conversation
  validation remain open, so S4 remains 0/14.
- [Evidence and limits](docs/validation/s4-calendar-app-turn.md).

### S4 paused handoff — 2026-09-07

- Implementation baseline: `e57c342`. Goal is paused at the user's request; this
  documentation checkpoint does not resume implementation or accept S4.
- `2e00e1c` adds native saved-proposal inspection, conversation status cards and
  navigation to existing S3 review. `e57c342` adds encrypted Calendar-scoped
  start/resume/get/recover and native/Dart storage transport, isolated from samples.
- Baseline validation: 216 workspace Rust tests, three keyring example tests,
  25 native assertions, 189 Flutter tests, analysis, formatting/Clippy with existing
  exclusions, Rust/macOS builds and deep strict signature verification pass.
- At the pause point, three local files held an uncommitted Calendar turn draft.
  The later 2026-09-08 increment below supersedes that draft with tested native and
  Dart transport; controller/UI execution wiring remains.
- [Handoff, exact local files, validation limits and ordered remaining work](docs/validation/s4-handoff.md).
  Actual key/model/source/privacy gates remain open. S4 stays 0/14; S1/S3 is unchanged.
- Earlier dated entries below are historical checkpoints; their then-remaining
  work may have been completed by the increments summarized here.

### S4 native Calendar turn dispatch — 2026-09-08

- Added a versioned encrypted-vault Calendar turn job with explicit session revision,
  prompt, model, bounded day/window and optional S3 destination.
- Native dispatch resolves the durable Calendar setup/View and current connection,
  validates destination scope and revision, streams Core events and does not finish
  until Manager proposal preparation has completed.
- Deterministic execution remains Fixture-only and Foundation Models retains its
  existing Personal-input denial; there is no silent model fallback.
- Added typed Dart begin/poll/stop/release transport with strict completed-response and
  changed-retry validation; controller and UI dispatch remain next.
- [Evidence and remaining app/live gates](docs/validation/s4-calendar-native-turn.md).
  FFI library tests pass 20/20, focused Flutter tests 2/2 and analysis pass. S4 remains
  0/14 and S1/S3 is unchanged.

### S4 assistant access experience feedback — 2026-09-08

- Removed user-facing conversation setup/unlock/lock actions. Vault creation and unlock
  are automatic on assistant load/resume while close/background still seal the session.
- Moved Expert, assignment and Calendar scope management from the assistant panel to
  Settings > Floe access.
- Flattened assistant access into Settings, replaced internal identifiers and counters
  with user-facing ability language, and unified paired internal controls per ability.
- Reworked the assistant launcher as a spacious action card instead of a dense outlined
  button.
- Added and applied the animated, accessible `FloeSwitch` design-system component.
- [Evidence and validation limits](docs/validation/s4-agent-experience-feedback.md).
  S4 remains 0/14 and live key/model/source/privacy gates remain unchanged.

### S4 read-only proposal inspection — 2026-09-07

- Added Core inspection of a committed Expert proposal's existing S3 action. It
  authenticates encrypted receipt/session/package identity and returns the original
  action without model/provider calls, publication or approval/execution transitions.
- Historical evidence validation is separate from live grant checks: revocation and
  source changes do not hide an existing action, but still block new publication.
  Bounded action reads reject mismatched or oversized ledger records; missing actions
  remain absent rather than being recreated. Protected-key/cancel/deadline checks
  prevent returning uncertain evidence as successful inspection.
- Added eight tests covering historical versus current permissions, absent/recorded
  actions, all S3 states, reopen/revocation, copied receipts, corrupted records and
  cancellation/key loss. Calendar tests use both data classes with fictional records
  and injected models/access/keys, not live Personal sources.
- Validation: 203 workspace Rust tests, three keyring example tests, 25 native
  assertions, 169 Flutter tests, analysis, formatting and Clippy with existing
  exclusions pass. Rust/macOS Debug builds and deep strict signature verification
  pass; UI goldens are unchanged.
- [Evidence and limits](docs/validation/s4-proposal-inspection.md). Native inspection
  jobs, conversation-card recovery controls and connected-turn dispatch remain, along
  with live key/model/source/privacy gates. S4 stays 0/14; S1/S3 is unchanged.

### S4 Calendar scope consent UI — 2026-09-07

- Added Calendar access under Tools & Experts, using the existing same-Person Day
  Calendar selection in desktop and narrow assistant layouts. It requests no OS
  permission and requires exact one-to-four selection plus separate confirmation.
- Installation remains disabled; saved-scope enablement is separate from package and
  Person assignment flags. Connection changes clear local consent without expanding
  grants; unavailable scopes can still be revoked. Pending setup refresh/retry retains
  its original intent, and lock removes source metadata immediately.
- Added ten projection/widget cases for consent, responsive layouts, actual Today
  wiring, source changes, duplicate/legacy handling, revocation and uncertain/late
  results. The new real-font consent golden is rendered and reviewed.
- Validation: 169 Flutter tests, analysis, 195 workspace Rust tests, three keyring
  example tests and 25 native assertions pass. The macOS Debug build and deep strict
  signature verification pass; existing UI goldens are unchanged.
- [Evidence and limits](docs/validation/s4-calendar-consent.md). Connected chat/native
  turn dispatch, proposal recovery and live key/model/source/privacy gates remain.
  S4 stays 0/14; S1/S3 acceptance is unchanged.

### S4 Calendar Expert management transport — 2026-09-07

- Added explicit native inspection/setup jobs and scoped Calendar binding enablement.
  Empty inspection provides vault instance/revision without initializing a registry;
  the dedicated management response exposes only consent scopes and setup identities,
  never events/keys/private-state bodies or an Agent capability.
- Added typed Dart gateway/controller management with stable setup intent, lost-reply
  reconciliation, non-optimistic toggles and serialized chat/configuration ownership.
  Lock clears scopes/pending intent immediately and discards late responses.
- Added four Rust and 13 Dart tests for contract validation, read-only inspection,
  worker ownership/key failure, durable replay, Unicode scope bounds, response loss,
  pending-intent reconciliation and lock races. Fixtures use injected keys and no OS
  source/model access.
- Validation: 195 workspace Rust tests, three keyring example tests, 25 native
  assertions, 159 Flutter tests, analysis, formatting and Clippy with existing
  exclusions pass. Rust/macOS Debug builds and deep strict signature verification
  pass; UI goldens are unchanged.
- [Evidence and limits](docs/validation/s4-calendar-management.md). Calendar selection,
  confirmation and binding controls are not yet wired to a screen; native connected
  chat dispatch and live gates remain. S4 stays 0/14; S1/S3 acceptance is unchanged.

### S4 atomic Calendar Expert setup — 2026-09-07

- Added a single-revision Core operation that atomically installs a fresh Calendar
  binding, provider-pinned built-ins, Person assignments and a durable setup receipt.
  All new components start disabled; setup does not infer consent or touch a source.
- Exact retry returns the original receipt after reopen, revocation or private-state
  advancement without regranting access. Changed intent and stale new requests fail;
  encrypted CAS prevents receipt replacement and appropriation of old components.
- Removed the sample-first dependency: an explicit sample Send can add its missing
  package pair to an existing Calendar registry without replacing prior configuration.
- Added 12 tests covering atomic rollback, pre/post-commit key loss, response-loss
  reconciliation, sample coexistence, immutable receipts and installed Expert turns
  into Pending S3 review. Tests use injected keys/access/models and fictional records.
- Validation: 191 workspace Rust tests, three keyring example tests, 25 native
  assertions, 146 Flutter tests, analysis, formatting and Clippy with existing
  exclusions pass. Rust/macOS Debug builds and deep strict signature verification
  pass; UI goldens are unchanged.
- [Evidence and limits](docs/validation/s4-calendar-setup.md). Native setup transport,
  scope consent/enablement UI and connected chat dispatch remain. S4 stays 0/14;
  live key/model/source gates and S1/S3 acceptance are unchanged.

### S4 durable Calendar View bindings — 2026-09-07

- Added encrypted Person/provider/exact-calendar scope bindings with fresh handles,
  default-off registration and explicit enablement. Saved bindings cannot be removed
  or retargeted; old unbound grants cannot be silently appropriated for Calendar.
- Core Calendar turns require an exact active binding before model/native dispatch.
  Proposal publication checks binding revocation and source connection revision,
  preventing old evidence from being republished against a newer connection state.
- Validation: 179 workspace Rust tests, three keyring example tests, 25 native
  assertions, formatting and Clippy with existing exclusions pass. Existing Calendar
  fixtures now explicitly bind their scopes; no personal source, production vault key
  or live model was accessed. All 146 Flutter tests, analysis, native/macOS Debug
  builds and deep strict signature verification pass; UI goldens are unchanged.
- [Evidence and limits](docs/validation/s4-calendar-bindings.md). Scope consent/setup
  UI, installation/assignment editing, native connected turns and live gates remain.
  S4 stays 0/14 and S1/S3 acceptance is unchanged.

### S4 app registry enablement — 2026-09-07

- Added an unlocked-panel Tools & Experts screen backed by the native vault worker.
  It shows installed versions, Person assignment flags and minimized grant/state
  counts, with explicit installation/assignment enablement switches.
- Configuration checks vault instance/revision and preserves private state/grants
  atomically. Inspect never initializes packages; no new grants or approvals are exposed.
- Added response-loss reconciliation, non-optimistic serialized controls, conflict
  refresh, immediate lock clearing and late-result/key/worker-failure protection.
- Validation: 172 workspace Rust and 146 Flutter tests, three keyring example tests,
  25 native assertions, analysis, formatting, Clippy with existing exclusions, native
  and macOS Debug builds, and deep strict signing verification pass. The new registry
  golden uses real fonts and was rendered/reviewed; 320px/200% widget checks pass.
- [Evidence and limits](docs/validation/s4-registry-management.md). New package/assignment
  installation, detailed source grants, connected chat dispatch and live key/model work
  remain. S4 stays 0/14, with S1/S3 acceptance unchanged.

### S4 Core Calendar Agent turns — 2026-09-07

- Connected ordinary AgentCommand/model calls to an explicitly assigned Calendar
  Expert, leased Core View, encrypted atomic receipts/private state and S3 proposal
  preparation. The model receives a resolved descriptor with a bounded input schema.
- Revalidated the lease inside result/answer transactions and before/after inference;
  prior-turn tool outputs are marked stale only in model requests, preserving history.
- Added child cancellation/drop ownership, synchronous Stop race protection, bounded
  deadlines and publication outcomes that preserve committed sessions and stable IDs.
- Fifteen Core fixtures and one registry case cover successful preparation, restart,
  permission/expiry/registry/key changes, transaction rollback, malformed model input,
  cancellation/deadline/drop and Personal-class fixture projection. No live OS access.
- Validation: 167 workspace Rust and 135 Flutter tests, three keyring example tests,
  25 native assertions, analysis, formatting and Clippy with existing exclusions pass.
  The Rust library/macOS Debug build and deep strict app signature verification pass.
- [Evidence and limits](docs/validation/s4-calendar-turn.md). The app still uses samples;
  registry/scope configuration, native dispatch, live models and proposal recovery UI
  remain. S4 stays 0/14, with S1/S3 and live key/model gates unchanged.

### S4 bounded Calendar Timeline View — 2026-09-07

- Added a Core-backed Expert View for exact Person/calendar grants, current connection
  revisions and bounded fresh import coverage. Old/failed/overfull sources fail rather
  than appearing empty or producing a false free slot; healthy explicit subsets work.
- Minimized provider metadata, clipped crossing/all-day intervals, preserved DST day
  bounds and added a revalidation lease with cancellation/deadline/drop ownership.
- Added a native read-access boundary that checks full permission, inventory and a
  process-local change generation without requesting access or reading event bodies.
- Validation: 151 workspace Rust tests, 135 Flutter tests, three keyring example
  tests and 25 native assertions pass; analysis, formatting, Clippy with existing
  exclusions, macOS Debug build and deep strict signature verification pass. A fixture
  runs Calendar mirror → View → actual Expert → encrypted result → S3 Review.
- [Evidence and limits](docs/validation/s4-calendar-timeline.md). This is not a live
  native/source gate or production Agent turn. The sample panel remains synthetic;
  per-turn host orchestration, registry configuration and live key/model work remain.
  S4 stays 0/14, with S1/S3 acceptance unchanged.

### S4 Manager-to-S3 action bridge — 2026-09-07

- Added a trusted Core entry point that resolves committed Expert proposal references
  through encrypted sessions, durable receipts and current registry grants. Ordinary
  vault CAS now also prevents rewriting historical evidence or removing classifications.
- Reused the S3 Calendar draft/ledger and executor: Person authority routes to Review,
  approved delegated work or blocked Activity. Execution rechecks current authority;
  automatic approval cannot survive an allow-to-ask/deny change during preflight.
- One invocation maps to one stable action. Retry/restart/uncertain publication keeps
  its original execution ID and terminal state. Synthetic data cannot target EventKit;
  source text and conversation content are not copied into the action ledger.
- Flutter shows Floe attribution with scoped Expert/conversation metadata in Technical
  details. The default sample panel still cannot initiate Calendar changes.
- Validation: 136 workspace Rust and 135 Flutter tests, three keyring example tests,
  analyzer, formatting, native/macOS Debug builds and signature verification pass;
  Clippy passes with the existing exclusions. Fifteen new Manager/Expert/vault/S3
  cases use fictional data, fake providers and injected keys. The 320-pixel/200%-text
  Review golden uses real fonts and was rendered/reviewed. This is not live acceptance.
- [Evidence and remaining app/live gates](docs/validation/s4-manager-actions.md).
  S4 remains 0/14; S1/S3 and live key/model/Connector gates are unchanged.

### S4 encrypted Expert persistence — 2026-09-07

- Persisted packages, installations, Person assignments, grants and private state
  in the same encrypted vault as Agent sessions. Explicit first sample Send seeds
  a recognized old vault transactionally; open/unlock never recreates missing state.
- Paired Expert results, private-state updates and durable invocation receipts
  commit atomically. Abandoned drafts, failed writes, older invocation replay,
  configuration races and key loss cannot publish partial successful state.
- The secure native worker retains assignments/state across host restart and new
  chats. Configuration CAS cannot rewrite private counters or existing namespaces.
  The legacy plaintext fixture remains ephemeral and is not a fallback.
- Validation: 121 workspace Rust and 133 Flutter tests, three keyring example tests,
  analyzer, formatting, native/macOS Debug builds and signature verification pass;
  Clippy passes with the existing Calendar exclusions. Eleven encrypted-store cases
  include pre/post-commit key loss. These use synthetic data and injected keys,
  not live OS-key provisioning or a process-crash matrix.
- [Evidence and remaining gates](docs/validation/s4-expert-persistence.md).
  S4 remains 0/14; live key/model/source and S1/S3 acceptance are unchanged.

### S4 bounded Expert and registry foundation — 2026-09-07

- Added kind-separated Tool/Expert package references, pinned installations,
  Person assignments, explicit enablement, exact grants and revisioned private
  state. Registry snapshots validate on restore, but are not yet persisted by
  the host; the sample registry is rebuilt per turn.
- Native Schedule and deterministic declarative Experts share a bounded typed
  invocation/result path with denied-by-default Timeline Views. Revocation,
  stale/foreign/malformed Views, cancellation and budgets block state updates.
- The sample capability now records structured Schedule Expert evidence in its
  durable conversation. Flutter renders bounded, scoped source details rather
  than raw internal JSON; actual Dart/native restart tests cover the result.
- Validation: 108 Rust and 133 Flutter tests, analyzer, formatting, native build
  and Clippy with existing exclusions pass; macOS Debug build/signature verified;
  320-pixel/200%-text source golden
  rendered and reviewed. S4 remains 0/14, with no live source or personal-data access.
- [Evidence, limits and next work](docs/validation/s4-expert-foundation.md).

### S4 signed-key probe and native model adapter — 2026-09-07

- Added a disposable signed Core/keyring smoke harness with exact-slot cleanup
  and retained markers on uncertainty. Read-only NoEntry succeeds, but production
  vault creation fails and cleanup reports a missing entitlement. Signing alone
  did not pass the gate; no personal database or existing key was accessed.
- Added a macOS Foundation Models adapter behind the common Rust ModelRunner.
  Bounded structured output, policy checks, one native job, cancellation/deadline
  ownership and conservative full-context token reservations are implemented.
- The actual bundled availability probe reports AppleIntelligenceNotEnabled.
  Live generation is not verified; the default app remains on encrypted samples.
- Validation: 101 workspace Rust tests, three keyring example tests and native
  Swift fixture checks pass; macOS Debug build, formatting and Clippy with existing
  exclusions pass. No acceptance criterion is promoted.
- [Signed-key result and cleanup](docs/validation/s4-keyring-live-smoke.md);
  [native model evidence and remaining gates](docs/validation/s4-local-model.md).

### S4 vault host and panel integration — 2026-09-07

- Connected the default Today assistant to the encrypted vault through a new
  versioned submit/poll/stop/release C ABI and typed Dart gateway. The old sample
  route remains preset-only test infrastructure, not a fallback or migration.
- Moved key/DB operations and sample turns to a dedicated native worker. Blocked
  OS key calls do not block polling, Stop or native-handle shutdown; the original
  worker retains ownership until it actually finishes and releases the vault.
- Added explicit storage setup, unlock and lock controls. Close/navigation/app
  inactivity clears presented messages immediately, cancels/drains work and
  requests lock; late completion cannot repopulate a sealed controller.
- Added response-loss reconciliation without duplicate provisioning, worker
  ownership/cancellation tests, native read-only status tests and secure-panel
  checks at 320/390 widths and 200% text.
- Validation: 96 Rust and 129 Flutter tests pass; Flutter analyzer, native build,
  macOS Debug build/signature verification, formatting and Clippy with the
  existing Calendar exclusions pass.
- No personal text or real model is enabled. A signing identity is available,
  but live key creation/reopen, physical lock/denial and repair/cleanup still
  require validation. S4 remains 0/14; S1/S3 acceptance is unchanged.
- [Evidence and remaining work](docs/validation/s4-agent-vault-host.md).

### S4 encrypted session-store component — 2026-09-07

- Added a separate Person-scoped Turso AES-256-GCM session vault, encrypted
  identity binding, bounded session CAS and a lifetime exclusive file lock.
- Adopted the keyring-rs ecosystem (`keyring-core` + explicit Apple protected
  store) instead of directly implementing Security.framework calls. Keys stay
  native; the backend requests device-local, when-unlocked access.
- Key loss/change, failed provisioning, wrong keys, tampering and missing/empty
  databases fail closed without silent key replacement or plaintext fallback.
- Synthetic tests cover DB/WAL content, reopen, identity/revision isolation,
  competing processes and interrupted runtime recovery after key loss.
- Validation: 91 Rust and 117 Flutter tests pass; Flutter analyzer, native
  library/macOS Debug app builds, signature verification, formatting and Clippy
  with the two existing Calendar exclusions pass.
- This component is not connected to the sample panel/C ABI. No live Keychain
  access or personal data was used; signed-host key access and lifecycle gates
  remain open. S4 stays 0/14 and S1/S3 acceptance is unchanged.
- [Evidence, library choice and remaining gates](docs/validation/s4-agent-vault.md).

### S4 sample assistant panel — 2026-09-07

- Added one user-invoked Floe entry on Today: a contextual desktop panel and a
  narrow-screen sheet. Sample questions, source disclosure, progress, Stop,
  retry, read-only reload, explicit interrupted-session recovery and new/resumed
  conversations now use the shared Rust session/event contract.
- Added a bounded native run slot with begin/poll/stop/release, replayable event
  cursors and cancellation on native-handle shutdown. Calendar requests remain
  usable while the cooperative fixture model is waiting.
- The panel never accepts free text, reads connected sources or sends data to a
  model provider. Responses are synthetic; model delay is intentional fixture
  latency, not token streaming. Personal chat stays locked pending the vault.
- Controller/native tests cover cancellation, duplicate starts, response replay,
  shutdown, persistence and transport recovery; widget checks cover 320/390
  widths, 200% text, keyboard focus and the desktop/sheet entry paths.
- Validation: 77 Rust and 117 Flutter tests pass (three new native and 13 new
  Dart/widget tests); Flutter analyzer and macOS Debug app build pass. Strict
  whole-workspace Clippy still reports the two existing Calendar warnings.
- [Validation and remaining gates](docs/validation/s4-agent-panel.md).
  S4 stays 0/14; S1/S3 live acceptance is unchanged.

### S4 Agent contract foundation — 2026-09-07

- Began preparatory S4 implementation at the user's request without promoting the
  slice past its S3/session-vault prerequisites. S1/S3 live status is unchanged;
  S4 remains 0/14, not Integrated or Accepted.
- Added provider/UI-independent `floe-agent` ports, versioned commands/events,
  bounded multi-turn execution, read-only capability discovery, cancellation,
  deadline/token/cost/output/context/session limits and repeated-call halting.
- Added explicit inference placement/consent/projection checks, sensitive history
  classification retention, stale-source checks and fail-closed vault availability.
  Raw device data and credentials are excluded from Agent context entirely.
- Added CAS-persisted synthetic sessions and a preset-only fixture route through
  JSON/C ABI and typed Dart gateway. Completed capability input/results stay
  paired; final assistant text and successful outcome commit atomically.
- No personal chat storage, actual model, native chat UI, live connectors or S3
  mutation integration is enabled. Native fixture responses batch events; live
  streaming/stop transport, encrypted vault and Expert registries remain next work.
- [Validation, reproduction and exact boundaries](docs/validation/s4-agent-foundation.md).
- Validation: 74 Rust tests and 104 Flutter tests pass, including 24 new Rust
  Agent/core/ABI tests and three Dart tests. Flutter analyzer, Agent Clippy and
  formatting checks pass; whole-workspace Clippy retains two existing Calendar warnings.

### Decision-first UI checkpoint — 2026-09-06

- Follow-up removes timezone/UTC/offset fields from user input, review and event
  detail surfaces. Planning accepts device-local date/time with consistent field
  spacing; UTC and fixed-offset scheduling metadata remain internal boundaries.
- Parallel Select/Dropdown prototype and action-review simplification are integrated.
  Shared controls support keyboard/typeahead, focus recovery and restrained motion;
  the review prioritizes the user's decision and collapses technical metadata.
- Recorded the product-wide abstraction principle; applied it in prototype and
  Flutter review without changing execution authority or the write-disabled gate.
- 48 component contracts, 26 action assertions, 13 selection navigation assertions,
  11 source guards, production prototype build and 72 Flutter tests pass; analyze clean.
- Browser checks cover desktop/390/320 layouts, Select/Dropdown keyboard flow,
  diagnostic disclosure, rejection and lookup/read-only recovery simulations.
- [Evidence and remaining accessibility checks](docs/validation/decision-first-ui.md).
  S3 remains 2/5 verified; this presentation pass is not new live acceptance evidence.

### S3 native execution checkpoint — 2026-09-06

- After user-granted Calendar permission, the signed Flutter app completed explicit
  approval → native create → MethodChannel read/import → Day Canvas. Relaunch and
  read retries retained one event; exact cleanup removed it without changing the
  successful ledger or creating a replacement. S3-A3/A4 verified (2/5), not Accepted.
- Existing 11 calendar selections stayed unchanged. At this checkpoint, both
  Debug and Release were restored to write-disabled builds; ad-hoc rebuilds may
  require a fresh OS grant. The current rollout policy enables the executor in all
  builds.
- Connected proposal preparation, explicit approval, native execution, lookup-only
  recovery and separate Calendar read retry.
- Live disposable create through the actual Rust/native adapter survived injected
  response loss and process restart; duplicate create was blocked. Exact cleanup
  and absence verification passed. Existing calendar selections were preserved.
- Fixed legacy mirror CAS compatibility (`8c068da`); diagnosed ad-hoc signing/TCC
  mismatch and began user-authorized Floe-only Calendar reauthorization.
- 46 Rust tests, 72 Flutter tests and nine native assertions pass.
- [Implementation, evidence and remaining gates](docs/validation/s3-native-executor.md).

### S3 native review UI checkpoint — 2026-09-06

- Today loads saved proposals, opens immutable review and records approve/reject
  through Rust. No proposal producer or live write is enabled.
- Persistent visible outcomes, guarded approval, in-flight/response-loss handling
  and read-only reload; narrow/desktop and keyboard widget coverage.
- [Behavior and validation boundaries](docs/validation/s3-action-review-ui.md).
  S3 remains 0/5; native execution and S1/live gates remain pending.

### S3 decision bridge checkpoint — 2026-09-06

- Added Person-scoped action list/get, proposal and explicit approve/reject over
  JSON/C ABI, with typed Dart access and durable reload. Rust owns action clocks.
- No execution/receipt/policy input is exposed; native UI and live writes remain gated.
- [Contract and automated validation](docs/validation/s3-action-bridge.md).
  S3 remains 0/5 and S1 is not Verified.

### S1 scope and recovery checkpoint — 2026-09-05

- Selected retains explicit IDs; All discovers new calendars on refresh/date reads.
  Legacy connections remain Selected. Scope UI was implemented in the prototype first.
- Per-source recovery, missing-source cache, disconnect revision tombstones,
  23/25-hour civil-day reads/timelines and ordinary EventKit move identity are implemented.
- [Validation and remaining live gates](docs/validation/s1-scope-recovery.md).
- Historical notes below predate this two-mode decision; auto-inclusion is required
  only in All mode. S1 remains 0/4 verified, not complete from fixture evidence.

### Live EventKit and prototype-first checkpoint — 2026-09-05

- User-approved isolated helper created one disposable event in a dedicated iCloud
  Calendar, deliberately lost its response, blocked a second create and recovered
  exactly one matching execution marker from fresh processes.
- Existing app re-imported it with matching ID, Person and Asia/Seoul times. External
  test edit preserved occurrence identity and advanced revision 0→1; test deletion
  disappeared on refresh. The empty test calendar remains; existing events untouched.
- **S1 defect:** ordinary refresh omitted the new calendar; reconnect required an
  explicit selection. S1 is not Verified; complete acceptance remains 0/4.
- Native permission deny/revoke cycles, DST/recurrence, partial failure and offline
  restart remain unverified. This is local EventKit evidence, not cloud durability.
- UI is implemented/reviewed in the HTML prototype first: approval/rejection,
  preflight/create/re-import, blocked states, lookup-only recovery and read-only retry.
  No Flutter UI or production write API added. S3 remains 0/5.
- Prototype build, 42 component contracts and 26 reducer assertions pass; seven
  browser scenarios validated. Swift helper compiles with warnings denied/signs.
- [Full live record and remaining gates](docs/validation/eventkit-live-poc.md).

The reusable [performance-class routing](docs/decisions/0011-inference-performance-classes.md)
and local Go model gateway remain implemented. No current product feature invokes
the gateway after removal of the focus-time experiment. This does not advance S8
server/sync acceptance.

### Local connection console checkpoint

[ADR 0010](docs/decisions/0010-local-connection-console.md) and its
[validation record](docs/validation/local-connections.md) cover reusable infrastructure.
Rust and Flutter suites, paired non-default-port fixture, Go race/vet, installed
Codex handshake and disposable Keychain round trip pass.
Local server/dashboard are running and the operator confirmed native app pairing.
Keychain persistence across a relaunch remains a manual checkpoint because native
UI automation timed out. No real model or OAuth consent is claimed.

## Acceptance Evidence

### S3 executor checkpoint — 2026-09-05

- Immutable calendar-create proposals with explicit approve/reject and expiry.
- Rust-owned durable execution IDs and atomic CAS transitions; duplicate execution
  claims cannot dispatch twice. Ambiguous/interrupted execution uses lookup only.
- Trusted policy/Person/target checks, connection-change detection, provider
  preflight contract for capability/permission/timezone/conflicts, and receipt matching.
- Fixture covers successful re-import, reopen, concurrent execution, cancellation
  after write, typed failures and ambiguous recovery without blind retries.
- `cargo test --workspace`: 35 passed, including 11 new action tests;
  Clippy with warnings denied and formatting checks pass.
- [Validation and adapter contract](docs/validation/s3-calendar-action.md).
  No Flutter/FFI action surface, EventKit write adapter, live provider PoC or UI
  acceptance is claimed. S3 remains 0/5; S1 remains Deferred, not Verified.

S1 automated and native-build evidence is described in
[the validation runbook](docs/validation/s1-calendar.md). Live results remain pending;
fixture-only evidence cannot satisfy the live integration acceptance gate.

Evidence build: `0d52e2620938fc7008572937e72dc8d6572c6e0b`, 2026-09-04,
macOS 26.2 (25C56), arm64. Connector integration is fixture; EventKit is SDK/build-only.

| Criterion | Result | Evidence | Remaining live gate |
| --- | --- | --- | --- |
| S1-A1 | Pending | `calendar_gateway_test.dart`: typed denial retains cache; settings/refresh actions pass. Native EventKit app compiles/signature verifies. | Real prompt, selection, revocation/reconnect |
| S1-A2 | Pending | Rust calendar tests and Flutter FFI/widget tests: provenance, Person, IDs, change token, all-day and UTC+09 midnight | Real normalization, recurring exceptions and timezone cases |
| S1-A3 | Pending | Rust calendar tests: idempotent import, update, range-only deletion, stale/invalid batch rejection | Real external edit/delete and recurring identity |
| S1-A4 | Pending | Rust and native FFI tests: cached events/error survive reopen; retry clears failure | Signed app lifecycle and provider-failure demo |

### S1 implementation checkpoint

- EventKit calendar listing, explicit permission request, date-range read, and settings recovery.
- User-approved OS full-access exception; the app exposes no external write operation.
- Person-scoped selection, stable occurrence provenance, change token, and atomic CAS mirror persistence.
- Duplicate/stale/invalid-batch rejection, range-scoped reconciliation, preserved cache on failure/restart.
- Source labels, multiple all-day events, expanded timeline for midnight/off-hours, and manual refresh.
- Fixture tests cover Rust storage/projection and the actual Dart/JSON/C ABI boundary.
- Live EventKit permission/read behavior, recurring identity, DST, S3 create PoC, and three-day dogfood remain unverified.

### S1 UI reference

The HTML prototype now uses a quiet unified Today view of all connected calendars,
connection inventory, and five popup flows without an on-page prototype lab.
User feedback expands the target from one selected calendar to all calendars available
through macOS Calendar; [ADR 0008](docs/decisions/0008-unified-calendar-read.md) records
the scope and pending native migration. Existing native single-selection evidence does
not establish acceptance of this new multi-calendar contract.
[UI specification](docs/design/s1-calendar-ui.md) records behavior, responsive checks,
and proposed-vs-native boundaries. This changes the design reference only; native
implementation and live acceptance counts are unchanged.

## Existing Personal Day Baseline

ADR 0004 remains partially delivered, not accepted. Existing delivered work is
preserved below. Non-blocking local UI breadth is deferred while S1 and S3 are
prioritized; MVP acceptance and its two-week dogfood requirement remain separate.

## Current Checkpoint

- [x] Rust workspace and Personal Timeline domain baseline
- [x] Event, Task, Note, and Capture separation
- [x] Capture provenance and revision-aware mutations
- [x] Deterministic Day Snapshot with Now, Next, and overdue projection
- [x] Embedded Turso persistence and schema migration on macOS
- [x] Versioned JSON protocol and C ABI
- [x] Dedicated Dart FFI isolate and native handle lifecycle
- [x] Flutter Day Canvas and typed Universal Capture
- [x] Explicit Event, Task, and Note classification
- [x] Task completion/reopen and item deletion
- [x] macOS dylib build, embedding, signing, and persistence verification
- [x] Squircle-first responsive shell, Day Canvas, Notes, and Task Detail baseline
- [ ] Event, Task, and Note editing UI
- [ ] Explicit conflict recovery UI
- [ ] Dense-day folding and complete timeline treatment
- [ ] Calendar integration
- [ ] Two-week Day Canvas dogfood

## MVP Areas — Separate Product Scope

| Area | Status | Completed | Remaining |
| --- | --- | --- | --- |
| Day Canvas | Partial; non-blocking breadth deferred | Now/Next, unified local projection, empty and overdue states | Conflict UX, editing, folding, interventions |
| Universal Capture | Partial | Typed capture, original input, explicit classification, provenance | Voice/STT, correction flow, semantic candidates |
| Local Personal-Day Store | Partial | Rust-owned Turso, CRUD core, deterministic snapshots, reopen persistence | Encryption, export/forget, sync, multi-person management |
| Minimal Assistant | Not started | Reusable Go inference infrastructure only | Select and validate a product use case |

## Roadmap Status

These are delivered capability statuses, not sequential work gates. Planned
cross-phase coverage is defined in the slice plan and does not change these statuses.

| Phase | Status | Notes |
| --- | --- | --- |
| Phase 0 — Architecture PoCs | Partial | macOS embedded Turso and Flutter↔Rust boundary validated; other PoCs remain |
| Phase 1 — Personal Day | Partial | Technical vertical slice works; product breadth and dogfood remain |
| Phase 2 — Connected Floe | Partial | S1 EventKit read path implemented; live validation and other connectors remain |
| Phase 3 — Personal Memory | Not started | Memory, people and identity resolution remain |
| Phase 3.5 — Expert Ecosystem | Not started | Package, permissions, sandbox, SDK, and marketplace remain |
| Phase 4 — Cross-device | Not started | Sync, Device Agent, and native packaging remain |
| Phase 5 — Ambient Floe | Not started | Wake word, transcription, handoff, and interventions remain |
| Phase 6 — Hosted/Self-host | Partial; local inference only | Local Go gateway; hosted server, accounts, deployment and administration remain |

## S1 Automated Validation

2026-09-04, macOS arm64. Dependency mode: fixture for connector behavior;
EventKit native adapter compilation only, not a live read.

- Rust: 22 tests passing (`cargo test --workspace`, including existing External-source compatibility).
- Flutter: 39 tests passing, including actual native FFI tests and load-error recovery (none skipped).
- Clippy: clean with warnings denied; Flutter analyzer: clean.
- macOS: debug app builds with Calendar entitlement and EventKit usage strings.
- No external calendar data collected or changed; live acceptance remains 0/4.

### Follow-up test run

2026-09-04 14:05 KST, source `8fafc96` (implementation `0d52e26`), same macOS environment:

- `cargo test --workspace`: 20 passed; `cargo build -p floe-ffi`: passed.
- `flutter test`: 38 passed, no skips, including native FFI and Calendar fixture tests.
- Clippy with warnings denied, Flutter analyzer, and debug app signature verification pass.
- Existing debug app launches; process sampling shows the main thread waiting normally
  in the AppKit event loop rather than blocked in Calendar code.
- Live UI automation is blocked: Computer Use repeatedly returns `-10005 timeoutReached`
  while acquiring the Floe window, including by resolved bundle ID `app.floe.floeClient`.
  This is not evidence of an EventKit permission/read failure or a successful UI demo.
- No Calendar permission prompt was accepted, no calendar selected, and no live data
  collected or modified in this run. S1-A1–A4 stay pending; resume with a visible
  connection/permission result supplied by the user or working UI automation.

## Local-data Compatibility Fix — 2026-09-04

- User reported only the retry button was visible. An isolated diagnostic copy of
  the existing store reproduced `storage: unknown variant External` through the
  bundled C ABI. This is independent of the UI automation timeout noted above.
- Added lossless `External` source decoding/encoding and a separate legacy-source
  wire variant; existing provenance is not rewritten into a newly selected Calendar.
- Existing external events remain read-only; no stored records are deleted or reset.
- Day-load failures now show the actual error and retry action instead of an unexplained button.
- The same diagnostic copy loads successfully with the fixed native library.
  Regression tests cover old JSON persistence/reopen, provenance round-trip,
  write rejection, and visible error/retry recovery. No private data is committed.
- Live EventKit acceptance remains pending; this validates local-data compatibility,
  not permission grants or provider reads.
- Rebuilt and signature-verified the debug app, then relaunched it against the
  existing store. UI inspection now succeeds and shows Day Canvas plus **외부 Calendar
  연결 / 연결 / 권한 설정**, rather than only retry. The temporary diagnostic copy was removed.

## Historical Personal Day Validation

Last recorded baseline: 2026-09-03. These results are preserved from the previous
progress record, not rerun or newly verified by the 2026-09-04 planning change.

- Rust: 15 tests passing
- Flutter: 12 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

## Next Priorities

1. Complete S1's live Calendar criteria.
2. Complete signed-app S3 approval/create/collection and provider-failure validation.
   Prototype review, Flutter UI and native Rust binding are implemented; runtime
   authority and fresh safety checks remain mandatory.
3. Start S4 with text chat, durable sessions, the bounded Agent loop and shared
   Tool/Expert/Connector registries; validate Codex authentication, a device-local
   Foundation Model/sLLM and sensitive routing; add Gmail, Contacts, location/ETA/weather
   plus physical-device Screen Time and Apple Health gates before connecting Expert output to S3.
4. Follow with S5 source-backed Memory and governed Playbook learning,
   then S6 press-to-talk voice and S7 local wake-up. Do not begin S8 server/sync
   as a shortcut.
5. Evaluate an officially supported OAuth adapter and Apple native availability separately; do not assume CLIProxyAPI adoption.

Deferred, not completed: Event/Task/Note editing UI, general conflict recovery UI,
dense-day folding, and the separate two-week Personal Day dogfood. If one blocks
the active slice, pull in only the necessary portion; slice-specific error and
conflict handling remains mandatory.

## Update Rules

- Update this file in the same commit as a milestone status change.
- Record only delivered, validated, in-progress, or blocked work.
- Do not duplicate product requirements or architecture decisions here.
- Do not mark roadmap work complete based only on scaffolding.
- Update validation counts whenever tests are added or removed.
- Track at most one slice in Implementing, Integrated, or Verified; Dogfooding may overlap the next slice.
- Advance states only with evidence; record blockers separately and regress status when acceptance fails.
- Distinguish fixture, sandbox, and live evidence for each dependency; record known limitations.
- Keep criterion definitions in the slice plan and results here; do not restore subjective phase percentages as the primary delivery metric.
