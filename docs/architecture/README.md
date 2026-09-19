# Architecture documentation

This directory describes Floe's current architecture, responsibilities and dependency boundaries. Refactoring execution is separate: three Stage overview documents define direction and Stage 2/3 link to step-specific execution plans:

| Refactoring scope | Document |
|---|---|
| Completed semantic ownership placement | [Stage 1 — Physical Ownership](../refactoring/stage-1.md) |
| Active canonical internal-runtime cutover | [Stage 2 — Canonical Internal Runtime](../refactoring/stage-2.md) |
| Product-boundary and final-composition cutover | [Stage 3 — Product Boundary and Final Composition](../refactoring/stage-3.md) |

| Needed information | Source |
|---|---|
| Module dependency policy | [Dependency policy](../../tools/architecture/module-dependencies.json) |
| Product meaning and long-term scope | [Product planning](../planning/README.md) |
| Individual design decisions | [ADR index](../decisions/README.md) |
| Active refactoring status | [Stage 2](../refactoring/stage-2.md) |
| Historical plans and acceptance snapshots | Git history |

Target policy and the current manifest may differ while Stage 2 is active. Do not describe a transitional caller as final architecture merely because its owner types already exist. Distinguish source-level wiring, compile/test evidence and actual product behavior.
