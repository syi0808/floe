import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';

import '../../support/interaction_page_fixture.dart';

const _session = '00000000-0000-4000-8000-000000000001';
const _first = '00000000-0000-4000-8000-000000000003';
const _second = '00000000-0000-4000-8000-000000000002';
void main() {
  test(
    'wire returns one bounded page and only advances on explicit request',
    () async {
      final wire = _Wire(
        interactionPageFixture(_session, [
          interactionFixture(id: _first, sessionId: _session),
        ], more: true),
      );
      final client = AppWireConversationClient(wire);
      final first = await client.listInteractions(_session);
      expect(wire.queries, hasLength(1));
      expect(first.nextCursor, isNotNull);
      wire.response = interactionPageFixture(_session, [
        interactionFixture(id: _second, sessionId: _session),
      ]);
      final next = await client.listInteractions(
        _session,
        after: first.nextCursor,
      );
      expect(next.nextCursor, isNull);
      expect(wire.queries.last['after'], first.nextCursor!.toJson());
      expect(next.interactions.single.id, _second);
    },
  );

  for (final issue in [
    'missing_cursor',
    'wrong_session',
    'duplicate',
    'wrong_last_id',
    'wrong_last_time',
    'empty_with_cursor',
    'too_many',
  ]) {
    test(
      'wire rejects $issue without treating a partial list as complete',
      () async {
        final snapshot = interactionFixture(id: _first, sessionId: _session);
        final response = interactionPageFixture(_session, [
          snapshot,
        ], more: true);
        switch (issue) {
          case 'missing_cursor':
            response.remove('next_cursor');
          case 'wrong_session':
            snapshot['session_id'] = _second;
          case 'duplicate':
            response['interactions'] = [snapshot, snapshot];
          case 'wrong_last_id':
            (response['next_cursor'] as Map)['interaction_id'] = _second;
          case 'wrong_last_time':
            (response['next_cursor'] as Map)['created_at_unix_ms'] = 1;
          case 'empty_with_cursor':
            response['interactions'] = [];
          case 'too_many':
            response['interactions'] = List.filled(65, snapshot);
        }
        await expectLater(
          AppWireConversationClient(_Wire(response)).listInteractions(_session),
          throwsFormatException,
        );
      },
    );
  }

  test(
    'same cursor or active cursor after terminal history cannot loop',
    () async {
      final response = interactionPageFixture(_session, [
        interactionFixture(id: _first, sessionId: _session),
      ], more: true);
      final client = AppWireConversationClient(_Wire(response));
      final cursor = AgentInteractionCursor.parse(response['next_cursor']);
      await expectLater(
        client.listInteractions(_session, after: cursor),
        throwsFormatException,
      );
      await expectLater(
        client.listInteractions(
          _session,
          after: AgentInteractionCursor(
            terminal: true,
            createdAtUnixMs: 2000,
            interactionId: _first,
          ),
        ),
        throwsFormatException,
      );
    },
  );
}

class _Wire implements AppWireTransport {
  _Wire(this.response);
  Map<String, dynamic> response;
  final queries = <Map<String, dynamic>>[];
  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    queries.add(Map<String, dynamic>.from(request['query'] as Map));
    return response;
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw StateError('Read-only interaction page dispatched a command');
  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw UnimplementedError();
  @override
  Future<void> close() async {}
}
