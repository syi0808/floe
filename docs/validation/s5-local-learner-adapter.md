# S5 Device-local Structured Learner Adapter

Date: 2026-09-10

## Scope

- Added `StructuredLearnerModel<Model>` as the production bridge from the isolated
  Learner port to the existing governed `ModelRunner` boundary.
- Added a dedicated Learner role and protocol prompt for one strict JSON Memory review
  result, separate from Manager persona and capability instructions.
- Raised the default Learner token allowance to 8,192 so the local Foundation runner's
  4,096-token context reservation can complete while preserving one bounded recovery
  opportunity in the shared model-attempt layer.

## Enforced boundaries

- Device-local placement is required before any model dispatch.
- Policy is fixed to background, Personal data, device-local placement and no external
  transfer consent.
- The model request contains no capabilities, active experts, replay, persona or raw
  evidence. The completed digest is the only user message and confirmed Memory is
  projected only as contextual data.
- The response must contain exactly one Answer step, no replay, the current schema
  version, no unknown JSON fields and either one typed proposal or `null`.
- Learner Playbooks are rejected at registry construction so procedural instructions
  cannot be added through the normal Playbook mechanism.

## Validation

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- The focused adapter tests verify local policy/request isolation, strict structured
  decoding, remote placement denial and the absence of model dispatch on denial.

## Remaining work

- The adapter is not yet invoked by the vault worker.
- Add idle-only discovery/claim/run/settle scheduling and cancel it when foreground work
  arrives, without delaying or changing a foreground response.
- Add device availability retry/backoff tests and an end-to-end local smoke test before
  claiming any S5 acceptance criterion.
