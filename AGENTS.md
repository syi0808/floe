# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.

# Local development data and compatibility

- Floe is currently used only for local testing. Local Floe application data is disposable and may be reset or deleted when needed for development or validation.
- Backward compatibility is not required. Prefer a clean current implementation over compatibility shims or migrations that exist solely to preserve old local test data, schemas, or APIs.
- Limit any reset to Floe-owned local test data. This permission does not cover source code, unrelated files, or data in connected external accounts and providers.

# Development and validation workflow

- Prioritize completing usable, end-to-end features during active development. Implement the feature first, then run it and directly exercise the relevant user flow.
- Add focused regression tests after validating behavior, targeting bugs actually found and important behavior that must not break. Do not write exhaustive tests for every component or edge case as a prerequisite to implementation.
- Keep validation proportional to the change: use relevant build checks and targeted tests while iterating. Avoid repeatedly running broad suites before the feature is integrated.
- Preserve essential authorization and data-loss protections. Do not weaken safety checks or existing regression assertions merely to accelerate development or make tests pass.
- Report what was directly exercised, what regression coverage was added, and any unverified behavior. If direct validation is unavailable, state the limitation rather than treating additional test scaffolding as feature completion.
