# Internal installation startup

`floe_app::open_default(support_directory)` is the product startup entry point.
It admits one internal installation and returns a host whose `CallerContext`
carries the verified Person/device identity and host epoch. Ordinary startup
has no product profile-selection wizard. `floe_app::open(database_path)` is an
explicit existing-only diagnostic entry point; it uses the same identity,
storage-profile and installation-lock checks.

This document describes the current code, not the removed debug move-aside
implementation. Debug assertions never authorize resetting or archiving data.

## Identity and physical ownership

The native installation boundary owns these nonsecret physical records:

- `local_installation.json`: a closed schema-1 object with `schema_version`,
  canonical nonnil `person_id`, exact bounded `device_id`, and `phase`
  (`initializing` or `ready`). Fresh identities are random and persisted once.
- `local_device_id`: the same exact device string, without a newline or trimming.
- `local_installation.create-attempt`: a closed immutable schema-1 object with
  `schema_version`, `person_id`, and `device_id`, written durably before the
  sole admitted database creation attempt for that identity.
- `local_installation.lock`: the process-safe exclusive installation lease.
- `local_installation.ready`: the synced temporary record used to publish the
  validated installation's ready phase.

`people/<person_id>/floe.db` is the encrypted product store, not a plaintext Day
store. It owns host-lifetime product state, including Day and source lifecycle
records. Its purpose-separated root key is independent of the Agent Vault key,
provider credentials and ready owner generation. The storage adapter creates a
root key only during admitted fresh product-store creation. Existing opens load
that exact key; missing, malformed or unavailable custody never generates a
replacement. A previous plaintext SQLite header is unsupported and is preserved.

An existing valid `selected_profile.json` is a read-only reference to existing
data and is not rewritten. Without an installation record or selection, only
an unambiguous existing Person database and valid device identity can be reused.
Multiple databases are never resolved by directory order. Invalid, unreadable,
ambiguous, incomplete and unsupported states are distinct from proven fresh
absence. Known diagnostics/recovery directories and OS metadata do not authorize
adopting or deleting their contents; unknown root entries are not silently
classified as a fresh installation.

Identity files, lock files and data-directory leaves reject symlinks. The
platform-provided support path is canonicalized before recording child paths.
App canonicalizes an explicit database path before profile validation, identity
loading and installation locking, so aliases cannot select another lease.

## Build-selected storage profiles

Production `os-keyring` custody and isolated `development-storage` custody are
selected at build time. An OS key-store failure never falls back to file keys.
A debug build does not itself select development custody; the explicit
`development-storage` feature is restricted to debug-assertion builds.

A development-storage build selects the dedicated
`<support_directory>/development-storage/client` installation. Its parent holds
the exact `floe-development-profile` marker. Only the dedicated child and its
validated marker are created; the normal installation at the supplied base is
not imported, reset or used as a fallback. Production opens reject the marked
development profile. Existing-only diagnostic opens must satisfy the same
build-selected profile checks.

Both profiles use encrypted product and Agent Vault databases. Development file
keys are private fixture custody, not a plaintext database mode or a way to
unlock a production profile. Use an explicitly selected fresh development root
when a pre-stable stored format is incompatible; retain the original root and
its keys for inspection.

## Creation and crash order

1. Select the build's permitted support root, acquire its installation lease,
   and inspect bounded identity/data records. A legacy
   `local_installation.reset` marker makes default startup incomplete; startup
   does not resume an old archive/reset manifest.
2. For proven fresh state, sync the `initializing` record before creating the
   Person directory or device file. An interrupted retry retains the same IDs.
3. Create only the directories/device data permitted by that recorded identity,
   syncing new files and parent directories.
4. Sync the immutable creation-attempt marker before the storage adapter's
   `create_new`. It exclusively creates a new main file and purpose-separated
   key, then creates and verifies the encrypted schema/identity. It never
   truncates or initializes an existing database.
5. Checkpoint new schema pages into the main file, require a nonbusy result,
   and sync the file and parent. An already present initializing database is
   inspected through `open_existing`; an empty, partial, incompatible or
   unreadable file is not initialized or repaired.
6. Existing opens load the existing product key, retain the selected physical
   main file, validate encrypted schema and identity read-only, then open and
   revalidate the same pinned file for ordinary use. Failed validation preserves
   the files, creation marker and key.
7. Re-read Person/device identity after store validation. Publish the same
   identity's ready record with durable replacement of the initializing record.
   Existing data being adopted receives an installation record only after
   successful validation; identities are never guessed or rewritten.
8. Transfer the actual installation-lock File into `TursoStore` and construct
   the host resources. Runtime preparation separately owns Agent Vault
   creation/unlock and ready-generation activation; installation completion
   does not unlock that generation or preflight/reset its key.

The retained lock is declared after the product database and drops last. Every
retained repository/store `Arc` keeps the installation excluded. Diagnostics
participate in that exclusion. Host shutdown closes admissions and drains
owners; a timeout does not prove a retained background store has released its
lease. Busy admission fails without moving or replacing live state.

## Failure and recovery boundary

Every build fails closed on invalid, incomplete or incompatible local state,
missing/unavailable keys and uncertain storage outcomes. A crash after the
creation-attempt marker but before database creation remains incomplete. A
partial existing creation stays preserved. Neither generic error text nor a
known corruption classification grants permission to replace data or keys.

There is no current automatic archive/reset implementation, client reset route,
`VaultResetEvidence` recovery path or debug-only plaintext-store fallback.
Historical recovery directories and unfinished reset markers are retained for
explicit operator inspection, not replayed as startup instructions. No startup
failure automatically replays uncertain external operations.

Current implementation references: `crates/app/src/composition.rs`,
`crates/app/src/storage_profile.rs`, `crates/platform/native/src/installation.rs`,
`crates/adapters/vault/src/engine.rs`, `crates/adapters/vault/src/product_keys.rs`,
and the Runtime preparation owner. Native identity/lease tests and App's missing
key/incomplete creation regressions enforce the preservation boundary; Linux
fixtures do not qualify Apple Keychain behavior.
