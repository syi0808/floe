# Sleep-window aggregation check

Compile and run the production acquisition helper with synthetic input only:

```sh
output="$(mktemp -d)/sleep-window"
swiftc -warnings-as-errors \
  apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitAcquisition.swift \
  apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/SleepWindowAggregation.swift \
  tools/validation/health/sleep-window.swift -o "$output"
"$output"
```

The 17 checks exercise clipping, union/duplicates, awake-only and empty results,
200 segments, the complete 4,096-record budget, overflow and invalid input. The
provider requests one extra sample and refuses overflow before returning any
aggregate. Missing/malformed sample results fail closed. This budget is not a
claim that HealthKit exposes every sample: unavailable or read-restricted data
retains the existing semantics.

The overlap predicate follows Apple's [HKQueryOptions contract](https://developer.apple.com/documentation/healthkit/hkqueryoptions):
no strict endpoint option includes samples crossing the observation window;
aggregation clips them locally. Quantity-statistics query policy is unchanged.

Linux Swift execution qualifies this Foundation-only aggregation. It does not
qualify iOS HealthKit queries, authorization, local-model privacy transformation
or native UI. Those require the separate approved Apple test environment. Never
copy real Health samples into fixtures or logs.
