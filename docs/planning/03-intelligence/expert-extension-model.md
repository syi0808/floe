# Expert Extension Model

> Status: Core extensibility direction

## Goal

Floe ships with first-party experts such as Health, Schedule, and Communication, but **Expert is an extension point** rather than a closed internal class hierarchy.

Users should eventually be able to:

- install third-party experts
- create private experts for themselves
- configure an expert per Person
- disable or replace built-in experts
- publish experts to a marketplace

The product still presents **one Manager Secretary** to the user.

```text
Installed Experts
├─ Health
├─ Schedule
├─ Communication
├─ Job Search            # user-installed
└─ Family Routine        # user-created
       ↓
Manager Secretary
       ↓
User
```

Experts are advisors. They do not become independent user-facing personalities by default.

---

# Expert as a Semantic Contract

An Expert is conceptually:

```text
Expert
├─ identity / metadata
├─ trigger subscriptions
├─ required domain views
├─ required skills/capabilities
├─ memory view
├─ private state
├─ model/heuristic logic
├─ output contract
└─ permission declaration
```

The contract must not depend on how the Expert is implemented.

Possible implementations:

```text
Built-in native Expert
Declarative Expert
Sandboxed code Expert
```

---

# Package vs Installation vs Instance

These concepts are deliberately separated.

## ExpertPackage

Shareable artifact.

```text
ExpertPackage {
  id
  version
  publisher
  apiVersion
  manifest
  logic
  assets
}
```

A Marketplace distributes packages.

## ExpertInstallation

An ExpertPackage installed into one Floe Instance.

Contains:

- approved package version
- trust status
- enabled/disabled state
- instance-wide policy

## ExpertAssignment

Binding of an installed Expert to one `Person`.

```text
ExpertAssignment {
  expertInstallationId
  personId
  configuration
  grantedPermissions
  stateNamespace
}
```

This matters because a self-hosted family instance can install an Expert once while different Persons choose different permissions/configuration.

---

# Built-in Experts

Built-in Health/Schedule/Communication experts should use the **same semantic Expert contract** where practical.

They do not have to execute in the same sandbox as third-party code.

```text
Built-in Expert
→ trusted/native implementation
→ same Trigger / View / Output semantics

Marketplace Expert
→ restricted runtime
→ same Trigger / View / Output semantics
```

This avoids designing one internal architecture and a second weaker plugin architecture.

---

# Event-driven Lifecycle

Third-party Experts are not allowed to create arbitrary permanent daemon loops.

They react to host-defined triggers.

```text
Trigger
  ↓
Expert Invocation
  ↓
Read granted Views
  ↓
Compute
  ↓
Structured Output
  ↓
Manager / Policy
```

Trigger examples:

```text
calendar.changed
mail.received
person.updated
state.condition.changed
morning.summary
user.requested
schedule.daily
```

Scheduled triggers are registered through Floe; the Expert itself does not own an unrestricted scheduler.

---

# Inputs: Views, Not Database Access

Experts never receive direct Turso/database access.

Instead Floe exposes stable domain views.

Examples:

```text
TimelineView
PeopleView
MemoryView
HealthStateView
CommunicationView
CurrentStateView
```

A permissioned Health expert might receive:

```text
HealthStateView {
  recovery
  sleepDebt
  activityLoad
  trends
}
```

rather than raw HealthKit samples.

This is both an API stability boundary and a privacy boundary.

---

# Connector Dependencies

Experts do not own connector credentials.

An Expert declares capabilities it needs.

Example:

```text
JobSearchExpert
requires:
  mail.search
  timeline.read
  task.create.propose
```

At install/enable time Floe resolves whether the Person has compatible ConnectorConnections.

```text
Expert
  ↓ requires
Capability
  ↓ provided by
Connector / Floe-native domain
```

Connector and Expert ecosystems remain separate.

---

# Outputs

Experts return structured candidates rather than mutating product state arbitrarily.

Initial output types:

```text
InsightCandidate
InterventionCandidate
ActionProposal
MemoryCandidate
StateSuggestion
```

Examples:

```text
Health Expert
→ InterventionCandidate("오늘 운동 강도를 낮추는 편이 좋음")

Job Search Expert
→ ActionProposal(CreateTask(...))

Relationship Reminder Expert
→ InsightCandidate(...)
```

The Manager decides whether and how these reach the user.

---

# Memory Rules

Every Expert receives a private state namespace.

```text
Person
└─ ExpertAssignment
   └─ Private Expert State
```

An Expert cannot directly write authoritative Personal Memory.

It can produce:

```text
MemoryCandidate
```

which passes through the normal Memory Policy and provenance pipeline.

This prevents a marketplace package from silently rewriting who the user is.

---

# UI Rules

Marketplace Experts do not get arbitrary Flutter widget execution in the initial design.

Allowed UI surfaces are host-rendered schemas such as:

- Expert settings
- permission card
- status/health
- Intervention
- Insight details
- Action confirmation

This preserves Floe's Calm UI principle.

Future structured Day Canvas contributions may be added, but arbitrary plugin UI is not a baseline capability.
