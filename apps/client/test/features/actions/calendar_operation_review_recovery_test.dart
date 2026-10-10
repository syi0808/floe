import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';
import 'package:floe_client/features/conversation/presentation/agent_interaction_card.dart';
import 'package:floe_client/features/conversation/presentation/agent_panel.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  testWidgets(
    'controller replacement reloads the operation review for the new runtime',
    (tester) async {
      final original = await _Harness.create(
        interactionReads: [_operationReview(title: 'Old source event')],
      );
      addTearDown(original.dispose);
      final replacement = await _Harness.create(
        interactionReads: [
          _operationReview(
            title: 'Replacement source event',
            sourceDigest: _digestB,
            targetDigest: _targetDigestB,
            reviewId: _reviewIdB,
          ),
        ],
      );
      addTearDown(replacement.dispose);

      await tester.pumpWidget(_reviewApp(original.controller));
      await tester.pumpAndSettle();
      expect(
        (original.controller.interactionFor(_interactionId)!.target
                as AgentOperationApprovalTarget)
            .operation
            .title,
        'Old source event',
      );
      expect(original.transport.interactionGetCount, 1);

      original.runtime.closeAdmission();
      await tester.pumpWidget(_reviewApp(replacement.controller));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));

      expect(replacement.transport.interactionGetCount, 1);
      expect(replacement.controller.interactionFor(_interactionId), isNotNull);
      expect(
        (replacement.controller.interactionFor(_interactionId)!.target
                as AgentOperationApprovalTarget)
            .operation
            .title,
        'Replacement source event',
      );
      expect(replacement.transport.commands, isEmpty);
    },
  );

  testWidgets(
    'repeated approval taps are ignored and an uncertain retry keeps the exact command',
    (tester) async {
      final response = Completer<Map<String, dynamic>>();
      final started = Completer<void>();
      final harness = await _Harness.create(
        interactionReads: [_operationReview()],
        onResolve: (request, call) {
          if (call == 1) {
            started.complete();
            return response.future;
          }
          return Future.value(
            _resolveResult(
              request,
              outcome: 'resolved',
              snapshot: _operationReview(
                state: 'resolved',
                actionState: 'approved',
                actions: const [],
              ),
            ),
          );
        },
      );
      addTearDown(harness.dispose);

      await tester.pumpWidget(_reviewApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Allow'));
      await started.future.timeout(const Duration(seconds: 1));
      await tester.pump();
      await tester.tap(find.text('Allow'));
      expect(harness.transport.commands, hasLength(1));

      response.completeError(_indeterminateConflict);
      await tester.pumpAndSettle();
      expect(harness.controller.needsReload, isTrue);

      // The panel's recovery action reloads the owner snapshot before the
      // person can retry the same review.
      await harness.controller.load();
      await harness.controller.refreshInteractions();
      await tester.pumpAndSettle();
      await tester.tap(find.text('Allow'));
      await tester.pumpAndSettle();

      expect(harness.transport.commands, hasLength(2));
      final first = harness.transport.commands[0];
      final retry = harness.transport.commands[1];
      expect(retry['command_id'], first['command_id']);
      expect(retry['command'], first['command']);
      expect(retry['request_id'], isNot(first['request_id']));
      expect(retry['command'], {
        'kind': 'conversation.interaction.resolve',
        'interaction_id': _interactionId,
        'session_id': _sessionId,
        'expected_revision': 1,
        'decision': 'approve',
        'target_digest': _targetDigestA,
      });
      expect(
        harness.controller.interactionFor(_interactionId)?.state,
        AgentInteractionState.resolved,
      );
    },
  );

  testWidgets(
    'a source change while approval is pending makes the old review stale',
    (tester) async {
      final response = Completer<Map<String, dynamic>>();
      final started = Completer<void>();
      final harness = await _Harness.create(
        interactionReads: [_operationReview()],
        onResolve: (request, _) {
          started.complete();
          return response.future;
        },
      );
      addTearDown(harness.dispose);

      await tester.pumpWidget(_reviewApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Allow'));
      await started.future.timeout(const Duration(seconds: 1));
      await tester.pump();

      // The selected Calendar source changes after this review was displayed.
      // The owner response marks the original immutable review stale.
      harness.transport.currentInteraction = _operationReview(
        state: 'stale',
        actionState: 'blocked',
        blockedReason: 'source_changed',
        actions: const [],
      );
      response.complete(
        _resolveResult(
          harness.transport.commands.single,
          outcome: 'stale',
          snapshot: harness.transport.currentInteraction!,
        ),
      );
      await tester.pumpAndSettle();

      final sent = harness.transport.commands.single;
      final command = sent['command'] as Map<String, dynamic>;
      expect(command['target_digest'], _targetDigestA);
      expect(command['expected_revision'], 1);
      expect(
        harness.controller.interactionFor(_interactionId)?.state,
        AgentInteractionState.stale,
      );
      expect(find.text('Allow'), findsNothing);
      expect(harness.transport.commands, hasLength(1));
    },
  );

  testWidgets(
    'closing the review view leaves the admitted decision to reconcile on reopen',
    (tester) async {
      final response = Completer<Map<String, dynamic>>();
      final started = Completer<void>();
      final harness = await _Harness.create(
        interactionReads: [_operationReview()],
        onResolve: (request, _) {
          started.complete();
          return response.future;
        },
      );
      addTearDown(harness.dispose);
      var panelOpen = true;

      Widget app() => MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => panelOpen
                ? AgentPanel(
                    controller: harness.controller,
                    dayGateway: _UnusedDayGateway(),
                    onClose: () {
                      harness.controller.detachView();
                      setState(() => panelOpen = false);
                    },
                  )
                : TextButton(
                    onPressed: () {
                      harness.controller.attachView();
                      setState(() => panelOpen = true);
                    },
                    child: const Text('Reopen review'),
                  ),
          ),
        ),
      );

      // Seed the durable card through the normal owner list projection.
      await harness.controller.refreshInteractions();
      await tester.pumpWidget(app());
      await tester.pumpAndSettle();
      await tester.tap(find.text('Allow'));
      await started.future.timeout(const Duration(seconds: 1));
      await tester.pump();
      await tester.tap(find.byTooltip('Close'));
      harness.controller.detachView();
      await tester.pumpAndSettle();

      final resolved = _operationReview(
        state: 'resolved',
        actionState: 'approved',
        actions: const [],
      );
      harness.transport.currentInteraction = resolved;
      response.complete(
        _resolveResult(
          harness.transport.commands.single,
          outcome: 'resolved',
          snapshot: resolved,
        ),
      );
      await tester.pumpAndSettle();
      expect(
        harness.controller.interactionFor(_interactionId)?.state,
        AgentInteractionState.resolved,
      );

      await tester.tap(find.text('Reopen review'));
      await tester.pumpAndSettle();
      expect(find.text('Resolved'), findsOneWidget);
      expect(find.text('Allow'), findsNothing);
      expect(harness.transport.commands, hasLength(1));
    },
  );

  for (final lifecycle in _Retirement.values) {
    test(
      'late approval completion after ${lifecycle.name} cannot restore its review',
      () async {
        final response = Completer<Map<String, dynamic>>();
        final started = Completer<void>();
        final harness = await _Harness.create(
          interactionReads: [_operationReview()],
          onResolve: (request, _) {
            started.complete();
            return response.future;
          },
        );
        addTearDown(harness.dispose);
        expect(harness.controller.interactionFor(_interactionId), isNull);
        await harness.controller.ensureInteraction(_interactionId);
        final exactPending = harness.controller.interactionFor(_interactionId)!;
        final completion = harness.controller.decideInteraction(
          exactPending,
          AgentInteractionDecision.approve,
        );
        await started.future.timeout(const Duration(seconds: 1));
        var notifications = 0;
        harness.controller.addListener(() => notifications++);

        if (lifecycle == _Retirement.runtime) {
          harness.runtime.closeAdmission();
          expect(harness.controller.session, isNull);
          expect(harness.controller.interactionFor(_interactionId), isNull);
        } else {
          harness.disposeController();
        }
        final afterRetirement = notifications;

        response.complete(
          _resolveResult(
            harness.transport.commands.single,
            outcome: 'resolved',
            snapshot: _operationReview(
              state: 'resolved',
              actionState: 'approved',
              actions: const [],
            ),
          ),
        );
        await completion;

        if (lifecycle == _Retirement.runtime) {
          expect(harness.controller.session, isNull);
          expect(harness.controller.interactionFor(_interactionId), isNull);
        } else {
          expect(
            harness.controller.interactionFor(_interactionId),
            same(exactPending),
          );
        }
        expect(notifications, afterRetirement);
      },
    );
  }
}

Widget _reviewApp(ConversationController controller) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: _reviewCard(controller)),
);

Widget _reviewCard(ConversationController controller) =>
    AgentInteractionCard(controller: controller, interactionId: _interactionId);

final class _Harness {
  _Harness(
    this.runtime,
    this.readModel,
    this.transport,
    this.gateway,
    this.controller,
  );

  final RuntimeController runtime;
  final AppReadModel readModel;
  final _ReviewTransport transport;
  final AppWireConversationGateway gateway;
  final ConversationController controller;
  bool _disposed = false;
  bool _controllerDisposed = false;

  static Future<_Harness> create({
    List<Map<String, dynamic>> interactionReads = const [],
    _ResolveHandler? onResolve,
  }) async {
    final runtime = _readyRuntime();
    await runtime.open();
    final readModel = AppReadModel();
    final transport = _ReviewTransport(
      interactionReads: interactionReads,
      onResolve: onResolve,
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
    final harness = _Harness(
      runtime,
      readModel,
      transport,
      gateway,
      controller,
    );
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

typedef _ResolveHandler = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
  int call,
);

final class _ReviewTransport implements AppWireTransport {
  _ReviewTransport({
    required List<Map<String, dynamic>> interactionReads,
    this.onResolve,
  }) : interactionReads = List.of(interactionReads),
       currentInteraction = interactionReads.isEmpty
           ? null
           : interactionReads.first;

  final List<Map<String, dynamic>> interactionReads;
  final List<Map<String, dynamic>> commands = [];
  final _ResolveHandler? onResolve;
  Map<String, dynamic>? currentInteraction;
  int interactionGetCount = 0;

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final query = Map<String, dynamic>.from(request['query'] as Map);
    switch (query['kind']) {
      case 'conversation.session.resume':
        return _sessionEnvelope();
      case 'conversation.session.get':
        return _sessionEnvelope();
      case 'conversation.interaction.get':
        interactionGetCount++;
        if (interactionReads.isNotEmpty) {
          currentInteraction = interactionReads.removeAt(0);
        }
        final snapshot = currentInteraction;
        if (snapshot == null) {
          return {
            'kind': 'unknown_interaction',
            'interaction_id': query['interaction_id'],
          };
        }
        return {'kind': 'interaction', ...snapshot};
      case 'conversation.interaction.list':
        return {
          'kind': 'interaction_list',
          'session_id': query['session_id'],
          'interactions': [?currentInteraction],
        };
      case 'conversation.get_command':
        return {'kind': 'unknown_command', 'command_id': query['command_id']};
      default:
        throw StateError('Unexpected synthetic query: $query');
    }
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => request.containsKey('cursor')
      ? {
          'kind': 'events',
          'runtime_epoch': request['runtime_epoch'],
          'next_cursor': request['cursor'],
          'events': const <Map<String, dynamic>>[],
        }
      : {'kind': 'resync_required', 'runtime_epoch': 1, 'snapshot_cursor': 0};

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final recorded = Map<String, dynamic>.from(request);
    commands.add(recorded);
    final command = Map<String, dynamic>.from(request['command'] as Map);
    if (command['kind'] != 'conversation.interaction.resolve') {
      return Future.error(StateError('Unexpected synthetic command: $command'));
    }
    final handler = onResolve;
    if (handler != null) return handler(recorded, commands.length);
    return Future.value(
      _resolveResult(
        recorded,
        outcome: 'resolved',
        snapshot: _operationReview(
          state: 'resolved',
          actionState: 'approved',
          actions: const [],
        ),
      ),
    );
  }

  @override
  Future<void> close() async {}
}

enum _Retirement { runtime, controller }

RuntimeController _readyRuntime() =>
    RuntimeController(gateway: _ReadyRuntimeGateway(), personId: _personId);

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

final class _UnusedDayGateway implements DayGateway {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

const _personId = '00000000-0000-4000-8000-000000000001';
const _sessionId = '00000000-0000-4000-8000-000000000010';
const _originRunId = '00000000-0000-4000-8000-000000000020';
const _interactionId = '00000000-0000-4000-8000-000000000030';
const _actionId = '00000000-0000-4000-8000-000000000040';
const _reviewIdA = '00000000-0000-4000-8000-000000000041';
const _reviewIdB = '00000000-0000-4000-8000-000000000042';
const _targetDigestA =
    '1111111111111111111111111111111111111111111111111111111111111111';
const _targetDigestB =
    '2222222222222222222222222222222222222222222222222222222222222222';
const _digestA =
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const _digestB =
    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const _indeterminateConflict = AppWireTransportException(
  'conflict',
  'Synthetic command acknowledgement was lost.',
  commandOutcome: CommandOutcome.indeterminate,
);

Map<String, dynamic> _operationReview({
  String title = 'Old source event',
  String state = 'pending',
  String actionState = 'pending_review',
  String? blockedReason,
  int revision = 1,
  String sourceDigest = _digestA,
  String targetDigest = _targetDigestA,
  String reviewId = _reviewIdA,
  List<String> actions = const ['allow'],
}) {
  final review = <String, Object?>{
    'id': reviewId,
    'operation_id': _actionId,
    'effect_digest': _digestA,
    'source_digest': sourceDigest,
    'person_id': _personId,
    'device_id': 'synthetic-device',
    'policy_revision': 3,
    'created_at': '2026-10-10T10:00:00Z',
    'expires_at': '2026-10-10T11:00:00Z',
  };
  final operation = <String, Object?>{
    'action_ref': _actionId,
    'revision': revision,
    'origin': 'expert',
    'effect': {
      'kind': 'create',
      'destination_label': 'Personal Calendar',
      'title': title,
      'schedule': {
        'starts_at': '2026-10-12T10:00:00Z',
        'ends_at': '2026-10-12T11:00:00Z',
        'timezone': 'UTC',
      },
    },
    'review_ref': review,
    'created_at': '2026-10-10T10:00:00Z',
    'expires_at': '2026-10-10T11:00:00Z',
    'status': {'state': actionState, 'reason': ?blockedReason},
    'allowed_actions': actionState == 'pending_review'
        ? ['approve', 'reject']
        : <String>[],
    'next_observation_after_ms': null,
  };
  return {
    'interaction_id': _interactionId,
    'session_id': _sessionId,
    'origin_run_id': _originRunId,
    'interaction_kind': 'operation_approval',
    'state': state,
    'revision': revision,
    'target_digest': targetDigest,
    'created_at': '2026-10-10T10:00:00Z',
    'expires_at': '2026-10-10T11:00:00Z',
    'requirement': {'kind': 'operation_approval', 'review_ref': review},
    'target': {'kind': 'operation_approval', 'operation': operation},
    'actions': actions,
  };
}

Map<String, dynamic> _resolveResult(
  Map<String, dynamic> request, {
  required String outcome,
  required Map<String, dynamic> snapshot,
}) => {
  'kind': 'interaction_operation',
  'command_id': request['command_id'],
  'outcome': outcome,
  'snapshot': snapshot,
};

Map<String, dynamic> _sessionEnvelope() => {
  'kind': 'conversation_session',
  'session': {
    'id': _sessionId,
    'person_id': _personId,
    'revision': 12,
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
    'messages': const <Map<String, Object?>>[],
    'has_earlier_messages': false,
  },
};

int _requestNumber = 1000;
String _nextRequestId() =>
    '00000000-0000-4000-8000-${(_requestNumber++).toString().padLeft(12, '0')}';
