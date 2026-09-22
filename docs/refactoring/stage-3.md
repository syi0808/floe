# Stage 3 — Product Boundary and Final Composition

**Status: active**

**Stage 3 entry baseline (historical):** `39263a1339d0519b78a20680c0eea73506ff6d6f` (`2-F`)

Stage 3 answers: **do the remaining production callers and product boundaries use the canonical owners that Stage 2 froze, without reconstructing execution topology outside those owners?**

Stage 1 ownership is complete. Stage 2 canonical internal runtime is complete and frozen. Stage 3 does not reopen either stage; it carries the frozen owner/runtime architecture through the remaining domain callers, App composition, product wire, Apple/native/server callers, compatibility deletion and final product validation.

This document is the Stage 3 progress source of truth. The linked step documents own the concrete scope for each checkpoint.

## Current checkpoint

**Active: 3-E — Outer Compatibility Deletion.**

3-E is activated as the next checkpoint; its implementation has not started. Use [the 3-E execution plan](stage-3/3-e.md) for the next change set. 3-D is **complete · frozen**: its [Residual closure Agent report](stage-3/3-d.md#residual-closure-agent-report) records coherent Go owner extraction, real Flutter pairing, protected Rust Access success and revocation failure through a test-owned current saved-connection store, and all final gates on source snapshot `864a39fbd5e5205b4afc5de993c9c4db822716a6`. 3-C remains **complete · frozen**; see [its Agent report and exact Flutter handoff](stage-3/3-c.md#agent-report). No 3-D implementation is part of 3-C closure. 3-B remains **complete · frozen**; see [its Agent report](stage-3/3-b.md#agent-report).

3-A is **complete · frozen**. Core convergence landed at `8bdd50628819119331c8c1b67307eaee8392bab9`; residual closure on execution baseline `beb6a624b0e912fe94faa1ea7628c72a96c8283b` deleted the caller-zero Schedule agent-turn runtime and its legacy model quarantine. Schedule now has only the canonical Expert endpoint model path. Owner regressions and proposal fixtures no longer require the deleted runtime; see [the residual closure report](stage-3/3-a.md#residual-closure-agent-report).

## Progress

- [x] **3-A — Remaining root/domain caller convergence** — [execution plan](stage-3/3-a.md) (complete · frozen; residual closure complete)
- [x] **3-B — AppHost composition closure** — [execution plan](stage-3/3-b.md) (complete · frozen)
- [x] **3-C — Protocol and FFI contract cutover** — [execution plan](stage-3/3-c.md) (complete · frozen)
- [x] **3-D — Flutter, native and server caller cutover** — [execution plan](stage-3/3-d.md) (complete · frozen; residual closure complete)
- [ ] **3-E — Outer compatibility deletion** — [execution plan](stage-3/3-e.md)
- [ ] **3-F — End-to-end product validation** — [execution plan](stage-3/3-f.md)

## Fixed Stage 3 ownership rule

Product callers submit **user or connection intent**, not model/source execution topology.

Allowed outer intent includes:

- session/run/command identity;
- command text;
- continuation and retry intent;
- explicit user-selected model profile where the product actually exposes one;
- connection, pairing, grant or review intent stated in the owning domain's terms;
- source/provider identity fields that are genuinely part of a connection or reviewed resource contract.

Do not expose as product execution input:

- raw model endpoint/base URL;
- model bearer/token;
- a pre-resolved Foundation-versus-Server choice;
- a combined model route bundle carrying pairing/source state;
- source connector catalog as model routing input;
- internal Access permit/recipient/consent flags;
- provider-specific retry, usage or placement policy.

A connection or pairing flow may still need to identify the connection it is operating on. That must be expressed as Connections/Access intent or a stable reference, not by reusing an Inference model-route DTO.

## Canonical product path

~~~text
Flutter
  → protocol / FFI conversion
  → AppHost request admission
  → typed owner services
  → Stage 2 canonical runtime
       Conversation / Experts / Context / Access / Inference / Execution
  → provider / native / server adapters
~~~

Outer layers translate intent, provide concrete adapter implementations and present results. They do not regain business ownership.

## Remaining Stage 3 product-boundary debt

General Conversation, delegated built-in Experts, the production Schedule endpoint and the isolated Knowledge Learner execute models through canonical Inference. Domain prompt, policy and budget semantics remain with their owners. `PreparedFoundationTransport` reaches the existing bundled native transport using the canonical attempt identity. These are closed 3-A boundaries, not remaining migration work.

The remaining debt belongs to 3-E and 3-F; the closed product boundaries are recorded here for ownership context:

### Closed canonical App composition

3-B closed the request credential seam, root/Expert/Schedule current-connection composition, owner-derived Expert availability and canonical AppWire query/event service cutover. App constructs opaque provider capabilities through a shared host-scoped store; it no longer reads raw saved credentials or interprets model profiles on the canonical runtime path. See [the 3-B report](stage-3/3-b.md#agent-report) for the implementation and validation evidence.

Concrete AppComposition getters and `AppHost::legacy_services()` remain only for live old ABI callers. Their remaining caller cutover and eventual caller-zero deletion belong to 3-D/3-E.

### Closed Rust protocol and FFI contracts

Canonical Conversation remains on `command_v2/query_v2/events_v2` and `AppCommandDto::ConversationStartTurn`. 3-C proved the old AgentVault ConversationTurn sender count was zero and deleted that wire path without removing internal turn machinery or live ConversationSession callers.

Pairing and remote Access now have separate typed Rust protocol/FFI services admitted through AppHost's verified caller identity. Pairing accepts only setup endpoint/evidence; protected authority and Calendar/View grant operations prepare transports from the current saved connection inside provider adapters. Rust `AgentRemoteRouteDto`, `RemoteTurnRoute` and old remote AgentVault route operations are deleted, not compatibility aliases.

3-D has moved Flutter production pairing and protected Access callers onto these owner APIs. Pairing accepts setup intent, protected Access receives no saved connection route, and approved pairing results remain retained until secure persistence succeeds. The frozen [3-C report](stage-3/3-c.md#agent-report) remains historical handoff evidence, not the current Flutter caller inventory. 3-E deletes the remaining unrelated compatibility only after its own caller-zero proof.

### Live compatibility and Apple delivery

The separate AgentFixture runtime in `crates/app/src/agent_fixture.rs`, driven through the vault host, now has no production Flutter caller. Explicit Rust/Flutter test helpers still use its ABI; final Rust compatibility deletion remains 3-E work after 3-D closure. Production Conversation uses fixture-neutral shared models and has no synthetic fallback. Its remaining unrelated AgentVault session/management callers are classified in the 3-D report. This compatibility is not retained for Schedule tests and is not part of the completed 3-A residual.

Canonical Foundation transport is implemented. 3-D has verified the macOS release bundle, both dylib loads, supported local-model smoke and universal iOS simulator packaging. Physical-device FoundationModels execution is not inferred from simulator packaging. Build artifacts from the same source snapshot and use an explicitly selected fresh development profile when stored meaning changes.

### Closed validation-harness repair

3-D repaired the stale validation entry points discovered on the baseline:

- `tools/validation/check-local-model.sh` validates current provider-adapter and Inference packages; the smoke script uses the actual `floe-app` example;
- `apps/client/integration/local_server_pairing_test.dart` uses current Connections imports and the real native pairing owner gateway, with explicitly memory-only test credential persistence.

3-D residual closure repaired the Go extraction without owner → application dependencies or duplicate Console/state aliases. Go race/vet/build and isolated Keychain smoke pass. The Flutter integration completes strict native pairing, exact memory-store persistence, release and server revocation; the separate provider integration proves canonical protected Rust Access succeeds through the exact paired current saved connection and fails closed after server revocation. Neither test overwrites the shared saved-connection Keychain slot. Final Rust, architecture, Flutter, macOS, iOS simulator, native and supported local-model gates pass from the recorded source snapshot. 3-E remains a separate, not-yet-started implementation checkpoint.

## Execution order

~~~text
3-A  domain execution convergence (complete · frozen)
  A0 canonical Foundation prepared transport
  A1 delegated built-in Expert execution
  A2 Schedule / Calendar execution
  A3 Knowledge Learner inference
  A4 closure / caller proof
  R0-R4 caller-zero Schedule residual closure (complete)

3-B  AppHost composition closure (complete · frozen)

3-C  protocol / FFI product-contract cutover (complete · frozen)

3-D  Flutter / native / server real-caller cutover (complete · frozen)

3-E  delete caller-zero compatibility

3-F  final Apple-first product validation
~~~

Do not skip directly to 3-E because a symbol is named `legacy`, `compat`, `RemoteRoute` or `v2`. Caller ownership and the final product contract determine deletion.

## Stage 3 exit gate

Stage 3 is complete only when all are true:

- General Conversation and delegated Expert execution run through canonical owners;
- Schedule/Calendar no longer owns a parallel model runtime;
- Knowledge Learner retains isolated learning policy while delegating model execution ownership to Inference;
- AppHost/App constructs and injects concrete services but performs no model route, source authorization, grant or recipient policy;
- FFI/product wire expresses owner intent rather than provider/model topology;
- Flutter does not construct internal model route/bearer bundles for Rust execution;
- raw model credentials do not cross product protocol boundaries;
- canonical Foundation prepared transport reaches the supported bundled native model;
- source connector discovery remains separate from model profile discovery;
- caller-zero outer/runtime compatibility is deleted rather than renamed;
- Apple-focused end-to-end, exact-recipient revocation, durable recovery and uncertain external-write scenarios are validated;
- final workspace, architecture, Flutter, Apple/native and relevant server checks are green from one recorded source snapshot.
