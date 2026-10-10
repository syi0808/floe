import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';
import 'package:floe_client/features/conversation/presentation/agent_panel.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';

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

  testWidgets(
    'failed send does not restore a draft into a replacement panel controller',
    (tester) async {
      _usePanelViewport(tester);

      final original = await _Harness.create();
      final replacement = await _Harness.create(
        sessionId: _replacementSessionId,
      );
      addTearDown(original.dispose);
      addTearDown(replacement.dispose);

      await tester.pumpWidget(_panelApp(original.controller));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byType(TextFormField),
        'old conversation draft',
      );
      await tester.tap(find.byTooltip('Ask Floe'));
      await tester.pump();
      await original.transport.startTurnRequested.future.timeout(
        const Duration(seconds: 1),
      );

      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        isEmpty,
      );
      await tester.pumpWidget(_panelApp(replacement.controller));
      await tester.pumpAndSettle();

      original.transport.pendingStartTurn.completeError(
        const AppWireTransportException(
          'synthetic_failure',
          'Synthetic delayed send failure.',
          commandOutcome: CommandOutcome.notAdmitted,
        ),
      );
      await tester.pumpAndSettle();

      expect(replacement.controller.session?.id, _replacementSessionId);
      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        isEmpty,
      );
    },
  );

  testWidgets('failed send restores the draft for its original session', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);

    await tester.pumpWidget(_panelApp(harness.controller));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'keep this draft');
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump();
    await harness.transport.startTurnRequested.future.timeout(
      const Duration(seconds: 1),
    );

    harness.transport.pendingStartTurn.completeError(
      const AppWireTransportException(
        'synthetic_failure',
        'Synthetic delayed send failure.',
        commandOutcome: CommandOutcome.notAdmitted,
      ),
    );
    await tester.pumpAndSettle();

    expect(harness.controller.failure, isNotNull);
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      'keep this draft',
    );
  });

  testWidgets(
    'terminal same-session failure restores draft after newer snapshot',
    (tester) async {
      _usePanelViewport(tester);
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final originalSession = harness.controller.session!;
      harness.transport
        ..returnTerminalFailure = true
        ..terminalSession = _session(
          revision: originalSession.revision + 1,
          messages: [_message(_m5), _message(_m6)],
        );

      await tester.pumpWidget(_panelApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextFormField), 'keep terminal draft');
      await tester.tap(find.byTooltip('Ask Floe'));
      await tester.pump();
      await harness.transport.startTurnRequested.future.timeout(
        const Duration(seconds: 1),
      );
      await tester.pumpAndSettle();

      expect(harness.controller.failure, 'model_unavailable');
      expect(harness.controller.session?.id, originalSession.id);
      expect(
        harness.controller.session?.revision,
        originalSession.revision + 1,
      );
      expect(identical(originalSession, harness.controller.session), isFalse);
      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        'keep terminal draft',
      );
    },
  );

  testWidgets('Load earlier shows pending and ignores repeated taps', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final response = Completer<Map<String, dynamic>>();
    final requested = Completer<void>();
    harness.transport.enqueuePendingPage(response, requested);

    await tester.pumpWidget(_panelApp(harness.controller));
    await tester.pumpAndSettle();
    final loadEarlier = find.text('Load earlier messages');
    await tester.tap(loadEarlier);
    await tester.tap(loadEarlier);
    await requested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    expect(find.text('Loading earlier messages…'), findsOneWidget);
    expect(harness.controller.loadingEarlier, isTrue);
    expect(harness.transport.historyQueries, hasLength(1));

    response.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();
    expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
    expect(harness.transport.commands, isEmpty);
  });

  testWidgets(
    'overlapping history error is visible and keeps messages intact',
    (tester) async {
      _usePanelViewport(tester);
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(
        _session(messages: [_message(_m1), _message(_m6)]),
      );

      await tester.pumpWidget(_panelApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Load earlier messages'));
      await tester.pumpAndSettle();

      expect(
        find.text('Earlier messages could not be loaded. Try again.'),
        findsOneWidget,
      );
      expect(_messageIds(harness.controller), before);
      expect(find.text('synthetic $_m5'), findsOneWidget);
      expect(find.text('synthetic $_m6'), findsOneWidget);
      expect(harness.transport.commands, isEmpty);
    },
  );

  testWidgets('unmount during history read leaves it alive without dispatch', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final response = Completer<Map<String, dynamic>>();
    final requested = Completer<void>();
    harness.transport.enqueuePendingPage(response, requested);
    var panelVisible = true;

    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => panelVisible
                ? AgentPanel(
                    controller: harness.controller,
                    dayGateway: _UnusedDayGateway(),
                    onClose: () => setState(() => panelVisible = false),
                  )
                : const SizedBox.shrink(),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await requested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    expect(response.isCompleted, isFalse);
    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(find.byType(AgentPanel), findsNothing);

    response.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();

    expect(harness.transport.historyQueries, hasLength(1));
    expect(harness.transport.commands, isEmpty);
    expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
  });

  testWidgets('controller replacement fences a pending history callback', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final original = await _Harness.create();
    final replacement = await _Harness.create(sessionId: _replacementSessionId);
    addTearDown(original.dispose);
    addTearDown(replacement.dispose);
    final oldResponse = Completer<Map<String, dynamic>>();
    final oldRequested = Completer<void>();
    final newResponse = Completer<Map<String, dynamic>>();
    final newRequested = Completer<void>();
    original.transport.enqueuePendingPage(oldResponse, oldRequested);
    replacement.transport.enqueuePendingPage(newResponse, newRequested);

    await tester.pumpWidget(_panelApp(original.controller));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await oldRequested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    await tester.pumpWidget(_panelApp(replacement.controller));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await newRequested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    oldResponse.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();

    expect(find.text('Loading earlier messages…'), findsOneWidget);
    expect(replacement.controller.loadingEarlier, isTrue);
    expect(replacement.transport.historyQueries, hasLength(1));
    expect(replacement.controller.session?.id, _replacementSessionId);
    expect(_messageIds(replacement.controller), [_m5, _m6]);

    newResponse.complete(
      _sessionEnvelope(
        _session(
          id: _replacementSessionId,
          messages: [_message(_m1), _message(_m2)],
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(_messageIds(replacement.controller), [_m1, _m2, _m5, _m6]);
    expect(original.controller.earlierFailure, isNull);
  });

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

void _usePanelViewport(WidgetTester tester) {
  tester.view.physicalSize = const Size(1000, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Widget _panelApp(ConversationController controller) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: AgentPanel(
      controller: controller,
      dayGateway: _UnusedDayGateway(),
      onClose: () {},
    ),
  ),
);

final class _Harness {
  _Harness(this.runtime, this.readModel, this.transport, this.controller);

  final RuntimeController runtime;
  final AppReadModel readModel;
  final _ConversationTransport transport;
  final ConversationController controller;
  bool _disposed = false;
  bool _controllerDisposed = false;

  static Future<_Harness> create({String sessionId = _sessionId}) async {
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: _personId,
    );
    await runtime.open();
    final readModel = AppReadModel();
    final transport = _ConversationTransport(
      _session(
        id: sessionId,
        messages: [_message(_m5), _message(_m6)],
        hasEarlier: true,
      ),
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
  final Completer<void> startTurnRequested = Completer<void>();
  final Completer<Map<String, dynamic>> pendingStartTurn =
      Completer<Map<String, dynamic>>();
  bool returnTerminalFailure = false;
  Map<String, Object?>? terminalSession;
  int _startTurnRequests = 0;
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
        if (terminalSession != null &&
            query['session_id'] == terminalSession!['id']) {
          return _sessionEnvelope(terminalSession!);
        }
        throw StateError('Unexpected Conversation session query: $query');
      case 'conversation.get_run':
        if (returnTerminalFailure && query['run_id'] == _terminalRunId) {
          return _failedRunEnvelope(query['run_id'] as String);
        }
        throw StateError('Unexpected Conversation Run query: $query');
      case 'conversation.interaction.list':
        return {
          'kind': 'interaction_list',
          'session_id': query['session_id'],
          'interactions': const <Map<String, dynamic>>[],
        };
      case 'conversation.get_command':
        return {'kind': 'unknown_command', 'command_id': query['command_id']};
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
    if (command['kind'] == 'conversation.start_turn') {
      _startTurnRequests++;
      if (returnTerminalFailure) {
        startTurnRequested.complete();
        return {
          'kind': 'command_receipt',
          'command_id': request['command_id'],
          'runtime_epoch': 1,
          'admission': 'accepted',
          'run_id': _terminalRunId,
          'session_revision': terminalSession?['revision'],
        };
      }
      if (_startTurnRequests == 1) {
        startTurnRequested.complete();
        return pendingStartTurn.future;
      }
      throw const AppWireTransportException(
        'synthetic_failure',
        'Synthetic delayed send failure.',
        commandOutcome: CommandOutcome.notAdmitted,
      );
    }
    throw StateError('Unexpected Conversation command: $command');
  }

  @override
  Future<void> close() async {}
}

final class _UnusedDayGateway implements DayGateway {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
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

Map<String, dynamic> _failedRunEnvelope(String runId) => {
  'kind': 'run_snapshot',
  'run_id': runId,
  'session_id': _sessionId,
  'revision': 1,
  'runtime_epoch': 1,
  'executor_generation': 1,
  'state': 'finished',
  'progress': 'failed',
  'task_refs': const <String>[],
  'attempt_refs': const <String>[],
  'report': {
    'execution': 'failed',
    'reply': 'not_produced',
    'issues': const [
      {'code': 'model_unavailable', 'message': 'Synthetic model failure.'},
    ],
    'action_refs': const <String>[],
    'interaction_refs': const <String>[],
    'final_message_ref': null,
  },
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
const _terminalRunId = '00000000-0000-4000-8000-000000000108';
