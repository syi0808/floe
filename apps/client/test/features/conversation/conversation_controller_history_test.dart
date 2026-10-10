import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';

void main() {
  test(
    'earlier pages prepend in order and advance the exclusive cursor',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      harness.transport
        ..enqueuePage(
          _session(messages: [_message(_m3), _message(_m4)], hasEarlier: true),
        )
        ..enqueuePage(_session(messages: [_message(_m1), _message(_m2)]));

      await harness.controller.loadEarlierMessages();

      expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
      expect(harness.controller.hasEarlierMessages, isTrue);
      expect(harness.controller.earlierFailure, isNull);

      await harness.controller.loadEarlierMessages();

      expect(_messageIds(harness.controller), [_m1, _m2, _m3, _m4, _m5, _m6]);
      expect(harness.controller.hasEarlierMessages, isFalse);
      expect(harness.controller.earlierFailure, isNull);
      expect(
        harness.transport.historyQueries
            .map((query) => query['before_message_id'])
            .toList(),
        [_m5, _m3],
      );
    },
  );

  test(
    'overlapping history page is rejected without duplicating messages',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      // m6 overlaps the current window but is beyond the exclusive m5 cursor.
      harness.transport.enqueuePage(
        _session(messages: [_message(_m1), _message(_m6)]),
      );

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
      expect(_messageIds(harness.controller).toSet(), hasLength(before.length));
    },
  );

  test(
    'an empty earlier page cannot claim that more history remains',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(_session(messages: [], hasEarlier: true));

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
      expect(harness.controller.hasEarlierMessages, isTrue);
    },
  );

  for (final scenario in <_RejectedPageScenario>[
    _RejectedPageScenario(
      'malformed message window',
      _session(messages: [_message(_m1), _message(_m1)]),
    ),
    _RejectedPageScenario(
      'wrong session identity',
      _session(id: _otherSessionId, messages: [_message(_m1)]),
    ),
    _RejectedPageScenario(
      'wrong person identity',
      _session(personId: _otherPersonId, messages: [_message(_m1)]),
    ),
  ]) {
    test('rejects ${scenario.name} history page', () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(scenario.response);

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
    });
  }

  test(
    'session replacement fences a pending earlier-page completion',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.transport.replacementSession = _session(
        id: _replacementSessionId,
        messages: [_message(_replacementMessageId)],
      );
      await harness.controller.load(newSession: true);
      expect(harness.controller.session?.id, _replacementSessionId);

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m1), _message(_m2)])),
      );
      await earlierLoad;

      expect(harness.controller.session?.id, _replacementSessionId);
      expect(_messageIds(harness.controller), [_replacementMessageId]);
      expect(harness.controller.earlierFailure, isNull);
    },
  );

  test(
    'runtime invalidation fences a pending earlier-page completion',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      var notifications = 0;
      harness.controller.addListener(() => notifications++);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.runtime.closeAdmission();
      expect(harness.controller.session, isNull);
      expect(harness.controller.messages, isEmpty);
      final notificationsAfterInvalidation = notifications;

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m1), _message(_m2)])),
      );
      await earlierLoad;

      expect(harness.controller.session, isNull);
      expect(harness.controller.messages, isEmpty);
      expect(harness.controller.earlierFailure, isNull);
      expect(notifications, notificationsAfterInvalidation);
    },
  );

  test(
    'dispose during history read prevents completion notification',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      var notifications = 0;
      harness.controller.addListener(() => notifications++);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.disposeController();
      final notificationsAfterDispose = notifications;
      response.complete(_sessionEnvelope(_session(messages: [_message(_m1)])));
      await earlierLoad;

      expect(notifications, notificationsAfterDispose);
    },
  );

  test(
    'detach and attach keep a history read alive without model dispatch',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.controller.detachView();
      harness.controller.attachView();
      expect(harness.transport.historyQueries, hasLength(1));
      expect(response.isCompleted, isFalse);
      expect(harness.transport.commands, isEmpty);

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
      );
      await earlierLoad;

      expect(harness.controller.earlierFailure, isNull);
      expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
      expect(harness.transport.commands, isEmpty);
      expect(harness.transport.historyQueries, hasLength(1));
    },
  );
}

final class _Harness {
  _Harness(this.runtime, this.readModel, this.transport, this.controller);

  final RuntimeController runtime;
  final AppReadModel readModel;
  final _ConversationTransport transport;
  final ConversationController controller;
  bool _disposed = false;
  bool _controllerDisposed = false;

  static Future<_Harness> create() async {
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: _personId,
    );
    await runtime.open();
    final readModel = AppReadModel();
    final transport = _ConversationTransport(
      _session(messages: [_message(_m5), _message(_m6)], hasEarlier: true),
    );
    final client = AppWireConversationClient(transport, newId: _nextRequestId);
    final gateway = AppWireConversationGateway(
      transport,
      runtimeClient: client,
      readModel: readModel,
    );
    final controller = ConversationController(
      gateway: gateway,
      personId: _personId,
      owners: LocalOwnerGateways(runtime: runtime),
    );
    final harness = _Harness(runtime, readModel, transport, controller);
    await controller.load();
    return harness;
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    disposeController();
    runtime.dispose();
    readModel.dispose();
  }

  void disposeController() {
    if (_controllerDisposed) return;
    _controllerDisposed = true;
    controller.dispose();
  }
}

final class _ConversationTransport implements AppWireTransport {
  _ConversationTransport(this.initialSession);

  final Map<String, Object?> initialSession;
  Map<String, Object?>? replacementSession;
  final List<Map<String, dynamic>> historyQueries = [];
  final List<Map<String, dynamic>> commands = [];
  final List<Future<Map<String, dynamic>> Function()> _historyReplies = [];

  void enqueuePage(Map<String, Object?> session) {
    _historyReplies.add(() async => _sessionEnvelope(session));
  }

  void enqueuePendingPage(
    Completer<Map<String, dynamic>> response,
    Completer<void> requested,
  ) {
    _historyReplies.add(() {
      if (!requested.isCompleted) requested.complete();
      return response.future;
    });
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final query = Map<String, dynamic>.from(request['query'] as Map);
    switch (query['kind']) {
      case 'conversation.session.resume':
        return _sessionEnvelope(initialSession);
      case 'conversation.session.get':
        if (query['before_message_id'] is String) {
          historyQueries.add(query);
          if (_historyReplies.isEmpty) {
            throw StateError('No synthetic history page was queued.');
          }
          return _historyReplies.removeAt(0)();
        }
        throw StateError('Unexpected Conversation session query: $query');
      default:
        throw StateError('Unexpected AppWire query: $query');
    }
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    if (!request.containsKey('cursor')) {
      return {
        'kind': 'resync_required',
        'runtime_epoch': 1,
        'snapshot_cursor': 0,
      };
    }
    return {
      'kind': 'events',
      'runtime_epoch': request['runtime_epoch'],
      'next_cursor': request['cursor'],
      'events': const <Map<String, dynamic>>[],
    };
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final command = Map<String, dynamic>.from(request['command'] as Map);
    commands.add(command);
    if (command['kind'] == 'conversation.session.start') {
      return _sessionEnvelope(replacementSession ?? initialSession);
    }
    throw StateError('Unexpected Conversation command: $command');
  }

  @override
  Future<void> close() async {}
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);
}

final class _RejectedPageScenario {
  const _RejectedPageScenario(this.name, this.response);

  final String name;
  final Map<String, Object?> response;
}

List<String> _messageIds(ConversationController controller) =>
    controller.messages.map((message) => message.messageId).toList();

Map<String, dynamic> _sessionEnvelope(Map<String, Object?> session) => {
  'kind': 'conversation_session',
  'session': session,
};

Map<String, Object?> _session({
  String id = _sessionId,
  String personId = _personId,
  int revision = 12,
  required List<Map<String, Object?>> messages,
  bool hasEarlier = false,
}) => {
  'id': id,
  'person_id': personId,
  'revision': revision,
  'usage': {
    'unknown_token_attempts': 0,
    'unknown_cost_attempts': 0,
    'model_attempts': 0,
    'estimated_tokens': 0,
    'iterations': 0,
    'capability_calls': 0,
    'tokens': 0,
    'cost_micros': 0,
    'estimated_cost_micros': 0,
  },
  'messages': messages,
  'has_earlier_messages': hasEarlier,
};

Map<String, Object?> _message(String id) => {
  'kind': 'user',
  'message_id': id,
  'turn_id': _turnId,
  'text': 'synthetic $id',
};

int _requestNumber = 1000;
String _nextRequestId() =>
    '00000000-0000-4000-8000-${(_requestNumber++).toString().padLeft(12, '0')}';

const _personId = '00000000-0000-4000-8000-000000000001';
const _otherPersonId = '00000000-0000-4000-8000-000000000002';
const _sessionId = '00000000-0000-4000-8000-000000000010';
const _otherSessionId = '00000000-0000-4000-8000-000000000011';
const _replacementSessionId = '00000000-0000-4000-8000-000000000012';
const _turnId = '00000000-0000-4000-8000-000000000020';
const _m1 = '00000000-0000-4000-8000-000000000101';
const _m2 = '00000000-0000-4000-8000-000000000102';
const _m3 = '00000000-0000-4000-8000-000000000103';
const _m4 = '00000000-0000-4000-8000-000000000104';
const _m5 = '00000000-0000-4000-8000-000000000105';
const _m6 = '00000000-0000-4000-8000-000000000106';
const _replacementMessageId = '00000000-0000-4000-8000-000000000107';
