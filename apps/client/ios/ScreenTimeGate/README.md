# Apple Screen Time feasibility gate

This package isolates the S5.5 iPhone/iPad Screen Time feasibility gate from the Flutter runner.
It uses only the public `FamilyControls` and `DeviceActivity` frameworks. It never selects,
exports, or persists application/domain tokens and contains no `ManagedSettings` shield mutation.

The output is either an explicit capability result or `attention.coarse`. Aggregate input exists
only inside the reducer call; exported JSON omits durations, interruption counts, app identity,
domains, notifications, pickups, and raw activity history.

## Live integration requirements

1. Add this package to the signed iOS/iPadOS app and report extension targets.
2. Enable the Family Controls capability for both targets and obtain Apple's distribution approval.
   Pass `entitlementProvisioned: true` only after inspecting the signed app and extension
   entitlements; iOS has no public runtime API that proves distribution approval.
3. Add a Device Activity Report extension that reduces aggregate segments before crossing the
   extension boundary. Do not return per-app, per-category, or per-domain rows.
4. Call `requestIndividualAuthorization` from an explicit user gesture on a physical device.
5. Record OS version, device class, signing identity, authorization result, entitlement result, and
   applicable storefront/region policy in acceptance evidence.

The package does not add entitlements or project integration and does not establish live or
distribution acceptance. Simulator and unit-test results are contract evidence only.
An unknown region gate remains `region_unknown`; callers must not promote it to `supported`.
