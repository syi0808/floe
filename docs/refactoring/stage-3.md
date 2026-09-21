# Stage 3 — Product Boundary and Final Composition

**Status: active**

**Stage 3 entry baseline (historical):** `39263a1339d0519b78a20680c0eea73506ff6d6f` (`2-F`)

Stage 3 answers: **do the remaining production callers and product boundaries use the canonical owners that Stage 2 froze, without reconstructing execution topology outside those owners?**

Stage 1 ownership is complete. Stage 2 canonical internal runtime is complete and frozen. Stage 3 does not reopen either stage; it carries the frozen owner/runtime architecture through the remaining domain callers, App composition, product wire, Apple/native/server callers, compatibility deletion and final product validation.

This document is the Stage 3 progress source of truth. The linked step documents own the concrete scope for each checkpoint.

## Current checkpoint

**Active: 3-B — AppHost Composition Closure.**

Use [the 3-B execution plan](stage-3/3-b.md) for the next change set. No 3-B implementation is part of 3-A closure.

3-A is **complete · frozen**. Core convergence landed at `8bdd50628819119331c8c1b67307eaee8392bab9`; residual closure on execution baseline `beb6a624b0e912fe94faa1ea7628c72a96c8283b` deleted the caller-zero Schedule agent-turn runtime and its legacy model quarantine. Schedule now has only the canonical Expert endpoint model path. Owner regressions and proposal fixtures no longer require the deleted runtime; see [the residual closure report](stage-3/3-a.md#residual-closure-agent-report).

## Progress

- [x] **3-A — Remaining root/domain caller convergence** — [execution plan](stage-3/3-a.md) (complete · frozen; residual closure complete)
- [ ] **3-B — AppHost composition closure** — [execution plan](stage-3/3-b.md)
- [ ] **3-C — Protocol and FFI contract cutover** — [execution plan](stage-3/3-c.md)
- [ ] **3-D — Flutter, native and server caller cutover** — [execution plan](stage-3/3-d.md)
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

## Remaining Stage 3 debt after 3-A

General Conversation, delegated built-in Experts, the production Schedule endpoint and the isolated Knowledge Learner execute models through canonical Inference. Domain prompt, policy and budget semantics remain with their owners. `PreparedFoundationTransport` reaches the existing bundled native transport using the canonical attempt identity. These are closed 3-A boundaries, not remaining migration work.

The remaining debt belongs to 3-B through 3-F:

### App composition seams

General Conversation command admission is already intent-only, but App still contains:

- the `SavedConnectionSource`/HostSlot credential seam on `ConversationTurnRequest`;
- per-turn provider/source preparation such as `ServerSourceClient::prepare`;
- concrete VaultBridge reads from AppWire query/events;
- `AppHost::legacy_services()` used by old FFI paths.

3-B closes these without moving owner policy back into App.

### Product-wire duplication

The canonical Conversation product command already exists as
`AppCommandDto::ConversationStartTurn`.

The old AgentVault `ConversationTurn` DTO and `AgentRemoteRouteDto` remain. The latter mixes endpoint, bearer, model purpose/consent, pairing identity and source data and is still used by remote authority/pairing/grant product calls.

3-C separates connection/pairing intent from model execution topology. 3-D migrates the real callers. 3-E deletes the old shapes only after caller count reaches zero.

### Live compatibility and Apple delivery

The separate AgentFixture runtime in `crates/app/src/agent_fixture.rs`, driven through the vault host, remains live product-boundary debt for 3-D/3-E. Its Conversation legacy runtime and provider compatibility support are not retained for Schedule tests and are not part of the completed 3-A residual.

Canonical Foundation transport is implemented; packaged macOS, iPhone and iPad caller/bindings/dylib validation remains 3-D/3-F work. Build these artifacts from the same source snapshot and use an explicitly selected fresh development profile when stored meaning changes.

### Validation-harness debt already known

Before 3-F can be a trustworthy gate, repair the stale validation entry points discovered on this baseline:

- `tools/validation/check-local-model.sh` still invokes removed package `floe-infra`;
- `apps/client/integration/local_server_pairing_test.dart` still imports removed
  `features/server/local_server_client.dart` rather than the current Connections path.

These are validation-harness debt, not reasons to weaken architecture or skip product evidence.

## Execution order

~~~text
3-A  domain execution convergence (complete · frozen)
  A0 canonical Foundation prepared transport
  A1 delegated built-in Expert execution
  A2 Schedule / Calendar execution
  A3 Knowledge Learner inference
  A4 closure / caller proof
  R0-R4 caller-zero Schedule residual closure (complete)

3-B  AppHost composition closure

3-C  protocol / FFI product-contract cutover

3-D  Flutter / native / server real-caller cutover

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
