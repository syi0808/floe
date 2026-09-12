# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.

# Local development data and compatibility

- Floe is currently used only for local testing. Local Floe application data is disposable and may be reset or deleted when needed for development or validation.
- Backward compatibility is not required. Prefer a clean current implementation over compatibility shims or migrations that exist solely to preserve old local test data, schemas, or APIs.
- Limit any reset to Floe-owned local test data. This permission does not cover source code, unrelated files, or data in connected external accounts and providers.
