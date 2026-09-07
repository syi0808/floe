# S4 encrypted session-store component

Date: 2026-09-07. Evidence: synthetic records and in-memory test keys, macOS arm64.

## Scope

`EncryptedAgentVault` implements the existing `floe-agent::SessionStore` port in
Rust Core. It is a separately validated component, **not** a migration or a newly
enabled personal-chat route. The C ABI/Dart sample panel still uses its original
synthetic-only table. No personal data, real model, connector or live Keychain
operation is exercised by the automated suite.

S4 remains **0/14**. P0-F is not passed: signed-host OS key access, lock/lifecycle
handling and UI/transport integration must still be demonstrated together.
S1/S3 acceptance is unchanged.

## Storage boundary

The caller supplies an existing, current-user-owned private root directory
(0700 on Unix). Each Person gets an independent directory:

```text
<private root>/<Person UUID>/
  host.lock
  vault.id
  sessions.db
  sessions.db-wal
```

`create` exclusively creates a new Person directory, persists a random vault ID,
inserts a random 256-bit key through the key provider, verifies the stored key,
then initializes the encrypted database. `open` never provisions a key or creates
a missing database. An incomplete create is intentionally left unavailable for
explicit repair; it is not automatically deleted or retried with new keys.

The database contains an encrypted version/Person/vault identity and complete
serialized sessions, including user/assistant messages, capability input/results,
active-turn pointers and terminal outcomes. Session updates use one SQL revision
CAS; IDs, schema version, revision increments, size and forbidden data classes
are checked. This is not an automatic detector of misclassified secret text.

Encryption uses the already-pinned Turso engine's native `aes256gcm` option, not
application-written cryptography or plaintext JSON plus an encryption flag.
Turso documents page and WAL encryption, with the first 100 header bytes excluded.
See [Turso encryption](https://docs.turso.tech/tursodb/encryption) and its
[Rust API](https://docs.turso.tech/sdk/rust/reference).
Person/vault identifiers, paths, file sizes and access timing are not hidden.
There is no sync, export, backup, rollback protection or key rotation in this slice.

## Keyring library choice

At the user's suggestion, direct Security.framework FFI was removed in favor of
the keyring-rs ecosystem. Its current maintainers recommend `keyring-core` plus
the required store crates when applications need explicit backend control, rather
than linking the all-in-one CLI. See the
[keyring-rs README](https://github.com/open-source-cooperative/keyring-rs).

The adapter uses `keyring-core` 1.0.0 and macOS
`apple-native-keyring-store` 1.0.2 with only `protected` enabled. It constructs
entries directly, never relying on a process-global default store or falling back
to a mock, plaintext file or legacy Keychain. The service is
`com.floe.agent-vault.v1`; account names combine Person and random vault UUIDs.
The selected policy is `WhenUnlockedThisDeviceOnly`, with cloud synchronization
disabled. Other OS backends are not enabled and return `VaultUnavailable`.
See the [Apple backend documentation](https://docs.rs/apple-native-keyring-store/latest/apple_native_keyring_store/)
and [Apple accessibility policy](https://developer.apple.com/documentation/security/ksecattraccessiblewhenunlockedthisdeviceonly).

The generic keyring setter is an upsert, not an atomic create-only operation.
The adapter only calls it after an explicit `NoEntry` result, while vault creation
holds the exclusive host lock and uses a fresh random key slot. All other lookup
errors, malformed keys and existing entries fail. This protects cooperating Floe
hosts, not an unrelated same-user process deliberately racing that exact key slot.

Keys have no serde/Debug implementation and never enter Dart, JSON commands,
database parameters, URLs or application error messages. Floe-owned byte buffers
are zeroized; the Turso API requires an owned hex string and both upstream libraries
may hold internal copies. Complete process-memory erasure is **not** claimed.

Before each storage operation the adapter reloads and constant-time compares the
key. Missing, inaccessible or changed keys latch that handle unavailable, including
after the key is restored; unlocking requires an explicit drop/reopen. A failed
commit cannot emit a successful final answer. Already-returned messages and an
in-flight model call are not erased/cancelled by this storage-only check. Native
lock/background notifications and presentation clearing remain integration work.

The protected backend requires suitable signed-host configuration. There is no
direct authentication-UI override in Floe; no claim of bounded, noninteractive
Keychain latency or real locked-device behavior is made from mock tests.

## Ownership and recovery

A nonblocking exclusive file lock is held for the entire vault lifetime and
released after the database is dropped. A second handle/process receives Conflict
before key access or recovery. Floe never unlinks the lock to take ownership.
Root/Person symlinks, nonprivate directories and symlinked lock/marker/database
paths are rejected; the parent root is trusted application-managed storage.
This does not defend against a malicious same-user process replacing directory
entries while the host is running.

Interrupted sessions reopen with committed messages intact. Runtime recovery
marks Interrupted without replaying models or capabilities. The existing fixture
transport is not switched to this lock; production integration must combine this
cross-process ownership with its within-handle run/recovery guard.

## Automated evidence

Recorded: 91 Rust tests (14 new, including the subprocess helper) and 117 Flutter
tests pass. Flutter analyzer, native library/macOS Debug app builds, deep/strict
signature verification, formatting and whitespace checks pass. The app was not
launched. Clippy passes with only the two existing Calendar exclusions below.

- Keyring binary roundtrip, malformed length, existing-key preservation and
  inaccessible-store errors using its mock store; explicit Apple backend policy
  and Person/vault scope inspected without contacting the OS store.
- Distinct keys per Person; wrong-Person reads/writes, stale/skipped revisions,
  unsupported versions, prohibited data classes and oversized payloads rejected.
- Synthetic user text, tool input/result and assistant text absent from every
  vault file before and after WAL checkpoint; encrypted records survive reopen.
- Wrong/missing keys and ciphertext modification fail without replacing the DB;
  a plaintext database is rejected without automatic migration.
- Relabeling encrypted files as another Person fails even when supplied the
  matching key, because the encrypted identity must also match.
- Incomplete provisioning and missing/empty databases stay unavailable; a
  revoked handle stays locked until reopened.
- A separate test process cannot acquire a live vault. Key loss during a runtime
  turn prevents assistant/success events; explicit recovery preserves the user
  message, performs no model replay and permits a later deliberate turn.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- \
  -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
cd apps/client
flutter analyze
flutter test
flutter build macos --debug
```

The two Clippy exclusions are the previously documented Calendar warnings;
no unrelated Calendar changes or source suppressions are introduced.

## Next gate

Connect an explicitly provisioned vault to the signed host and expose only typed
availability/session operations to Dart. Demonstrate real key creation/reopen,
locked/denied/missing-key behavior, bounded worker access, shutdown/crash recovery
and precise disposable cleanup. Define repair/deletion and key-orphan handling
before enabling personal free text. Multi-platform backend, self-host key ownership
and cross-device key delivery remain separate gates.
