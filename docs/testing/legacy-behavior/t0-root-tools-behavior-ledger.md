> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 root/tooling behavior ledger

Baseline `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Static extraction only, no tests/compiler/build/formatter run. D=durable safety,H=product/tooling hypothesis,O=obsolete representation,S=support. Source hashes and exact test declaration/standalone entrypoint anchors are in t0-root-tools-source-index.json. Narratives enumerate table/branch cases; emitted historical "passed" strings are not observed results.

## Python unittest suites

### tools/architecture/test_check_boundaries.py

Actual unittest registrations:10,not11 claimed in preparation plan. Dependencies:check_boundaries production functions/subprocess CLI,json,tempfile. run_check builds synthetic Cargo manifests/policy in temp root and parses checker JSON;no Cargo/compiler execution. D means dependency enforcement remains required;exact old package/name assertions can change.

- RT-P01 `DependencyExtractionTests.test_workspace_alias_and_target_build_are_production_edges` (D):workspace runtime alias resolves floe-runtime;target-unix build alias generator resolves floe-generator;ordinary fixture dev dependency separately returns fixture. Production ordered list excludes dev.
- RT-P02 `DependencyExtractionTests.test_target_specific_dev_wiring_is_separate` (D):target cfg(test) dev fixture workspace alias resolves floe-fixture;production dependency list empty.
- RT-P03 `GraphTests.test_dag` (D):a→b with leaf b yields no graph errors.
- RT-P04 `GraphTests.test_cycle` (D):a→b→a yields errors.
- RT-P05 `GraphTests.test_transitive_forbidden_path` (D):a→b→c with forbidden a→c yields errors even without direct a→c edge.
- RT-P06 `CliTests.test_default_repo_and_policy_paths_work` (H):invoke checker without arguments from system temp directory;exit0,JSON mode final,nodes>0. Source-relative defaults must not depend on current directory. This historically checks actual repository graph;not executed during T0.
- RT-P07 `CliTests.test_dev_wiring_is_reported_but_not_gated` (D):synthetic kernel manifest with ordinary fixture and unix platform-fixture dev deps/custom one-node policy;exit0,dev_wiring exact two entries. Test topology is reported separately from production gate.
- RT-P08 `CliTests.test_target_crate_cannot_depend_on_unmigrated_legacy_crate` (D/O):Conversation target depends on agent-contract found at obsolete crates/floe-agent-contract;checker nonzero and unmigrated/legacy error. New final graph still rejects wrong paths;obsolete path need not remain special-case code.
- RT-P09 `CliTests.test_final_mode_requires_all_target_crates` (D):empty synthetic workspace/default policy→nonzero with missing target crate.
- RT-P10 `CliTests.test_target_manifest_must_be_a_workspace_member` (D):valid kernel manifest on disk but empty workspace membership/custom policy→nonzero with exact target-not-member error.

### tools/validation/test_test_fixtures.py

Four registrations. setUp temp source/cache;compile fake copies source bytes and counts calls. cached_build real file locking/hash/publication logic with compiler subprocess mocked. Keep cache semantics in later rebuilt infrastructure only if still needed.

- RT-P11 `FixtureBuildTests.test_same_inputs_build_once_and_missing_output_rebuilds` (D/H):same source/toolchain twice returns same path with1compile;unlink output then rebuild→2compiles.
- RT-P12 `FixtureBuildTests.test_source_toolchain_and_modified_output_invalidate` (D/H):initial build;tamper output→rebuild;change source→different artifact path;change toolchain→another path;total4compiles. Stamps alone cannot trust modified binary.
- RT-P13 `FixtureBuildTests.test_failure_does_not_publish_an_artifact` (D):mock compiler raises CalledProcessError;exception propagates,no */server,*/sha256,*/build-* entries published.
- RT-P14 `FixtureBuildTests.test_concurrent_callers_share_one_complete_build` (D):two ThreadPool callers while first fake compile blocked;release within5s;both get identical path and compile count1. Lock protects complete immutable artifact publication.

### tools/validation/test_native_build.py

Six registrations. Temp path intentionally contains spaces;fake executable emulates xcrun/swiftc/install_name_tool/codesign/lipo and writes operation log;native_build.sh executed via zsh but real toolchain never used in these tests. Model.swift,bundle output,SDK/sign/build scripts are controlled fixtures. Production native_build.sh must remain.

- RT-P15 `NativeBuildTests.test_unchanged_build_skips_compile_copy_and_sign` (D/H):build twice;second stdout unchanged;operation log exactly swiftc,install_name_tool,codesign once.
- RT-P16 `NativeBuildTests.test_source_flags_sdk_identity_and_script_invalidate` (D/H):initial then source change,Swift6flag change,SDKROOT change,sign identity change,build_native.sh change;swiftc count6. Each affects cache identity.
- RT-P17 `NativeBuildTests.test_missing_or_modified_output_rebuilds` (D):initial build,unlink output,rebuild,tamper output,rebuild→swiftc3.
- RT-P18 `NativeBuildTests.test_failed_build_preserves_output_and_does_not_mark_fresh` (D):initial output snapshot;source fail makes fake Swift exit9;old output bytes intact. Repair source/rebuild contains repaired;no derived/floe-native/build.* leftovers.
- RT-P19 `NativeBuildTests.test_embed_tracks_each_architecture_and_skips_unchanged` (D/H):two-input embed twice then modify second architecture;only2lipo operations,showing unchanged skip and all-input hash.
- RT-P20 `NativeBuildTests.test_single_library_embed_tracks_source_content` (D/H):one-input embed twice then source change;codesign count2.

## tools/validation/LocalModelHostTests.swift standalone async main

Dependencies:actual LocalModel.swift types/ABI,synthetic LocalModelHost closures,Gate actor controlling completion,terminal/waitForStart bounded200×5ms polls. OS26availability annotation;no real Foundation generation is used. S1 will replace role-specific selection/discovery/learner seams;do not infer actual model quality from synthetic host tests.

- RT-L01 (D/H):five UTF-8 stable-instruction lengths4097,8192,8613,9216,9217 constructed using Korean syllables+ASCII;byte count exact. Valid iff≤localModelMaxStableInstructionsBytes;accepted starts Pending,generator receives exact text,terminal Done,then release;over-bound fails invalid_input without generation. Exact limit remains contract review.
- RT-L02 (D):unavailable apple_intelligence_not_enabled returns model_unavailable with exact availability;generator fatalError guard proves it must not be called.
- RT-L03 (D):available start→Pending→Done/Synthetic answer;identical request ID/start rejoins Done;different ID while occupied→conflict;poll unknown ID→not_found;release original→released.
- RT-L04 (D):maxOutputBytes1 starts Pending then terminal budget_exceeded;release allowed.
- RT-L05 (D):block generator at gate;explicit cancel returns Pending;after gate completion terminal cancelled with no step;release. Cancellation suppresses late answer.
- RT-L06 (D):cancel blocked work then release observer→released;another start still conflicts while generator physically alive. After gate finishes,eventual new start becomes Pending. Release cannot free execution capacity prematurely.
- RT-L07 (D):deadline50ms with generator gated;after80ms poll remains Pending and new request conflicts;finish gate→deadline_exceeded;release. Deadline does not imply old physical work already stopped.
- RT-L08 (D):availability JSON with unknown secret field fails strict decoding;exact availability JSON succeeds. FFI nil pointer/0length returns invalid_input. Secret-bearing availability buffer returns invalid_input without echoing marker;all returned pointers freed.
- RT-L09 (D/H):canonical run_frame discovery with one capability→one native tool;one Expert→one delegation tool;empty lists→empty tools. Malformed JSON and {}reject. Legacy scoped_instructions tool discovery throws invalid_model_output;legacy learner-looking scoped instructions classify general.
- RT-L10 (D/H):currentUserRequest chooses exact current_turn User text despite following Tool;old conversation.current_turn nesting returns nil.
- RT-L11 (D/H):four user texts(ordinary inspect,English do-not-use-tools,Korean do-not-use-tools,translation of quoted do-not-delegate) with same canonical catalog all advertise exact floe_capability_0/floe_delegate names and equal schemas. Natural language cannot mutate authority/catalog;this does not force tool use against user intent.
- RT-L12 (H/O):learner detection true only governed-memory-review purpose plus empty tools/Experts. Same purpose with capability→false/denied;with Expert→denied. Everyday purpose plus learner phrase→false;unknown-role empty catalog→general. Target uses common role-neutral runtime/explicit purpose;preserve no unauthorized learner tools without name-dispatch coupling.
- RT-L13 (D/H):nil learner proposal encodes schema1/proposal null. Valid Preference/Fact/confidence1000/timeless Alex+claim encodes statement "Alex: prefers afternoon meetings",target/base null,observed_at epoch1970-01-01,valid dates null. Epoch sentinel is legacy shape,not real observation evidence.
- RT-L14 (D/H):blank subject and blank claim each reject. Starts invalid date rejects,valid Sept14 accepts. Expires Sept15 accepts,invalid date rejects. Range equal endpoints rejects;Sept14→15 accepts;fractional six-digit valid timestamps also accept. No real memory mutation is exercised.

## tools/validation/calendar/native-tests.swift standalone main

Dependencies:actual EventKitActions.swift Proposal/localConflict/calendarViewAccess and C ABI;synthetic permission/lookup/generation closures. No OS permission/event access is intended by these paths. Literal-person restriction is obsolete and must become verified host identity.

- RT-C01 (D):whole-second proposal passes writable precision. For each starts_at and ends_at with .500seconds,precision validation throws. Each fractional case through preflight and create ABI returns provider_unavailable before Calendar effect (2×2cases).
- RT-C02 (D/H):UTC interval across NewYork DST remains3600seconds;execution marker URL includes execution ID. UTC+09:00 timezone metadata resolves to TimeZone.
- RT-C03 (D):overlap07:30–08:30 versus07–08→true;adjacent08–09→false;soft-deleted overlap→false;floating all-day Mar8–9→true across DST.
- RT-C04 (O/D):random foreign person Proposal throws;hard-coded accepted person is obsolete,actor isolation remains. Capabilities ABI reports writes_enabled=true(default-build availability,not permission).
- RT-C05 (D):update proposal retains exact external-event| identity;own original excluded from conflicts;another overlapping event still conflicts;delete mutation sets deleting=true.
- RT-C06 (D):valid Calendar access request(own person,EventKit,device/connection/revision1,unsorted second/first,live deadline) checks permission twice,returns sorted first/second and stable generation. Source set not widened.
- RT-C07 (D):five independent input mutations schema2,foreign person,empty IDs,duplicate first/first,past deadline each throw before permission closure touched.
- RT-C08 (H/D):eleven current Calendar IDs accepted and sorted;old ten-item grant assumption must not limit native inventory.
- RT-C09 (D):generation changes between checks→throw with exactly2generation reads. First permission denial→throw before inventory lookup. Second permission denial after lookup→throw with exactly2checks. No stale payload release after withdrawal.

## Support files and retained tools

- `tests/composition/main.rs` (S):five comment-only lines,no executable declaration/assertion/harness. Safe deletion candidate after manifest/reference scan;not counted as test.
- `build_test_fixtures.py` (S):cache hashes exact command/toolchain/cwd,source bytes and builder itself;Go dependency/embed files and env/CGO/SDK identity;Calendar Swift+SDK inputs. Exclusive per-key flock;compile private temp then atomic artifact/checksum publication;dylib install-name/ad-hoc signing. Its executable consumers are only old provider native/live-server suites,App native fixture test,client pairing/calendar launchers and Python tests. Remove only with every consumer documented/removed;never execute during T0.
- `calendar/ResponseLoss.swift` (S,external-effect danger):serial locked forwarding shim. Invalid JSON returns uncertain_result. Create allowed only exact disposable title "Floe S3 — disposable" and calendar "iCloud · Floe Validation". Load exact sibling real dylib;unavailable symbols→provider_unavailable. Forward real call/free its result;successful create data intentionally converted to timeout,other replies passed through. This is failure injection for a real write,not permission to execute or delete provider data. Its ignored App test owns behavior evidence.
- `calendar/check-native.sh` (S):compile real EventKit adapter+standalone synthetic assertions in private temp directory,execute,cleanup temp. `check-local-model.sh` (S):compile/run synthetic Swift host,then provider/inference Rust tests. Both obsolete suite launchers can retire only after ledger;product adapters stay.
- Test-only native-calendar-fixture/product-conversation Info.plist files are consumer-bound launch metadata;retire only with covered client launchers. Retain local-model-smoke/vault-smoke/CLI plist and opt-in runners.
- KEEP `check_boundaries.py`,module dependency policy,production native build helpers,`calendar/core-check.c` C ABI diagnostic,`calendar/inspect.swift` exact external-effect recovery tool,run-local-model-smoke/run-vault-keyring-smoke and scripts launch/reset entrypoints. No retained diagnostic executed;reset script never validation.
- KEEP shared expert-report,manager-guidance and both remote-authorization JSON corpora until final contract/consumer audit. Their old assertions are evidence,not target requirements;cryptographic/provider versions are not globally renamed.
