# Synthetic Manager guidance evaluation

`corpus.json` is the frozen 18-case corpus from checkpoint 02 of the Manager
guidance execution plan. Its aliases and expected outcomes are harness data,
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

## Explicit live evaluation

After the operator approves model use, run from the repository root:

```sh
FLOE_MANAGER_EVAL_APPROVED=1 \
  tools/validation/run-local-model-smoke.sh --exercise-manager-guidance
```

For server evaluation, the operator must also explicitly supply
`FLOE_MANAGER_EVAL_CONNECTION_FILE` and `FLOE_MANAGER_EVAL_RECIPIENT`:

```sh
FLOE_MANAGER_EVAL_APPROVED=1 \
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
synthetic response steps, and the unchanged rubric. Save stdout separately from
build output if collecting reports. Run against a committed, clean snapshot.

`choice_accepted` is a mechanical shape/identity check, not truth evaluation.
Successful shape checks emit `REVIEW_REQUIRED` with `behavior_review=pending`.
Review the answer, delegation assignment, scope, freshness, and uncertainty
against the fixed rubric before reporting behavioral PASS. Invalid choices
remain `BEHAVIOR_FAILURE`; model execution failures retain their error category.
Missing prerequisites remain `UNVERIFIED`, never PASS. No secret connection,
endpoint, token, or raw receipt/replay is included in reports.
