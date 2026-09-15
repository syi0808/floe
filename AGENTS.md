# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.

# Platform priority

- Apple ecosystem devices are the first product target. Prioritize macOS, iPhone, and iPad implementation and validation.
- Android is out of scope for current implementation work. Do not spend time extending Android support, maintaining platform parity, or running Android builds/tests unless explicitly requested.
- Existing Android code may remain dormant; Apple delivery must not be blocked by Android compatibility or validation.

# Active refactoring instructions

- Resolve the active document edition from [docs/refactoring/README.md](docs/refactoring/README.md). Use that edition's PLAN, EXECUTION_PLAN, step files and AGENT_PROMPT together; do not mix prescriptions from different editions.
- Refactoring documents have version history under `docs/refactoring/versions/`. A document edition is not an application or schema version. Preserve prior editions; record meaningful plan changes in a new edition and update the index/changelog.
- [docs/refactoring/migration-ledger.md](docs/refactoring/migration-ledger.md) is the only mutable progress record. Read its current checkpoint, then the relevant execution step and actual callers. Do not repeatedly reread historical bundles or create another STATUS file.
- One coding agent performs the refactoring sequentially, including investigation, implementation, review, integration and validation. Do not spawn subagents or delegate reviews. Keep one active change set in one workspace; build-tool parallelism is allowed.
- This workflow does not change Floe's product architecture: the Manager still selects Experts through A2A, and product Run/Task concurrency and cancellation scopes remain independent.
- Preserve current user changes. Check the actual HEAD and working tree before editing; do not reset to the plan's historical source anchor.

# Structure first, behavior validation second

- Stage A implements the approved module boundaries, single state owners, concrete repositories/adapters and actual callers, then removes the corresponding old paths. Empty traits, mock-only composition, fake success and disabling supported features do not complete Stage A.
- During Stage A, use relevant type/compile, dependency, public-boundary and old-reference checks. Do not require live app, Keychain, OAuth, real LLM or full-suite success after every structural change.
- Run a narrow controlled check early only when a feasibility assumption could invalidate the design, or when changing a high-risk invariant such as authority, keys, transaction atomicity, duplicate external writes or child cancellation. Reuse existing focused regressions; do not build exhaustive scaffolding as a prerequisite.
- Stage B follows the structure review and exercises regression, integration, the actual Apple app and model/provider behavior. Diagnose the normal-app Vault failure there; preserve safe key/Vault incident classification in Stage A.
- A short compile break may be resolved within the same contract change set. Do not accumulate unrelated broken targets or add compatibility wrappers just to keep the old app running.
- Record structure, wiring/removal, actual checks and behavior evidence separately. Structure complete with behavior `not_run` is not product acceptance. Do not copy an old pass to a new source snapshot.

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
- Keep product requirements, architecture, versioned refactoring plans, mutable state and historical evidence separate. Start at [docs/refactoring/README.md](docs/refactoring/README.md) for execution; [docs/architecture/README.md](docs/architecture/README.md) describes document ownership. Archived plans and old slice next-demo lists do not override the active edition.
