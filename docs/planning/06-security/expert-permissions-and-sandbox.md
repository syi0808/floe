# Expert Permissions & Sandbox

> Status: Core marketplace security boundary

## Threat Model

An Expert may be authored by a third party while Floe contains:

- email
- health-derived state
- relationships
- personal memory
- location/context
- calendar
- connector action authority

Therefore a Marketplace Expert must be treated as **untrusted code/configuration**.

---

# Deny by Default

An installed Expert receives no implicit access to:

- Personal Memory
- connectors
- network
- filesystem
- raw Health
- other Experts' state
- arbitrary model providers
- action execution

Every capability is explicitly granted.

---

# Permission Families

## Domain Read

Examples:

```text
timeline.read.current_day
timeline.read.range
people.read.basic
memory.read.projected
health.read.derived
mail.read.metadata
mail.read.content
```

Permissions should be semantic and scoped, not table-level.

## Action Proposal

Examples:

```text
task.create.propose
calendar.move.propose
mail.draft.propose
mail.send.propose
```

Even with proposal permission the normal Floe Action Authority policy still applies.

## Memory

Examples:

```text
memory.candidate.write
expert_state.read
expert_state.write
```

Direct authoritative memory write is not exposed to third-party Experts.

## Network

Default: denied.

Experts should use Floe Connectors/Skills rather than arbitrary outbound HTTP.

A future `network.egress` permission may allow explicitly declared domains, but it is high risk and requires prominent review.

---

# Permission Projection

The same underlying data can expose multiple privacy levels.

Example:

```text
Health
├─ raw samples                  # system/internal only by default
├─ derived detailed state      # sensitive
└─ coarse condition projection # lower exposure
```

An Expert's manifest asks for the minimum view required.

---

# Permission Changes

Package update rules:

```text
same or reduced permissions
→ normal update policy

new permission / broader scope
→ explicit user re-approval
```

Silent privilege expansion is forbidden.

---

# Runtime Isolation

For code-based third-party Experts, a WebAssembly sandbox is the preferred direction for desktop/server execution.

The WebAssembly Component Model is useful because the host can expose only explicit typed interfaces rather than a general process environment.

Candidate runtime:

```text
Wasmtime
+ Component Model / WIT
```

Host imports define the Expert capability surface.

The guest is not given generic WASI filesystem/network capabilities unless explicitly required.

Resource controls should include:

- memory limit
- execution timeout/interruption
- CPU/fuel budget where appropriate
- output size limit
- invocation concurrency limit

---

# Mobile Third-party Code

Arbitrary third-party native code is not a baseline mobile extension mechanism.

Initial mobile strategy:

```text
Declarative Expert
→ host-provided primitives
→ local execution possible

Code Expert
→ server or desktop sandbox
```

Built-in Experts can still use native Swift/Kotlin/Rust code.

This avoids turning iOS/Android clients into arbitrary code hosts.

---

# Credential Isolation

Experts never receive OAuth refresh tokens/API keys.

```text
Expert
  ↓ capability call
Floe Host
  ↓ policy
Connector
  ↓ credential vault
External Service
```

The host mediates every access.

---

# Audit

For third-party Expert invocations, Floe should be able to record:

- package/version
- trigger
- capabilities used
- data view categories accessed
- model/provider usage
- output type
- proposed actions

Audit records should avoid storing sensitive payloads unless necessary.
