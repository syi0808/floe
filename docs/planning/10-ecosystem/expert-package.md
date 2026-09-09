# Expert Package Format

> Status: Draft ecosystem contract

## Goal

Define a package that can represent first-party, user-created, and Marketplace Experts without exposing Floe internals.

Conceptual extension:

```text
.floeexpert
```

The exact archive format is TBD.

---

# Suggested Layout

```text
my-expert/
├─ expert.toml
├─ README.md
├─ config.schema.json
├─ prompts/
│  └─ ...
├─ rules/
│  └─ ...
├─ component/
│  └─ expert.wasm        # optional
├─ assets/
│  └─ icon.*
└─ LICENSE
```

## Manifest

Candidate fields:

```toml
id = "dev.example.job-search"
name = "Job Search Expert"
description = "채용 활동의 맥락, 후속 조치와 일정 현실성을 함께 검토하는 전문가"
version = "1.2.0"
expert_api = "1"
publisher = "example"

execution = "declarative" # declarative | component | builtin

[compatibility]
min_floe = "..."

[agent_card]
protocol_version = "1.0"
domain_tags = ["job-search", "communication", "schedule"]
skills = ["채용 과정의 다음 단계를 독립적인 관점에서 검토"]

[permissions]
read = [
  "mail.read.content",
  "timeline.read.range"
]
propose = [
  "task.create"
]

[triggers]
events = ["mail.received"]
schedules = ["daily"]
```

Exact syntax is not finalized.

The package metadata is projected into an A2A-aligned Agent Card. `description` and
`skills` are bounded discovery text, not callable commands, prompt instructions or
permission grants. Floe derives `supportedInterfaces` from the installed transport;
the package cannot claim an endpoint or authentication mode that the host does not
provide.

For the in-process runtime the Agent Card remains registry metadata. A future remote
binding may publish it through the standard discovery operation without changing the
package identity or Manager-facing compact projection.

---

# Dependency Types

An Expert may declare dependencies on Floe capabilities rather than concrete connector brands.

Good:

```text
requires mail.read
```

Less desirable:

```text
requires GmailConnector implementation X
```

Provider-specific dependency is allowed only when the service semantics actually matter.

---

# Configuration

Marketplace package configuration should be rendered from a host schema where possible.

This avoids arbitrary plugin UI.

Examples:

- select calendar
- choose people
- intervention frequency
- workday hours
- threshold

---

# Package Signing

Published packages should be content-addressed and signed.

Important identity:

```text
publisher
package id
version
content hash
signature
```

Self-host users may install unsigned packages only under an explicit developer/untrusted mode.

---

# Compatibility

Package must declare the Expert API version it targets.

Floe should evolve stable Expert APIs independently from internal implementation.

```text
Floe internals change
      ↓
Expert API remains compatible
```

Breaking Expert API versions can coexist for a migration period.
