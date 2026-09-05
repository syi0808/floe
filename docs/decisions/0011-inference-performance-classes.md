# ADR 0011: Inference performance classes and device-local bypass

## Status

Accepted for S2.

## Context

Floe users should not need to understand providers, model identifiers, context
windows, or reasoning controls. Exposing a gateway target in the app also couples
product behavior to deployment configuration and makes a server-side model change
require client reconfiguration.

Some inference can execute inside the Floe app through a native runtime such as
Apple Foundation Models. Sending that work through a loopback HTTP gateway adds
an unnecessary trust boundary and makes device-local behavior harder to explain.

## Decision

The client-to-server contract uses one of three product-owned performance classes:

- `fast`: latency-sensitive, low-cost work.
- `balanced`: the default trade-off for ordinary assistance.
- `high_effort`: quality-sensitive work that benefits from stronger reasoning.

Product code, not an end-user setting, selects the class. The S2 focus suggestion
requests `high_effort`. The Go server maps each class to an administrator-managed
internal target and reasoning effort. Provider, endpoint, model, and reasoning
settings are visible only in the server dashboard; target identifiers remain an
internal persistence and execution detail.

Authenticated apps may query class availability and whether the selected route
requires external-transfer consent. They cannot discover the underlying target,
provider, model, endpoint, or reasoning effort. Generation responses identify the
requested class, not the resolved model.

Device-local inference does not use the Go gateway. It implements the same domain
model interface inside the native client boundary. A model process running beside
the Go server, including Ollama, is still server-side inference from the client's
perspective and therefore remains behind the gateway.

## Consequences

- Operators can replace models and tune reasoning without updating paired apps.
- User-facing flows describe the task and data boundary rather than model jargon.
- Features must deliberately select a stable performance class.
- The server must reject missing or invalid class routes without falling back to an
  undisclosed model.
- Device-local selection and fallback policy remain a separate client runtime task;
  S2 currently exercises the server-backed `high_effort` path.
