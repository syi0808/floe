# Apple Feasibility Provider

This Swift package implements the device-native foundation for `schedule.feasibility` on iOS,
iPadOS and macOS. It performs one explicit current-location request, requests an ETA for an
explicit event destination, and reduces the event-window WeatherKit forecast to a coarse impact.
Only the bounded view leaves the package; coordinates and forecast details are never retained or
included in its output.

The app target integrating this package must provide `NSLocationWhenInUseUsageDescription`, enable
WeatherKit for its App ID and target, and carry a valid WeatherKit entitlement/provisioning profile.
Weather attribution links returned beside the provider-neutral view must be rendered wherever the
weather result is displayed. The concrete provider requires a signed app and live network/provider
access; package tests inject protocol-backed fixtures and make no location or WeatherKit request.
