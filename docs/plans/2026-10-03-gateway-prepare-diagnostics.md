# Gateway Prepare local-storage diagnosis

The user reported storage_unavailable for connections.gateway.prepare_setup, request dbd19b99-2742-4d94-b4aa-655180eaa619, Error ID floe-1791052811450085-3. Existing records confirm the local command failure but cannot identify the SQL versus protected-store stage. Prepare performs no Gateway HTTP request.

Read-only app metadata showed ad-hoc signing, no TeamIdentifier and no application/team/keychain-access-group entitlements. That is relevant evidence, not proof of the failing OSStatus. No credential was read or changed during investigation.

## Bounded debug instrumentation

The patch emits fixed-field warning events through the existing Rust tracing subscriber:
- gateway_prepare_failed: command UUID, derived target UUID, setup receipt lookup / adapter prepare / setup receipt store, closed PairingError variant.
- gateway_setup_storage_failure: derived target UUID, expectation / slot lock / read / recheck / staging / write / readback / worker, closed error kind. Only Prepare supplies this diagnostic context.
- native_keychain_failure: fixed SecItem operation, numeric OSStatus, closed error kind.

Events contain no endpoint, service/account, query dictionary, SQL text, stored values, bearer, proof or key material. Debug-only logging changes no returned error, authorization, no-prompt/ThisDeviceOnly policy, deadline, worker lifetime or locking. No UI layout or public error protocol changes.

## Manual collection boundary

Rust tracing writes process output; it is not automatically included in Flutter incidents.ndjson. For an explicitly coordinated manual retry, the user may run from the repository root:

```sh
umask 077
floe_diagnostic_log=$(mktemp "${TMPDIR:-/tmp}/floe-prepare.XXXXXX")
printf 'Private diagnostic log: %s\n' "$floe_diagnostic_log"
./scripts/run-local.sh 2>&1 | tee "$floe_diagnostic_log"
```

The user performs Prepare themselves. Inspect only matching fixed diagnostic events and correlation IDs; do not upload the full raw console log, which may include unrelated private activity. Preserve the log and all prior evidence. No tool-side app launch, pairing, key operation, security setting change or user-data reset is authorized by this document.

The diagnostic patch is source-reviewed but awaits compilation/formatting and a manual failing-stage observation. It is not a guessed credential-storage fix.


Cloud workspace build with --locked, dependency policy (23 nodes/126 edges) and diff audit passed on839ea999. The captured formatting-only delta for three Rust files is integrated; native keychain formatting was unchanged. No tests, real key operations or diagnostic-triggering runtime calls ran. macOS rebuild/manual observation remains pending.
