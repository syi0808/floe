# Synthetic Manager guidance evaluation

`corpus.json` is the frozen 22-case Checkpoint 07 corpus of the Agent execution
environment grounding plan. The first 18 cases retain their original payloads,
rubrics and expectations. The four appended cases are `F02` (assistant-only stale
assertion), `G01/G02` (English/Korean guessing) and `S05` (Korean unavailable
synthesis). The five generic cards are unchanged. Its aliases and expected outcomes are harness data,
not AgentCard fields. Do not rewrite inputs, retune production prompts, or relax
the rubrics to repair model failures.

The existing `local_model_smoke` example uses the canonical Conversation
projection and shared Inference. Each case runs three independent repetitions.
Synthesis cases replay a validated synthetic Task result, then make at most one
additional model call. This does not execute Experts, read sources, or settle
durable Vault Tasks.

## Offline contract checks

```sh
cargo test -p floe-app --example local_model_smoke
```

These tests never call a live model. They cover corpus validation, discovery
variants, canonical projection, Task exchange identity, whole-batch choice
classification, bounded synthesis, report fields, and credential-file admission.
The original 18-case canonical serde payload SHA-256 is frozen as
`13de056b5ee1c2d1d5f7fc693248d59bf8bbf1489f0152b084f74e12a9bc59ef`.
`Case` serialization excludes `review_focus` to retain that baseline representation;
reports serialize the typed focus tags separately. Focus metadata never changes
model inputs or classification and is not an automated semantic grader.

## Explicit live evaluation

After the operator approves model use, run from the repository root:

```sh
FLOE_MANAGER_EVAL_APPROVED=1 \
FLOE_MANAGER_EVAL_STAGE=manager \
  tools/validation/run-local-model-smoke.sh --exercise-manager-guidance
```

For server evaluation, the operator must also explicitly supply
`FLOE_MANAGER_EVAL_CONNECTION_FILE` and `FLOE_MANAGER_EVAL_RECIPIENT`:

```sh
FLOE_MANAGER_EVAL_APPROVED=1 \
FLOE_MANAGER_EVAL_STAGE=manager \
FLOE_MANAGER_EVAL_CONNECTION_FILE=/absolute/path/to/existing-connection.json \
FLOE_MANAGER_EVAL_RECIPIENT='exact observed recipient' \
  tools/validation/run-local-model-smoke.sh --exercise-manager-guidance-server
```

The connection file contains the existing saved connection's `base_url`,
`token`, `client_id`, `person_id`, and `device_id`. It must be a regular,
non-symlink file owned by the current user, with no group/other permissions.
The runner never discovers, exports, creates, or changes credentials, accounts,
pairing, Keychain entries, or development profiles. Remote consent is in-memory,
Synthetic-only, and bound to the observed exact recipient and current lineage.
An explicit server profile cannot silently fall back to Foundation.

`FLOE_MANAGER_EVAL_STAGE` selects the report label: `baseline` (default),
`native`, `manager`, or `card`. `FLOE_MANAGER_EVAL_MODEL_ID`, when supplied,
is labeled `operator_configuration`, not a provider-observed model identity.
Without it, upstream model identity remains unavailable. Keep the same corpus,
provider/model configuration, and runtime across stages; if model identity
cannot be confirmed, do not claim an A/B improvement rate.

The signed smoke prints JSON Lines containing case/repetition, commit, prompt
revisions/hashes, ordered Card description hashes, provider/profile, complete
synthetic response steps, accepted shape/Agent expectations, typed review focus,
and the unchanged rubric. A complete provider run contains exactly 81 case-phase
records (22 selection plus five synthesis phases, each repeated three times),
followed by one final summary with commit/corpus/model/configuration identity and
expected record count. Save stdout separately from build output. Raw JSONL stays
local; record its SHA-256 and semantic review summary in the plan execution report.
Run against one committed, clean snapshot with constant corpus/configuration
identity. Never cherry-pick failed cases for reruns: an invalid provider/transport
run requires a fresh whole-corpus run, retaining the invalid report digest/reason.

`choice_accepted` is a mechanical shape/identity check, not truth evaluation.
Successful shape checks emit `REVIEW_REQUIRED` with `behavior_review=pending`.
Review the answer, delegation assignment, scope, freshness, and uncertainty
against the fixed rubric and review focus before reporting behavioral PASS.
The absolute CP07 hard gate (zero failures in every hard class across all three
repetitions) is primary; A/B claims additionally require confirmed constant
model/configuration identity. Invalid choices
remain `BEHAVIOR_FAILURE`; model execution failures retain their error category.
Missing prerequisites remain `UNVERIFIED`, never PASS. No secret connection,
endpoint, token, or raw receipt/replay is included in reports.
