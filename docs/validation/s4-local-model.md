# S4 native on-device model adapter

Date: 2026-09-07. Profile: Apple Foundation Models, macOS 26, arm64.
This is adapter implementation and availability evidence, not S4-M1 acceptance.

## Delivered boundary

`floe_ffi::local_model::FoundationModelRunner` implements the existing Agent
`ModelRunner`, returning the common structured Answer/Call or `AgentFailure`.
The app build embeds and signs `libfloe_local_model.dylib`; Rust loads it only
from the current application's Frameworks directory. The native ABI uses bounded
version-1 JSON with request UUIDs, start/poll/cancel/release and availability.
No API credential, Go service, cloud model, download or remote fallback is used.

The Swift adapter creates a fresh `LanguageModelSession(model: .default, tools: [],
instructions: ...)` per call. Host instructions remain separate from JSON-rendered
scoped policy, retrieved untrusted evidence, recent messages, advertised capabilities
and ephemeral output limits. `@Generable` constrains the response shape; both native
and Rust validation reject invalid answer/call combinations. Rust additionally
checks that a requested capability was advertised. Swift never invokes tools or
Calendar APIs and never returns a reasoning field or framework debug descriptions.

The runner defaults to a synthetic-only construction. The trusted encrypted Calendar
vault host now uses a separate encrypted construction that may admit Personal-class
requests after the same placement, classification, projection freshness and read-only
descriptor checks. Synthetic construction still rejects Personal input before native
dispatch. This is a trusted host classification boundary, not an arbitrary-text content
classifier. The Calendar panel path remains bounded to briefing/focus requests; general
personal free-text input is not enabled.

## Bounds and cancellation

- Swift retains one job per process. Identical start reattaches to the same result;
  conflicting requests fail. Poll is non-consuming. Generation runs in a detached
  task, not inside the FFI call or under the host lock.
- Cancellation/deadline requests cancel that task, but do not claim it has stopped.
  An unfinished release discards presentation ownership while retaining the slot
  until the actual task finishes. A stalled foreign call cannot be replaced by
  another generation. Completion after a monotonic deadline is never success.
- Rust polls at 20 ms and checks its cancellation token/deadline. Dropping the
  future releases/cancels the native lease, including on runtime timeout. Native
  completion retains only one bounded final result; partial generated structures
  are not emitted or persisted. Token-level UI streaming remains future work.
- Requests are at most 32 KiB; instructions 4096 bytes; rendered prompt 12288
  bytes; returned step at most the requested limit capped at 16 KiB. The SDK is
  given a maximum response of 1024 tokens. Oversized context/output fails rather
  than truncating evidence or switching models.

### Token accounting

The installed macOS 26 SDK has no exact usage counter exposed in this path.
Each dispatched successful call charges **4096 reserved tokens**, the entire
documented per-session context window, rather than claiming measured usage.
The reservation covers instructions, schema, prompt and response. Less than 4096
remaining tokens fails before dispatch. Default 8192-token turns can therefore
perform two calls, for example a capability request followed by an answer.
Local provider monetary cost is zero; this is not a claim of zero energy cost.

This follows Apple's [context-window boundary](https://developer.apple.com/documentation/technotes/tn3193-managing-the-on-device-foundation-model-s-context-window).
The profile is restricted to macOS major version 26 so a future SDK/model window
cannot silently invalidate its accounting; other profiles require explicit
implementation and verification. An SDK context-window error maps to BudgetExceeded.
No exact token count or live generation performance has been measured here.

## Evidence

On this macOS 26.2 machine, the actual bundled Rust → C ABI → Swift availability
probe returns **AppleIntelligenceNotEnabled**. No generation was attempted and
Apple Intelligence settings were not changed. This distinguishes a correctly
loaded adapter from a usable live model; it does not pass the local model gate.

- Swift 6 strict-concurrency compilation with warnings as errors passes at a
  macOS 12 deployment target. FoundationModels is weak-linked; older-OS runtime
  behavior is still unverified and returns an explicit unsupported state by design.
- Swift injected-generator tests pass: unavailable dispatch denial, structured
  answer, result replay, wrong request, conflicting owner, output limit, cancel
  and release while generation is still blocked, deadline and strict decoding.
- Five Rust adapter tests pass: common step mapping, conservative reservation,
  separated input layers, pre-dispatch privacy/budget/stale/cancel rejection,
  unadvertised call rejection, malformed/oversized/wrong-request responses, and
  native lease release on deadline or dropped future. These are transport fixtures,
  not a live-model prompt-injection or usefulness evaluation.
- All 101 workspace Rust tests pass. The separate keyring example's three
  cleanup-confinement tests pass. Formatting and Clippy with the existing two
  Calendar exclusions pass. The macOS Debug app builds with the new native library.

```sh
bash tools/validation/check-local-model.sh
bash tools/validation/run-local-model-smoke.sh --availability
```

After the user enables Apple Intelligence and the model becomes available:

```sh
bash tools/validation/run-local-model-smoke.sh --exercise
```

The exercise makes one fixed fictional scheduling request with no sources,
credentials or persistence; it reports the validated common step and reservation.
The helper uses ad-hoc signing and does not launch the normal app or access Calendar.

## Still required

Live synthetic generation, multi-call Agent scenarios and interruption/error tests;
real-model streaming and panel selection; encrypted personal-chat integration after
the key/lifecycle gate; supported remote adapter/authentication and outbound-capture
tests; Expert/connector integration and end-to-end acceptance. S4 remains **0/14**.
