# Native Contacts acquisition

The Android Contacts bridge supports only an explicit finite selection of opaque
identity handles returned by `readContacts`. `inspectContactsSubject` re-queries
the provider-owned contact IDs while `READ_CONTACTS` is granted and returns a
native SHA-256 subject fingerprint. Permission revocation, deletion, or identity
replacement fails closed. The bridge does not expose all-account acquisition or
request permissions in the background.
