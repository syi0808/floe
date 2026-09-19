# Architecture documentation

This directory describes Floe's current architecture, responsibilities and dependency boundaries. Refactoring execution is separate and intentionally consists of only three documents:

| Refactoring scope | Document |
|---|---|
| Completed semantic ownership placement | [Stage 1 — Physical Ownership](../refactoring/stage-1.md) |
| Active canonical internal-runtime cutover | [Stage 2 — Canonical Internal Runtime](../refactoring/stage-2.md) |
| Product-boundary and final-composition cutover | [Stage 3 — Product Boundary and Final Composition](../refactoring/stage-3.md) |

| Needed information | Source |
|---|---|
| Approved target module boundaries | [Dependency policy](../../tools/architecture/module-dependencies.json) |
| Product meaning and long-term scope | [Product planning](../planning/README.md) |
| Individual design decisions | [ADRs](../decisions/) |
| Executed validation evidence | [Validation](../validation/) |
| Dated product history | [History](../history/README.md) |

Target policy and the current manifest may differ while Stage 2 is active. Do not describe a transitional caller as final architecture merely because its owner types already exist. Distinguish source-level wiring, compile/test evidence and actual product behavior.
