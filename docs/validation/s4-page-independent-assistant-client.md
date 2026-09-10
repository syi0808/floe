# S4 page-independent assistant client

Date: 2026-09-10.

## Implemented

- The Day Canvas mounts the same Person-scoped assistant conversation used by any
  other presentation surface. It no longer passes a selected day, Calendar source
  or evidence interval into `AgentController`.
- `AgentController` starts, resumes, recovers and sends only through the generic
  conversation transport for Personal sessions.
- Calendar setup and proposal inspection remain independent settings and review
  flows. They do not select a chat session type.
- Calendar-specific Dart session and turn transports were removed. The native
  gateway no longer emits `calendar_session` or `calendar_turn` actions.
- The synthetic fixture remains isolated as a development/test transport and cannot
  be invoked for a Personal conversation.

## Automated evidence

```text
flutter analyze
No issues found.

flutter test test/agent_vault_gateway_test.dart \
  test/agent_conversation_controller_test.dart \
  test/agent_fixture_gateway_test.dart \
  test/agent_proposal_card_test.dart
All tests passed.
```

The full Flutter suite also completes 207 tests. Five pre-existing failures remain:
three registry-settings expectations for a surface no longer mounted by Settings,
and two unrelated sub-percent golden differences.

## Follow-up cleanup

The migration-only Rust protocol and FFI Calendar session/turn actions were removed
in the following increment. The protocol now rejects both action kinds, and the
encrypted vault no longer creates Calendar-scoped conversations. End-to-end “today,
then this week” native evidence coverage remains the next acceptance increment.
