# 2-B.5 Final P0 — Close Public Saved-Credential Read

**Status: active — final 2-B blocker**

**Code baseline reviewed:** 526e9593ef670796acbd4141a882938b2779ca06

This is the authoritative plan for the final residual 2-B P0.

The broad 2-B.5 audit and the fixed-credential injection P1 are already complete/frozen. Do not reopen them. Do not start 2-C in this change set.

## Finding

ConversationTurnRequest is intentionally public, but it currently exposes:

    pub fn stored_server_connection(
        &self,
    ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure>

Production ConversationTurnRequest::new(...) always binds the host saved-connection slot.

Therefore an external Rust caller can currently do:

    ConversationTurnRequest::new(...)
      → stored_server_connection()
      → host credential/keychain read
      → SavedServerConnection
      → token / base_url / consent fields

SavedServerConnection contains a public raw token field.

This violates the 2-B secret-boundary invariant:

    raw model credential outside provider/credential boundary = 0

This is a P0 because the App public API can disclose a stored credential, even though injection is already closed.

## Goal

Production callers may express turn intent but must not be able to read the saved credential used by internal model/source composition.

Required boundary:

    external crate
      → ConversationTurnRequest::new(...)      allowed
      → request.stored_server_connection()     impossible at compile time
      → request.saved_server_connection field  impossible
      → SavedConnectionSource                  impossible

Internal App composition may still read the saved connection where required:

    vault_host
    conversation_turn
    staged legacy Expert compatibility

Those reads remain crate-internal and do not change current Access authority semantics.

## Scope

### In scope

- hide ConversationTurnRequest::stored_server_connection from external crates;
- prove the public Worker/Vault path cannot read or inject raw saved credentials;
- keep existing internal App callers working;
- preserve fixed credential injection under App-only compile-time tests;
- preserve current-recipient admit/consume/post-response revalidation;
- restore 2-B complete/frozen after validation.

### Out of scope

Do not:

- change SavedServerConnection ownership/type placement;
- make broad Inference public-surface cleanup;
- remove RemoteRoute / RemoteTurnRoute / AgentRemoteRouteDto;
- re-audit 2-B.1 through 2-B.4;
- re-run the broad B5 audit except required regression validation;
- remove LegacyDelegationPort or staged endpoint context;
- wire canonical Foundation native transport;
- start 2-C.

SavedServerConnection and other migration/public compatibility types remain assigned to 2-D/2-E/Stage 3 unless they cross this canonical App boundary.

## P0-A — Close the public read

Primary file:

- crates/app/src/turn_request.rs

Required change:

    pub fn stored_server_connection(...)
        ↓
    pub(crate) fn stored_server_connection(...)

Equivalent private visibility is acceptable if all internal callers remain clean.

Do not change the semantics of the method.

HostSlot must still call the host credential store.

Under cfg(test), Fixed must still return the injected Some/None value without touching ambient Keychain state.

### Static gate

Normal library build must expose no public method on ConversationTurnRequest that returns:

- SavedServerConnection;
- token/bearer;
- base_url/endpoint;
- credential-source state.

Do not replace the method with another public getter returning an equivalent secret-bearing shape.

## P0-B — Preserve internal composition only

Inspect all actual callers.

Expected internal callers:

- crates/app/src/vault_host.rs
- crates/app/src/vault_host/conversation_turn.rs
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs

They may continue calling the crate-private method.

No call site should require widening visibility.

Do not move credential reads into Conversation, Context, Access, or protocol/FFI.

## P0-C — Compile-time public API proof

Create an external temporary probe crate against the normal floe-app library build.

Required positive proof:

    let request = floe_app::ConversationTurnRequest::new(...);

must compile.

Required negative proof:

    request.stored_server_connection();

must fail to compile because the method is private.

Also keep the already-proven negative gates:

- floe_app::SavedConnectionSource inaccessible;
- saved_server_connection field inaccessible;
- no TurnSavedConnection public type;
- no Fixed production variant.

This probe is validation evidence only. Do not add another production helper solely for the probe.

## P0-D — Security regressions

Re-run focused tests proving the visibility change did not alter credential/authority behavior.

Required:

- production constructor still binds HostSlot;
- App test Fixed(Some) remains hermetic;
- App test Fixed(None) remains hermetic;
- root explicit server model still uses the saved connection internally;
- unconsented external recipient still denies before provider POST;
- revoke-before-handoff still produces provider calls 0;
- revoke-after-handoff still suppresses content and keeps usage;
- FFI turn conversion still uses ConversationTurnRequest::new and rejects legacy remote_route.

The current authority sequence must remain:

    Access admit
      → current saved-connection store read
    Access consume
      → current saved-connection store read
    provider handoff
    Access post-response revalidate
      → current saved-connection store read

Do not replace the current store with a construction-time snapshot.

## P0-E — Validation and final 2-B closure

Focused:

    cargo check -p floe-app --lib
    cargo test -p floe-app --lib
    cargo test -p floe-ffi
    cargo test -p floe-inference
    cargo test -p floe-provider-adapters
    cargo test -p floe-access

Closure:

    cargo check --workspace --lib
    cargo test --workspace --no-fail-fast
    python3 tools/architecture/check_boundaries.py
    git diff --check

The known mock-socket WouldBlock flake is non-blocking only if it reproduces on the unchanged baseline. Do not weaken the harness to hide it.

## Completion conditions

This P0 is complete only when all are true:

1. stored_server_connection is not callable from an external production crate;
2. ConversationTurnRequest public API cannot return SavedServerConnection or an equivalent raw credential;
3. external callers can still construct a normal intent-only ConversationTurnRequest;
4. public Worker/Vault paths cannot inject or extract the saved credential through ConversationTurnRequest;
5. internal App source/model/legacy-Expert composition continues to function;
6. test Fixed Some/None remains compile-time test-only and hermetic;
7. current recipient authority still re-reads current state at admit/consume/revalidate;
8. focused/workspace/architecture validation is green.

## Documentation update on completion

When all gates are green:

- mark this document Status: complete;
- restore docs/refactoring/stage-2/2-b5.md to Status: complete / frozen;
- check the residual credential-read P0 in docs/refactoring/stage-2/2-b.md;
- re-check 2-B and 2-B.5 in docs/refactoring/stage-2.md;
- restore Current checkpoint to 2-C — Delegation ownership convergence;
- stop.

Do not implement 2-C in the same change set.

## Agent report format

Report only:

1. changed files/symbols;
2. final ConversationTurnRequest public surface;
3. external compile-probe positive/negative results;
4. proof Worker/Vault public paths cannot read or inject saved credentials;
5. current-authority admit/consume/revalidate behavior;
6. focused/workspace test results;
7. architecture check;
8. remaining 2-B P0/P1, if any.

No PR. Do not create another planning/status document.
