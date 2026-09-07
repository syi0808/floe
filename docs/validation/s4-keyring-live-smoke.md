# S4 signed keyring smoke harness

Date: 2026-09-07. This checkpoint uses a disposable synthetic vault, not the
Flutter app or personal conversations. P0-F and S4 remain unaccepted.

## Observed result

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
means the current signer alone is insufficient for this harness. Do not switch
to plaintext or a legacy store to make the test pass.

## Reproduction

```sh
export FLOE_CODESIGN_IDENTITY='<development signing identity>'
bash tools/validation/run-vault-keyring-smoke.sh --probe
export FLOE_VAULT_SMOKE_PROFILE='<matching provisioning profile>'
export FLOE_VAULT_SMOKE_ENTITLEMENTS='<matching entitlements plist>'
bash tools/validation/run-vault-keyring-smoke.sh --exercise
```

The helper bundle identifier is `app.floe.validation.vault`. The script requires
an explicit signing identity and accepts profile/entitlements only as a pair.
It neither registers an App ID nor changes a developer account or system settings.
The OS may ask the user to authorize use of the signing private key.

`--exercise` creates a unique private temporary root and new Person/vault IDs,
uses the production keyring-backed Core store for a synthetic turn, reopens and
compares that session, deletes only its own key, then checks fail-closed access
and reopen. Successful cleanup verifies exact-slot absence and removes the root.
If cleanup is uncertain, the root is retained and its path reported. Retry that
cleanup with the same identity and matching provisioning before creating another
test vault:

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

Still required: provisioned create/reopen/key-loss/cleanup success, the same
boundary through the actual app, physical lock/denial/lifecycle and crash cases,
and product repair/deletion/orphan handling. A read probe cannot close these gates.
