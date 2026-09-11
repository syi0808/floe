# Apple context signing

The Runner requests only the capabilities needed by its current Apple context providers:

- HealthKit read access for the device-local derived wellbeing projection.
- WeatherKit for event-time weather used by the feasibility projection.

HealthKit clinical records, HealthKit background delivery, write access, and Family Controls are
not enabled. The Screen Time provider must continue to report `entitlement_unavailable` until a
separate Device Activity report extension exists and Apple approves the Family Controls
distribution entitlement.

## Development signing

The App ID matching `app.floe.floeClient` must have HealthKit and WeatherKit enabled in the Apple
Developer account. Regenerate development and distribution provisioning profiles after enabling
those services, and select the matching Team in Xcode. A locally signed build cannot exercise
either provider when the profile omits the corresponding entitlement, even though an unsigned
simulator build compiles.

Use an iPhone or an iPad running iPadOS 17 or later for HealthKit validation. Grant only the read
types requested by Floe and verify the derived view; the simulator and an empty Health store do not
provide live evidence. WeatherKit validation also requires a network-connected signed device and
the application must render the attribution URLs returned with every feasibility result.

Do not add Family Controls to `Runner.entitlements` as a development workaround. Screen Time live
validation requires its own approved App ID capability, provisioning profile, report extension,
and distribution entitlement before the native gate may set `entitlementProvisioned` to `true`.
