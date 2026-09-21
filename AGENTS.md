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
- Current code and manifests define implementation reality; [current architecture](docs/architecture/README.md) describes the intended ownership of that reality. ADRs explain rationale; Stage documents own refactoring execution.
- Do not treat historical slice numbers, old validation matrices, removed crate paths or implementation-status prose inside ADRs as current state.
- If a task changes a durable architecture boundary, update the corresponding current architecture document in the same change set. If it changes the reason for a durable decision, amend or supersede the ADR rather than adding a second progress document.

# Active refactoring instructions

- Use [Stage 3](docs/refactoring/stage-3.md) as the active progress overview and read only the linked execution plan for the current Stage 3 step. [Stage 1](docs/refactoring/stage-1.md) is completed ownership context and [Stage 2](docs/refactoring/stage-2.md) is the completed/frozen internal runtime.
- Stage 3 is the active progress source of truth. Its step-specific documents under `docs/refactoring/stage-3/` contain concrete execution plans. Follow the current checkpoint and step order. Do not recreate a versioned plan bundle, migration ledger, STATUS file, or parallel task board.
- One coding agent performs the refactoring sequentially. Keep one active change set in one workspace; build-tool parallelism is allowed.
- The Manager still selects Experts through A2A, and product Run/Task concurrency and cancellation scopes remain independent.
- Preserve current user changes. Check actual HEAD and working tree before editing; do not reset to historical plan commits.

# Stage execution policy

- Stage 1 ownership is closed. Do not reopen it for normal caller cutover, API cleanup or product-boundary work.
- Stage 2 converged the canonical internal runtime and is closed/frozen. Do not reopen it for product-boundary work; Stage 3 carries that runtime through the remaining callers.
- Stage 2 triage (P0/P1/P2) closed with the frozen Stage 2 slices; do not repurpose it to gate or delay Stage 3 product-boundary work.
- Stage 3 owns AppHost/FFI/Flutter/native/server product-boundary cutover, remaining root/domain callers, outer compatibility deletion and end-to-end product validation.
- A short compile break may be resolved inside the same contract change set. Do not accumulate unrelated broken targets or add wrappers merely to preserve an old caller.
- Preserve authorization, exact-recipient consent, key identity, provenance, CAS, durable pre-dispatch intent, cancellation direction and uncertain external-write recovery.

# Local development data, schemas and compatibility

- Floe is currently used only for local testing. Backward compatibility for old local APIs and data is not required. Replace the current implementation directly; do not add v3/next paths, old decoders, migration chains or permanent Legacy bridges.
- Do not increase schema numbers during this refactor. The reviewed source has app wire 2 and Conversation storage 7. If the checkout has already changed, record it rather than rolling numbers back. Authority revisions, executor generations and runtime epochs must still advance normally.
- A fixed schema number does not make an old binary or database compatible. Build Flutter, bindings and the bundled dylib from the same source snapshot; use an explicitly selected fresh Floe development profile when stored format or meaning changes.
- Do not automatically delete a database or replace an existing key after an open/access error. Any explicit reset is limited to identified Floe-owned local test data and exact key slots; preserve uncertain external-operation records. It never authorizes deleting source, unrelated files or connected-provider data.
- Do not globally rename external provider/A2A/OAuth versions, signed challenge fields or credential namespaces when removing app `_v2` aliases.

# Safety and scope

- Preserve authorization, exact-recipient consent, key identity, provenance, CAS, durable pre-dispatch intent, cancellation direction and uncertain external-write recovery. Do not weaken checks or regression assertions to make a build pass.
- Query, preview, observer timeout and screen disposal are not implicit Run cancellation. Do not hold a global Vault transaction while waiting for model or provider I/O.
- Refactoring instructions alone do not authorize push, deployment or external-account changes; follow the user's explicit scope for the current task.
- Keep product requirements, current architecture, current refactoring stages and historical evidence separate. Start with [Stage 3](docs/refactoring/stage-3.md) for active execution; [docs/architecture/README.md](docs/architecture/README.md) describes architecture-document ownership. Historical plans in Git history do not override the current Stage documents.
