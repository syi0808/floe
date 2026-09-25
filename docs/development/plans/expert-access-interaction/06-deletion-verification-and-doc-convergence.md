# Checkpoint 06 — final deletion, verification and documentation convergence

- **Status:** active execution plan.
- **Source baseline:** c137b35bf44ea12b1d2f5aa57b2f86aa6d0f1761 (Checkpoint 05-G completion).
- **Prerequisite:** Checkpoints 01–05 complete. Do not reopen their deleted runtime paths.
- **Goal:** remove the last transition-only provenance/model-consent surfaces, narrow obsolete public/test APIs, prove the final topology, converge durable docs, and remove this temporary execution-plan directory from the active tree.
- **Next execution:** 06-A.
- **Final state:** no Checkpoint 07. After 06-D, current source, architecture, product docs and accepted ADRs are authoritative; Git history retains this plan.

Execute in order:

1. [06-A — recorded provenance and budget-continuation convergence](06-a-recorded-provenance-and-continuation.md)
2. [06-B — legacy model transport and saved-consent deletion](06-b-legacy-inference-and-consent-deletion.md)
3. [06-C — residual public surface and durable docs convergence](06-c-residual-surface-and-doc-convergence.md)
4. [06-D — final verification and execution-plan archive](06-d-final-verification-and-plan-archive.md)

## 1. Verified remaining architecture

Checkpoint 05 completed the intended product flow, but a current-code audit found two real transition designs.

### 1.1 Source-history message heuristic still participates in production

Canonical history already uses recorded DependencyCoverage:

~~~text
recorded coverage
  -> EvidenceReader
  -> DependencyResolver
  -> Context history projection
~~~

But production still also carries:

~~~text
SourceHistoryBoundary
  -> classify successful Capability / completed Delegation by message shape
  -> conservatively reject budget continuation
~~~

Baseline anchors:

- crates/contracts/agent/src/expert_model.rs — public SourceHistoryBoundary.
- crates/modules/conversation/src/turn/source_history.rs — ConservativeSourceHistoryBoundary, carries_source_history, bounded_source_history_start.
- crates/modules/conversation/src/application/admission.rs — TurnPreparationRequest.boundary and the heuristic continuation rejection.
- crates/app/src/vault_host/conversation_turn.rs — injects ConservativeSourceHistoryBoundary.
- crates/modules/conversation/src/application/history_projection.rs — narrow_by_source_boundary remains but current search finds no production caller.

Do not simply delete this fallback. Budget continuation can execute a validated pending batch without recalling the model.

The required replacement already exists in durable data: ValidatedModelBatch.projection_coverage is the exact DependencyCoverage of the AuthorizedModelProjection that produced the stored steps. 06-A reauthorizes that coverage before pending-step execution and again before terminal output release, then deletes the heuristic layer.

### 1.2 Legacy model transport and saved global-consent shape remain

Canonical production model flow:

~~~text
ModelProvider
 -> PreparedModelProfile
 -> Access admit / consume
 -> PreparedModelTransport
 -> provider
~~~

Caller audit still finds a legacy branch used by smoke/tests:

- ModelTransport / ModelTransportRequest / ModelTransportResponse;
- FoundationModelRunner;
- ServerModelRunner;
- ModelRouteConfig;
- RemoteRoute / RoutePairing;
- LEGACY_INFERENCE_CONSUMER.

These survive only because examples/tests still call them. The canonical architecture docs already describe one provider path, so 06-B migrates those callers and deletes the branch.

Checkpoint 05 also moved exact-recipient consent to Access-owned contextual consent, but the saved connection shape still persists allow_external and external_recipients. SavedConnectionAdmission explicitly ignores them as product authority, so they are transition state.

The same spelling on the Go/server inference request has different semantics: request-scoped external-transfer safety. Keep that fence. After 06-B, its value comes only from the exact Access-consumed dispatch target, never saved connection state or Flutter.

### 1.3 Bounded caller-zero/public cleanup remains

Known candidates:
- calendar_first_party_consumers wrapper around first_party_observe policy;
- set_calendar_observe production helper currently found through tests;
- other pub/test helpers left only for completed checkpoint wiring.

Delete only after caller-zero proof. Keep low-level owner errors such as AccessReviewRequired when still semantically real. Keep old-wire rejection fixtures when they prove removed APIs fail closed.

## 2. Fixed final architecture

### Provenance

There is one answer to whether derived history or a pending validated batch may still be used:

~~~text
recorded DependencyCoverage
 -> current DependencyResolver
 -> exact dependency reauthorization
~~~

Message type, capability id, Expert id or a string table never decides source authority.

Budget continuation:
- reuses the exact validated batch and cursor;
- does not recall the model;
- may execute only while batch.projection_coverage reauthorizes;
- rechecks the same coverage before terminal output release.

Independent remains independent. Unknown never authorizes derived replay.

### External model recipient

There is one product authority path:

~~~text
selected candidate
 -> Access exact-recipient contextual authority
 -> admit
 -> consume
 -> request-scoped admitted transport target
 -> PreparedModelTransport
 -> provider
 -> Access post-response revalidation
~~~

Saved pairing/connection state carries endpoint, credential and pairing identity only. It carries no recipient approval.

Server transport maps the consumed target:

~~~text
External(exact recipient)
  -> allow_external = true
  -> expected_recipient = exact recipient

Device/server-local
  -> allow_external = false
  -> no expected recipient
~~~

### Documentation

After 06-D:
- current docs describe only current owners and paths;
- accepted ADRs preserve rationale, not checkpoint status;
- product docs describe current UX;
- docs/development/plans/expert-access-interaction is absent from the active tree;
- Git history is the archive.

## 3. Global invariants

1. Do not weaken dependency, grant, policy, recipient or source reauthorization to delete a fallback.
2. Do not ask the model again instead of resuming a valid pending batch.
3. Do not execute a pending batch step after its recorded projection coverage becomes stale.
4. Do not release a resumed answer after coverage revocation racing with execution.
5. DependencyCoverage::Unknown is never Independent.
6. Do not reintroduce saved/global recipient consent or a Settings toggle.
7. Keep the Go/server request-scoped allow_external fence.
8. Do not add dual saved-credential decoders or migration objects solely for disposable local state.
9. Do not add a second model transport abstraction.
10. Observe, Act and external processing remain independent.
11. Preserve Checkpoint 05 interaction CAS, crash recovery, one linked child, and action uncertainty recovery.
12. No Vault transaction spans model/provider/native I/O.
13. No Android parity expansion and no iOS validation requirement unless separately requested.
14. Do not archive this plan until all code/docs/gates are green.

## 4. Stop conditions

Stop and update the active child if implementation appears to require:
- a duplicate provenance field when projection_coverage is sufficient;
- turning budget continuation into a fresh model call;
- classifying source history by capability/Expert id;
- a provider bypass around Access consume/revalidation;
- keeping saved allow_external/recipient lists as authority for compatibility;
- deleting the server request fence;
- converting all PolicyDenied/AccessReviewRequired errors to interactions;
- deleting rejection tests that are the only proof removed wire fails closed;
- deleting the plan directory before final verification.

## 5. Definition of done

- [ ] continuation reauthorizes exact ValidatedModelBatch.projection_coverage before pending-step execution.
- [ ] resumed terminal output rechecks the exact batch coverage before release.
- [ ] Independent continuation works; stale/Unknown dependent continuation fails closed.
- [ ] SourceHistoryBoundary, ConservativeSourceHistoryBoundary, source_history.rs, narrow_by_source_boundary, carries_source_history, bounded_source_history_start and App boundary wiring are gone.
- [ ] canonical history remains recorded-coverage based.
- [ ] legacy ModelTransport port/request/response and Runner implementations are gone.
- [ ] smoke/provider tests use ModelProvider / PreparedModelTransport or InferenceService.
- [ ] ModelRouteConfig, RemoteRoute, RoutePairing and LEGACY_INFERENCE_CONSUMER are gone after canonical values move.
- [ ] saved server connection no longer persists allow_external / external_recipients.
- [ ] Flutter ServerConnection has no allowExternal/externalRecipients/withExternalConsent/coversExternalRecipient.
- [ ] external server inference receives request-scoped external permission only after Access consume.
- [ ] server-local/device inference never receives external approval.
- [ ] caller-zero transitional helpers/exports are deleted or justified.
- [ ] current architecture/product/ADR docs match source.
- [ ] full applicable Rust/architecture/FFI/Flutter/macOS gates pass.
- [ ] Go gates pass if server code changes.
- [ ] ignored/live credential tests are reported honestly.
- [ ] no active docs link points at the execution-plan directory.
- [ ] the entire execution-plan directory is deleted in the final archive commit.
- [ ] no replacement status/checkpoint ledger is created.

## 6. Required final report

After 06-D report:
1. 06-A semantic SHA(s).
2. 06-B semantic SHA(s).
3. 06-C semantic SHA(s).
4. 06-D verification and final archive SHA(s).
5. final owner/runtime topology.
6. continuation provenance evidence.
7. recipient/transport evidence.
8. deleted public/runtime surfaces.
9. residual searches and justified fixtures.
10. exact verification commands/results/skips/ignored tests.
11. durable docs/ADR changes.
12. proof the execution-plan directory is absent.
13. remaining blocker.

Do not leave a Checkpoint 06 complete status file. The archive commit and Git history are the completion record.
