# ADR 0001: Restart Floe as a product-first greenfield repository

- **Date:** 2026-09-02
- **Status:** accepted

## Context

The prior repository embodied an earlier multi-agent Python implementation and accumulated documentation, deployment files, credentials, and code that do not establish the current product boundary. The current direction defines Floe around a personal timeline, state, memory, integration fabric, and calm end-user experience—not agent topology.

## Decision

Remove all prior tracked project content while retaining the existing MIT `LICENSE`. Begin the replacement repository with product documentation and defer application-stack selection until the first domain and interaction slice is specified.

## Consequences

- Historical code remains recoverable in Git history but is not an active dependency.
- Old credentials, deployment manifests, tests, and agent-specific abstractions are deliberately excluded.
- New implementation work starts from the Personal Day MVP rather than reproducing the former architecture.
- The attached planning bundle is source material, not copied product content; decisions must be revalidated through dogfooding and technical PoCs.

## Revisit when

Revisit this decision if an old component is proposed for reuse. It must first be evaluated against the current product brief and imported intentionally, with a new ADR.