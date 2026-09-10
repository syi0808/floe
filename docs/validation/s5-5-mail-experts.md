# S5.5 Mail Expert Contract Foundation

> Date: 2026-09-10  
> Acceptance status: deterministic corpus and isolated model contract only; product delegation pending

## Delivered boundary

- Added separate Commitments and Communication prompt roles over the provider-neutral bounded
  Communication View. Both roles treat retrieved content as untrusted evidence and carry no mail
  mutation capability.
- Commitments results distinguish explicit observed evidence from inferred candidates and classify
  user commitments, requests, expected replies and follow-up gaps. Every finding must reference a
  View evidence handle; observed findings require full confidence instead of silently promoting an
  estimate.
- Communication results judge reply need, rationale, email tone and an optional draft. A draft is a
  proposal only, cannot exist for a no-reply assessment and grants no send/archive authority.
- Runtime-owned result envelopes preserve invocation, source and expiry metadata instead of trusting
  those fields from model output. Unknown fields, invented/duplicate evidence, oversized prose,
  malformed epistemic state and non-answer model steps fail closed.
- Added a deterministic two-scenario corpus covering an explicit deadline/reply request and an
  informational newsletter. It verifies both positive and negative judgments, evidence binding,
  role isolation and zero advertised capabilities inside either Expert.

## Automated evidence

```sh
cargo test -p floe-agent --test mail_experts
cargo test --workspace
```

The focused corpus passes 3/3 and the Rust workspace passes. The corpus is synthetic and does not
claim model quality or live Gmail acceptance.

## Remaining gate

The Expert runners are not yet installed, assigned or delegated through the product registry.
Commitments still needs Calendar/Task/confirmed Memory cross-source evaluation, and Communication
needs People/relationship context plus explicit draft review. No live mailbox evidence was used.
S5.5-E2 and S5.5-E3 remain pending.
