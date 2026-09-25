# 05: obsolete-path deletion and extensibility proof

Prerequisite: 04 complete. Deletion is required in every earlier checkpoint; this is the final conceptual audit, not permission to leave obsolete callers until the end.

## 05-A: remove stale ServerSourceClient APIs

Inspect `crates/adapters/providers/src/sources/server.rs` and every production/test caller. Baseline convenience methods include `read_communication_view`, `read_work_context_view`, `read_calendar_context_view`, `read_confirmed_interaction_view`, `read_logistics_view`, `read_people_view`, `read_attention_view`, and `read_wellbeing_view`. Several directly post to unsuffixed `/v1/views/...`; the current `server/internal/transport/http/source.go` requires preview/admit/read/release and does not list all those personal Views.

Do not label a legitimate typed View or connector adapter as Expert coupling solely because it contains a domain name. Audit the route and caller, not just spelling.

| Caller/API condition | Required action |
|---|---|
| No remaining production caller | Delete method, dead request/response types, exports and fixtures. |
| Live caller can use the canonical authorized read | Move caller, test exact admission protocol, then delete old method. |
| Capability is not implemented by the server | Report explicit unsupported/unavailable semantics; do not invent endpoint success or silently treat failure as complete empty evidence. |
| Legitimate connector metadata or exact source preview | Retain at its real owner with bounds, identity and cancellation checks. |

Do not resurrect an unadmitted endpoint on the server to save a Rust wrapper. Inspect test-local HTTP servers that still accept obsolete routes, including inline App tests. Remove only their obsolete success paths; preserve authentication, signature, release, malformed-input and response-loss tests on the real protocol.

The server's domain View handlers may remain product-owned static implementations. Fully dynamic connector/View plugins are not this task. If source protocol semantics change, move Rust and Go callers/fixtures together and run the Go gate.

## 05-B: test deletion by meaning

| Existing baseline anchor | What to preserve / remove |
|---|---|
| `crates/experts/builtin/tests/registry.rs` | Keep source-independent discovery, Person isolation and user enablement meaning. Remove builtin-only forced topology. |
| `crates/app/src/vault_host/tests/vault_registry/builtin_setup.rs` | Keep install/CAS/reopen properties under generic owner tests; delete obsolete setup machinery. |
| `crates/app/src/vault_host/tests/schedule_host.rs` | Inspect current fixture-only role. Replace synthetic Calendar/TimelineRead wiring with generic Task/Actions evidence; delete file only after surviving assertions have a home. |
| `crates/app/src/vault_host/tests/conversation_flows.rs` | Preserve current conversation/source/interaction behavior; do not delete because its historical filename was Calendar-related. |
| `crates/app/src/vault_host/tests/proposals.rs`, `expert_actions.rs`, `expert_actions/inspection.rs` | Preserve exact approval/evidence/uncertain-effect recovery using new contracts. |
| `crates/adapters/providers/tests/native_calendar.rs` | Retain real EventKit request, subject, generation and response checks; genericize irrelevant fixture subjects only. |
| `server/internal/application/calendar_admission_test.go` | Retain signed preview/admission/read/release and verified provider identity tests. |
| `apps/client/test/features/experts/agent_expert_result_test.dart`, support fixtures | Remove Schedule-shaped common parser assumptions; keep identity/bounds and real serializer interoperability at new surface. |
| Interaction/consent tests in App, Conversation, Vault and Flutter | Preserve origin verification, immutable reviews, CAS, exact command rejoin and linked-resume behavior. |

Baseline-absent paths are not work to recreate: `ScheduleEndpoint` and its old App directory, `experts/builtin/tests/calendar_setup.rs`, Registry calendar_setup/calendar_access modules, old Flutter Calendar Expert controller/dialog/domain/support tests. Verify they remain absent; do not restore them to get an old test compiling.

A test-count decrease is allowed. Each deleted safety assertion needs a demonstrated replacement at its semantic owner; an obsolete topology assertion needs no compatibility implementation. Audit inline `#[cfg(test)]`, `_test.go`, `_test.dart`, fake gateways, JSON/golden fixtures and README examples.

## 05-C: two extensibility experiments

### E1: add one bundled Expert

In an isolated test change, add a test Expert using an already supported View/capability and only modify `crates/experts/builtin/**` for its production registration. Demonstrate generic product list/detail, installation/assignment, explicit binding, Directory discovery, Manager delegation, source read, terminal result and generic client display. No App, contracts, Vault, provider, protocol, Flutter gateway or server route edit may be required just for this Expert.

Keep the fixture under test configuration or remove the demonstrator after recording evidence; do not ship a fake Expert to end users. A compile-time bundle registration entry is allowed. An App test-only ID switch is not.

### E2: inject a package with a non-builtin ID

Supply a statically controlled test package such as `test.extensibility.example` through the normal registration API. Do not derive trust from its string prefix. Exercise installation, exact assignment, configured source access and a missing-permission -> trusted interaction -> reviewed resolution -> fresh linked resume case. Verify it cannot use Manager/another Expert's selection or assume first-party grants.

Use production TaskCoordinator, admission, Context/Access and product DTO code with fakes only at actual model/provider/OS/storage test boundaries where needed. A fake Directory that always approves or a fake grant builder that bypasses product policy does not prove extensibility. This is a conformance test, not proof of arbitrary untrusted-code sandboxing.

### Required evidence

Record the experiment diff, actual tests and fixtures, source targets used, which boundaries are real versus faked, and the expected common-production-diff of zero for package addition. Validate an unknown package with an ordinary artifact shape; generalized dispatch with a Schedule-only result requirement is failure.

## 05-D: residual searches and machine enforcement

Run from repository root; commands are audit selectors, not automatic deletion commands:

```sh
rg -n 'BuiltinExpertKind|BuiltinContextSource|BuiltinExpertSetup|BuiltinSourceBinding|builtin_setups' crates apps/client/lib apps/client/test server docs
rg -n 'CalendarExpert|ScheduleEndpoint|experts\.calendar\.install|AgentCalendarExpert' crates apps/client server docs
rg -n 'PackageImplementation|FindFocusWindow|focus_minimum_minutes|builtin_expert' crates apps/client server docs
rg -n 'ExpertInput|ExpertInsight|ExpertFocusProposal|StatefulFocusProposal|experts\.builtin|calendar\.expert' crates apps/client server docs
rg -n 'read_communication_view|read_work_context_view|read_calendar_context_view|read_confirmed_interaction_view|read_logistics_view|read_people_view|read_attention_view|read_wellbeing_view' crates
rg -n 'enabled_builtin_expert_cards|required_tools|granted_tool_assignments|modelCalls == 1|view_calls.*!= 1' crates apps/client
```

Also inspect source enumeration, `grants(128)`, `calendar_connection`, `from_package_id`, first-party prefixes, fallback/compat branches and old source URL fixtures by meaning. Exact grant lookups, product-owned connector code, package-local domain implementations and honest historical ADR text may remain. The plan itself lists obsolete names as deletion instructions; classify those matches, do not falsify a zero count.

Add focused machine checks/tests to the existing architecture tooling: no builtin implementation imports from common owners; no individual Expert enum dispatch in App; no shared Focus/TimelineRead registry assumptions; no package-specific general client gateway; consistent generated contracts. Keep a narrowly justified, owner-scoped allowance for package-local concepts rather than globally banning words such as Calendar.

Test the checker with forbidden examples. A text grep alone cannot prove that runtime never selects a substitute source, so retain E1/E2 and 04's no-discovery/isolation tests. A blanket allowlist of current offending files is not convergence.

## Completion gate

All old production paths and old-shape fixtures are removed or individually justified; new generic APIs have no forwarding-only compatibility facade. E1/E2 pass through the real owner chain. Source grant selection is exact, read authority remains Access-owned, and generic result/action boundaries remain safe.

Run targeted deletion/conformance tests and the applicable broad gates in [06](06-verification.md). Report every residual by owner and reason, and actual assertion migrations rather than only total test counts. Hand off a clean final change set to 06.
