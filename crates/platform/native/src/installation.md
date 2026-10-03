# Internal installation startup

`floe_app::open_default(support_directory)` is the product startup entry point.
It returns an admitted host whose caller supplies the authoritative Person and
device identity. There is no product profile selection or setup wizard.
`floe_app::open(database_path)` is an existing-only diagnostic entry point.

## Identity and physical ownership

The native installation boundary owns these nonsecret physical records:

- `local_installation.json`: a closed schema-1 object containing `schema_version`,
  `person_id` (canonical nonnil UUID string), `device_id` (exact nonempty bounded
  string without whitespace/control characters), and `phase` (`initializing` or
  `ready`). New identities are random UUIDs, persisted once.
- `local_device_id`: the same exact device string, without a newline or trimming.
- `people/<person_id>/floe.db`: the plain Day/Connections store.
- `local_installation.create-attempt`: an immutable closed schema-1 object with
  `schema_version`, `person_id`, and `device_id`, durably written before the sole
  database creation attempt for that identity.
- `local_installation.lock`: the process-safe exclusive installation lease.

An existing valid `selected_profile.json` is a read-only reference to existing
data; it is not rewritten. Without an installation record or selection, exactly
one existing Person database plus a valid device identity can be reused. Multiple
databases are never resolved by ordering or by choosing the first directory.
Malformed, missing, unreadable, ambiguous and unsupported states remain distinct
from proven fresh absence. Fresh root admission permits the diagnostics and
recovery directories, an empty people directory, and a regular `.DS_Store` OS
metadata file; it never deletes them. Unknown root entries are not silently
classified as harmless installation state.

Identity files, lock files and data directories reject symlink leaves. The
platform-provided support path is canonicalized before recording child paths;
normal OS ancestor aliases do not become a second installation identity.

The exclusive lease spans preparation, store validation, and ready publication.
App then consumes its actual locked File into TursoStore before publishing any
owner. The database is declared before this retained lock, so the lock drops last;
every retained repository/store Arc keeps the installation excluded. Diagnostic
open acquires the same lease. Busy admission fails without resetting. The normal
AppHost retirement path closes/drains owners; a retained background store keeps
the lock even after a caller's close timeout. No reset moves a live admitted store.

## Creation and crash order

1. Acquire the installation lease and inspect bounded identity/data records.
2. For a genuinely fresh installation, create and sync the `initializing` record
   before creating the Person directory or device file. Retry uses those same IDs.
3. Create missing directories/device data only as the exact recorded initialization
   permits. Sync each new file and its parent directory.
4. Create and sync the immutable create-attempt marker, then call the plain
   storage adapter's `create_new`. It never truncates or initializes an existing
   file. Marker plus missing database is incomplete, never a fresh create retry.
5. Checkpoint newly initialized schema pages into the main file, require a
   nonbusy checkpoint result, and sync the main file and its parent. Existing
   databases, including an interrupted initialization's database, must
   pass existing-store schema validation. A partial file is never initialized.
6. Debug startup inspects any existing encrypted Vault read-only before owner
   activation, using the existing exact key slot and the Vault's own host lock.
7. Re-read physical Person/device identity after validation. Publish `ready` by
   syncing `local_installation.ready` and renaming it over the identical identity's
   initializing record. An existing database being adopted receives its first
   installation record only after successful validation. No identity is rewritten.
8. Transfer the installation lease to the store, then construct/activate owners.

A crash before a durable identity can leave an invalid bounded record; it is not
permission to guess its identity. A crash after the create-attempt marker but
before database creation leaves an incomplete installation. Stable builds fail
explicitly in either case. The development-only policy below can preserve and
replace this installation as a distinct fresh identity.

## Development-only move-aside policy

Only `cfg(debug_assertions)` enables development recovery. Stable builds never
archive automatically. Trigger evidence is limited to typed invalid/ambiguous/
incomplete installation state, successfully observed incompatible plain-store
header/schema, typed Turso `Corrupt`/`NotAdb`, or the encrypted Vault preflight's
typed missing/malformed key, incompatible schema, identity mismatch or stored-data
corruption. Error text is never parsed for permission to reset. Generic database
errors, failed I/O, permission denial, busy/locked state and unavailable Keychain
access remain failures. A cryptographic cause erased into a generic engine error
cannot be claimed as a verified wrong key or corruption.

Before an encrypted reset, the preflight's opaque `VaultResetEvidence` retains
the existing Vault host-lock File through the complete move. App drops its plain
store before moving anything and never exposes a client-supplied reset command.
The new store cannot repeatedly reset in one open attempt.

Every archive, including an invalid-installation archive before Vault preflight,
also acquires all existing encrypted `host.lock` files in the bounded known Vault
layouts covered by the move. It inspects both original and recovery trees when
resuming. Only the exact device/inode of a supplied retained preflight lock is
excluded from reacquisition; every other lock is retained until all moves finish.
Missing/malformed lock files, busy locks and failed enumeration stop recovery.
Symlink directories are never traversed. An older binary does not participate in
the installation lease: close all older app/diagnostic instances before running
development recovery. This code cannot claim exclusion against an older process
that holds only a plain database without the new installation lock.

Recovery creates `recovery/<unix_milliseconds>-<random_uuid>/manifest.json`. Its
closed schema-1 fields are `schema_version`, `recovery_id`, `started_at_millis`,
`reason`, and `entries`. The entries are limited to the present members of:

`local_installation.json`, `local_installation.create-attempt`,
`local_installation.ready`, `local_device_id`, `selected_profile.json`,
`selected_profile.json.tmp`, `people`, `floe.db`, `floe.db-wal`, `floe.db-shm`, and
`floe.db.agent-vaults`.

The complete manifest is synced before writing identical immutable bytes to
`local_installation.reset`. Each directory entry is then renamed on the same
filesystem into that recovery directory, with directory syncs after each move.
Existing destinations are never overwritten. A source absent with its exact
destination present is a completed step; both present or both missing fail.
Startup resumes a pending manifest under the same exclusive installation lease
before admitting any other state. After all moves, the root reset marker is moved
to `completed.json` inside the recovery directory. A malformed pending reset
manifest fails instead of starting another recovery around it.

This preserves the complete old data tree, including journals and uncertain
operation evidence. Root diagnostics, prior recovery directories and every old
Keychain entry remain intact. Symlink entries themselves may be archived; their
targets and all external provider data remain untouched. No files or credentials
are deleted and no historical external operation is replayed.

Encrypted Vault creation/unlock retains the existing product lifecycle after App
startup. This bootstrap neither creates a Vault key nor unlocks a ready generation.
Source implementation is separate from authorization to run a development reset
against an actual user's installation.
