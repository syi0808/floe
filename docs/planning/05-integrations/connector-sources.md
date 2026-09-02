# Connector Sources & Reuse Strategy

> Status: Accepted direction

## Goal

Floe does not implement every third-party integration from zero.

It reuses mature open-source integration ecosystems as implementation references, edge-case catalogs, auth/API knowledge, and potential build-time conversion sources. It does **not** require their workflow/runtime architecture.

## Activepieces

Activepieces is the primary connector corpus currently under consideration.

Activepieces Pieces are TypeScript/npm packages and may contain arbitrary `run()` logic and npm dependencies.

Therefore Floe distinguishes:

```text
Source reuse
≠
Runtime compatibility
```

Preferred reuse:

```text
Activepieces Piece
      ↓
Inspect / Import / Port
      ↓
Floe ConnectorSpec or Native Connector
      ↓
Rust / Go
```

Node is not required in production merely because the original source was TypeScript.

## Reuse Levels

### Level 1 — ConnectorSpec Import

For standard HTTP/OAuth/pagination/webhook Pieces, attempt mechanical or assisted conversion into Floe's portable declarative connector format.

### Level 2 — Native Port

Port provider-specific logic to Go or Rust while preserving attribution/license requirements.

### Level 3 — Reference Only

Use Activepieces implementation as behavioral reference and implement independently when its framework/runtime assumptions are too strong.

## Build-time Importer

Potential tooling:

```text
TypeScript Piece Source
      ↓
OXC-based AST analysis
      ↓
Activepieces pattern recognizer
      ↓
Portable connector subset
      ↓
Floe ConnectorSpec
```

The converter should be conservative. If semantics cannot be represented safely, it reports unsupported behavior instead of attempting magical transpilation.

## Other Sources

The same strategy can apply to n8n connectors, official OpenAPI specifications, provider SDKs, and other permissively licensed connector catalogs.

Floe Connector Contract remains the canonical target.

## Licensing

Connector source reuse is evaluated per source/package. Activepieces Community Edition is MIT-licensed, while enterprise portions use a commercial license. Floe should preserve required copyright/license notices for ported code and avoid assuming every third-party dependency inside a Piece has the same license.
