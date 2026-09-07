# ADR 0013: Prove the conversational agent before memory and voice

- **Date:** 2026-09-07
- **Status:** accepted; S4 connector scope amended by [ADR 0014](0014-s4-connected-agent-sources.md)
- **Amends:** [ADR 0012](0012-memory-and-expert-first-slices.md)

## Context

Floe cannot validate Memory quality or self-improvement without first having a
real conversational Agent loop whose sessions, Expert calls, tools, outcomes and
user corrections provide evidence. Likewise, transcription and wake-up should
connect to that same Agent session rather than create parallel assistant runtimes.

The previous sequence put Reviewable Memory in S4 and a narrow Schedule Expert
loop in S5. It still left chat, multi-turn execution, procedural learning and
voice as implicit future work. This made the core product loop less explicit than
the server and device slices it was meant to precede.

Hermes Agent provides a useful implementation reference for a platform-neutral,
tool-using conversational loop and an externalized learning system. Floe should
adopt those mechanisms selectively, not inherit Hermes' broad terminal-agent
authority or file-backed personal-memory semantics.

## Decision

- **S4 Conversational Agent and Expert Foundation** delivers chat with one Floe
  Manager, multi-turn session persistence, an interruptible budgeted Agent loop,
  registry-discovered tools/Experts and one Schedule Expert path into the S3
  action gate.
- **S5 Governed Memory and Self-Improvement** uses S4 conversations, outcomes and
  corrections as evidence. It separates Personal Memory, session recall and
  procedural Playbook knowledge; all inferred durable changes are staged for Review
  by default and remain versioned, reversible and source-backed.
- **S6 Transcription and Voice Mode** adds press-to-talk, duplex voice and an
  explicit user-started transcription session, connected to the same S4
  AgentSession and S5 candidate Review boundary.
- **S7 Local Wake-up and Ambient Invocation** validates on-device wake detection,
  privacy, resource cost and resident Device Agent lifecycle before proactive
  intervention behavior.
- Re-number cross-device/server to **S8** and event-driven intervention to **S9**.
  Distribution follows the accepted Agent, Memory and voice contracts.
- S4 is designed around stable semantic ports rather than one model, client,
  transport or Expert implementation. Built-in and declarative fixture Experts
  use the same invocation/result contract; later Wasm or remote workers must fit
  behind that host boundary.

## Hermes mechanisms adopted as references

- one platform-agnostic Agent core behind UI/gateway entry adapters;
- typed tool registry and optional subsystem registration;
- persistent, searchable sessions with bounded compaction and recovery pointers;
- stable/context/volatile prompt layers and progressively loaded procedural knowledge;
- factual memory distinct from procedural Playbooks;
- background learning review isolated from the foreground conversation;
- staged memory/skill writes, mutation ledger, rollback, pin and recoverable archive;
- cancellation, iteration/resource budgets and repeated-call stall guardrails.

Floe changes the safety defaults: agent-authored Memory and Playbook changes require
Review initially; background learning cannot invoke external mutation tools; and
the Agent cannot rewrite identity, product policy, permission grants, audit logs,
Expert signatures or model weights.

## Consequences

- S4 can ship useful chat without claiming durable learning.
- S5 receives real, typed evidence from S4 instead of inventing a disconnected
  Memory compiler demo.
- Text, voice and wake-up share one session and action-authority pipeline.
- The first implementation requires explicit registries, versioned events and
  capability boundaries earlier, reducing later extension rewrites.
- Cross-device/server and intervention move to S8/S9.

## References

- [Agent Runtime and Governed Learning](../planning/03-intelligence/agent-runtime-and-learning.md)
- [Vertical Slice Delivery](../planning/08-engineering/vertical-slice-delivery.md)
- [Hermes Agent architecture](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/architecture.md)
- [Hermes Agent loop](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/agent-loop.md)
- [Hermes memory](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/memory.md)
- [Hermes skills](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/skills.md)
- [Hermes curator](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/curator.md)
