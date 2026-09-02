# Open questions

**Status:** active decision backlog  
**Last updated:** 2026-09-02

## Resolve before the first implementation commitment

1. **Initial client surface:** native macOS, web-first, or another shell for the Day Canvas?
2. **Local store:** which persistence model best supports provenance, edits, deletion, and future sync?
3. **MVP input boundary:** manual entry only, or a read-only calendar import from the beginning?
4. **Now/Next policy:** what time horizon and ranking rules work in actual dogfooding?
5. **Action confirmation:** which actions, if any, may use a remembered confirmation preference?

## Resolve through later PoCs

- Voice invocation latency, accidental activation, CPU/battery impact, and local-audio boundary.
- Health-derived state without exporting raw health data.
- Connector contract, authorization mapping, sync semantics, and provider update cost.
- Memory claim/evidence schema, identity-merge thresholds, sensitivity rules, and deletion propagation.
- Multi-device conflict behavior and encryption key hierarchy.

## Deferred by intent

- Agent runtime topology.
- Model-provider routing and local-model distribution.
- Hosted versus self-host operating model.
- Feature parity across Apple, Android, and Windows.

These are not ignored; they are deferred because they do not determine whether the Personal Day MVP is useful.

## Source gap

A user-supplied ChatGPT shared conversation is intended to inform this backlog. Its content was not retrievable from the current environment, so none of its unverified decisions are represented here yet.
