# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.

# Platform priority

- Apple ecosystem devices are the first product target. Prioritize macOS, iPhone, and iPad implementation and validation.
- Android is out of scope for current implementation work. Do not spend time extending Android support, maintaining platform parity, or running Android builds/tests unless explicitly requested.
- Existing Android code may remain dormant; Apple delivery must not be blocked by Android compatibility or validation.

# Documentation reading policy

- Start at [docs/README.md](docs/README.md) and follow only the documents relevant to the task. Do not recursively read the entire documentation tree as default context.
- Current code and manifests define implementation reality; [current architecture](docs/architecture/README.md) describes the intended ownership of that reality. ADRs explain durable rationale; task-specific execution plans own temporary migration sequencing.
- Do not treat historical slice numbers, old validation matrices, removed crate paths or implementation-status prose inside ADRs as current state.
- If a task changes a durable architecture boundary, update the corresponding current architecture document in the same change set. If it changes the reason for a durable decision, amend or supersede the ADR rather than adding a second progress document.

# Engineering posture and architecture convergence

- Floe is pre-stable and under active development. Internal backward compatibility is not a default goal unless the current product requirement, an external protocol, or durable user data explicitly requires it.
- Optimize for the simplest correct final system, not the smallest diff. If the current ownership, contract, lifecycle or dependency structure is the root cause, change that structure instead of routing around it.
- Keep one semantic owner and one canonical runtime path for each concept. A completed change must not leave competing internal authorities, old/new execution paths or duplicate representations merely for transition convenience.
- Adapters belong at real provider, OS, storage, transport or other external boundaries. An adapter between competing internal designs is exceptional and must have explicit scope, a removal condition and a bounded lifetime.
- When replacing an internal contract, migrate the in-scope callers, delete the obsolete path, search for residual symbols/branches and verify the resulting architecture in the same change set.
- Avoid speculative abstractions, forwarding-only layers, migration-only optional state and unnecessary public/FFI surface. New indirection or state must represent a real boundary, owner, policy or demonstrated variation.
- Stop local patching and reassess the design when a change would require duplicated authority, a second internal compatibility path, public/FFI widening only to bridge old callers, migration-only optionality, dependency-direction violations or provider-specific data leaking into an owner contract.
- Repository-wide invariants are defined in [architecture invariants](docs/architecture/invariants.md); the development rationale and change policy are in [architecture evolution](docs/development/architecture-evolution.md).
- For architecture-affecting work, follow the repo-local [architecture-change skill](.agents/skills/architecture-change/SKILL.md). For code/build/runtime changes, finish with the [code-change-verification skill](.agents/skills/code-change-verification/SKILL.md).

# Local development data, schemas and compatibility

- Floe is currently used only for local testing. Backward compatibility for old local APIs and data is not required. Replace the current implementation directly; do not add v3/next paths, old decoders, migration chains or permanent Legacy bridges.
- Do not bump schema or protocol versions merely to preserve obsolete local compatibility. A real stored/wire meaning change must be deliberate, coordinated with all same-snapshot callers, and documented at the owning boundary. Authority revisions, executor generations and runtime epochs must still advance normally.
- A fixed schema number does not make an old binary or database compatible. Build Flutter, bindings and the bundled dylib from the same source snapshot; use an explicitly selected fresh Floe development profile when stored format or meaning changes.
- Do not automatically delete a database or replace an existing key after an open/access error. Any explicit reset is limited to identified Floe-owned local test data and exact key slots; preserve uncertain external-operation records. It never authorizes deleting source, unrelated files or connected-provider data.
- Do not globally rename external provider/A2A/OAuth versions, signed challenge fields or credential namespaces when removing app `_v2` aliases.

# Safety and scope

- Preserve authorization, exact-recipient consent, key identity, provenance, CAS, durable pre-dispatch intent, cancellation direction and uncertain external-write recovery. Do not weaken checks or regression assertions to make a build pass.
- Query, preview, observer timeout and screen disposal are not implicit Run cancellation. Do not hold a global Vault transaction while waiting for model or provider I/O.
- Task instructions do not authorize push, deployment or external-account changes unless the user explicitly requests them.
- Keep product requirements, current architecture, temporary execution plans and historical evidence separate. Start from source plus [docs/architecture/README.md](docs/architecture/README.md); read a task-specific plan only when the current task actually has one. Historical plans in Git history do not override current source, architecture or accepted ADRs.
