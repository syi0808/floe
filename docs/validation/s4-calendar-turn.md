# S4 Calendar Agent turn orchestration

Date: 2026-09-07. Core integration; S4 remains **0/14**.

## Connected path

`FloeCore::run_calendar_agent_turn` runs the existing `AgentRuntime` with a supplied
`ModelRunner`, the encrypted vault, the persisted registry and a Core-backed Calendar
View. It accepts ordinary versioned `AgentCommand` text, not a sample prompt enum.
The application must supply the inference decision/context, an explicitly authorized
assignment and immutable Calendar lease, budget and optional Calendar destination.
These are trusted host configuration, not model-controlled permission arguments.

The host does not install packages, enable assignments, broaden calendar scopes,
refresh imports or request OS permission. The assigned Expert and its required Tool
must already be enabled for the same Person/View. Their resolved data class must
match both the Calendar provider projection and the inference decision. Missing or
disabled configuration is rejected before any model or source call.

The registry publishes the resolved read-only Expert descriptor and a bounded input
JSON schema. The optional `input_schema` field accepts older descriptor payloads
without the field; existing sample descriptors remain unchanged on serialization.
The common model instructions explain that the call's input string encodes a value
matching this schema. Native model prompt serialization preserves the descriptor.
The host independently parses `ExpertInput`, rejecting unknown fields and enforcing
the existing focus-duration limits. The model cannot supply calendar IDs, grants,
approval, an execution receipt or an executor command in this input.

## Commit and inference boundary

1. The runtime persists the User/active-turn pointer in the encrypted session.
2. A model may call the registered Schedule or declarative Expert through the same
   bounded capability port. The Expert reads only the granted Calendar View.
3. Before committing a successful result, a validation callback runs inside the
   vault's existing immediate transaction, after staging the registry/receipt writes
   but before updating the session. It rechecks native read access, mirror revision
   and freshness, cancellation and deadline. Failure rolls back the receipt, private
   state and result together; no `MessageCommitted` event is emitted for the result.
4. The host checks the current durable registry revision, key and View lease before
   and after each model call. Final Assistant commits revalidate the lease in the
   same session transaction boundary. A later failure does not rewrite previously
   committed observations, but prevents a new answer from becoming committed fact.
5. Prior-turn successful capability outputs are replaced with typed `StaleContext`
   results only in the model request. The persisted transcript is unchanged. A new
   turn must obtain new source evidence rather than replaying an old tool result as
   current. Historical User/Assistant text remains conversation history, not proof
   of current source state. This is not yet full layered retrieval/compaction.

If no View could be read, the model receives the typed capability failure and may
explain the missing source; it never receives a fabricated empty schedule. If a
previously consumed View becomes invalid, the turn halts instead of continuing with
stale evidence. A concurrent registry edit wins the CAS and can leave an interrupted
pointer for explicit recovery, rather than being overwritten by a halt commit.

There is no transaction spanning OS permission state, the Calendar mirror and the
vault. Revalidation is a checked observation, not a lock on EventKit or proof of
changes since import. The [Calendar View limits](s4-calendar-timeline.md) still apply.

## Proposals, stop and recovery

Only a completed turn with a host-selected destination automatically prepares its
current-turn explicit proposals through the existing Manager-to-S3 bridge. Briefing
insights and prior-turn proposals are not silently published. Authority still routes
to Review, delegated approval or blocking; this API never executes a provider write.

Publication failures are returned alongside the completed session and the stable
session/invocation reference. They do not roll back the conversation or imply that
an action cannot already be durable. After uncertain publication, reconcile the same
action ID and exact destination through the existing bridge; do not rerun the model
to invent a replacement. The app still needs that recovery flow and conversation links.
The runtime's `Finished` event describes the conversation outcome, not the later
publication outcomes; native dispatch must await the enclosing result before releasing
ownership or reporting proposal preparation as finished.

Every turn has a child cancellation token. Stop is forwarded to it, and dropping the
turn or finishing it cancels owned model delivery without cancelling the caller's
parent token. Synchronous parent cancellation is checked at mutation/publication
boundaries as well, so an immediately returning model cannot outrun Stop. Runtime
budgets cannot exceed the existing defaults, including the 30-second turn deadline.
Pending model calls and View checks have explicit deadline/cancellation guards.

Dropping an in-flight turn preserves the existing interrupted-session contract.
Recovery changes the outcome without rerunning model, Expert or Calendar actions.
Key loss remains latched unavailable until explicit reopen. Synchronous OS key calls
still require the existing owned worker; an async deadline cannot kill a key dialog.

## Automated evidence

- Fifteen Core tests exercise the real runtime/Expert/vault/Manager path using
  fictional Calendar mirrors, injected keys, access stamps and a scripted model.
- Cases include successful Review preparation, reopen/multi-turn history, absent
  permissions, result-transaction rollback, answer-commit denial, publication failure,
  scope/class/registry/policy denial, malformed model input, pending and synchronous
  cancellation, deadlines, dropped ownership, expiry, key loss and registry revocation.
- An EventKit-shaped fixture exercises Personal classification with encrypted storage
  and no synthetic fallback. It does not access EventKit or prove production key use.
- A registry test checks the resolved input schema, older descriptor decoding and
  Tool revocation. The native model transport fixture checks schema preservation.
- All **167 workspace Rust tests**, the separate keyring example's three tests and
  **25 native assertions** pass. Formatting and Clippy pass with the two pre-existing
  Calendar exclusions. All **135 Flutter tests** and Flutter analysis pass.
- The rebuilt Rust library, macOS Debug build and deep strict app signature
  verification pass. No Flutter UI or golden changes were needed for this Core host.

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

## Remaining app and live work

The default panel is unchanged: preset synthetic turns, not connected personal chat.
This Core path is not exposed through the app's native worker/C ABI yet. The app now
has [registry enablement management](s4-registry-management.md), but durable scope
binding, new installations/assignments, explicit destination/model selection, Calendar
refresh, proposal/recovery UI and actual local/remote generation remain to connect.
The current native local model adapter deliberately still denies Personal input until
the production vault/key and model gates pass; this change does not loosen it.

No live source, personal conversation, OS permission request, provider write or new
keyring operation was performed. S1/S3 acceptance, live key/model gates, supported
remote authentication, other Connectors, transfer capture and trace/replay evaluation
remain open. Component fixtures do not establish full A1/A4/A5/M1/C1 acceptance.
