# 04 residual closure: exact selection continuity and terminal fences

Baseline: checkpoint 04 implementation is present on `bab3f0f774b2d46414c69cdec8262840d9c7950f`. This document reopens **only** the six residuals found while reviewing that completed implementation. It does not redesign the 04 binding model and does not begin checkpoint 05 deletion/conformance.

The existing checkpoint-04 completion evidence remains valid for the surfaces it exercised. The closure exists because several edge paths did not carry the admitted selection all the way to the concrete settings/read/handoff/terminal-commit boundary.

The proposal-card golden exception remains unchanged. Keep `apps/client/test/features/actions/agent_proposal_card_test.dart` and `apps/client/test/goldens/agent_proposal_card.png` byte-identical to this baseline.

## Confirmed residuals on the baseline

| ID | Residual | Baseline evidence | Required closure |
|---|---|---|---|
| R1 | Hosted Calendar candidates have no remote execution-owner input | `expert_binding_settings.rs::current_candidates` prepares remote producer metadata only for Mail/Work/Logistics; `source_candidates.rs` requires `remote_execution_owner` for hosted Calendar. | Product candidate inspection must produce Google/Microsoft Calendar candidates from current Calendar connection + current pinned producer. |
| R2 | Native Calendar exact selection is widened back to the full current connection during read admission/reauthorization | `SelectedCalendarContextReader` starts from admitted refs, but `read_native_calendar_view -> admit_current_native_calendar_read -> connection_calendar_ids` uses every current Calendar resource. | Initial read and dependency reauthorization must admit the exact selected Calendar subset only. |
| R3 | Requirement key identity is lost below `read_requirement` | Host finds exact key, but remote reader and endpoint-built readers later find by capability; duplicate keys with one capability may use another requirement's selection. | The exact `AdmittedRequirementSelection.selected` must travel with the individual read call; no lower layer reselects by capability. |
| R4 | Expert binding fence does not run at the actual model provider handoff | App wrapper validates before and after `InferenceExecutor::execute`, while Inference performs profile planning/Access admission before `transport.generate`. | A generic Inference-owned handoff fence must be consumed immediately before provider transport and rechecked after response; binding refusal must not trigger model fallback. |
| R5 | Stateless successful Task completion lacks transaction-local binding revalidation | Stateful settlement checks current selection inside the Vault transaction; `settlement == None` goes through `compare_and_swap_task`, which does not. | Every new `Completed` transition must revalidate current admission/selection in the same Vault transaction, regardless of settlement payload. Historical completed replay stays exempt. |
| R6 | Remote candidate discovery failure prevents saved-binding display/removal | `current_candidates` must successfully observe remote catalog before saved refs are projected; `replace(candidate_ids=[])` also calls live discovery first. | Saved refs must remain inspectable as unavailable, and removal/exact lost-ack rejoin must not require live source discovery. New nonempty selection still requires fresh candidate resolution. |

These are checkpoint-04 correctness residuals, not checkpoint-05 dead-code cleanup. The caller-zero provider convenience method `ServerSourceClient::read_confirmed_interaction_view` remains a 05 deletion target.

## Exit state

This closure is complete only when:

1. hosted Calendar candidate inspection works through the actual App settings path;
2. saved remote selections remain visible/removable while live catalog discovery is unavailable;
3. every Expert requirement read carries its exact requirement key, capability/version and selected refs into Context/App acquisition without a capability-only lookup;
4. native Calendar reads and later dependency reauthorization use exactly the admitted selected resource subset;
5. Manager/product `SourceReader` current-selection behavior remains separate and unchanged;
6. Inference runs an owner-neutral execution fence after Access consumption and immediately before provider handoff, plus after provider response before the result leaves Inference;
7. Expert binding-fence rejection at that handoff performs zero provider payload calls and never falls through to another model candidate;
8. all successful Task completion paths, stateful or stateless, transactionally validate the stored Task selection against current Registry before writing `Completed`;
9. failed/cancelled/timed-out/interrupted transitions remain writable even when binding changed, so fencing itself can be durably reported;
10. completed historical replay still returns stored output after rebind/disable without source/model/runner execution;
11. the six permanent regressions use deterministic barriers/fixtures and are included in targeted verification;
12. 04 current architecture/status docs match the fixed runtime and 05 remains Not started.

## RC-0: executable baseline and regression tests

Before production edits:

1. record local/origin HEAD and clean/dirty state;
2. run the existing focused 04 suites that cover bindings, registered runners, Context remote/native reads, Inference, Vault Task and Flutter Expert settings;
3. add/reproduce permanent regressions for R1-R6 on the baseline behavior;
4. do not make a dedicated red commit if it would leave the branch intentionally broken; red tests may land atomically with their fixes.

The six required regression shapes are:

### R1 hosted Calendar candidate

Drive the real App candidate inspection owner path, not `discover_source_candidates` alone.

Fixture:

- Person/device with a hosted Calendar connection using Google or Microsoft;
- at least one selected calendar resource;
- current saved paired-server connection for the same Person/device;
- current pinned remote producer/execution owner.

Expected baseline finding: no candidate. Final expected result: one candidate per exact selected calendar resource, with the pinned producer as execution owner.

Do not obtain the Calendar account by scanning generic remote connectors at runtime; the product Calendar connection remains the target. Only the remote execution-owner metadata is missing.

### R2 native Calendar subset

Use a current EventKit connection with resources `A, B` and an Expert admitted selection containing only `A`.

Exercise the actual Context native-calendar read and subsequent dependency reauthorization. Final assertions:

- OS/source check is asked for `A` only;
- grant admission names `A` only;
- view/dependency resources name `A` only;
- adding B to the current connection does not widen A;
- later dependency authorization re-admits `A` only;
- B is never read, admitted or added to coverage.

### R3 duplicate requirement keys sharing one capability

Create a statically supplied test Expert manifest with two different requirement keys sharing the same capability and distinct saved refs:

```text
key_a -> mail.communication -> source A
key_b -> mail.communication -> source B
```

or an equivalent capability whose deterministic fixture can prove distinct targets.

The runner must call both keys through the normal `BuiltinExpertHost::read_requirement` seam. Each call must receive the payload/dependency from its own selected ref. There must be no `find(|requirement| requirement.capability == ...)` below the key-resolution boundary for Expert selected reads.

Use an App/product-host integration test if the existing remote test harness can expose distinct A/B markers without external services. Also add Context owner tests for the selection-carrying port itself.

### R4 model provider handoff

Use a deterministic fake/fixture provider with:

- a barrier during profile/preparation or immediately before handoff;
- a transport call counter/marker.

Allow the Expert's outer binding validation to pass, then rebind before the concrete provider handoff. Final assertions:

- the internal handoff fence observes stale binding;
- transport `generate` count remains zero;
- no alternate model candidate is attempted;
- the original fence failure reaches the Expert Task as a hard failure;
- model recipient/Access admission semantics remain unchanged.

### R5 stateless terminal commit

Create a real Vault Task under selection A, transition it to Working, obtain a valid stateless ExpertReport result, then rebind to B **after endpoint/final App fence but before durable Completed CAS**.

At the Vault/repository boundary, the Completed transition must return Conflict/stale failure without writing a successful result.

Also prove:

- a Failed transition after the same binding change can still be written;
- existing already-Completed Task replay remains available;
- stateful settlement keeps its current atomic private-state behavior.

A test-only TaskRepository wrapper may pause before delegating to the real Vault repository, but do not add a production timing hook. The invariant itself must be enforced by the real Vault transaction.

### R6 offline saved selection

Persist a remote selected ref, then make the paired source inventory unavailable.

Final assertions:

- candidate inspection returns the saved ref as `selected + unavailable` rather than dropping it;
- product UI can render that saved unavailable item;
- remove selection with an empty candidate list succeeds under exact assignment/package/definition/binding CAS without a live catalog;
- exact same lost-ack operation rejoins without live discovery;
- a nonempty replacement still requires a fresh live candidate ID and fails closed while discovery is unavailable.

## RC-1: candidate/settings continuity

### RC-1A: split saved binding context from live discovery

Refactor `crates/app/src/vault_host/expert_binding_settings.rs` so the immutable current owner context is loaded independently of network/source discovery.

Conceptually separate:

```text
BindingContext
  Registry instance
  assignment / installation / manifest
  exact requirement
  current binding entry/revision

LiveCandidateDiscovery
  current Context/Connections candidate metadata
```

The saved binding projection is derived from `BindingContext`. It must not depend on a successful provider catalog request.

`inspect` behavior:

- successful live discovery -> merge live candidates with persisted selected refs;
- transient/unavailable live discovery with persisted refs -> return those persisted refs as unavailable;
- never fabricate a different live target;
- foreign/invalid Registry identity remains a hard error.

If no saved ref exists and discovery itself is unavailable, preserve an explicit unavailable/error outcome rather than mislabeling it as a confirmed “no compatible source” result.

### RC-1B: hosted Calendar execution owner

When the requirement is `calendar.timeline`:

- EventKit uses the verified local device owner and needs no remote producer;
- Google/Microsoft hosted Calendar uses the current paired server admission plus current pinned producer execution owner;
- the Calendar connection/resources still come from the product Calendar owner, not generic connector enumeration.

Do not require `observe_source_connections` merely to rediscover a Calendar already selected by the Calendar product owner.

### RC-1C: remove without discovery

For `candidate_ids.is_empty()`:

1. load and validate exact `BindingContext`;
2. validate package/version/definition/requirement and expected binding revision;
3. call Experts `replace_binding` with an empty exact selection;
4. republish Directory;
5. return a product result projecting the new saved state even if live discovery remains unavailable.

For an exact lost-ack retry, compare the request against the persisted selected candidate IDs and operation receipt without live discovery.

For any nonempty selection, continue to re-enumerate live candidates and resolve every candidate ID freshly before mutation.

### RC-1D: Flutter recovery

Update the controller/UI only as needed so:

- a saved unavailable selection remains visible;
- Remove is possible from assignment/requirement state even when candidate discovery failed;
- Save of new targets is disabled/fails while live candidate discovery is unavailable;
- conflict reload semantics remain.

Do not expose technical `SourceSelectionReference` values.

## RC-2: requirement-key exact-selection seam

### RC-2A: Context selected-read port

Keep the existing generic `SourceReader` for Manager/product current-selection reads.

Add a separate Context-owned selected-read seam for Expert reads, for example:

```text
trait SelectedSourceReader {
  read_selected(request, selected_refs)
}
```

or an equivalent owner-clean API. The selected refs are explicit input. The reader must not own or search an entire `ExpertExecutionSelection`.

Extend `DeclaredSourceRequirement` (or replace it with one equivalent pure Context struct) so the exact requirement call carries:

```text
key
capability
contract_version
selected_refs
```

`read_declared_source` resolves by key once and then forwards that exact selected set. It never reconstructs selection by capability.

### RC-2B: local driver carries selected refs

Extend `LocalExpertSourceDriver::read` and the App driver so Calendar/Attention/Contacts/Wellbeing/Tasks/Memory receive the exact selected refs for that requirement invocation.

Remove endpoint construction logic that preselects the “first requirement with capability X”. Readers may hold source-owner services, but the requirement-specific target set arrives on each call.

For singleton native/intrinsic capabilities, require exactly one ref at the driver/owner boundary. For Calendar, preserve the admitted bounded multi-resource set.

### RC-2C: remote reader no longer searches capability

Refactor `BoundRemoteViewReader` into a selected-reader implementation that receives the exact refs from the caller.

Delete logic equivalent to:

```rust
selection.requirements.iter()
    .find(|requirement| requirement.capability == request.source().as_str())
```

Validate that every supplied ref matches request capability/version and pass only those refs to `read_selected_remote_view`.

The binding fence remains before and after the whole requirement read. Manager's unbound `RemoteViewReader` remains only for Manager/product-owned direct reads.

### RC-2D: native Calendar exact subset end to end

Change native Calendar read admission to accept an explicit expected calendar resource set.

The current connection is used to verify:

- same Person/device/provider/connection;
- selected resources still exist;
- source authority/current subject/grant are valid.

It is **not** used to replace the selected resource set with every current calendar.

Update `admit_current_native_calendar_read` or its replacement so the expected selected resource IDs are passed through:

```text
SelectedCalendarContextReader/read call
-> read_native_calendar_view
-> native source check
-> VaultNativeCalendarGrants::admit
-> admits_native_calendar_read
-> CalendarLeaseKey
-> ContextDependency.resources
```

All must use the same exact selected subset.

For `authorize_native_calendar_dependency`, derive the expected reauthorization resource set from the durable dependency/lease evidence produced by the original selected read. Do not use all calendars in the current connection. A current connection that merely gained another calendar must not widen the dependency.

Keep strict source/subject/revision checks; do not weaken adapter equality to make subset reads pass.

## RC-3: exact model-handoff binding fence

The current App wrapper is too coarse because it surrounds all of `InferenceExecutor::execute`.

### RC-3A: owner-neutral Inference fence

Introduce an Inference-owned, owner-neutral fence interface, conceptually:

```text
trait InferenceExecutionFence: Sync {
  validate() -> BoxFuture<Result<(), AgentFailure>>
}
```

Inference must not depend on Experts/Vault/App. App supplies an implementation that validates the current Expert admission/selection against Vault Registry.

Update `InferenceExecutor` so a caller can pass an optional fence/guard. Root conversation/Learner callers pass none unless they have their own legitimate guard.

### RC-3B: handoff location

Inside canonical Inference:

1. normal profile planning and exact-recipient Access admission continue;
2. consume the Access dispatch permit as today;
3. **after Access consume and immediately before `PreparedModelTransport::generate`**, validate the optional execution fence;
4. only then mark the model attempt dispatched and hand bytes to the provider;
5. after provider response, validate Access as today and validate the execution fence again before returning usable output.

A fence failure is a hard, no-fallback failure. Do not let auto profile routing try another candidate after a binding fence rejects. Preserve the original `AgentFailure` to the caller where practical; use an internal attempt classification rather than globally reclassifying unrelated provider `Conflict` failures.

Do not hold a Vault transaction across provider I/O.

### RC-3C: App integration

Replace or simplify `BindingFencedInferenceExecutor` so the same current-selection validator is supplied to the internal Inference handoff boundary. Avoid duplicate authority ownership.

Keep endpoint/read/report fences that catch changes at their own boundaries; the new handoff fence closes only the outbound model gap.

Add tests in both owners:

- Inference unit test: generic fence rejection after Access consume yields zero transport calls and no fallback;
- App test: a real Expert binding changes after the outer call starts but before provider handoff; zero provider payload calls and Task failure.

## RC-4: stateless successful terminal fence

### RC-4A: shared Vault helper

Add one transaction-local helper in Vault for “may this current Task be committed successfully under its admitted selection?”

It loads the current Registry inside the same transaction and validates:

```text
Task admission
Task immutable selection
current assignment/install/package/definition
current enabled state
current binding revision/digest
```

Reuse this helper from stateful settlement and stateless successful completion where possible.

### RC-4B: compare-and-swap success

In `compare_and_swap_task`:

- when transitioning a nonterminal Task to `TaskState::Completed`, call the current-selection helper before `write_task`;
- on binding/assignment drift, return Conflict/stale and do not persist the success;
- when writing Failed/Rejected/Cancelled/TimedOut/Interrupted, do not require current binding equality merely to persist the terminal failure.

Existing Completed Task reads/replays never call this success-admission check.

### RC-4C: repository flow

Keep `TaskRepository::settle` API if still adequate; the invariant belongs to the real Vault transaction, not TaskCoordinator timing.

Do not require an `EndpointSettlement` just to get the binding fence.

## RC-5: regression matrix and deletion gate

Mandatory final regressions:

| Scenario | Required final result |
|---|---|
| Hosted Google/Microsoft Calendar settings inspection | exact Calendar candidates produced with pinned remote execution owner |
| EventKit current connection [A,B], admitted selection [A] | source/grant/view/dependency/reauthorization use A only |
| Same capability, key_a=A and key_b=B | each read uses only its own selected refs |
| Rebind after Expert outer model fence, before provider handoff | provider generate count 0; no model fallback |
| Rebind after endpoint result, before stateless Completed write | success commit rejected transactionally |
| Same race but Task failure write | failure can be durably recorded |
| Saved remote binding + catalog offline | saved target visible unavailable; Remove succeeds |
| Nonempty replacement + catalog offline | mutation rejected/unchanged |
| Exact lost-ack removal retry while offline | rejoin succeeds without double revision |
| Historical Completed Task after rebind/disable | stored result replayed with zero execution |
| Manager direct source read | retains existing product-owned current selection and is not forced through Expert binding seam |

Residual search must prove:

```text
.find(|requirement| requirement.capability
calendar_selection = self.selection.requirements
selected: self.selection.requirements
admit_current_native_calendar_read(
connection_calendar_ids(
BindingFencedInferenceExecutor
transport.generate(
compare_and_swap_task(
current_candidates(
observe_source_connections(
```

Every match is classified. In particular:

- no Expert selected reader may search the whole selection by capability;
- native Calendar read/reauthorization cannot derive resource scope from the full current connection;
- a model provider handoff must have the generic execution-fence call immediately before it;
- new Completed Vault transitions must have transaction-local selection validation;
- remove-selection path must not require live remote discovery.

The caller-zero `read_confirmed_interaction_view` remains explicitly deferred to checkpoint 05.

## RC-6: verification, docs and status convergence

### Focused Rust gates

At minimum:

```sh
cargo test -p floe-context
cargo test -p floe-inference
cargo test -p floe-experts
cargo test -p floe-vault
cargo test -p floe-app registered_runner_ --lib -- --test-threads=1
cargo test -p floe-app vault_registry --lib -- --test-threads=1
cargo test -p floe-app first_party_observe --lib -- --test-threads=1
cargo test -p floe-protocol
cargo test -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
```

Run the concrete new hosted-Calendar/settings, duplicate-key, handoff-fence and stateless-completion tests explicitly as well. A zero-test filter is not evidence.

### Flutter gates

```sh
(
  cd apps/client
  flutter test test/features/experts
  flutter test test/features/conversation
  flutter test test/features/connections
)
```

Must include a saved-unavailable source/remove UX regression.

### Broad gate

Use the repository-qualified serial Rust full-workspace gate:

```sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi

(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)

git diff --check
```

Report this as serial Rust full-workspace validation, not default-parallel reliability. If parallel runs are performed, report them separately.

Go/server is not intended to change. This closure must not add the ConfirmedInteractions server route. If server code changes for an independently proven reason, run `go test -race ./...` and `go vet ./...` from `server/` and explain the scope expansion.

iOS is not required; report not executed if skipped. Android parity remains out of scope.

Verify the proposal-card golden assertion file and PNG are byte-identical to `bab3f0f774b2d46414c69cdec8262840d9c7950f`.

### Documentation/status

Update current architecture only for the fixed runtime facts:

- requirement key -> exact selected refs survives to the acquisition owner;
- native Calendar selected subset remains exact through read and dependency reauthorization;
- Expert binding fence is checked at actual model provider handoff;
- every new successful Task terminal commit is transactionally fenced;
- saved binding inspection/removal does not depend on a live remote candidate catalog;
- hosted Calendar candidate settings use current pinned producer identity.

Preserve the original checkpoint-04 evidence and append closure commit/test evidence.

Only after RC-0 through RC-6 pass, restore 04 to `Complete` in the plan README. Leave checkpoint 05 `Not started`.

## Recommended commits

Use these only when intermediate trees are coherent:

1. **04-RC1** — candidate/settings continuity: hosted Calendar + offline saved-binding/remove;
2. **04-RC2** — requirement-key selected-read seam + native Calendar exact subset;
3. **04-RC3** — Inference provider-handoff execution fence;
4. **04-RC4** — stateless successful terminal transaction fence;
5. **04-RC5** — cross-owner race/product regressions and residual cleanup;
6. **04-RC6** — broad verification, architecture/status convergence.

Combine adjacent steps instead of introducing temporary fallback readers, old/new Inference APIs or duplicate terminal paths.

## Required completion report

Report:

1. starting local/origin HEAD and worktree;
2. actual RC1-RC6 commit SHAs;
3. hosted Calendar candidate construction and product-path regression;
4. saved remote binding behavior while catalog is unavailable, including remove and lost-ack retry;
5. final selected-read API and proof requirement key is not collapsed to capability;
6. native Calendar exact resource flow through initial read and dependency reauthorization;
7. Inference fence type, exact pre-handoff/post-response call locations, and zero-provider-call race proof;
8. stateless Completed transaction fence and proof failure writes/historical replay remain valid;
9. Manager current-selection path preserved separately;
10. residual search classifications, including caller-zero ConfirmedInteractions convenience deferred to 05;
11. exact focused/broad/Flutter commands and results, ignored tests and serial-vs-parallel qualification;
12. golden byte comparison;
13. current architecture/status docs changed;
14. final local HEAD, clean worktree, no push, and checkpoint 05 still Not started.
