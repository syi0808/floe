# S4 bounded Expert and registry foundation

Date: 2026-09-07. All source data in this checkpoint is synthetic.
S4 remains **0/14**; this is preparatory runtime and sample-path integration.

## Registry contract

`floe-agent` now provides versioned package, installation and Person-assignment
records. Tool and Expert references use distinct kind-tagged namespaces; their
implementations and permitted dependencies are validated separately. Packages
are immutable at a registered ID/version. An upgrade installs a separately pinned
version rather than changing existing assignments or inheriting grants silently.
New installations and assignments start disabled and require explicit enablement.
All registry mutations use an expected global revision and checked increments.

The current built-in Tool implementation is TimelineRead. Expert implementations
are native Schedule and a bounded declarative FindFocusWindow rule. Unsupported
implementation/schema combinations, raw/credential Tool classes and incompatible
dependencies are rejected. There is no arbitrary code, native symbol loading,
Wasm, package-owned scheduler, credential field or network API in this contract.

Grants bind an Expert assignment to an exact-version Tool assignment belonging to
the same Person and to at most four View handles. The Tool must itself hold the
requested View grant. Both the Tool and Expert installations/assignments must be
enabled. Private state is reached only through the instance → Person → installation
→ package/version → assignment namespace. Its initial schema tracks completed
invocations, a revision and the most recent invocation ID, not conversation content
or authoritative Memory. Foreign-Person state reads fail.

Registry snapshots are versioned and validated on restore: instance, duplicate
identities, package versions, grant edges, state shape and collection bounds are
checked. The limits are 64 packages, 128 installations and 256 assignments.
Snapshots are **not yet backed by a durable/encrypted registry store**. The sample
host creates a fresh synthetic registry per turn; snapshot round-trip tests do not
prove restart persistence of installations, grants or private state. Only the
completed Expert result is currently durable as part of the Agent conversation.

## Common Expert invocation

Native Schedule and declarative fixture execute through the same
`ExpertInvocation` / `ExpertResult` and `ExpertHost`:

1. Check API version, instance, Person, assignment, registry revision, enabled
   dependencies, granted View handles and allowed output data classes.
2. Issue one bounded Timeline View read through the granted Tool boundary.
   The View request carries Person/handle, deadline, cancellation, byte/item limits.
3. Validate returned identity, class, freshness, source metadata, unique evidence,
   intervals and projection limits before running Expert logic.
4. Compute structured commitments, focus opportunity or no-focus evidence. Busy
   intervals are sorted and overlapping intervals are merged when finding a gap.
5. For an explicit ProposeFocus input, return a structured time-window proposal.
   This is advice only: there is no executor, Calendar ID or approval authority.
6. Recheck deadline, freshness, grants and registry revision before atomically
   advancing in-memory private state and returning the result.

The result includes package/version, invocation/assignment/Person/instance IDs,
View/source provenance, data class, expiry, view-call usage and state revision.
The granted source title is carried as `untrusted_title`; it is never interpreted
as instructions or a permission grant. There is no hidden-reasoning output.

The current deterministic profiles read one View, at most 32 items and a one-day
range. View and result sizes are capped at 16 KiB; source metadata and untrusted
titles are bounded; there are at most eight allowed insights and one focus proposal.
The effective deadline is the caller's deadline or 30 seconds, whichever is earlier.
These Experts do not invoke a model or recursively invoke another Expert.

Cancellation/timeout/dropping an invocation drops the pending read and cancels
the child token; a View provider must honor that token if it owns a separate worker.
Failed or oversized reads/results do not update state. Registry changes during a
read invalidate its result, even if a revoked grant was re-enabled before return.
The latest completed invocation ID cannot immediately be reapplied. This is not
a durable arbitrary-history idempotency ledger or replay archive.

## Sample and presentation integration

The existing `fixture.schedule.read` capability now invokes the registered native
Schedule Expert over a minimized synthetic View rather than returning a hard-coded
source string. Its structured result is stored as the existing paired capability
message, so Agent stop/retry/restart semantics and the encrypted default sample
route continue to use their original contracts. A source failure produces an
unavailable-source answer rather than a fabricated successful briefing.

Flutter validates and renders synthetic Expert evidence as source attribution,
commitment and possible-focus lines instead of exposing internal JSON and UUIDs.
It checks call/Person identity, version, class, permitted fields, size and intervals.
Malformed, personal or unsupported structured payloads do not render as sample
evidence; legacy plaintext synthetic records remain readable. The renderer accepts
briefing results, not executable proposals or arbitrary widgets. Sample clock
values are explicitly labeled as sample time, not real event timezone evidence.

## Verified evidence

- Seven Rust Expert tests cover native/declarative parity, isolated private state,
  explicit proposal output without mutation, untrusted source titles, exact grants,
  wrong Person/instance/View/class, disabled dependencies, schema/upgrade/snapshot
  validation, stale/oversized input/output, overlapping schedules and immediate replay.
- Controllably blocked View tests cover cancellation, deadline, dropped futures,
  revocation/re-enable while running and success of an independent later invocation.
- Core tests decode the actual Expert result from a committed Agent message and
  verify its call/Person/package provenance; that message survives database reopen.
- Actual Dart → C ABI → Rust tests decode Expert source content and preserve the
  result through core restart. These do not provision or read production keys.
- Three Dart parser tests and a 320-pixel/200%-text widget test cover structured
  evidence, invalid payloads and readable source disclosure. The new
  `agent_expert_source.png` golden is rendered and visually reviewed.
- All 108 workspace Rust and 133 Flutter tests pass. Flutter analyzer, formatting,
  native Rust build and Clippy with the existing two Calendar exclusions pass.
  The macOS Debug app builds and passes deep/strict signature verification.

## Remaining acceptance work

Next, add the durable registry/private-state store and explicit lifecycle/upgrade
handling; bind configuration, trust/permission management and assignments into the
host/app instead of rebuilding fixture state. Registry persistence must retain the
personal-data encryption/key-unavailable boundary, not add a plaintext fallback.

The focus advice still needs Manager conversion to the existing S3
Review/Policy/Validation/Executor path. Declarative configuration breadth,
Communication/Health Experts, live bounded Connector Views, trace/replay archives,
real-model evaluation and all previously recorded live key/model/source gates
remain open. No personal data, real Calendar operation, Memory or voice is enabled.
