# Headless dashboard DOM tests

This package runs the checked-in dashboard `index.html` and `app.js` in jsdom.
The DOM harness replaces `fetch`, timers, confirmation, `Date.now`, and the
timeout signal with bounded deterministic test doubles. It does not load linked
resources, open a browser, contact a server, or read a Floe profile.

Run from this directory:

```sh
pnpm install --frozen-lockfile
pnpm test
```

The suite checks dashboard DOM state and event-handler behavior, including
login/session handling, pairing action visibility, request identity and CSRF,
refresh coalescing, and stale response fencing. This is not browser cookie,
layout, visual, or end-to-end qualification.

The in-process HTTP handler integration tests live alongside the server
transport package. Run them from `server/` with:

```sh
go test ./internal/transport/http
```
