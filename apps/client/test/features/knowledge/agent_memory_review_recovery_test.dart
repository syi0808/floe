import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';
import 'package:floe_client/features/knowledge/infrastructure/app_wire_memory_gateway.dart';
import 'package:floe_client/features/settings/presentation/agent_memory_review_settings.dart';

void main() {
  group('AgentMemoryController review recovery', () {
    test('submits allowed approve and reject decisions', () async {
      final transport = _ScriptedMemoryTransport(
        onQuery: _successfulQuery,
        onCommand: _acknowledge,
      );
      final harness = _MemoryHarness(transport);

      await harness.controller.loadReview();
      expect(harness.controller.candidates, hasLength(2));

      await harness.controller.decide(
        _approveCandidateId,
        AgentMemoryDecision.approve,
      );
      await harness.controller.decide(
        _rejectCandidateId,
        AgentMemoryDecision.reject,
      );

      expect(transport.commands, hasLength(2));
      expect(transport.commands.map((request) => request['command']), [
        {
          'kind': 'memory.decide',
          'candidate_id': _approveCandidateId,
          'decision': 'approve',
        },
        {
          'kind': 'memory.decide',
          'candidate_id': _rejectCandidateId,
          'decision': 'reject',
        },
      ]);
      expect(
        harness.controller.acknowledgement?.decision,
        AgentMemoryDecision.reject,
      );
      expect(harness.controller.busy, isFalse);
      harness.controller.dispose();
    });

    test(
      'duplicate taps and new decisions cannot replace a pending decision',
      () async {
        final pending = await _startPendingDecision();
        final controller = pending.harness.controller;

        expect(controller.busy, isTrue);
        expect(controller.canReview, isFalse);
        await controller.decide(
          _approveCandidateId,
          AgentMemoryDecision.approve,
        );
        await controller.decide(_rejectCandidateId, AgentMemoryDecision.reject);
        expect(pending.transport.commands, hasLength(1));

        pending.response.completeError(_indeterminateConflict);
        await pending.completion;

        expect(controller.canRetryDecision, isTrue);
        expect(controller.canReview, isFalse);
        await controller.decide(_rejectCandidateId, AgentMemoryDecision.reject);
        expect(pending.transport.commands, hasLength(1));
        expect(pending.harness.gateway.pendingCandidateId, _approveCandidateId);
        expect(
          pending.harness.gateway.pendingDecision,
          AgentMemoryDecision.approve,
        );
        _expectPendingDecisionRetained(pending);
        controller.dispose();
      },
    );

    test(
      'retries an indeterminate decision with the exact command body',
      () async {
        final transport = _ScriptedMemoryTransport(
          onQuery: _successfulQuery,
          onCommand: (request, call) async {
            if (call == 1) throw _indeterminateConflict;
            return _acknowledgementResponse(request);
          },
        );
        final harness = _MemoryHarness(transport);

        await harness.controller.loadReview();
        await harness.controller.decide(
          _approveCandidateId,
          AgentMemoryDecision.approve,
        );

        expect(harness.controller.canReview, isFalse);
        expect(harness.controller.canRetryDecision, isTrue);
        expect(harness.controller.reviewFailure, 'conflict');
        expect(harness.gateway.pendingCandidateId, _approveCandidateId);
        expect(harness.gateway.pendingDecision, AgentMemoryDecision.approve);

        await harness.controller.retryPendingDecision();

        expect(transport.commands, hasLength(2));
        expect(
          transport.commands[1]['command_id'],
          transport.commands[0]['command_id'],
        );
        expect(
          transport.commands[1]['command'],
          transport.commands[0]['command'],
        );
        expect(transport.commands[1]['command'], {
          'kind': 'memory.decide',
          'candidate_id': _approveCandidateId,
          'decision': 'approve',
        });
        expect(harness.gateway.pendingCommandId, isNull);
        expect(
          harness.controller.acknowledgement?.decision,
          AgentMemoryDecision.approve,
        );
        expect(harness.controller.canRetryDecision, isFalse);
        await harness.controller.retryPendingDecision();
        expect(transport.commands, hasLength(2));
        harness.controller.dispose();
      },
    );

    test(
      'keeps a confirmed acknowledgement when review refresh fails',
      () async {
        final transport = _ScriptedMemoryTransport(
          onQuery: (request, call) async {
            final kind = (request['query'] as Map<String, dynamic>)['kind'];
            if (kind == 'memory.review' && call > 1) {
              throw const AppWireTransportException(
                'storage_unavailable',
                'Synthetic refresh failure.',
              );
            }
            return _successfulQuery(request, call);
          },
          onCommand: _acknowledge,
        );
        final harness = _MemoryHarness(transport);

        await harness.controller.loadReview();
        await harness.controller.decide(
          _approveCandidateId,
          AgentMemoryDecision.approve,
        );

        expect(harness.controller.acknowledgement, isNotNull);
        expect(
          harness.controller.acknowledgement?.decision,
          AgentMemoryDecision.approve,
        );
        expect(harness.controller.reviewFailure, 'storage_unavailable');
        expect(harness.controller.candidates, isNull);
        expect(harness.controller.overview, isNotNull);
        expect(harness.fatalFailures, isEmpty);
        expect(harness.gateway.pendingCommandId, isNull);
        expect(harness.controller.canRetryDecision, isFalse);
        await harness.controller.retryPendingDecision();
        expect(transport.commands, hasLength(1));
        harness.controller.dispose();
      },
    );

    for (final lifecycle in _PendingLifecycle.values) {
      for (final completionKind in _LateCompletionKind.values) {
        test(
          'ignores a late ${completionKind.name} after ${lifecycle.name}',
          () async {
            final pending = await _startPendingDecision();
            final controller = pending.harness.controller;
            final candidatesBeforeLifecycle = controller.candidates;
            var notifications = 0;
            controller.addListener(() => notifications++);

            if (lifecycle == _PendingLifecycle.clear) {
              controller.clear();
            } else {
              controller.dispose();
            }
            final notificationsAfterLifecycle = notifications;

            switch (completionKind) {
              case _LateCompletionKind.success:
                pending.response.complete(
                  _acknowledgementResponse(pending.submittedRequest),
                );
              case _LateCompletionKind.indeterminateError:
                pending.response.completeError(_indeterminateConflict);
            }
            await pending.completion;

            if (lifecycle == _PendingLifecycle.clear) {
              expect(controller.candidates, isNull);
              expect(controller.busy, isFalse);
            } else {
              expect(controller.candidates, same(candidatesBeforeLifecycle));
            }
            expect(controller.acknowledgement, isNull);
            expect(controller.reviewFailure, isNull);
            expect(pending.transport.queries, hasLength(1));
            expect(notifications, notificationsAfterLifecycle);
            expect(pending.harness.fatalFailures, isEmpty);

            if (completionKind == _LateCompletionKind.success) {
              expect(pending.harness.gateway.pendingCommandId, isNull);
              expect(pending.harness.gateway.pendingCandidateId, isNull);
              expect(pending.harness.gateway.pendingDecision, isNull);
            } else {
              _expectPendingDecisionRetained(pending);
            }

            if (lifecycle == _PendingLifecycle.clear) controller.dispose();
          },
        );
      }
    }

    test('propagates a fatal owner failure to the fatal handler', () async {
      final transport = _ScriptedMemoryTransport(
        onQuery: _successfulQuery,
        onCommand: (request, call) => Future.error(
          const AppWireTransportException(
            'owner_failure',
            'Synthetic fatal owner failure.',
            ownerFailure: _fatalOwnerFailure,
            commandOutcome: CommandOutcome.admitted,
          ),
        ),
      );
      final harness = _MemoryHarness(transport);

      await harness.controller.loadReview();
      await harness.controller.decide(
        _approveCandidateId,
        AgentMemoryDecision.approve,
      );

      expect(harness.fatalFailures, hasLength(1));
      expect(
        harness.fatalFailures.single.ownerFailure,
        same(_fatalOwnerFailure),
      );
      expect(harness.fatalFailures.single.reloadRequired, isTrue);
      expect(harness.fatalFailures.single.sealSession, isTrue);
      expect(harness.controller.reviewFailure, 'session_unavailable');
      harness.controller.dispose();
    });
  });

  testWidgets('the review widget exposes and retries an uncertain decision', (
    tester,
  ) async {
    final commandStarted = Completer<void>();
    final firstCommand = Completer<Map<String, dynamic>>();
    final retryCommand = Completer<Map<String, dynamic>>();
    late Map<String, dynamic> retryRequest;
    final transport = _ScriptedMemoryTransport(
      onQuery: _successfulQuery,
      onCommand: (request, call) {
        if (call == 1) {
          commandStarted.complete();
          return firstCommand.future;
        }
        retryRequest = request;
        return retryCommand.future;
      },
    );
    final harness = _MemoryHarness(transport);

    await harness.controller.loadReview();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AgentMemoryReviewSettings(controller: harness.controller),
        ),
      ),
    );

    _expectValidDecisionControlsEnabled(tester);
    await tester.tap(_approveButton(_approveCandidateId));
    await commandStarted.future;
    await tester.pump();
    expect(find.byKey(const ValueKey('memory-decision-retry')), findsNothing);
    _expectDecisionControlsDisabled(tester);
    await tester.tap(_approveButton(_approveCandidateId));
    await tester.tap(_rejectButton(_rejectCandidateId));
    expect(transport.commands, hasLength(1));

    firstCommand.completeError(_indeterminateConflict);
    await tester.pumpAndSettle();
    expect(
      find.text('Memory review is temporarily unavailable.'),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('memory-decision-retry')), findsOneWidget);
    expect(tester.widget<OutlinedButton>(_retryButton()).onPressed, isNotNull);
    expect(
      find.byKey(const ValueKey('memory-approve-$_approveCandidateId')),
      findsNothing,
    );
    expect(
      find.byKey(const ValueKey('memory-reject-$_approveCandidateId')),
      findsNothing,
    );
    expect(
      find.byKey(const ValueKey('memory-reject-$_rejectCandidateId')),
      findsNothing,
    );

    harness.controller.clear();
    await harness.controller.loadReview();
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('memory-decision-retry')), findsOneWidget);
    expect(tester.widget<OutlinedButton>(_retryButton()).onPressed, isNotNull);
    _expectDecisionControlsDisabled(tester);
    await tester.tap(_approveButton(_approveCandidateId));
    await tester.tap(_rejectButton(_rejectCandidateId));
    expect(transport.commands, hasLength(1));

    await tester.tap(find.byKey(const ValueKey('memory-decision-retry')));
    await tester.pump();
    expect(find.byKey(const ValueKey('memory-decision-retry')), findsNothing);
    _expectDecisionControlsDisabled(tester);
    await tester.tap(_approveButton(_approveCandidateId));
    await tester.tap(_rejectButton(_rejectCandidateId));
    expect(transport.commands, hasLength(2));

    retryCommand.complete(_acknowledgementResponse(retryRequest));
    await tester.pumpAndSettle();

    expect(
      find.byKey(const ValueKey('memory-decision-acknowledgement')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('memory-decision-retry')), findsNothing);
    expect(transport.commands, hasLength(2));
    expect(
      transport.commands[1]['command_id'],
      transport.commands[0]['command_id'],
    );
    await tester.pumpWidget(const SizedBox.shrink());
    harness.controller.dispose();
  });
}

const _personId = '00000000-0000-4000-8000-000000000001';
const _approveCandidateId = '00000000-0000-4000-8000-000000000002';
const _rejectCandidateId = '00000000-0000-4000-8000-000000000003';

const _indeterminateConflict = AppWireTransportException(
  'conflict',
  'Synthetic indeterminate command result.',
  metadata: {'reason_code': 'conflict'},
  commandOutcome: CommandOutcome.indeterminate,
);

enum _PendingLifecycle { clear, dispose }

enum _LateCompletionKind { success, indeterminateError }

const _fatalOwnerFailure = OwnerFailure(
  domain: 'runtime',
  category: 'integrity',
  reason: 'session_unavailable',
  incidentId: 'incident-memory-review',
  correlationId: 'request-memory-review',
  reloadRequired: true,
  sealSession: true,
  recovery: 'reopen',
  safeActions: {'reopen'},
);

final class _MemoryHarness {
  _MemoryHarness(_ScriptedMemoryTransport transport)
    : gateway = AppWireMemoryGateway(transport) {
    controller = AgentMemoryController(
      memoryGateway: gateway,
      reviewGateway: gateway,
      personId: _personId,
      canOperate: () => true,
      onFatalFailure: fatalFailures.add,
    );
  }

  final AppWireMemoryGateway gateway;
  final List<AppOwnerException> fatalFailures = [];
  late final AgentMemoryController controller;
}

final class _PendingDecisionRun {
  const _PendingDecisionRun({
    required this.harness,
    required this.transport,
    required this.response,
    required this.completion,
    required this.submittedRequest,
  });

  final _MemoryHarness harness;
  final _ScriptedMemoryTransport transport;
  final Completer<Map<String, dynamic>> response;
  final Future<void> completion;
  final Map<String, dynamic> submittedRequest;
}

Future<_PendingDecisionRun> _startPendingDecision() async {
  final commandStarted = Completer<void>();
  final response = Completer<Map<String, dynamic>>();
  late Map<String, dynamic> submittedRequest;
  final transport = _ScriptedMemoryTransport(
    onQuery: _successfulQuery,
    onCommand: (request, call) {
      submittedRequest = request;
      commandStarted.complete();
      return response.future;
    },
  );
  final harness = _MemoryHarness(transport);

  await harness.controller.loadReview();
  final completion = harness.controller.decide(
    _approveCandidateId,
    AgentMemoryDecision.approve,
  );
  await commandStarted.future;

  return _PendingDecisionRun(
    harness: harness,
    transport: transport,
    response: response,
    completion: completion,
    submittedRequest: submittedRequest,
  );
}

Finder _approveButton(String candidateId) => find.descendant(
  of: find.byKey(ValueKey('memory-approve-$candidateId')),
  matching: find.byType(FilledButton),
);

Finder _rejectButton(String candidateId) => find.descendant(
  of: find.byKey(ValueKey('memory-reject-$candidateId')),
  matching: find.byType(TextButton),
);

void _expectDecisionControlsDisabled(WidgetTester tester) {
  final approve = _approveButton(_approveCandidateId);
  final reject = _rejectButton(_rejectCandidateId);
  expect(approve, findsOneWidget);
  expect(reject, findsOneWidget);
  expect(tester.widget<FilledButton>(approve).onPressed, isNull);
  expect(tester.widget<TextButton>(reject).onPressed, isNull);
}

void _expectValidDecisionControlsEnabled(WidgetTester tester) {
  final approve = _approveButton(_approveCandidateId);
  final reject = _rejectButton(_rejectCandidateId);
  expect(approve, findsOneWidget);
  expect(reject, findsOneWidget);
  expect(tester.widget<FilledButton>(approve).onPressed, isNotNull);
  expect(tester.widget<TextButton>(reject).onPressed, isNotNull);
}

void _expectPendingDecisionRetained(_PendingDecisionRun pending) {
  expect(pending.transport.commands, hasLength(1));
  expect(
    pending.harness.gateway.pendingCommandId,
    pending.submittedRequest['command_id'],
  );
  expect(pending.harness.gateway.pendingCandidateId, _approveCandidateId);
  expect(pending.harness.gateway.pendingDecision, AgentMemoryDecision.approve);
  expect(pending.transport.commands.single['command'], {
    'kind': 'memory.decide',
    'candidate_id': _approveCandidateId,
    'decision': 'approve',
  });
}

Finder _retryButton() => find.descendant(
  of: find.byKey(const ValueKey('memory-decision-retry')),
  matching: find.byType(OutlinedButton),
);

typedef _QueryHandler = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
  int call,
);

typedef _CommandHandler = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
  int call,
);

/// Synthetic AppWire seam; it never calls Rust, a provider, a model, or an OS
/// API.
final class _ScriptedMemoryTransport implements AppWireTransport {
  _ScriptedMemoryTransport({required this.onQuery, required this.onCommand});

  final _QueryHandler onQuery;
  final _CommandHandler onCommand;
  final List<Map<String, dynamic>> queries = [];
  final List<Map<String, dynamic>> commands = [];

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final recorded = Map<String, dynamic>.from(request);
    commands.add(recorded);
    return onCommand(recorded, commands.length);
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final recorded = Map<String, dynamic>.from(request);
    queries.add(recorded);
    return onQuery(recorded, queries.length);
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw UnimplementedError();

  @override
  Future<void> close() async {}
}

Future<Map<String, dynamic>> _successfulQuery(
  Map<String, dynamic> request,
  int call,
) async {
  final kind = (request['query'] as Map<String, dynamic>)['kind'];
  return switch (kind) {
    'memory.review' => _reviewResponse(),
    'memory.overview' => _overviewResponse(),
    _ => throw StateError('Unexpected query kind: $kind'),
  };
}

Future<Map<String, dynamic>> _acknowledge(
  Map<String, dynamic> request,
  int call,
) async => _acknowledgementResponse(request);

Map<String, dynamic> _reviewResponse() => {
  'kind': 'memory.review',
  'review': {
    'person_id': _personId,
    'candidates': [
      _candidate(_approveCandidateId, 'Mina prefers morning walks.', [
        'approve',
        'reject',
      ]),
      _candidate(_rejectCandidateId, 'Mina may enjoy a new hobby.', ['reject']),
    ],
  },
};

Map<String, dynamic> _candidate(
  String id,
  String statement,
  List<String> allowedActions,
) => {
  'candidate_id': id,
  'operation': 'create',
  'statement': statement,
  'memory_kind': 'preference',
  'epistemic_status': 'fact',
  'confidence_millis': 800,
  'source_count': 1,
  'created_at': '2026-10-10T00:00:00Z',
  'allowed_actions': allowedActions,
};

Map<String, dynamic> _overviewResponse() => {
  'kind': 'memory.overview',
  'overview': {
    'schema_version': 1,
    'person_id': _personId,
    'saved_count': 0,
    'pending_count': 0,
    'memories': <Map<String, dynamic>>[],
  },
};

Map<String, dynamic> _acknowledgementResponse(Map<String, dynamic> request) {
  final command = request['command'] as Map<String, dynamic>;
  return {
    'kind': 'memory.decision',
    'acknowledgement': {
      'command_id': request['command_id'],
      'candidate_id': command['candidate_id'],
      'decision': command['decision'],
      'committed_at': '2026-10-10T00:00:01Z',
      'resulting_target_id': null,
      'resulting_revision': null,
    },
  };
}
