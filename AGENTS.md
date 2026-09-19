# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.

# Platform priority

- Apple ecosystem devices are the first product target. Prioritize macOS, iPhone, and iPad implementation and validation.
- Android is out of scope for current implementation work. Do not spend time extending Android support, maintaining platform parity, or running Android builds/tests unless explicitly requested.
- Existing Android code may remain dormant; Apple delivery must not be blocked by Android compatibility or validation.

# Active refactoring instructions

- Use [Stage 2](docs/refactoring/stage-2.md) as the active progress overview and read only the linked execution plan for the current Stage 2 step. [Stage 1](docs/refactoring/stage-1.md) is completed ownership context; [Stage 3](docs/refactoring/stage-3.md) and its linked execution plans are future product-boundary work.
- Stage 2 is the active progress source of truth. Its step-specific documents under `docs/refactoring/stage-2/` contain concrete execution plans. Follow the current checkpoint, vertical-cutover order, freeze rules and P0/P1/P2 triage. Do not recreate a versioned plan bundle, migration ledger, STATUS file, or parallel task board.
- One coding agent performs the refactoring sequentially. Keep one active change set in one workspace; build-tool parallelism is allowed.
- The Manager still selects Experts through A2A, and product Run/Task concurrency and cancellation scopes remain independent.
- Preserve current user changes. Check actual HEAD and working tree before editing; do not reset to historical plan commits.

# Stage execution policy

- Stage 1 ownership is closed. Do not reopen it for normal caller cutover, API cleanup or product-boundary work.
- Stage 2 converges the canonical internal runtime. Prefer a minimum sound foundation, then vertical owner cutover, old-path removal and integrated hardening.
- Use Stage 2 triage: P0 blocks only for authority/data-release risk, duplicate effects, budget/identity corruption, loss of durable pending work followed by re-planning, or ownership direction that blocks the next cutover. P1 waits for integrated hardening; P2 waits for cleanup.
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
- Keep product requirements, current architecture, current refactoring stages and historical evidence separate. Start with [Stage 2](docs/refactoring/stage-2.md) for active execution; [docs/architecture/README.md](docs/architecture/README.md) describes architecture-document ownership. Historical plans in Git history do not override the current Stage documents.
