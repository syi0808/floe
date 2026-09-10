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
- General conversations backed by a paired server now advertise the two Expert cards through the
  existing in-process A2A transport. The Manager selects delegation; each task obtains a fresh
  bounded Communication View, runs the matching isolated prompt and returns summary plus typed data
  artifact. Device-only conversations do not advertise mail Experts.

## Automated evidence

```sh
cargo test -p floe-agent --test mail_experts
cargo test -p floe-ffi commitments_delegation_reads_fresh_view_and_returns_typed_artifact --lib
cargo test --workspace
```

The focused corpus passes 3/3, the composed paired-server View → isolated Commitments model → typed
A2A artifact test passes, and the Rust workspace passes. The evidence is synthetic and does not
claim model quality or live Gmail acceptance.

## Remaining gate

The built-in stateless runners are not yet installed or assigned through the durable product
registry, and the existing Calendar-scoped conversation path still takes precedence when configured.
Commitments needs Calendar/Task/confirmed Memory cross-source evaluation, and Communication needs
People/relationship context plus explicit draft review. No live mailbox evidence was used. S5.5-E2
and S5.5-E3 remain pending.
