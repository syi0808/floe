# Floe Apple Health

HealthKit reads only sleep analysis, steps and Apple exercise time over a bounded
36-hour window. The raw aggregate stays inside the native source provider.

Every read requires the independent local FoundationModels privacy transform.
`HealthPrivacyTransform.swift` owns its strict input/output contract, independent
job slot, 10-second deadline, 64-token response budget and bounded native ABI.
Input is limited to finite, nonnegative sleep hours (≤36), steps (≤1,000,000) and
exercise minutes (≤2,160), with at least one signal. Invalid input is rejected.
The model returns only closed capacity and recovery categories. There is no
Agent prompt, tool access, remote fallback or deterministic threshold fallback.

Production `HealthKitWellbeingProvider.currentHostProvider(sourceHandle:transformer:)`
requires `BundledHealthPrivacyTransformer`. It loads the same bundled
`libfloe_local_model.dylib` image that Rust uses to verify a one-time native
transform receipt. The receipt binds the original acquisition request, host
epoch, Person, device and native subject to the output digest and freshness.
A caller-provided boolean or copied View cannot replace that receipt.

After valid transform success, source code constructs the coarse
`wellbeing.derived` View, 30-minute expiry, confidence and opaque evidence handle.
Both categories unknown produces zero confidence and no evidence handle.
Failure does not refresh or substitute a cached projection. The result remains
HighlySensitive; a valid transform never grants source-processing permission.

HealthKit does not reveal whether read access was denied. Authorization-request
completion is not reported as confirmed read access. Empty data and restricted
read access remain indistinguishable. Mac Catalyst and unavailable Health stores
are unsupported; iPad requires iPadOS 17 or later.

Both macOS/iOS native build scripts and Xcode source inputs include the same
Health transform source alongside the local-model dylib source. FoundationModels
is weak-linked, and actual model readiness is checked before any transform.
Compilation and behavioral validation follow the repository's staged refactor
gates; this source change does not claim those gates have run.
