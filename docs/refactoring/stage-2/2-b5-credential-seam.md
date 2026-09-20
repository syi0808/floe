# 2-B.5 Final P1 — Production-Safe Saved Connection Injection

**Status: complete**

**Code baseline reviewed:** `16f7374461a2c4bc12e440f62f8107bc8a3f035f`

This document is the authoritative execution plan for the final residual 2-B.5 P1.

The integrated 2-B.5 audit is otherwise complete/frozen. Do not re-run a broad 2-B audit and do not start 2-C in this change set.

## Finding

2-B.5 correctly removed the runtime environment-variable seam that changed production keychain lookup semantics. Its replacement, however, is still production-reachable:

```rust
pub enum TurnSavedConnection {
    HostSlot,
    Fixed(Option<floe_inference::SavedServerConnection>),
}
```

and Floe App publicly re-exports it:

```rust
pub use turn_request::{
    ConversationTurnRequest,
    RemoteTurnRoute,
    TurnSavedConnection,
};
```

The worker API is also public:

```text
AppComposition::agent_vault()
  → VaultBridge::request(...)
  → WorkerOperation::Submit
  → WorkerAction::ConversationTurn
  → ConversationTurnRequest
  → TurnSavedConnection::Fixed(...)
```

Therefore a production Rust caller can bypass the host credential slot and provide an arbitrary fixed saved connection/consent source to the General Conversation runtime.

The normal ConversationCommands and FFI paths use HostSlot, so this is not an observed end-user disclosure P0. It is a P1 because the test injection seam remains reachable in production and can alter credential/current-authority semantics.

## Goal

Production General Conversation must have exactly one saved-connection source:

```text
host credential slot
  → current saved connection
  → verified person/device admission
  → current recipient authority re-read
```

Fixed saved-connection injection may exist only in app-crate test builds.

Required production invariant:

```text
external Rust caller
  → ConversationTurnRequest
  → cannot choose credential source
  → cannot inject SavedServerConnection
  → cannot select Fixed authority store
```

Required test invariant:

```text
#[cfg(test)] app tests
  → may inject fixed Some(connection) or fixed None
  → no ambient Keychain dependency
  → same Access admit/consume/revalidate semantics
```

## Scope

### In scope

- remove `TurnSavedConnection` from Floe App's public API;
- make production `ConversationTurnRequest` creation host-slot-only;
- keep fixed saved-connection injection compile-time test-only;
- migrate App composition and FFI conversion to the same production constructor;
- migrate App unit tests to a test-only constructor/helper;
- keep 2-B.2 current-authority semantics unchanged;
- restore 2-B complete/frozen after focused and workspace validation.

### Out of scope

Do not:

- re-audit 2-B.1 through 2-B.4;
- change Inference routing/fallback;
- change Access dispatch fences;
- remove `LegacyDelegationPort` or staged endpoint context — 2-C;
- clean dead/public compatibility surfaces — 2-D/2-E;
- delete `AgentRemoteRouteDto` — Stage 3;
- wire the canonical Foundation native transport in this change set.

The canonical `PreparedFoundationTransport` still returning `LocalModelUnavailable` is a separate product/native integration gap. It must be handled in Stage 3 native/product convergence and final product validation, not by widening this residual P1.

## Fixed design

### 1. Credential-source choice is not a public turn contract

`ConversationTurnRequest` may remain a public worker value, but external production callers must not be able to select its credential source.

Preferred shape:

```rust
pub struct ConversationTurnRequest {
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub profile: ProfileSelection,
    pub continuation: bool,
    pub retry_of: Option<RunId>,
    pub device_id: String,

    saved_server_connection: SavedConnectionSource,
}

enum SavedConnectionSource {
    HostSlot,
    #[cfg(test)]
    Fixed(Option<floe_inference::SavedServerConnection>),
}
```

Equivalent private naming is acceptable.

Do not expose `SavedConnectionSource` or `TurnSavedConnection` through `floe_app::lib.rs`.

### 2. Add one production constructor

Provide a public constructor/factory for legitimate production turn creation, for example:

```rust
ConversationTurnRequest::new(
    session_id,
    expected_revision,
    text,
    device_id,
    profile,
    continuation,
    retry_of,
)
```

It must always bind:

```text
saved connection source = HostSlot
```

The exact argument order/name may differ.

Both:

- `AppComposition::start_turn`;
- FFI `conversation_turn_request(...)`

must use this production constructor instead of a struct literal that can name credential state.

Do not add a public optional credential parameter.

### 3. Fixed injection is compile-time test-only

App crate unit tests may use a crate-private helper under `#[cfg(test)]`, for example:

```rust
#[cfg(test)]
pub(crate) fn with_fixed_saved_connection_for_test(
    self,
    saved: Option<SavedServerConnection>,
) -> Self
```

or a dedicated `fixed_for_test(...)` constructor.

Requirements:

- helper not compiled into the normal library;
- `Fixed` variant not compiled into the normal library;
- helper not public outside the App crate;
- tests may inject both `Some(saved)` and `None`;
- no environment variable controls credential behavior.

Do not introduce a Cargo feature that production callers can enable to regain arbitrary credential injection.

### 4. Internal model/source composition may read the private source

The following internal composition points may inspect the private source:

- `ConversationTurnRequest::stored_server_connection()`;
- `root_model_provider`;
- `root_recipient_authority`;
- source-client preparation;
- staged legacy Expert compatibility until 2-C/3-A.

Their public signatures must not force re-exposure of the private credential-source type.

If necessary, make `HostInferenceRoutes::root_model_provider` and `root_recipient_authority` crate-private/internal helpers, or pass an already-resolved internal store abstraction.

Do not perform broad visibility cleanup unrelated to hiding this seam.

### 5. Production current-authority behavior remains current

For HostSlot:

```text
Access admit
  → current host slot
Access consume
  → current host slot again
provider
Access post-response revalidate
  → current host slot again
```

Do not replace current authority with a one-time loaded snapshot while removing the public seam.

For test Fixed storage, retain the per-check store shape so the existing revocation regressions can still mutate/reload deterministic test authority where needed.

### 6. Worker public surface cannot reintroduce injection

After the change, this public path:

```text
VaultBridge::request
→ WorkerAction::ConversationTurn
→ ConversationTurnRequest
```

must not provide a way to construct a turn with arbitrary endpoint/token/consent state.

It is acceptable for `WorkerAction::ConversationTurn` and `ConversationTurnRequest` to remain public for now; 2-E owns broader public-surface cleanup.

The security gate is that the public request can express turn intent, not credential source.

## Execution sequence

## R1 — Hide the credential-source type

Primary files:

- `crates/app/src/turn_request.rs`
- `crates/app/src/lib.rs`

Changes:

- [ ] replace public `TurnSavedConnection` with a private/internal source type;
- [ ] compile `Fixed` only under `#[cfg(test)]`;
- [ ] remove `TurnSavedConnection` from `floe_app` public re-exports;
- [ ] make the saved-connection source field private;
- [ ] add one host-slot-only production constructor;
- [ ] add a crate-private `#[cfg(test)]` fixed-injection helper.

Static gates:

```text
public floe_app::TurnSavedConnection = absent
production Fixed variant             = absent
public ConversationTurnRequest credential/source parameter = absent
```

## R2 — Migrate production callers

Primary files:

- `crates/app/src/composition.rs`
- `crates/bindings/ffi/src/conversion/worker.rs`
- `crates/app/src/inference_routes.rs`
- `crates/app/src/vault_host/conversation_turn.rs` only as required by private access

Changes:

- [ ] `AppComposition::start_turn` uses the production constructor;
- [ ] FFI conversion uses the production constructor;
- [ ] neither caller imports/constructs a saved connection source;
- [ ] internal provider/authority helpers no longer expose the hidden source type publicly;
- [ ] FFI legacy `remote_route` rejection remains unchanged.

Required regression:

- [ ] normal App/FFI turn construction always resolves through HostSlot semantics.

## R3 — Migrate hermetic tests

Primary files:

- `crates/app/src/vault_host/tests/calendar_experts.rs`
- App unit tests that currently name `TurnSavedConnection::Fixed`

Changes:

- [ ] replace direct public Fixed construction with the crate-private test helper;
- [ ] keep fixed Some(saved) server-model tests hermetic;
- [ ] keep fixed None local-only tests hermetic;
- [ ] no test reads ambient Keychain unless explicitly testing Keychain integration;
- [ ] no `FLOE_TEST_EMPTY_KEYCHAIN` or replacement runtime environment seam.

Required regressions:

- [ ] fixed absent credential produces deterministic local-only test behavior;
- [ ] fixed saved credential still exercises canonical server-model path;
- [ ] current-recipient revoke-before-handoff and revoke-after-handoff regressions remain green.

## R4 — Prove production reachability is closed

Inspect the real public path, not grep alone.

Required proof:

```text
external crate
  can construct ConversationTurnRequest only through host-slot production API
  cannot name Fixed
  cannot supply SavedServerConnection
  cannot choose bearer/base URL/recipient/consent
```

At minimum verify:

- [ ] `floe_app::TurnSavedConnection` no longer exists;
- [ ] `ConversationTurnRequest` public construction has no credential parameter;
- [ ] `WorkerAction::ConversationTurn` cannot carry caller-selected credential source;
- [ ] FFI conversion creates only the production request shape;
- [ ] normal `ConversationCommands` path creates only the production request shape.

Do not add a runtime assertion as a substitute for compile-time inaccessibility.

## R5 — Validation and 2-B closure

Focused:

```sh
cargo check -p floe-app --lib
cargo test -p floe-app --lib
cargo test -p floe-ffi
cargo test -p floe-inference
cargo test -p floe-provider-adapters
cargo test -p floe-access
```

Closure:

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
```

The known mock-socket `WouldBlock` flake remains non-blocking only if it reproduces on the unchanged baseline; do not weaken the harness to hide it.

## Completion conditions

This residual P1 is complete only when:

1. production build has no `Fixed` saved-connection source;
2. `TurnSavedConnection` is not part of Floe App public API;
3. external production callers cannot inject `SavedServerConnection` through a Conversation turn;
4. App composition and FFI construct the same HostSlot-only production request;
5. tests retain deterministic fixed Some/None injection under compile-time test gating;
6. current recipient authority still re-reads its store at admit/consume/revalidate;
7. all focused/workspace/architecture checks are green.

## Documentation update on completion

When all conditions are green:

- mark this document `Status: complete`;
- restore `docs/refactoring/stage-2/2-b5.md` to `Status: complete / frozen`;
- mark the residual P1 checkbox complete in `docs/refactoring/stage-2/2-b.md`;
- mark 2-B and 2-B.5 complete again in `docs/refactoring/stage-2.md`;
- restore Stage 2 Current checkpoint to **2-C — Delegation ownership convergence**;
- stop.

Do not implement 2-C in the same change set.

## Agent report format

Report only:

1. changed files/symbols;
2. final production `ConversationTurnRequest` construction API;
3. proof that Fixed credential injection is compile-time test-only;
4. proof that the public Worker/Vault path cannot inject saved credentials;
5. current-authority admit/consume/revalidate behavior after the change;
6. focused/workspace test results;
7. architecture check;
8. remaining 2-B P0/P1, if any.

No PR. Do not create another planning/status document.
