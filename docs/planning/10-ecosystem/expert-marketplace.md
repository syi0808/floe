# Expert Marketplace

> Status: Long-term product direction

## Purpose

Floe's built-in Experts cover general personal-assistant domains, but users have long-tail needs.

Examples:

- job search / recruiting
- study planning
- travel preparation
- pet care
- project-specific routine
- family coordination
- finance organization
- developer workflow

A Marketplace can distribute domain expertise without bloating the core product.

---

# Marketplace Unit

The product being distributed is an `ExpertPackage`, not an unrestricted application plugin.

Marketplace listing should surface:

- publisher
- permissions
- data categories accessed
- execution location
- network access
- models used/required
- Connector capability dependencies
- intervention behavior
- source/license status

Privacy impact should be visible before installation.

---

# Installation Flow

Example:

```text
Install "Job Search Expert"

Can access
✓ Email content
✓ Calendar availability
✓ Job-search memory projection

Can propose
✓ Create tasks
✓ Create calendar blocks

Cannot
- Send email automatically
- Access health data
- Access raw relationship memory
- Use arbitrary network

[Install]
```

The permission card is part of the product UX, not a developer-only manifest view.

---

# Trust Levels

Possible levels:

```text
Built by Floe
Verified Publisher
Community
Local / Developer
```

Trust level does not silently grant extra runtime permissions.

It mainly affects:

- review expectations
- update defaults
- warning UI
- discovery/ranking

---

# Sensitive Domains

Some categories should receive stronger marketplace policy.

Examples:

- health
- finance
- family/children
- highly sensitive personal memory

An Expert requesting high-sensitivity views may require additional review or may be prohibited from arbitrary network egress.

---

# Updates

Automatic update is safe only when permission surface does not expand.

```text
v1 → v1.1
same permissions
→ auto-update allowed by policy

v1.1 → v2
adds mail.read.content
→ user re-approval required
```

---

# Marketplace Is Optional

Self-hosting must not depend on the Floe Marketplace.

Users should be able to install:

- local package
- private registry package
- source-built package

Marketplace is a discovery/trust/distribution layer, not a runtime dependency.
