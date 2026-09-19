# ADR 0003: Adopt native-first connectors and capability-based Experts

- **Date:** 2026-09-02
- **Status:** accepted as a recommended baseline; validate through Phase 0 PoCs
- **Supersedes:** the Connector Host portion of ADR 0002

## Context

Planning v0.8 replaces the default Node/TypeScript Connector Host with a native-first connector model and establishes Experts as a public extension boundary. Floe must preserve self-host simplicity, privacy boundaries, and one calm user-facing assistant while enabling future user-created and marketplace functionality.

## Decision

### Connectors

- Default runtime has **no Node/TypeScript dependency**.
- Device-owned, OS-private, or sensitive sources run through native adapters and Rust Core.
- Always-online SaaS sources run in the Go server/control plane.
- Standard OAuth/REST/pagination/webhook/polling patterns may be expressed as portable `ConnectorSpec` definitions consumed by Go/Rust runtimes.
- Activepieces is a build-time implementation corpus and reference source. Drop-in execution or full TypeScript-to-Rust translation is not a product goal.
- Initial connector domains are Calendar, Mail, Contacts, and Health. Tasks and Notes are Floe-native; Voice, Location, Notifications, Secure Storage, and Invocation are Device Providers.
- Retention defaults are Calendar = Mirror, Mail = Index + On-demand, Contacts = Identity Reference, Health = Derived Only.

### Experts

- Expert is a public semantic contract with triggers, permissioned domain views, structured outputs, configuration, and private assignment-scoped state.
- The Manager Secretary remains the single user-facing assistant; Experts advise rather than independently address the user.
- `ExpertPackage`, `ExpertInstallation`, and per-Person `ExpertAssignment` are separate concepts.
- Declarative Experts are the default extension mechanism. Sandboxed code Experts target WebAssembly Component Model/WIT with Wasmtime as the desktop/server candidate.
- Marketplace or third-party Experts are deny-by-default: no direct database, credentials, raw health, unrestricted network/filesystem, direct authoritative memory mutation, arbitrary scheduling, or arbitrary Flutter UI.
- Experts may emit structured candidates only. Memory writes pass through the normal policy/provenance pipeline, and all actions pass through the existing Action Authority boundary.

## Consequences

- The repository layout moves connector logic into Go server code, Rust/device-native connector traits, and `connectors/spec`/build-time importers; no default `connector-host` app is needed.
- Connector source data remains untrusted content. It cannot become a system prompt, persistent policy, or instruction.
- Third-party capabilities require explicit, semantic, scoped grants and re-approval whenever a package expands its permissions.
- The extension contract, sandbox behavior, resource budgets, audit events, and ConnectorSpec portability require dedicated PoCs before ecosystem rollout.

## Historical planning note

The imported planning bundle that informed this ADR contained duplicated internal D-number labels. That bundle is now retained only in Git history. This ADR, the current product documents and current architecture are the active sources for the durable connector/Expert boundary.

## References

- [Manager, Experts and extensions](../product/intelligence.md)
- [Integrations, privacy and distribution](../product/integrations-and-privacy.md)
- [Current architecture](../architecture/README.md)
