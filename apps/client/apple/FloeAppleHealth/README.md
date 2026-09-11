# Floe Apple Health

This standalone package contains the S5.5 Apple HealthKit provider foundation. It requests read
access only to sleep analysis, step count and Apple exercise time, performs a bounded 36-hour read,
and reduces the result on-device to the shared coarse `wellbeing.derived` contract.

The boundary object contains capacity, recovery, confidence and opaque per-window evidence handles.
It cannot encode raw samples, durations, counts, HealthKit metadata, source revisions or provider
identifiers. Its in-memory View expires after 30 minutes.

HealthKit does not reveal whether read access was denied. Consequently
`no_data_or_read_access_limited` deliberately represents both an empty store/window and read access
that the user may have withheld or later changed. The provider never reports a definite revoked
state that HealthKit cannot prove.

`HealthKitWellbeingProvider.currentHostProvider(sourceHandle:)` is the native wiring entry point.
It rejects Mac Catalyst and unavailable Health stores, and requires iPadOS 17 or later while keeping
supported iPhone hosts available. The Flutter channel, Xcode target membership, HealthKit capability,
usage descriptions and signing entitlements remain host integration work.
