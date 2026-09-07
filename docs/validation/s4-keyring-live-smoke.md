# S4 keyring smoke harness

Date: 2026-09-07. This checkpoint uses a disposable synthetic vault, not the
Flutter app or personal conversations. P0-F and S4 remain unaccepted.

## Current backend

Floe now stores the random encrypted-database key in the user's macOS login
Keychain. This backend supports local and ad-hoc-signed development builds without
an Apple Developer Program provisioning profile. The harness defaults to ad-hoc
signing; `FLOE_CODESIGN_IDENTITY` remains an optional override. `--exercise` may
cause macOS to show a Keychain access prompt.

Migration to Apple Protected Data is intentionally deferred until a provisioned
release and must copy each existing key before switching readers. The encrypted
conversation database remains unchanged.

The read-only probe and disposable `--exercise` both pass on macOS 26.2 arm64
with an ad-hoc signature. The exercise verified a real login-Keychain key create,
encrypted sample turn, reopen, registry reopen, key-loss failure, exact key
deletion and temporary-file cleanup.

## Prior protected-backend result

On macOS 26.2 arm64, the Core example builds and an Apple Development signature
passes deep/strict verification. No matching provisioning profile or application
identifier/keychain entitlements were supplied to this helper bundle.

- A read of a newly random, nonexistent protected-store slot returns NoEntry.
  This proves only the read probe completed, **not** that provisioning works.
- Production `EncryptedAgentVault::create` fails. Only the private Person
  directory, `host.lock` and `vault.id` exist; no sessions database was created.
- Exact-slot cleanup also fails with `PlatformFailure: A required entitlement
  isn't present.` A subsequent cleanup attempt reports the same OS error.
- The temporary root is retained, with its Person/vault markers, rather than
  losing the only reference for possible orphan cleanup. Key creation/absence is
  not claimed verified. No existing production key or database was accessed.

The backend's [upstream guidance](https://github.com/open-source-cooperative/apple-native-keyring-store)
requires a signed/provisioned client for protected storage. The observed failure
means the signer alone was insufficient for that harness. Floe did not switch to
plaintext; the current login-Keychain backend still keeps keys outside the
encrypted conversation database.

## Current reproduction

```sh
bash tools/validation/run-vault-keyring-smoke.sh --probe
bash tools/validation/run-vault-keyring-smoke.sh --exercise
```

The helper bundle identifier is `app.floe.validation.vault`. The script neither
registers an App ID nor changes a developer account or system settings.

`--exercise` creates a unique private temporary root and new Person/vault IDs,
uses the production keyring-backed Core store for a synthetic turn, reopens and
compares that session and its persisted Expert registry, deletes only its own key,
then checks fail-closed access
and reopen. Successful cleanup verifies exact-slot absence and removes the root.
If cleanup is uncertain, the root is retained and its path reported. Retry that
cleanup using the same macOS login Keychain before creating another test vault:

```sh
bash tools/validation/run-vault-keyring-smoke.sh \
  --cleanup '<reported temporary root>' '<Person directory UUID>'
```

Cleanup accepts only a current-user-owned 0700 smoke root directly under the
system temporary directory, with exactly one specified private Person directory
and a regular private UUID marker. Root/directory/marker symlinks are rejected.
It never enumerates keychain items, and removes files only after exact-slot
deletion/absence succeeds. This is a validation utility, not the product's
repair/deletion flow. Do not remove retained markers until cleanup is verified.

## Validation and remaining work

The example's ordinary tests check cleanup path confinement without accessing
the real key store. Run `cargo test -p floe-core --example vault_keyring_smoke`.
The signing script passes `bash -n`; the example builds on the current host.

Still required: the same login-Keychain boundary through the actual app, Keychain
lock/denial/lifecycle and crash cases, and product repair/deletion/orphan handling.
The disposable smoke does not close those gates.
