> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: runtime, wire and Vault

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Full-source static extraction; no execution or removal. D = durable safety/property; P = product hypothesis; O = obsolete representation; H = harness. Mixed classification preserves safety meaning without freezing old shape. Entry headings provide source registration/span; the file hash binds all prose to exact baseline bytes.

## apps/client/test/app/runtime/agent_vault_controller_test.dart

Full source read: lines1–124; SHA-256 `86ceaf2bba1c2e3eb4ccdc8402836f5979024b93c3bcf713a04f84edfcfe4736`.

Current owner: AgentController presentation and AgentVaultGateway lifecycle. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'secure storage setup and unlock happen automatically' (test; lines11–33; P/D)

Starting with missing storage, load automatically creates exactly one Vault and permits sending; send a greeting, closeView clears presentation/session, then load unlocks the same saved session with messages rather than creating another Vault. Auto-open presentation is a product choice; key continuity and no replacement are durable.

### 'automatic reload waits for an in-flight vault lock' (test; lines35–47; D)

Load a ready controller, race closeView and load, and await both. Reload waits for the in-flight lock: final state ready with session, exactly one lock and unlock. Serialized owner lifecycle prevents a stale close from sealing a newly loaded session.

### 'closing during a turn clears immediately and never republishes completion' (test; lines49–68; D/O)

With a held turn already running, closeView clears messages/session immediately; completion cannot republish them, sending remains disabled, and the legacy fake records one lock and one stop. Preserve stale-result sealing, but the implicit closeView cancellation expectation conflicts with accepted view-disposal-not-cancellation semantics and must retire rather than become a requirement.

### 'key failure removes presented messages and cannot create a replacement' (test; lines70–88; D)

Create/unlock and send once, then make key access fail. Reload becomes unavailable, clears presented messages and records vault_unavailable; a later send cannot begin another turn or create a replacement key. Re-enable only the fake failure flag for teardown.

### 'model failure keeps the unlocked vault and confirmed messages' (test; lines90–108; D)

A model failure omits a usable completion session and reports server_model_invalid_output. Sending records that failure and needsReload while keeping Vault ready, a session and confirmed messages. Model failure alone must not lock or replace storage.

### 'closing during unlock seals delayed loaded messages' (test; lines110–123; D)

Hold resume during unlock, call closeView, then release the delayed load. Both operations settle with empty messages, null session, locked Vault and one lock. Late loaded plaintext cannot leak back after sealing.

## apps/client/test/app/runtime/agent_vault_gateway_test.dart

Full source read: lines1–318; SHA-256 `c74944d0ebfd8d97f6fd63bc5d720b9937f910ff224278287425cb28721db9af`.

Current owner: NativeVaultLifecycleGateway / AppRuntime Vault owner boundary. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'real Dart/C ABI status does not provision storage or access keys' (test; lines15–86; D/H)

Open a real debug dylib through TestAppHost in a private temp profile, with no platform/dylib skip guard. Status reports missing while conversation runtime exists; status must not create a Vault directory or access/provision keys. Resume, proposal inspection, registry read and a repeated resume all fail vault_unavailable with the directory still absent. Teardown drains/closes runtime and removes only the private directory. These are real FFI/filesystem probes, not mocked native access.

### 'lost create response is drained without provisioning a second key' (test; lines88–99; D)

Fake transport persists a completed create operation then loses its submission response. create throws once, but later status drains that same pending result and reports ready; create counter remains one and pending result is released. Response loss cannot provision a second key.

### 'lost release acknowledgement is resolved by read-only lookup' (test; lines101–110; D)

Fake create completes but its release acknowledgment is lost after the pending result was removed. Initial create throws; later status uses read-only reconciliation and reports ready with only one creation, preventing redispatch after ambiguous release.

### 'request identity mismatch cannot be accepted as unlocked' (test; lines112–123; D)

Wrap an otherwise valid Vault response but replace operation_id with wrong-id. Status decoding must throw FormatException rather than accept it as unlocked; request identity is authoritative.

### 'completed failure retains request identity and stage' (test; lines125–177; D)

Return a completed, versioned model_unavailable failure with status stage, no retry and matching request correlation. The raised AgentVaultException retains request ID/stage/nonretryability; exactly one diagnostic preserves the same request, status operation and reason. The ready-shaped envelope cannot hide the completed failure.

### 'native failures require the versioned envelope' (test; lines179–196; D/O)

Return a plain capability_unavailable string where a structured failure envelope is required. Status fails FormatException. Preserve typed failure validation, but the precise historical versioned wire shape may retire.

### 'retryability is valid only for an explicit read retry' (test; lines198–233; D)

Return retryable=true paired with retry_policy=never and recovery_action=none. Status must reject that contradictory envelope, rather than granting a mutation/model retry based on a boolean alone.

### 'failure stage and correlation cannot be rewritten by transport' (test; lines235–270; D)

Return a failure with conversation_turn stage and wrong correlation while requesting Vault status. Reject FormatException instead of rewriting or accepting foreign operation metadata. _Transport is test-only: it records creates, retains one pending operation, simulates submit/release loss, rejects conflicts and reports not_found after release.

## apps/client/test/app/runtime/app_read_model_test.dart

Full source read: lines1–131; SHA-256 `4325fac92da40ca827c94568c4cc3ffa060c9e532b01ad1d7b0d8786857dcb37`.

Current owner: AppReadModel client projection reducer. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'ack before event and event before ack reduce to one backend state' (test; lines28–86; D)

Bootstrap epoch7/cursor10, mark a command pending without mutating the prior projection, apply its receipt then ordered command/Run events11/12. Only one command remains, pending clears and an executing Run disables sending. In the reversed branch, apply command event before receipt and still retain one command. Acknowledgment/event ordering must converge without duplicate authority.

### 'stale snapshots are ignored and cursor gaps require resync' (test; lines88–112; D)

Bootstrap finished Run revision2/cursor20; applying stale executing revision1 is accepted as a no-op and cannot regress stored revision. Receiving cursor22 without21 returns false and marks resyncRequired, preventing gap-derived state from being treated as current.

### 'runtime epoch changes seal old projections' (test; lines114–130; D)

Bootstrap epoch7 commands/Run then require epoch8 resync. Old projections are cleared; an epoch7 receipt is rejected, commands remain empty and cursor stays epoch8. Epoch transitions seal stale runtime identity.

## apps/client/test/app/runtime/app_wire_transport_test.dart

Full source read: lines1–149; SHA-256 `f763e1354f407c28ba1a434160dd46640a5b8636997e759af07e22380212c9b8`.

Current owner: NativeTransport/FloeClient / admitted AppHost FFI boundary. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'real Dart/C ABI v2 binds host identity and rejects route injection' (test; lines9–148; D/H/O)

On macOS with debug FFI only, open a private profile whose device marker is mac-local. For each remotePairingV2 and remoteAccessV2 owner, a nonexistent result lookup returns not_found, while prepare with injected route/bearer returns validation. With no ready Vault, command lookup and prepared/direct StartTurn fail unavailable. First event read requires resync with positive epoch; reading after that cursor returns no events. Adding remote_route/bearer to otherwise valid StartTurn fails validation rather than unavailable. Preserve host-derived identity, route/credential exclusion and query-only state; historical v2 method names are replaceable. Teardown closes client and removes only this private profile; unsupported platform/dylib path explicitly skips.

## apps/client/test/app/runtime/floe_client_test.dart

Full source read: lines1–509; SHA-256 `81b82ceed4e96418009ab6985e6fe93db675e8d111b430531e3edc1b1819a4bc`.

Current owner: FloeClient AppWire translation and command identity. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'StartTurn uses Rust whitespace and transmits normalized text' (test; lines9–36; P/D)

Prepare StartTurn from NEL/tab-padded hello and submit through fake transport. Command text and transmitted text are hello, matching Rust whitespace normalization. Normalization consistency matters; precise accepted whitespace is a product contract choice.

### 'StartTurn preserves line feeds and rejects other internal controls' (test; lines38–64; P/D)

Embedded line feed in hello-newline-world is preserved. Three distinct input branches with tab, carriage return or vertical tab inside text each throw FormatException before transport. Inputs must have predictable cross-language validation; exact control policy may be reconsidered.

### 'StartTurn applies the normalized UTF-8 byte limit' (test; lines66–95; P/D)

2730 Korean three-byte characters plus ab (8192 UTF-8 bytes) is accepted unchanged; append c and preparation rejects; whitespace-only input also rejects. Limits apply after normalization by bytes rather than Dart character count. Exact8192 is historical product contract, bounded nonempty admission is durable.

### 'StartTurn keeps its command identity across transport retries' (test; lines97–142; D)

Prepare one command from queued IDs and submit it twice. Both receipts refer to the same command/Run while each transport request gets a fresh ID; payloads omit person_id, device_id and remote_route. Stable effect identity survives transport retry without letting clients select authority/routing.

### 'query decoding restores command, Run, report, and final message' (test; lines144–203; D/O)

Fake queries dispatch get_command→accepted receipt, get_run→finished epoch7 report and get_message→assistant done. Query them in that order and recover typed finished state, epoch7 and final text. It verifies current parser/lookup composition, not event-stream or durable backend execution.

### 'StartTurn serializes a bounded continuation reference' (test; lines205–241; D/O)

Prepare continuation of a specific Run with executor generation3/level2 and submit. Mode is continue with exactly those bounded reference fields, preserving lineage instead of creating an unrelated new turn. Concrete serialization belongs to the new canonical wire review.

### 'StartTurn serializes an explicit profile selection' (test; lines243–271; O)

Prepare with local-fast explicit profile and require profile.kind=explicit/profile_id=local-fast. Accepted product routing removes concrete profile selection; retire this representation rather than retaining it for compatibility.

### 'Run report preserves typed retry recovery metadata' (test; lines273–310; D)

Decode a finished failed Run whose issue metadata carries server_model_unavailable and retry_read. Both fields survive exactly, allowing caller recovery to follow owner policy instead of inferring permission from a generic unavailable code.

### 'StartTurn serializes explicit retry lineage' (test; lines312–354; D)

Prepare an explicit retry of a given source Run and require retry_of in the submitted payload. Attempting to combine retry lineage with a different continuation reference throws FormatException; mutually exclusive recovery modes cannot silently merge.

### 'CancelRun keeps command identity across transport retries' (test; lines356–383; D)

Prepare one CancelRun and submit twice. Both use the same command ID and user_requested reason and decode accepted outcomes. Cancellation is explicit user intent with stable identity across transport attempts.

### 'close settles a pending request once and rejects new work' (test; lines385–413; D)

Hold a command response, close client and require the pending future to fail StateError once; then complete the late transport response and require any new lookup to fail. Transport is closed, and a late acknowledgment cannot reopen/settle work a second time. This tests client shutdown, not implicit cancellation of backend Run.

### 'events require resync then enforce epoch revision and cursor order' (test; lines415–472; D/O)

Read events without cursor and receive resync-required(epoch7,cursor10); read after it and decode a single Run event at11/revision2 as cancelled finished state. Positive ordered example only: despite the test label, malformed epoch/revision/cursor rejection is not independently exercised here. FakeTransport callbacks and queued IDs are local harness, not live streaming.

## apps/client/test/app/runtime/local_context_gateway_test.dart

Full source read: lines1–96; SHA-256 `613cd06384bc94a309287c25473e8942a89ed67efc7ecc1f939aef36d53208bd`.

Current owner: NativeLocalContextGateway / App-owned Context command boundary. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'native commands derive identity in App while retaining request and epoch evidence' (test; lines7–59; D)

Register an attention host, complete an acquisition and dispose that host through context.apply. Top-level payload contains only schema/request/command IDs plus command. Completion removes caller-supplied person/device authority while retaining acquisition request, host epoch and before/after subject fingerprint; input map remains unchanged. A foreign-device completion is rejected locally before a fourth transport call.

### 'native polls use queries and reject stale Person or wrong command correlation' (test; lines61–95; D)

Poll acquisitions via context.read/poll_acquisitions query and accept an empty result for the expected person. Switch response person to foreign and reject; separately return a wrong command_id for host registration and reject. Polls cannot mutate or accept foreign identity/correlation.

## apps/client/test/app/runtime/native_transport_error_test.dart

Full source read: lines1–26; SHA-256 `dcb677c088ade091887755faf649f72d0fb84c96a70b49b3c2b563372a870e43`.

Current owner: NativeTransportException structured-error decoder. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'native transport preserves structured Rust error context' (test; lines5–25; D/O)

Decode a synthetic Rust error envelope with storage code/request_id field and vault_unavailable/request-1/conversation_turn metadata. Preserve every structured field in NativeTransportException. This is parser evidence only, not proof a live Rust error serializes this shape.

## apps/client/test/app/runtime/owner_operation_test.dart

Full source read: lines1–92; SHA-256 `c0cdb7a06253af8df4da27b8707a2c37e9871a50f1ae742b34f23d9ee3622b67`.

Current owner: OwnerOperationObserver observation identity/release lifecycle. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'observer timeout keeps the same operation without cancellation or redispatch' (test; lines7–55; D)

With observer timeout zero, start an exact-action owner operation that is unfinished. Observation throws deadline_exceeded, retains operation identity, performs one start and no release. Mark it done and observe the same intent again: read the original ID, return uncertain-write and release once without cancellation or redispatch. Observer deadline is not effect cancellation.

### 'decoded result survives a lost release acknowledgement and is never resubmitted' (test; lines57–91; D)

Start a completed action, decode once, then lose the first release acknowledgment; next release lookup returns native not_found. First observe throws, second returns the retained decoded uncertain-write result. Totals must be one start, one decode, two release attempts: lost response cannot repeat an external action.

## Shared support and owner mapping

All seven `test/support/*.dart` sources were read fully. Their hashes are in the source ledger. They are **H**, not production owners, and can only be removed after all importers in the aggregate client ledger are covered.

- `agent_gateway.dart`: TestAgentGateway creates/saves in-memory schema1 sessions, can fail/hang/resume, and recovers by clearing active-turn plus incrementing revision without a model call. Its _TestConversationRuntime bootstraps epoch7, appends user/assistant messages, supports held completions and synthetic typed failures, and increments stop only on explicit cancel. It does not emulate native durability or provider effects.
- `agent_vault_gateway.dart`: TestVaultGateway extends that fake with missing/ready/locked state, create/unlock/lock counters, a delayed resume gate and key-unavailable failure that requests reload+seal. It rejects creating an existing Vault. No real Keychain is used.
- `agent_proposal.dart`: shared UUID/delegation/media fixtures plus mutable proposal inspection response, delayed inspection and optional failure; vault_unavailable alone requests reload+seal. These fixtures are consumed by Actions proposal/controller/widget tests.
- `agent_registry.dart`: registry schema3 fixture, install/assignment toggles guarded by registry revision, candidate discovery with binding revision, selected-candidate replacement and explicit empty-selection path during discovery failure. It represents test inputs, not target registry authority.
- `app_wire_transport.dart`: command/query callback adapter funnels both to call; events deliberately throws UnimplementedError. ownerOperationId prefers query operation ID then command ID then request ID; ownerFailure builds synthetic versioned never-retry envelopes, setting reload/seal only for reopen_vault. Tests using it do not prove event streaming.
- `app_host.dart`: real FFI Day/Action composition and identity marker behavior is recorded in the native ledger; it drains Day before runtime close.
- `server_credentials.dart`: MemoryServerCredentials is a mutable in-memory string with read/write/delete. It cannot prove secure OS persistence, unlike the separate mocked Keychain/readback tests.

Current owner dependencies: Vault lifecycle tests target NativeVaultLifecycleGateway and AgentController presentation; AppReadModel owns client projection reduction; FloeClient/NativeTransport translate AppWire; OwnerOperationObserver tracks observation identity without effect ownership; NativeLocalContextGateway strips caller authority and translates Context intent. Target dispositions re-prove safety through canonical App/owner APIs after structural closure, retire v2/profile/recipient shapes, and reassess UI automatic-opening choices. No fake should be retained as a forwarding production path.
