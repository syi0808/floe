import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';

enum _FirstPrepare { lost, malformed, succeeds }

typedef _ReadinessHandler = Future<RuntimeReadinessSnapshot> Function(
  int call,
  String requestId,
);

final class _RuntimeGateway implements RuntimeGateway {
  _RuntimeGateway(
    this.firstPrepare, {
    this.readinessHandler,
    this.prepareFailures = const [],
  });

  final _FirstPrepare firstPrepare;
  final _ReadinessHandler? readinessHandler;
  final List<String?> prepareFailures;
  final List<String> readinessIds = [];
  final List<String> prepareIds = [];
  final List<String> getIds = [];
  final List<String> acknowledgeIds = [];
  final Map<String, String?> _operationFailures = {};

  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async {
    readinessIds.add(requestId);
    if (readinessHandler != null) {
      return readinessHandler!(readinessIds.length, requestId);
    }
    return RuntimeReadinessSnapshot(
      state: readinessIds.length == 1
          ? RuntimeReadinessState.preparationRequired
          : RuntimeReadinessState.ready,
    );
  }

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async {
    prepareIds.add(operationId);
    if (prepareIds.length == 1 && firstPrepare == _FirstPrepare.lost) {
      throw const AppOwnerException(
        'interrupted',
        commandDisposition: NativeCommandDisposition.indeterminate,
      );
    }
    if (prepareIds.length == 1 && firstPrepare == _FirstPrepare.malformed) {
      return const RuntimePreparationResult(
        operationId: 'malformed-operation-id',
        done: true,
      );
    }
    final failure = prepareFailures.length >= prepareIds.length
        ? prepareFailures[prepareIds.length - 1]
        : null;
    _operationFailures[operationId] = failure;
    return RuntimePreparationResult(
      operationId: operationId,
      done: true,
      failure: failure,
    );
  }

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async {
    getIds.add(operationId);
    throw const AppOwnerException('not_found');
  }

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async {
    acknowledgeIds.add(operationId);
    return RuntimePreparationResult(
      operationId: operationId,
      done: true,
      failure: _operationFailures[operationId],
    );
  }
}

void main() {
  test(
    'uncertain admission rejoins and resubmits the same preparation UUID',
    () async {
      await _assertRecoveryKeepsIdentity(_FirstPrepare.lost);
    },
  );

  test(
    'malformed preparation response retains its UUID for recovery',
    () async {
      await _assertRecoveryKeepsIdentity(_FirstPrepare.malformed);
    },
  );

  test(
    'provider failures do not invalidate shared Runtime readiness',
    () async {
      final gateway = _RuntimeGateway(_FirstPrepare.succeeds);
      final controller = RuntimeController(
        gateway: gateway,
        personId: 'runtime-test-person',
      );
      await controller.open();

      final readinessQueries = gateway.readinessIds.length;
      controller.reportFailure(const AppOwnerException('model_unavailable'));

      expect(controller.ready, isTrue);
      expect(gateway.readinessIds.length, readinessQueries);
      controller.dispose();
    },
  );

  test(
    'late ready response cannot undo feature failure and queues pure reobserve',
    () async {
      final oldReady = Completer<RuntimeReadinessSnapshot>();
      final freshReadiness = Completer<RuntimeReadinessSnapshot>();
      final freshReadinessStarted = Completer<void>();
      final gateway = _RuntimeGateway(
        _FirstPrepare.succeeds,
        readinessHandler: (call, _) {
          if (call == 1) return oldReady.future;
          if (call == 2) {
            freshReadinessStarted.complete();
            return freshReadiness.future;
          }
          throw StateError('unexpected readiness call $call');
        },
      );
      final controller = RuntimeController(
        gateway: gateway,
        personId: 'runtime-test-person',
      );

      final opening = controller.open();
      expect(gateway.readinessIds, hasLength(1));
      controller.reportFailure(_vaultError('feature-incident'));
      expect(controller.state, RuntimeReadinessState.unavailable);
      expect(controller.ready, isFalse);

      oldReady.complete(
        const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
      );
      await opening;
      await freshReadinessStarted.future.timeout(const Duration(seconds: 1));

      expect(gateway.readinessIds, hasLength(2));
      expect(controller.state, RuntimeReadinessState.unavailable);
      expect(controller.failure?.incidentId, 'feature-incident');
      expect(controller.ready, isFalse);
      expect(gateway.prepareIds, isEmpty);

      final settled = Completer<void>();
      controller.addListener(() {
        if (!controller.busy && !settled.isCompleted) settled.complete();
      });
      freshReadiness.complete(
        RuntimeReadinessSnapshot(
          state: RuntimeReadinessState.unavailable,
          failure: _vaultFailure('fresh-incident'),
        ),
      );
      await settled.future.timeout(const Duration(seconds: 1));

      expect(controller.ready, isFalse);
      expect(controller.state, RuntimeReadinessState.unavailable);
      controller.dispose();
    },
  );

  test('closing admission fences an outstanding readiness response', () async {
    final pendingReadiness = Completer<RuntimeReadinessSnapshot>();
    final gateway = _RuntimeGateway(
      _FirstPrepare.succeeds,
      readinessHandler: (_, _) => pendingReadiness.future,
    );
    final controller = RuntimeController(
      gateway: gateway,
      personId: 'runtime-test-person',
    );
    var notifications = 0;
    controller.addListener(() => notifications++);

    final opening = controller.open();
    controller.closeAdmission();
    final notificationsAfterClose = notifications;
    pendingReadiness.complete(
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
    );
    await opening;

    expect(controller.state, RuntimeReadinessState.unknown);
    expect(controller.ready, isFalse);
    expect(controller.busy, isFalse);
    expect(notifications, notificationsAfterClose);
    controller.dispose();
  });

  test(
    'Retry after a failed receipt starts a new prepare with a new UUID',
    () async {
      final retryReadiness = RuntimeReadinessSnapshot(
        state: RuntimeReadinessState.preparationRequired,
        failure: _vaultFailure('retry-incident'),
      );
      final snapshots = [
        const RuntimeReadinessSnapshot(
          state: RuntimeReadinessState.preparationRequired,
        ),
        retryReadiness,
        retryReadiness,
        const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
      ];
      final gateway = _RuntimeGateway(
        _FirstPrepare.succeeds,
        readinessHandler: (call, _) async => snapshots[call - 1],
        prepareFailures: const ['vault_locked'],
      );
      final controller = RuntimeController(
        gateway: gateway,
        personId: 'runtime-test-person',
      );

      await controller.open();
      expect(gateway.prepareIds, hasLength(1));
      expect(controller.state, RuntimeReadinessState.preparationRequired);
      expect(controller.canRecover, isTrue);

      await controller.recover();

      expect(gateway.prepareIds, hasLength(2));
      expect(gateway.prepareIds[1], isNot(gateway.prepareIds[0]));
      expect(gateway.acknowledgeIds, gateway.prepareIds);
      expect(controller.state, RuntimeReadinessState.ready);
      expect(controller.failure, isNull);
      expect(controller.ready, isTrue);
      controller.dispose();
    },
  );

  test('reobserve-only recovery does not submit preparation', () async {
    final snapshots = [
      RuntimeReadinessSnapshot(
        state: RuntimeReadinessState.unavailable,
        failure: _vaultFailure('reobserve-incident', allowRetry: false),
      ),
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
    ];
    final gateway = _RuntimeGateway(
      _FirstPrepare.succeeds,
      readinessHandler: (call, _) async => snapshots[call - 1],
    );
    final controller = RuntimeController(
      gateway: gateway,
      personId: 'runtime-test-person',
    );

    await controller.open();
    expect(controller.canRecover, isTrue);
    await controller.recover();

    expect(gateway.readinessIds, hasLength(2));
    expect(gateway.prepareIds, isEmpty);
    expect(controller.state, RuntimeReadinessState.ready);
    expect(controller.ready, isTrue);
    controller.dispose();
  });

  test(
    'failed historical receipt does not override fresh ready state',
    () async {
      final gateway = _RuntimeGateway(
        _FirstPrepare.succeeds,
        readinessHandler: (call, _) async => call == 1
            ? const RuntimeReadinessSnapshot(
                state: RuntimeReadinessState.preparationRequired,
              )
            : const RuntimeReadinessSnapshot(
                state: RuntimeReadinessState.ready,
              ),
        prepareFailures: const ['vault_locked'],
      );
      final controller = RuntimeController(
        gateway: gateway,
        personId: 'runtime-test-person',
      );

      await controller.open();

      expect(gateway.acknowledgeIds, gateway.prepareIds);
      expect(gateway.readinessIds, hasLength(2));
      expect(controller.state, RuntimeReadinessState.ready);
      expect(controller.failure, isNull);
      expect(controller.canRecover, isFalse);
      expect(controller.ready, isTrue);
      controller.dispose();
    },
  );

  test(
    'successful historical receipt does not override fresh unavailable state',
    () async {
      final gateway = _RuntimeGateway(
        _FirstPrepare.succeeds,
        readinessHandler: (call, _) async => call == 1
            ? const RuntimeReadinessSnapshot(
                state: RuntimeReadinessState.preparationRequired,
              )
            : RuntimeReadinessSnapshot(
                state: RuntimeReadinessState.unavailable,
                failure: _vaultFailure('latest-failure'),
              ),
      );
      final controller = RuntimeController(
        gateway: gateway,
        personId: 'runtime-test-person',
      );

      await controller.open();

      expect(gateway.acknowledgeIds, gateway.prepareIds);
      expect(controller.state, RuntimeReadinessState.unavailable);
      expect(controller.failure?.incidentId, 'latest-failure');
      expect(controller.ready, isFalse);
      controller.dispose();
    },
  );
}

Future<void> _assertRecoveryKeepsIdentity(_FirstPrepare firstPrepare) async {
  final gateway = _RuntimeGateway(firstPrepare);
  final controller = RuntimeController(
    gateway: gateway,
    personId: 'runtime-test-person',
  );

  await controller.open();
  expect(controller.hasPendingOperation, isTrue);
  expect(controller.canRecover, isTrue);
  final operationId = gateway.prepareIds.single;

  await controller.recover();

  expect(gateway.getIds, [operationId]);
  expect(gateway.prepareIds, [operationId, operationId]);
  expect(gateway.acknowledgeIds, [operationId]);
  expect(controller.hasPendingOperation, isFalse);
  expect(controller.ready, isTrue);
  controller.dispose();
}

OwnerFailure _vaultFailure(String incidentId, {bool allowRetry = true}) =>
    OwnerFailure(
      domain: 'vault',
      category: 'user_configuration',
      reason: 'vault_locked',
      incidentId: incidentId,
      correlationId: 'correlation-$incidentId',
      reloadRequired: true,
      sealSession: true,
      recovery: 'reobserve',
      safeActions: allowRetry ? const {'retry'} : const {},
    );

AppOwnerException _vaultError(String incidentId) {
  final failure = _vaultFailure(incidentId);
  return AppOwnerException.fromAppWire(failure.reason, ownerFailure: failure);
}
