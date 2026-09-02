# Floe Client

Flutter client for Floe's Personal Day experience.

## Run on macOS

```sh
flutter pub get
flutter run -d macos
```

The current adapter is `FakeDayGateway`, which exercises the complete capture and explicit-classification UI without bypassing the planned Rust gateway boundary. Replace it with the versioned FFI adapter in the next integration slice.

## Validate

```sh
flutter analyze
flutter test
flutter build macos
```
