# ADR 0011: Inference performance classes and device-local bypass

## Status

Accepted as reusable inference infrastructure, as amended by [ADR 0034](0034-gateway-reasoning-and-source-processing-authority.md).

## Amendment by ADR 0034

Product-purpose selection and Gateway-owned provider/model configuration remain accepted. The external-transfer-consent discovery/response language below is superseded: the client sees purpose availability and, when useful, Device/Gateway processing, never a concrete downstream recipient or a model-approval requirement. GatewayAllowed is source-owned processing policy and includes the Gateway's configured downstream providers.

The previously separate fallback-policy decision is now fixed for Manager, shipped Experts and Learner: Gateway Primary, device-local Fallback only on valid planning-time Primary absence. Credential, inventory, transport, timeout and source-permission failures are not absence. Internal operator/profile configuration remains possible without becoming product intent or user authority.

The original text below is retained as rationale where not superseded, including historical API/version references. This amendment does not assert that the replacement protocol or routing is already implemented; the [convergence plan](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/development/plans/reasoning-source-processing-convergence.md) owns the cutover.

## Context

Floe users should not need to understand providers, model identifiers, context
windows, or reasoning controls. Exposing a gateway target in the app also couples
product behavior to deployment configuration and makes a server-side model change
require client reconfiguration.

Some inference can execute inside the Floe app through a native runtime such as
Apple Foundation Models. Sending that work through a loopback HTTP gateway adds
an unnecessary trust boundary and makes device-local behavior harder to explain.

## Decision

The server keeps three internal performance classes:

- `fast`: latency-sensitive, low-cost work.
- `balanced`: the default trade-off for ordinary assistance.
- `high_effort`: quality-sensitive work that benefits from stronger reasoning.

The application-facing v2 contract selects a product purpose, never a class or model:

- `quick_response` maps to `fast`;
- `everyday_assistance` maps to `balanced`;
- `deep_work` maps to `high_effort`.

Product policy, not an end-user setting, selects the purpose. The Go server maps it
to an administrator-managed
internal target and reasoning effort. Provider, endpoint, model, and reasoning
settings are visible only in the server dashboard; target identifiers remain an
internal persistence and execution detail.

Authenticated apps may query purpose availability and whether the selected route
requires external-transfer consent. They cannot discover the underlying target,
provider, model, endpoint, or reasoning effort. Generation responses identify the
requested purpose, placement and external-transfer fact, not the resolved model.
The v1 class endpoint remains a compatibility surface; new clients use
`/v1/inference-purposes` and `/v1/generate`.

Device-local inference does not use the Go gateway. It implements the same domain
model interface inside the native client boundary. A model process running beside
the Go server, including Ollama, is still server-side inference from the client's
perspective and therefore remains behind the gateway.

## Consequences

- Operators can replace models and tune reasoning without updating paired apps.
- User-facing flows describe the task and data boundary rather than model jargon.
- Features must deliberately select a stable product purpose.
- The server must reject missing or invalid class routes without falling back to an
  undisclosed model.
- Device-local selection and fallback policy remain a separate client runtime task.
