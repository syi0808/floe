# Migrate bounded builtin packages to ExpertProgram

S2 is authorized after G1. Source only: no formatter, compiler, build, tests, checker, dependency changes, commits or publication. Read root AGENTS and the canonical/Rust/S2 Engine owner plans.

The coordinator assigns an exact subset of these disjoint folders: commitments + communication; relationships + focus_attention + wellbeing; work_context + life_logistics. Edit only the assigned folders under `crates/experts/builtin/src/`. Do not edit root lib.rs, registration.rs, catalog.rs, shared.rs, program_support.rs, prompts.rs, Cargo files, Schedule, other owners, App or Vault. Report necessary shared helper changes. Native work wires exports/registrations and removes old common host/model vocabulary.

## Frozen implementation API

Implement one package struct in each assigned `expert.rs`, exported from that folder's mod.rs:

- CommitmentsProgram
- CommunicationProgram
- RelationshipsProgram
- FocusAttentionProgram
- WellbeingProgram
- WorkContextProgram
- LifeLogisticsProgram

Each implements `floe_experts::ExpertProgram` exactly as defined in modules/experts/src/program.rs:

- specification(&ExpertProgramRequest) -> ExpertProgramSpec
- finalize(&ExpertProgramRequest, &[ExpertToolObservation], text:&str, artifacts:&[Artifact]) -> ExpertFinalOutput

The request contains exact actor, admitted DelegationRequest, ExpertAdmissionIdentity, ExpertExecutionSelection, existing ExpertPrivateState and owner now_unix_ms. Finalize is pure package judgment. Never call a model, Engine, source, journal, Task repository, review publisher or effect owner from a package.

Observations are actual settled source reads from the common ToolPort, not model-authored JSON. Each has call, requirement_key and outcome `ExpertSourceObservation::Ready {payload,coverage}` or `Unavailable {reason}`. Source review never reaches finalize; Engine journals the actual ToolReviewRequired and returns typed Task blockage. The Task owner handles missing required binding before Engine starts. Therefore remove `ExpertJudgment`, `ExpertModel`, `ExpertReasoner`, one-shot run_* loops, old host dispatch, and model/source-review fake Completed artifacts from these folders. Unavailable is distinct: a real captured Unavailable may produce the existing deterministic unavailable domain result with no findings. No observation at all is InvalidModelOutput and cannot masquerade as Unavailable.

## Preserve existing domain judgments

Mechanically extract the existing run_*_expert's post-model parse, bound, evidence-handle, source identity, confidence/epistemic status, deduplication, expiration and result construction logic into finalize. Preserve public domain result types, fields, serde shape, count/text bounds and all source freshness/identity validators. Do not weaken provenance checks to accept arbitrary model handles. The raw model text is the existing model output JSON; model artifacts must be empty (packages assemble their own result artifact after validation).

Obtain typed context views by decoding Ready payloads from matching declared requirement keys. Use request.request.execution_context.agent_context for the already-admitted root context; do not silently replace existing confirmed memories with an empty list. A real successful confirmed-memory tool read may replace that subset using its stored snapshot shape. Include optional calendar/task/work/confirmed-interaction inputs only if actually read. Preserve unavailable/missing optional context annotations where current package judgment uses them.

For single-shot evidence sources, `program_support::read_one<T>` rejects multiple conflicting Ready observations; do not silently choose a convenient observation. For calendar pagination or multiple source batches, package code must retain existing range/source continuity, duplicate-handle and completion checks when composing captured reads. These packages currently use a single nearby calendar query; Schedule pagination is owned by the native lane. All source selection and permission enforcement remains Context's responsibility, under the Task's pinned declared selection.

## Shared pure helpers already supplied

`crate::program_support` provides:

- specification(request,prompt,output_contract): complete tool specifications for all admitted declared requirements, with existing query schemas and no subdelegation.
- read_one<T>(observations,requirement_key): one decoded Ready payload or None; duplicates rejected.
- was_unavailable(observations,key): actual typed Unavailable observed.
- coverage(observations): exact merge of actual Ready dependencies.
- result(request,observations,artifact_name,media_type,result_text,&domain_result): deterministic Task-scoped artifact identity, actual coverage, ValidatedFinalPayload and no settlement.
- unavailable(request,observations,mandatory_key,artifact_name,media_type,summary): only for an actual unavailable read, never for an uncalled source or source review.

Reuse package prompts from prompts.rs. In specification, append an explicit bounded instruction to the role component that required evidence tools must be read before the final judgment and the final answer uses the package's existing JSON schema. Keep the prompt assembly valid and within existing component limits. No provider/profile/local constraints.

The Experts-owned common Engine calls finalize exactly once before the canonical ValidatedBatch and Output are journaled. It checks final payload size and that artifact coverage is a subset of the actual authorized projection. Task settlement requires exact canonical Output equality. Do not publish a second output or add a package journal. Remove unused dispatch modules/files in assigned folders once the old package loop has no remaining owner; native root registration will switch to Program values in the same slice.

Report changed paths, exact preserved validators and shared integration needs promptly. No tests or checks until whole G2.
