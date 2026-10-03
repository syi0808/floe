# T0 client behavior ledger: native acquisition, local publication and diagnostics

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Full-source static extraction; no execution or removal. D = durable safety/property; P = product hypothesis; O = obsolete representation; H = harness. Mixed classification preserves safety meaning without freezing old shape. Entry headings provide source registration/span; the file hash binds all prose to exact baseline bytes.

## apps/client/test/infrastructure/diagnostics/app_diagnostics_test.dart

Full source read: lines1–130; SHA-256 `025bb747e40e66970c68e69376c51519cf7c20850acb7b6e214eace1e7c1f6b9`.

Current owner: AppDiagnostics privacy-filtered ring/journal. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'diagnostics retain identifiers without error payloads' (test; lines9–37; D)

Record a StateError containing private prompt text plus typed failure metadata. Exported JSON preserves request/session IDs, failure domain/category/reason, incident,safe-actions and error type while excluding actual error payload. Diagnostics must retain correlation without copying private prompts.

### 'diagnostics use a bounded ring buffer' (test; lines39–47; D/P)

Append510 synthetic events to the in-memory ring. Keep exactly500, dropping event0–9 while retaining event10 through509 in order. Bounded retention is durable;500 is a configurable product limit.

### 'diagnostics persist across initialization' (test; lines49–64; D/H)

Initialize in a private temp directory, record persisted, flush and initialize again. The saved event survives reload. Test teardown deletes the journal and private directory; no existing user journal is used.

### 'diagnostics rotate bounded journal files' (test; lines66–92; D/P/H)

Initialize private journal with180byte files/max2, append20 events and flush. Exactly two incidents.ndjson-prefixed files remain. Bounded rotation is covered; file contents/order/oversize-line behavior are not separately asserted.

### 'sensitive failure text is excluded from the journal' (test; lines94–114; D/H)

Record both error text and failure text containing secret prompt, flush private journal and read its bytes. Neither secret phrase may occur. Privacy filtering applies to persisted output, not just the in-memory JSON view.

### 'journal write failures stay in memory' (test; lines116–129; D)

Initialize against impossible /dev/null/floe-diagnostics path. Recording an error returns normally and leaves one in-memory record. Journal I/O failure must not break diagnostics callers or erase available in-memory evidence.

## apps/client/test/infrastructure/native/android_context_gateway_test.dart

Full source read: lines1–111; SHA-256 `dc760d89d43976b93da68a820b375ba2a4fa4b0d48c42a29d535a0c1d722784b`.

Current owner: Android native-view validators and AndroidCalendarOption; dormant adapter. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'Android Calendar projection accepts only the strict bounded View' (test; lines5–35; D/O)

Validate a bounded Calendar timeline with one opaque event. Reject an added authority=create field and duplicated evidence identity. Native projection cannot carry permission or ambiguous duplicate identities. Dormant Android wire representation is not a delivery commitment.

### 'Android Contacts projection accepts only bounded identities' (test; lines37–66; D/O)

Accept one bounded opaque People identity; add a private note inside it and reject. Contact notes cannot leak through the projection even with otherwise valid metadata.

### 'Health Connect projection exposes only derived wellbeing' (test; lines68–93; D/O)

Accept legacy derived wellbeing with typical/recovered evidence; reject raw heart_rate_samples, and reject unknown/unknown while retaining nonzero confidence/evidence. Privacy/uncertainty honesty survives, but direct derived Health input is superseded by mandatory local transform.

### 'Android calendar choices exclude provider account metadata' (test; lines95–110; D/O)

Decode Calendar option with only id/display name; adding account_name private email rejects. Provider account metadata is excluded from the option boundary.

## apps/client/test/infrastructure/native/apple_context_gateway_test.dart

Full source read: lines1–105; SHA-256 `53e10c9cfd26fe418bce171aa8086813a5a7a82429c5436a6e6a6a00b6082f06`.

Current owner: AppleContextGateway native identity/inventory/view validators. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'Apple inventory contains exactly three supported entries' (test; lines7–22; P/D)

Inventory accepts exactly contacts.apple,health.apple,attention.apple. Removing one or adding an empty fourth entry throws. This is strict current inventory shape; platform capability honesty remains durable while exact count is product-specific.

### 'uses the validated local cache producer ID at the native boundary' (test; lines24–35; D)

appleNativeArguments preserves validated local device ID alone or merged with limit64. Constructing gateway with whitespace-containing different device rejects ArgumentError. Native identity is validated rather than supplied arbitrarily.

### 'accepts bounded Apple native views and capability gate' (test; lines37–74; D/O)

Validate three positive fixtures: opaque bounded People identity, legacy derived wellbeing, and entitlement_unavailable/notDetermined/unknown-region Screen Time capability. No OS calls occur. Health fixture is pre-transform representation, not accepted future reasoning input.

### 'rejects raw fields and a falsely supported Screen Time gate' (test; lines76–104; D)

Reject wellbeing that adds raw steps and reject falsely supported Screen Time with undetermined authorization/unknown region plus entitlement_provisioned extra field. This combined malformed fixture proves rejection, not which individual gate independently causes it.

## apps/client/test/infrastructure/native/attention_acquisition_broker_test.dart

Full source read: lines1–223; SHA-256 `aee7ec4006405a5ef8feb3cfa938aa49dd61606d6a4e01e31521ae2000d0b0e3`.

Current owner: AttentionAcquisitionBroker / App-owned acquisition correlation. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'completes a trusted inspect result once' (test; lines8–35; D)

Poll an inspect_subject request on registered host-a, echo exact request/person/device/mode with stable before/after native subject and session_observation permission, no view. Broker reports completed and no typed failure. Although label says once, this test performs only one poll; repeated-poll exactly-once behavior is not separately asserted.

### 'provider failure completes the pending request as a typed failure' (test; lines37–57; D)

Reader throws platform permission_denied. Broker handles it as a completed polling cycle, records typed permission_denied and no successful result, then disposes. Denial must release the queued request without invented observation.

### 'rejects a queued request from a different host epoch' (test; lines59–73; D)

Queue stale-host request for a host-a broker. Poll throws FormatException and sends no completion. Native work cannot be accepted across host epoch identity.

## apps/client/test/infrastructure/native/calendar_acquisition_broker_test.dart

Full source read: lines1–292; SHA-256 `54ed8803ef231abaed3ed7ffbe664895f345cc22c6cc534545aa52414b97807d`.

Current owner: CalendarAcquisitionBroker / reviewed native subject acquisition. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'polls one bounded request and completes exact identity' (test; lines7–31; D)

Poll one request with exact person/device/connection revision/provider/calendar IDs/range/reviewed fingerprint. Reader echoes identity, drops deadline/review-input field and returns stable matching fingerprints, authorized permission, one available calendar and one empty-success batch. Broker completes. Default provider is dormant Android; this is boundary validation, not Android parity work.

### 'disposed broker rejects late polling and completion' (test; lines33–47; D)

Start then dispose broker. A later poll rejects StateError and transport records disposal. No actual in-flight late completion is injected despite label wording; preserve the narrower observed lifecycle check.

### 'inspects an iOS subject without accepting event batches' (test; lines49–75; D)

For EventKit inspect_subject mode with no expected fingerprint, reader confirms mode/no prior review fingerprint, returns stable new subject/full_access and empty batches. Completion succeeds. This is positive subject-only inspection; it does not separately inject nonempty batches to demonstrate rejection.

### 'rejects a provider subject fingerprint that differs from review' (test; lines77–107; D)

For reviewed fingerprint a, return matching before/after b instead. Reject FormatException, send no successful completion and report provider_unavailable. Stable provider identity alone is insufficient if it differs from reviewed subject.

### 'completes provider denial without holding the queued request' (test; lines109–126; D)

Throw platform permission_denied during queued Calendar read. Poll reports handled=true and records typed permission_denied, rather than holding the request forever or returning empty-success data.

## apps/client/test/infrastructure/native/local_context_publication_test.dart

Full source read: lines1–531; SHA-256 `690451fc31c2f7da955dc6850fc84847f3cbd5642cadc768cd4b309c456a4032`.

Current owner: PublishingApple/MacOS/AndroidContextGateway / AndroidCalendarAdapter. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'publishes only validated Apple View payloads' (test; lines14–40; D/O)

Read validated Apple Contacts and legacy Wellbeing at fixed clock. Publish exactly people.identity then wellbeing.derived bound to expected person/device. Identity/privacy boundary is durable, but direct pre-transform Health publication is obsolete under accepted local-transform requirement and cannot be copied into target Context.

### 'rejects invalid native output and revokes its cached View' (test; lines42–58; D)

Inject raw_phone_number into native People output. readContacts throws FormatException, publishes nothing and revokes cached people.identity. Invalid new output cannot leave stale private projection active.

### 'revokes unknown, stale, denied, logout, and former Person data' (test; lines60–104; D/O)

Sequentially read unknown/unknown confidence0 wellbeing, receive denied Contacts permission, read stale People view, bind a new person and logout. Record revocations in order wellbeing,People,People,all-old-person,all-new-person. Unknown/stale/denied data and former identity caches must not remain readable. Legacy wellbeing representation retires; existing fixture retains evidence under unknown, so do not infer a general raw-schema rule from this wrapper-level revocation test.

### 'publishes macOS coarse attention and clears unknown attention' (test; lines106–131; D/P)

Publish a valid focused macOS Attention view, then read unknown with confidence0/no evidence. First publishes unchanged; second revokes attention.coarse rather than publishing a fabricated state. Exact focused scoring remains product-specific.

### 'publishes Android Contacts and Health Connect projections' (test; lines133–155; D/O)

Dormant Android fake Contacts and Health reads publish people.identity/wellbeing.derived and no steps field. Preserve raw-data exclusion; direct derived Health publication is obsolete and no Android runtime validation is claimed.

### 'Android denial and unknown health clear cached projections' (test; lines157–184; D/O)

Dormant Android Contacts permission denied plus unknown-zero-empty Health view cause no publication and ordered People/Wellbeing revocation. Permission loss cannot retain former cached evidence.

### 'Android selected calendars cross the durable Calendar adapter' (test; lines186–211; D/O/P)

AndroidCalendarAdapter reads selected calendar-one and converts one bounded coarse event to durable adapter record: provider android, external ID from evidence handle, can_modify=false, UTC timed interval2–3seconds. Translation must not confer write authority; concrete dormant wire shape may retire.

## apps/client/test/infrastructure/native/macos_context_gateway_test.dart

Full source read: lines1–64; SHA-256 `fc620c26d2be47aef2244641e33180d6047df937a7894d21e5ca034fd49d84f6`.

Current owner: validateMacOSAttentionView native projection validator. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'macOS Attention projection accepts a bounded coarse View' (test; lines5–24; D/P)

Accept focused coarse Attention with bounded timestamps,confidence750 and coarse evidence. Add bundle_id and reject, ensuring application identity cannot leak into this view; exact confidence/expiry are product choices.

### 'macOS Attention projection enforces unknown semantics' (test; lines26–45; D)

Accept unknown Attention only with confidence0/empty evidence. Changing confidence to500 throws. Unknown evidence cannot be inflated into certainty.

### 'macOS Attention projection cannot describe absence as available' (test; lines47–63; D)

Reject state available with high confidence sourced only from session_idle. Absence/idle does not establish user availability or interruptibility.

## apps/client/test/infrastructure/native/personal_acquisition_broker_test.dart

Full source read: lines1–265; SHA-256 `99f9ee890523127cdfa8a14986728f3f08055cf6ce0fca072ff7003fd4e7f2be`.

Current owner: PersonalAcquisitionBroker / App-owned acquisition correlation. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'Wellbeing acquisition accepts no selected handles' (test; lines7–35; D/O)

Change default queued request to wellbeing with empty selected_handles. Reader echoes request/host/person/device/domain, stable before/after subject fingerprint and authorized Apple Health provider; broker completes successfully then disposes. Wellbeing has no People selection. Minimal legacy wellbeing view fixture does not prove source-local privacy transform.

### 'invalid selection or unknown request shape never reaches the reader' (test; lines37–66; D)

Four input mutations each reject before reader/completion: People with empty selection; Wellbeing while retaining the default People selected handle; unknown domain; unexpected even-null field. Read counter stays0 and result remains absent. Strict acquisition intent cannot broaden or cross domains.

### 'completes exact selected People acquisition' (test; lines68–102; D)

Default People request selects exactly person.identity:a. Echo its identity/domain/subject fingerprint with authorized contacts provider and bounded empty People view; broker returns true and records completion. Positive identity path only; no other person/device mismatch injected here.

### 'rejects a result whose view domain changes' (test; lines104–130; D)

For the same People request, return a wellbeing.derived view while echoing outer domain. FormatException and no completion: outer correlation cannot disguise a cross-domain payload.

## Helpers and dependency interpretation

All eight infrastructure test files were read through their final helper declarations. Broker _FakeTransport classes implement the broad LocalContextTransport interface but only their own registration/poll/complete/fail/dispose state is meaningful; all unrelated domains are no-ops or empty lists. Attention polling returns no request after stored completion/failure, Calendar fixture repeatedly exposes the configured request until disposed, and Personal exposes it until complete. These are H, not evidence that native providers or unrelated domains are covered.

Publication fake Apple/MacOS/Android APIs return local maps, permissions and selected calendars; _RecordingTransport copies published maps and records revoke identities. No OS prompt, Health sample read or Context repository actually runs. AppDiagnostics tests uniquely do real temporary journal filesystem I/O. Current native gateways validate external OS boundary projections; acquisition brokers preserve App-owned acquisition correlation and native reviewed subject; publication wrappers must not become source authority. Re-prove these D properties under canonical source/Context owners after refactor; replace direct Health-derived paths with the required source-owned local privacy transform and keep HighlySensitive classification.
