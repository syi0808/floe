# Package management

- Use pnpm for JavaScript/TypeScript dependencies and scripts in this repository, not npm or Yarn.
- Keep the package's `packageManager` version and `pnpm-lock.yaml` in sync; do not add npm or Yarn lockfiles.
- Use Flutter's `flutter pub` commands for Dart dependencies. Prefer current stable releases compatible with the supported Flutter SDK; do not override SDK-pinned dependencies solely to force newer versions.
