# Floe Apple Health

FloeAppleHealth owns HealthKit acquisition and permission facts: sleep analysis,
steps and exercise time over a bounded 36-hour window. It returns numeric
acquisition values and never invokes a model or publishes a Wellbeing View.

FloeAppleWellbeing maps those values into HealthTransform input, invokes the
bundled Health operation, and constructs the sanitized source envelope only
after validated success. Its cache retains the existing freshness and permission
semantics. Raw aggregates stay inside this native pipeline.

The shared Transform interface and concrete HealthTransform live in the
FloeNative package. HealthTransform depends on the backend-neutral DeviceModel
contract. FoundationModels is the first device backend, with SDK handling
isolated from Health domain policy. Health receives no Agent context or tools,
and has no remote or deterministic semantic fallback. Input remains finite and
nonnegative: sleep hours at most 36, steps at most 1,000,000, exercise minutes at
most 2,160, with at least one signal. Output is the closed capacity/recovery pair.

The dynamic libfloe_local_model.dylib owns separate DeviceModel and Health job
hosts using one injected backend. Runner links only stateless contract/bridge
and source modules. The Health ABI retains its ten-second deadline, 64-token
budget, one-use receipts and completion-anchored replay retention. Rust consumes
the receipt from that same image and verifies acquisition request, host epoch,
Person, device, native subject, output digest and freshness. A copied View or
caller boolean cannot replace that proof.

After success, source code constructs wellbeing.derived, 30-minute expiry,
confidence and an opaque evidence handle. Both categories unknown produces
zero confidence and no evidence handle. Failure cannot refresh cached evidence.
HighlySensitive classification and source-processing permission remain separate
from transformation; transformation grants no authority.

HealthKit does not reveal whether read access was denied. Completed permission
prompts are not reported as confirmed read access. Empty data and restricted
read access remain indistinguishable. Mac Catalyst and unavailable Health stores
are unsupported; iPad requires iPadOS 17 or later.

Both native build scripts build the same SwiftPM dynamic product and retain
weak FoundationModels linkage and runtime availability checks. Build and behavior
qualification remain separate staged gates; this source change does not claim
those gates have passed.
