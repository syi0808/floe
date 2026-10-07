# Behavior verification

The [active execution plan](../plans/2026-10-02-architecture-refactor.md) owns the current sequence.

- T1: real Rust Conversation/Engine/Experts/Inference/Access/Vault; only external model/source/tool I/O is scripted. This is the primary repeatable regression layer.
- T2: real Rust/Go pairing, signed authority, HTTP and provider codecs, with external endpoints mocked.
- T3: a small production Flutter/FFI UI set, headless or actual desktop.
- T4: actual macOS EventKit/TCC/signing/production Keychain qualification.

The harness being planned or a generic Flutter sample passing does not establish Floe integration. Record the exact source, command, result and unverified coverage. Avoid per-file build/test loops and repeated full builds. [Test performance](../development/test-performance.md) covers normal commands and cache policy.

[Legacy behavior records](legacy-behavior/README.md) remain evidence for reconstruction. Historical migration command sheets and passing counts are in Git history rather than the active reading hierarchy.
