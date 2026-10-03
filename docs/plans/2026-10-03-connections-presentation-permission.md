# Connections cards and combined source permission

The user explicitly requested restoring Connections service cards (including macOS Calendar), keeping the removed Read-only and Source details presentation absent, and avoiding a separate approval for Gateway LLM requests. Calendar and Conversation flow redesign is outside this change.

## Presentation

Connections again uses responsive owner-backed service cards and current detail panels. Current IntegrationSummary/SourceSummary refs, availability and allowed actions remain authoritative. Linked sources are not duplicated. Gateway setup remains accessible in its current place; Settings navigation, readiness gates and recovery behavior are unchanged. Old controllers, raw source details and device-binding rows are not restored.

## Permission meaning

There was no active per-request model-recipient approval. The extra-looking prompt came from enabling a new source as DeviceOnly, then asking for a later source grant expansion when Gateway reasoning needed it. A genuinely absent grant now includes GatewayAllowed for trusted View categories in its initial immutable source review. The user still explicitly approves that source review. Settings defaults the requested review scope to the same combined permission and discloses processing before Allow. Intentional DeviceOnly remains available; existing grants are never silently expanded, and an explicit source review is required to change them.

Source identity, resource/category/consumer bounds, Context/Access live checks, pairing identity, OS connector permissions and external Action confirmation are unchanged. Health's only registered view is wellbeing.derived with Derived categories; raw Health remains local and the mandatory device transform/live receipt checks remain. Obsolete copy claiming server model use needs separate approval is removed.

This completed source boundary awaits coherent formatting, Rust compilation and Dart analysis before app builds. No real grants, provider calls, Keychain operations or permissions have been changed by the implementation tools.
