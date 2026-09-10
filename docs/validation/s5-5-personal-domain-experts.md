# S5.5 Personal Domain Expert Foundation

> Date: 2026-09-10  
> Acceptance status: isolated contract corpus only; live production delegation pending

## Delivered boundary

- Added separate Relationships, Focus & Attention and Wellbeing prompt roles and typed result
  contracts over the bounded Personal Views.
- Relationships can return evidence-linked follow-up candidates for a resolved identity. It cannot
  invent an identity/evidence handle, copy contacts or infer sensitive relationship attributes.
- Focus & Attention can recommend focus protection, interruption availability or no conclusion.
  It receives coarse state only and has no raw app/domain activity or notification authority.
- Wellbeing can recommend keeping a plan, reducing load, protecting recovery or no conclusion. It
  receives derived state only and its role explicitly prohibits raw samples and diagnosis.
- Runtime-owned envelopes bind every result to an invocation, source and expiry. Non-conclusion
  requires zero supporting handles; actionable judgments require handles present in the View.

## Automated evidence

```sh
cargo test -p floe-agent --test personal_experts
cargo test -p floe-agent prompts::tests::prompt_assembly_separates_role_persona_and_protocol --lib
```

The deterministic contract corpus passes 2/2, including positive cross-domain-shaped judgments and
invented identity, raw-app evidence and diagnostic-field rejection.

## Remaining gate

These Expert runners are isolated contract implementations. They are not yet installed/delegated in
the product and have no live People, Attention or Wellbeing adapter. Schedule/Memory cross-source
evaluation is also pending. S5.5-E4, S5.5-E5 and S5.5-E6 remain pending.
