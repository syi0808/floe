# FloeAppleContacts

Contacts acquisition is limited to an explicit finite selection of provider-issued
opaque identity handles. `inspectSelectedSubject` re-resolves those handles against
the current Contacts authorization state and returns a native subject fingerprint;
the fingerprint is generated and checked in the provider, never fabricated in Dart.
Deleted or no-longer-readable contacts fail closed. Full account-scope acquisition
is intentionally not exposed by this contract.

`FloeAppleContacts` is a read-only Apple Contacts provider for Floe's
`people.identity` View. It supports iOS/iPadOS and macOS without importing contact
notes, postal addresses, birthdays, images, or write authority.

## Host integration

1. Add this directory as a local Swift package to each Apple Runner target.
2. Add `NSContactsUsageDescription` to the target's `Info.plist`.
3. Generate and persist at least 32 random bytes in the device Keychain. Pass the
   same device-local secret to `AppleContactsProvider` after each launch; never sync
   the secret or derive it from a user identifier.
4. Expose `connectionSnapshot()`, `requestAuthorization()`, and
   `readPeopleView(selection:limit:)` through the platform channel.
5. Encode the returned value with `JSONEncoder`. Its coding keys match the Rust
   `PeopleView` contract.

`limited` means the system-selected Contacts subset on iOS/iPadOS. An explicit
`identityHandles` selection further narrows output using only opaque handles.
Missing selections yield `coverage_complete: false`; they never cause a fallback
to the full authorized set.

The projection is capped at 32,768 encoded JSON bytes to match Rust's Personal
Context budget. It deterministically removes trailing aliases first and then
trailing identities. Any budget reduction sets `coverage_complete` to `false`.

The host must not persist a returned View beyond its expiry or interpret it as an
address-book mirror. Relationship memories remain separate, confirmed Floe data.
