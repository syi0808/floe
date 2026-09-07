# S4 Manager-to-S3 Calendar action bridge

Date: 2026-09-07. Core integration with synthetic fixtures; S4 remains **0/14**.

Follow-up: the [bounded Calendar Timeline adapter](s4-calendar-timeline.md) now feeds
this bridge in a Core/Expert/vault fixture test. Production Manager orchestration
and live native/source verification remain open.

## Authority and input boundary

The trusted Core Manager can now call `prepare_expert_calendar_action` with a
Person/session/invocation reference, a host-selected Calendar destination/revision
and display timezone, cancellation and a deadline. It accepts no supplied Expert
JSON, title, approval, policy, execution ID or provider receipt. It is not exposed
as a model/Expert capability or a new C ABI command.

The bridge loads the paired Expert result from the encrypted vault and requires
its durable receipt to bind the same session, invocation and assignment. The common
registry validates the result shape, recorded state and currently enabled grants.
Exactly one explicit focus proposal is required; briefing insights cannot silently
become action requests. Ordinary vault session CAS now also preserves existing
messages and historical classifications, preventing an old receipt from authenticating
a rewritten result. Copying a result into another session does not copy its authority.

A vault immediate transaction holds registry configuration stable during publication.
Cancellation, a maximum 30-second preparation deadline, source expiry, clock rollback,
destination revision, disconnect and source errors reject new publication. Native
key calls still require the existing worker isolation; this does not claim an OS
key dialog can be forcibly interrupted by an async deadline.

The Manager creates the existing S3 immutable Calendar action with a fixed `Focus
time` title and only the proposed interval. It reads durable Person action authority:

- `ask`: pending action for the existing Review surface.
- `allow`: approved delegated action, with automatic origin recorded by Core.
- `deny`: blocked action for Activity, with no execution.

Preparation never invokes a Calendar provider. Subsequent execution uses the existing
S3 executor, policy, fresh Calendar preflight, timezone/conflict checks, one-shot
execution ledger and lookup-only uncertain-result recovery. For Agent-origin work,
Core rereads durable authority before and after provider preflight: automatic approval
cannot survive an `allow` → `ask` or `deny` change, and `deny` blocks reviewed work too.
Direct user Calendar actions and existing origin-less records keep their behavior.
The host supplies the timezone; authoritative timezone validity remains an S3 provider
preflight gate rather than a new partial timezone parser in the Manager.

## Stable publication and privacy

The bounded contract allows one action proposal per Expert invocation. That invocation
UUID is the stable action ID; the first successful insert owns its execution ID.
Exact retries return the existing action, including approved, rejected, blocked,
unknown or succeeded states. A changed destination/timezone/revision or provenance
conflicts instead of creating a second intent or resurrecting a terminal one.

The encrypted source vault and existing Calendar action store are separate databases.
There is no claim of an atomic transaction across them. If a timeout, dropped future
or key loss occurs after the action insert, the action can already be durable. Reopen
and reconcile its stable ID; never generate a replacement. A concurrent publisher
can receive a storage/lock conflict and must retry the same reference.

Only the minimized action projection and versioned opaque provenance are placed in
the existing S3 ledger: instance/session/invocation/assignment/View IDs, package version,
state revision, data class and automatic-origin flag. Conversation text, source titles,
raw result JSON, credentials and provider-native objects are not copied. This does not
make the existing Calendar ledger encrypted or hide its linkage metadata.

Synthetic evidence can target only the fixture provider. The ordinary Personal
projection can target EventKit; temporary AI context, highly sensitive data, raw data
and credentials cannot use this conversion. Sensitive-domain projections still need
their own explicit minimization/policy boundary. No production personal data was used.

Flutter retains optional scoped/versioned attribution, shows `Suggested by Floe`,
and keeps package/version, conversation and call IDs inside Technical details. This
presentation does not grant approval or execute work. Legacy actions omit attribution.

## Verified evidence

Fifteen new Core tests use the real deterministic Schedule Expert, encrypted Turso
vault and existing S3 methods with injected keys and fake Calendar providers:

- Committed result → pending Review → approval → one execution; both databases
  close/reopen, retain provenance and return the same action on retry.
- Domain-owned allow/ask/deny; automatic authority revoked during provider work;
  Calendar conflicts, invalid timezone and unsupported origin versions block writes.
- Response-loss recovery looks up the existing execution and never repeats create.
- Foreign/missing/copied references, absent receipts, rewritten history, missing
  explicit proposals and revoked grants cannot mint actions.
- Synthetic-to-live and sensitive-class routing denial; fictional data labeled
  Personal exercises the same EventKit-shaped path without accessing EventKit.
- Cancellation, elapsed deadline, expired source and clock rollback before publication;
  concurrent stable-ID publication, dropped registry scope, pre/post-publication key loss
  and cancellation after an action has already become durable.

The common registry test also checks recorded-result authorization before/after a
commit and after revocation. All **136 workspace Rust tests** and the separate
keyring example's three tests pass; formatting and Clippy pass with the two existing
Calendar exclusions. Native builds are validated separately from live key/model gates.

Two Flutter parser/presentation tests cover optional attribution, foreign/unsupported
metadata and a 320-pixel/200%-text Review dialog with expandable technical provenance.
`agent_action_review.png` uses the actual bundled fonts, not placeholder test glyphs.
The rendered golden was reviewed; at narrow widths and large text, the dialog places
Close above the heading so it does not split words to reserve button space. Text
scaling is preserved. All **135 Flutter tests**, the analyzer, native library/macOS
Debug builds and deep/strict app signature verification pass.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo test -p floe-core --example vault_keyring_smoke
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
cd apps/client
flutter test
flutter analyze
flutter build macos --debug
```

## Still required

The default encrypted sample panel still produces briefing-only synthetic results;
it does not create real Calendar actions. A [Core Calendar turn host](s4-calendar-turn.md)
now orchestrates model/Expert results and this bridge. App integration still needs native
worker dispatch, live authorized Timeline evidence, model/chat proposal flow,
destination selection and response-loss reconciliation in the app. Stored origin IDs
are disclosed, not yet navigable links back to a conversation.

No real Calendar create, live model conversation, production key provisioning or
personal dogfood is claimed. The existing [keyring entitlement gate](s4-keyring-live-smoke.md),
[native model availability gate](s4-local-model.md), live Connector work, broad bounded
execution/trace/replay evaluation and S1/S3 acceptance prerequisites remain open.
