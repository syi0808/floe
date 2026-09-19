# 2-B.2 Final P0 — Current Recipient Authority

**Status: active — final 2-B.2 blocker**

**Code baseline reviewed:** `cd73321b3159da94d1a7671e76e001eacd57b65a`

This document is the authoritative execution plan for the remaining 2-B.2 P0. The canonical model vertical cutover itself is complete. This task only closes the production exact-recipient revocation fence.

Do not start 2-B.3 in this change set.

## Finding

The Access contract is correct:

```text
admit_model_dispatch
  → ModelDispatchPermit
consume_model_dispatch
  → ModelDispatchFence
provider call
revalidate_model_dispatch
  → response release
```

and `ModelDispatchRecipientAuthority` is explicitly defined as the **current** exact-recipient authority.

The production App implementation is not current.

Today:

```rust
pub struct HostRecipientAuthority {
    consented_recipients: Vec<String>,
}
```

is built once from:

```rust
provider.consented_external_recipients()
```

before `InferenceService` starts. Every later Access check therefore reads the same snapshot.

That permits this unsafe sequence:

```text
saved connection consents to recipient R
→ authority snapshot captures R
→ Access admit succeeds
→ provider handoff starts
→ person revokes/removes R from current saved connection
→ post-response Access revalidation checks old snapshot
→ response can still be released
```

This violates the Stage 2 P0 rule for authority/data-release risk and the 2-B.2 exit condition that revocation between dispatch fences denies release.

## Goal

Make every production recipient check evaluate the **current saved-connection authority bound to the verified person/device**, not the provider's initial consent snapshot.

Required invariant:

```text
Access admit:
    load current saved connection
    bind to exact person/device
    exact recipient must still be consented

Access consume immediately before provider handoff:
    repeat the same current check

Access post-response revalidate:
    repeat the same current check

any removal / recipient change / consent revocation / identity mismatch:
    deny
```

The prepared provider transport may still hold the credential/endpoint snapshot it was created with. That is transport state. It must not be treated as current release authority.

## Scope

### In scope

- replace snapshot `HostRecipientAuthority`;
- introduce/inject a current saved-connection-backed implementation of `ModelDispatchRecipientAuthority`;
- bind every authority read to the verified person/device;
- exact current external recipient check;
- pre-handoff and post-response production regressions;
- restore 2-B.2 complete status after the P0 is green.

### Out of scope

Do not change:

- Access `ModelDispatchPermit` / `ModelDispatchFence` state machine unless a focused regression proves a defect;
- Inference profile ranking/fallback;
- provider profile discovery;
- provider secret-free public boundary;
- 2-B.1 recovery;
- 2-B.3 projection/tool work;
- `LegacyToolPort` / `LegacyDelegationPort`;
- Expert/Schedule model convergence;
- FFI/Flutter product DTOs.

Do not wire `AttemptLifecycle` into the canonical root path as part of this fix. Its remaining legacy telemetry role is a 2-D cleanup/design decision unless a current product contract proves otherwise.

## Ownership

The Access port remains:

```text
floe-access::ModelDispatchRecipientAuthority
```

Access owns the authorization decision.

The current saved-connection observation should be implemented by an adapter/composition dependency. Prefer an adapter implementation backed by the existing saved-connection store rather than an App-owned policy list.

A recommended shape is:

```rust
struct SavedConnectionRecipientAuthority<Store> {
    store: Store,
    person_id: String,
    device_id: String,
}
```

where `Store` provides the current saved connection.

Equivalent naming is acceptable.

The implementation:

1. loads the current saved connection on **every** `check_recipient`;
2. fails closed when no current connection exists;
3. reuses `floe_inference::admit_saved_connection` to bind stored person/device and validate consent state;
4. requires `allow_external == true`;
5. requires exact string membership in `external_recipients`;
6. never returns or logs the bearer token;
7. treats malformed/foreign/revoked state as denial, never as consent.

Do not move broader dispatch policy into the adapter. It reports current recipient authority only; Access continues to evaluate coverage, processing restrictions and data classes.

## Production source of truth

The normal product path currently supplies:

```text
ConversationTurnRequest.saved_server_connection = None
```

and uses the host Keychain saved-connection slot.

Therefore the production recipient authority must re-read that same current store on each Access check.

Use the existing:

```text
floe_provider_adapters::control::SavedServerConnectionStore
/load_saved_connection()
```

or a narrower equivalent adapter port.

Do not copy `RootModelProvider::consented_external_recipients()` into production authority state.

## Test-only injected connection

`ConversationTurnRequest.saved_server_connection = Some(...)` is currently used by App tests, while the real product boundary passes `None`.

Do not make the production authority snapshot-based just to preserve those fixtures.

Choose one of these narrow test strategies:

1. preferred: add a mutable/fake saved-connection store to the authority/provider test harness, using the same current-store semantics as production;
2. acceptable: retain a `#[cfg(test)]` fixed-store helper for existing non-revocation fixtures and use a mutable store for the new revocation regressions.

Do not add a new product DTO field or FFI route to inject authority state.

## Implementation steps

### P0-A — Replace snapshot authority

Primary files:

- `crates/app/src/inference_routes.rs`
- `crates/adapters/providers/src/control/server_connection.rs` or a sibling control adapter
- `crates/adapters/providers/src/control/mod.rs`

Changes:

- [ ] remove production `HostRecipientAuthority { consented_recipients: Vec<String> }`;
- [ ] add current saved-connection-backed recipient authority;
- [ ] bind authority construction to verified `person_id + device_id`;
- [ ] re-read current store in every `check_recipient`;
- [ ] use exact recipient equality only;
- [ ] keep credentials private/redacted.

### P0-B — Wire current authority into canonical root service

Primary file:

- `crates/app/src/vault_host/conversation_turn.rs`

Current code:

```text
provider
→ provider.consented_external_recipients()
→ snapshot HostRecipientAuthority
→ InferenceService
```

Target:

```text
provider = prepared from admitted saved connection
authority = current-store recipient authority(person, device)
InferenceService(provider, resolver, authority)
```

Requirements:

- [ ] provider preparation remains independent from release authority;
- [ ] production authority construction does not depend on `provider.consented_external_recipients()`;
- [ ] device-only dispatch remains unaffected;
- [ ] no bearer/base URL crosses into Access.

### P0-C — Prove pre-handoff revocation

Add a focused canonical integration regression.

Suggested name:

```text
recipient_revoked_before_handoff_never_posts_agent_request
```

Sequence:

1. current store initially consents to external recipient R;
2. canonical server profile resolves R;
3. revoke/remove R before `consume_model_dispatch`;
4. root attempt proceeds to the handoff fence.

Assert:

- result fails closed (`PolicyDenied` or the existing canonical authorization failure mapping);
- `/v1/agent` POST count = 0;
- no provider content reaches Engine;
- no successful usage settlement is fabricated.

Do not prove this only with the Access unit-test fake; exercise the production current-authority implementation.

### P0-D — Prove post-handoff revocation suppresses response

Add a focused canonical integration regression.

Suggested name:

```text
external_model_response_is_suppressed_after_recipient_consent_revocation
```

Sequence:

1. current store initially consents to R;
2. Access admit and consume succeed;
3. provider observes the `/v1/agent` handoff and blocks before responding;
4. mutate current saved-connection authority to remove R or disable external consent;
5. provider returns a valid response;
6. post-response `revalidate_model_dispatch` runs.

Assert:

- provider POST count = 1;
- result is denied before model content leaves Inference;
- response text is not committed/released;
- provider usage remains charged in the scope budget;
- the current store was consulted again after handoff.

This is the primary regression for this P0.

### P0-E — Current identity and replacement semantics

Add focused tests for the current authority implementation:

- [ ] connection removed → recipient denied;
- [ ] `allow_external` revoked → recipient denied;
- [ ] recipient removed → denied;
- [ ] different recipient added while requested recipient removed → denied;
- [ ] saved connection rebound to another person/device → denied;
- [ ] malformed stored connection → fail closed;
- [ ] unchanged exact consent → accepted.

A credential/token rotation with unchanged exact recipient may still pass the recipient-authority check; the already-prepared transport can then succeed or fail independently. Do not make Access inspect credentials.

## Security ordering

Keep the existing Inference ordering:

```text
Access admit
→ budget reserve
→ Access consume
→ mark_dispatched
→ provider
→ settle trustworthy usage
→ Access revalidate
→ release ModelResponse
```

Important consequences:

- revocation before consume → provider calls 0;
- revocation after handoff → provider may have been called and usage remains charged;
- failed post-response authority check discards response content;
- no retry/fallback may bypass an Access denial;
- fallback to another target still requires a new permit/fence.

## Static closure checks

After the fix:

- [ ] production canonical root authority stores no copied recipient list;
- [ ] production canonical root authority reads current saved connection per check;
- [ ] production root does not derive authority from `RootModelProvider::consented_external_recipients()`;
- [ ] Access still receives no bearer/base URL;
- [ ] provider prepared transport still exposes no secret through Inference public types;
- [ ] 2-B.3 code is untouched except documentation status if necessary.

## Validation

Focused:

```sh
cargo test -p floe-access
cargo test -p floe-provider-adapters
cargo test -p floe-app
cargo test -p floe-inference
```

Closure:

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
```

## Completion

When the production current-authority regressions are green:

1. mark this document `Status: complete`;
2. mark 2-B.2 complete again in `docs/refactoring/stage-2.md`;
3. update `docs/refactoring/stage-2/2-b.md` residual P0 checkbox;
4. restore Stage 2 Current checkpoint to **2-B.3**;
5. stop.

Do not implement 2-B.3 in the same change set.

## Agent report format

Report only:

1. changed files/symbols;
2. current-authority source and how every check reloads it;
3. verified person/device + exact-recipient binding;
4. pre-handoff revocation regression result;
5. post-handoff revocation regression result, including provider call count and usage behavior;
6. focused/workspace test results;
7. architecture check;
8. remaining 2-B.2 P0, if any.

No PR. Do not create another planning/status document.
