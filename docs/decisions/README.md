# Architecture decision records

ADRs explain durable decisions and their rationale. They are not the source of truth for current implementation progress.

## How to read ADRs

1. Start with [current architecture](../architecture/README.md).
2. Open an ADR only when the reason for a current boundary or invariant is needed.
3. Follow each ADR's `Amends`, `Extends` and `Supersedes` links before applying an older decision.
4. Treat implementation-status or slice-status language inside older ADRs as historical context unless the current architecture or active refactoring documents confirm it.

ADR status should describe the **decision**, not whether code, tests or live acceptance are complete. New ADRs should use a simple status such as `proposed`, `accepted`, `rejected` or `superseded`.

## Decision map

### Repository and implementation foundations

- [0001 — Greenfield reset](0001-greenfield-reset.md)
- [0002 — Implementation baseline](0002-implementation-baseline.md)
- [0003 — Native connectors and Experts](0003-native-connectors-and-experts.md)
- [0004 — Personal Day first slice](0004-personal-day-first-slice.md)
- [0005 — JSON/C ABI Flutter bridge](0005-json-c-abi-flutter-bridge.md)
- [0006 — Slice-driven delivery](0006-slice-driven-delivery.md)
- [0007 — EventKit Calendar read](0007-eventkit-calendar-read.md)
- [0008 — Unified Calendar read](0008-unified-calendar-read.md)

### Inference and agent runtime

- [0010 — Local connection console](0010-local-connection-console.md)
- [0011 — Inference performance classes](0011-inference-performance-classes.md)
- [0012 — Memory and Expert-first slices](0012-memory-and-expert-first-slices.md)
- [0013 — Conversation, learning and voice sequence](0013-conversational-agent-learning-and-voice-sequence.md)
- [0014 — Connected Agent sources](0014-s4-connected-agent-sources.md)
- [0015 — Privacy-aware inference](0015-s4-privacy-aware-inference.md)
- [0016 — Native Agent model protocol](0016-native-agent-model-protocol.md)
- [0017 — Agent context assembly](0017-agent-context-assembly.md)
- [0018 — Manager–Expert A2A delegation](0018-manager-expert-a2a-delegation.md)
- [0019 — Governed Memory and Playbook learning](0019-governed-memory-and-playbook-learning.md)
- [0020 — Ambient assistant, domain Experts and capability connectors](0020-ambient-assistant-expert-connector-model.md)
- [0021 — Connected-domain expansion](0021-s5-5-connected-domain-expansion.md)
- [0022 — Generalizable Agent guidance](0022-generalizable-agent-guidance.md)
- [0023 — Page-independent assistant conversation](0023-page-independent-assistant-conversation.md)

### Device context, connections and authority

- [0024 — Device context collection and convergence](0024-device-context-collection-and-convergence.md)
- [0025 — Person-owned connections](0025-person-owned-connections.md)
- [0026 — Server-owned provider OAuth](0026-server-owned-provider-oauth.md)
- [0027 — Connection authority and observation](0027-connection-authority-and-observation.md) — currently proposed; reconcile with current Access/Context/Connections ownership before promoting it.
- [0028 — Pairing-integrated authority and connection permissions](0028-pairing-integrated-authority-and-connection-permissions.md) — currently proposed; product ceremony and runtime authority must be evaluated separately.

There is no ADR 0009 in the repository; numbering is intentionally preserved rather than renumbered.
