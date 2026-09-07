# S4 app registry enablement management

Date: 2026-09-07. App integration; S4 remains **0/14**.

## User-visible path

The unlocked assistant panel now opens **Tools & Experts** through the same native
vault gateway/owned worker as conversation operations. The screen reads installed
Tool/Expert package IDs and pinned versions, installation enablement, this Person's
assignment enablement, existing grant counts and private-state revision/completion
counters. Separate switches distinguish installation-wide enablement from an
assignment's permission to run for this Person.

Both the installation and assignment must be enabled, and an Expert's required Tool
must also remain enabled. Disabling a Tool or Expert prevents new invocations through
the existing registry resolver. It does not delete private state, rewrite conversation
history, retract already prepared actions or revoke a user's existing S3 approvals.
Enabling restores only previously granted access; this screen cannot add sources,
replace View handles, change package versions or approve an action.

Opening the screen does not install or initialize anything. A vault without a registry
returns an empty state explaining the current sample setup. The first explicit sample
turn continues to initialize its existing sample Tool/Expert pair as before. There is
no new automatic installation on create, unlock, inspect or refresh.

## Native and storage boundary

`AgentVaultActionDto::Registry` adds inspect/configure jobs to the versioned native
submit/poll/stop/release protocol. This is an application configuration operation,
not an Agent/Expert capability. Its mutation contains only the vault instance ID,
expected registry revision, existing installation/assignment ID and desired boolean.
Unknown target kinds, grant fields and private-state input are rejected during decode.
The job's Person and the open vault must match; configuration cannot supply another
Person or move an assignment between installations.

Core uses existing validated registry operations and the encrypted registry CAS.
The same transaction preserves packages, private state, assignments and grants while
changing one enablement flag. Cancellation is checked before work and again before
transaction completion; a failed validation rolls back the staged update. A key check
also runs after reading the overview, so key loss during the read cannot return cached
settings. Vault instance/revision mismatches fail rather than overwriting newer state.

The overview deliberately omits private-state payloads, last invocation IDs, Tool/View
grant handles, Calendar IDs, conversation text and credentials. Only configuration
identifiers, package metadata and aggregate counts reach this screen. No schema
migration or plaintext configuration file was added.

## Ownership and response loss

The controller serializes registry jobs with sample turns and vault lifecycle work.
Switches are disabled while a job runs and update only after a confirmed response.
Configuration replies must match Person, vault instance and the next revision.
A conflict, invalid response or uncertain delivery clears the stale overview and
offers a read-only refresh instead of an optimistic toggle or blind retry.

Duplicate submits with the same native job ID and action return the owned job/result;
they do not dispatch another mutation. Reusing a stale revision in a new job conflicts.
The Dart gateway drains an uncertain prior job before refresh, so a lost mutation
response does not cause the change to be sent twice. Cancellation or delivery failure
after commit can still mean the change is durable; refresh is the reconciliation path.

Lock/navigation/inactivity clears presented registry state immediately and ignores late
read/configuration results. It drains the owned job before closing the vault, rather
than claiming to kill a blocked OS key call. A key-unavailable result or worker
interruption also clears the presented conversation and settings. Closing just the
settings dialog does not undo a submitted change.

## Automated evidence

- Three Core tests cover no implicit initialization, minimized overview output,
  preservation of state/grants through configuration and reopen, stale/unknown target
  denial, post-read key loss and rollback of a cancelled staged update.
- The native worker fixture checks locked/missing denial without provisioning, empty
  inspect, an explicit persisted enablement change, duplicate submission, cross-Person
  denial, stale replay, lock/unlock and actual Expert invocation denial afterward.
- A protocol test rejects raw grant/private-state input and unknown operation kinds.
- Eight Dart tests cover bounded parsing, native payload mapping, lost reply recovery,
  foreign responses, serialized/non-optimistic changes, conflicts, lock/late delivery
  and key/worker failures. The real Dart/C ABI missing-vault test also exercises registry
  inspect and verifies that it creates no vault directory or key.
- Three widget cases cover the panel-to-dialog flow, actual controller toggles at
  320 pixels/200% text and 520 pixels, Escape, empty state and refresh errors.
  `apps/client/test/goldens/agent_registry.png` uses loaded production fonts and was
  rendered and visually reviewed. No image-generation asset was used.
- All **172 workspace Rust tests** and **146 Flutter tests** pass, as do the separate
  keyring example's three tests and 25 native assertions. Flutter analysis, formatting,
  Clippy with the two pre-existing Calendar exclusions, the rebuilt Rust library,
  macOS Debug build and deep strict signature verification pass.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo test -p floe-core --example vault_keyring_smoke
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
bash tools/s3-validation/check-native.sh
cd apps/client
flutter test
flutter analyze
flutter build macos --debug
codesign --verify --deep --strict build/macos/Build/Products/Debug/floe_client.app
```

## Still required

This is enablement management for existing packages/assignments, not the complete
A3 installation/grant editor. Explicit package installation/version changes, new
Person assignments, dependency/scope details and durable Calendar scope bindings
remain to expose and govern. It must not be counted as complete registry acceptance.

The default panel still accepts only synthetic sample questions. Connecting the
Core Calendar turn host to native chat dispatch, model/destination selection and
proposal/recovery UI remains open. Production key provisioning, live local/remote
models, other Connectors and S1/S3 acceptance gates are unchanged. No live key creation,
personal chat, source access or Calendar write was performed for this checkpoint.
