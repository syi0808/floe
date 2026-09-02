# Expert Runtime

> Status: Recommended architecture direction

## Execution Classes

Floe supports multiple Expert implementation classes behind the same semantic contract.

```text
Expert Contract
├─ Native Built-in
├─ Declarative
└─ Sandboxed Component
```

---

# 1. Native Built-in Expert

Used by first-party Experts that need deep integration or maximum performance.

Examples:

- Health Expert
- Schedule Expert
- Communication Expert

Implementation may live in Go server or Rust Device Core depending on execution placement.

Even trusted Experts should consume domain Views and emit structured outputs where practical.

---

# 2. Declarative Expert

The default user-created extension mechanism.

Package contains:

- manifest
- trigger definitions
- view requirements
- rules
- prompt/model references
- output schemas
- configuration schema

Example concept:

```yaml
id: dev.floe.community.job-search
api_version: v1

triggers:
  - mail.received
  - schedule.daily

permissions:
  - mail.read.content
  - timeline.read.range
  - task.create.propose

logic:
  pipeline:
    - extract_job_context
    - detect_followup
    - propose_task
```

The host implements pipeline primitives.

Benefits:

- portable across server/device where supported
- no arbitrary native code
- easy permission analysis
- marketplace review easier
- user-generated Experts feasible

---

# 3. Sandboxed Code Expert

Used when declarative primitives are insufficient.

Recommended format direction:

```text
WebAssembly Component
+ WIT Expert API
```

The WebAssembly Component Model defines typed component interfaces via WIT, which fits a capability-based Expert host.

Candidate host:

```text
Desktop Rust Device Agent → Wasmtime
Server → dedicated Rust expert worker OR Go control plane calling a Rust/Wasmtime worker
```

The exact server embedding topology requires a PoC; Go remains the Floe control plane regardless.

---

# Expert Host API

Conceptual host interface:

```text
context.current_state()
timeline.query(...)
people.resolve(...)
memory.query_projection(...)
expert_state.get/set(...)
capability.invoke(...)
model.run(alias, input)
output.emit(...)
```

Only manifest-approved interfaces are linked into the invocation context.

---

# Model Usage

The global model architecture principle still applies: model choice is not delegated to a generic smart router.

For first-party Experts, domain implementation owns model selection.

For marketplace/declarative Experts, a package may declare **model requirements or aliases**, for example:

```text
model: local.small.language
```

or:

```text
model: remote.reasoning.standard
```

The Person/Instance maps allowed aliases to actual providers.

Third-party Experts do not receive provider credentials.

Sensitive model aliases can be prohibited by permission policy.

---

# Expert State

State is namespaced by:

```text
(instance, person, expert-package, assignment)
```

Schema version belongs to the ExpertPackage.

Updates may provide state migrations.

An Expert cannot read another Expert's private state unless an explicit future shared-state API is created.

---

# Invocation Semantics

Invocation input should be bounded and structured.

```text
ExpertInvocation {
  invocationId
  trigger
  timestamp
  personRef
  grantedViewHandles
  config
}
```

Output:

```text
ExpertResult {
  insights[]
  interventions[]
  actionProposals[]
  memoryCandidates[]
  stateUpdates[]
  diagnostics
}
```

The exact transport schema remains TBD.

---

# Resource Budgets

Marketplace Expert invocation receives limits.

Candidate dimensions:

- wall-clock deadline
- WASM memory
- host capability call count
- model token/cost budget
- output count/size
- scheduled invocation frequency

An Expert cannot schedule itself more frequently than host policy permits.
