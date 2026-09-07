# ADR 0015: Validate privacy-aware local and remote inference in S4

- **Date:** 2026-09-07
- **Status:** accepted
- **Amends:** [ADR 0013](0013-conversational-agent-learning-and-voice-sequence.md)
- **Related:** [ADR 0010](0010-local-connection-console.md),
  [ADR 0011](0011-inference-performance-classes.md)

## Context

S4 cannot validate the Floe Agent using only a fixture model. The product must prove
that the same Agent/Expert contract works with a device-local model and a remote
reasoning model without silently sending sensitive source data off device.

The repository already contains a local Go inference gateway and an experimental
Codex PKCE boundary, but no real Codex consent/refresh or product Agent flow has been
accepted. Apple Foundation Models and packaged sLLMs also have different availability,
capability and lifecycle constraints.

## Decision

- Add three model acceptance groups to S4: common model contract, subscription/remote
  authentication, and sensitive-context placement.
- Run at least one real device-local generative adapter in S4. Prefer the confirmed
  on-device profile of Apple Foundation Models on supported Apple hardware; a native
  packaged sLLM may satisfy the same gate on unsupported or non-Apple hardware. A
  Private Cloud Compute/server profile does not satisfy this gate, and both local
  implementations are not required for S4.
- Keep remote models behind the existing bounded `LanguageModel`/performance-class
  contract. A provider never receives Floe capability credentials or executes Floe
  tools directly.
- Evaluate Codex browser sign-in, token refresh, revocation/logout, workspace/account
  state and structured inference as an explicit S4 feasibility gate. Do not copy an
  existing Codex CLI/App credential.
- Treat direct reuse of the Codex OAuth wire protocol as experimental. OpenAI's
  documented ChatGPT sign-in covers Codex clients, not a general third-party OAuth
  contract for Floe. If an officially supportable integration cannot be established,
  record it as unavailable and retain API-key or other supported remote adapters.
- Require each domain to create an explicit `InferencePolicyDecision` before model
  selection. The decision contains purpose, data classes, allowed placements,
  performance class, projection version and external-transfer consent state.
- Never silently fall back from a local-only route to a remote route. Unavailable
  local inference returns a typed degraded state or asks for a separately visible
  remote-transfer decision.

## Sensitive routing rules

| Input class | Default placement | Remote rule |
| --- | --- | --- |
| Device-only raw data | device local | prohibited |
| Highly Sensitive | device local | only an explicit bounded projection and consent policy |
| Personal | declared local or remote | visible purpose and configured transfer policy |
| Temporary AI Context | declared remote route | ephemeral, minimized and auditable |

`derived` does not mean non-sensitive. Raw HealthKit samples, Screen Time records,
precise location history, credentials and unrestricted mail bodies cannot be added to
a remote prompt merely because another component summarized or retrieved them.

## S4 evidence

- deterministic fixture, one live local adapter and one supported remote adapter use
  the same structured request/result/error contract;
- outbound capture tests prove what crossed the process/device boundary;
- local unavailable, remote denied, consent missing, credential expired, quota and
  cancellation states degrade without changing placement;
- trace records the policy decision and model class without logging prompt contents,
  credentials or hidden reasoning.

## Consequences

- S4 grows from 11 to 14 acceptance criteria, but Memory and voice no longer inherit
  an unverified privacy-routing boundary.
- Apple Foundation Models is a local capability, not the universal Floe reasoning
  model; device-scale limitations remain visible.
- Codex subscription reuse can improve onboarding if officially supportable, but is
  not a dependency for the core local Agent experience.

## References

- [S4 Vertical Slice](../planning/08-engineering/vertical-slice-delivery.md)
- [Model Layer](../planning/03-intelligence/model-layer.md)
- [Sensitive Local Compute](../planning/06-security/sensitive-local-compute.md)
- [OpenAI Codex authentication](https://developers.openai.com/codex/auth)
- [Apple Foundation Models](https://developer.apple.com/documentation/FoundationModels/)
