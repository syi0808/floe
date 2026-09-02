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
version = "1.2.0"
expert_api = "1"
publisher = "example"

execution = "declarative" # declarative | component | builtin

[compatibility]
min_floe = "..."

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
