import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';

enum _FirstPrepare { lost, malformed, succeeds }

final class _RuntimeGateway implements RuntimeGateway {
  _RuntimeGateway(this.firstPrepare);

  final _FirstPrepare firstPrepare;
  final List<String> readinessIds = [];
  final List<String> prepareIds = [];
  final List<String> getIds = [];
  final List<String> acknowledgeIds = [];

  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async {
    readinessIds.add(requestId);
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
    return RuntimePreparationResult(operationId: operationId, done: true);
  }

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async {
    getIds.add(operationId);
    throw const AppOwnerException('not_found');
  }

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async {
    acknowledgeIds.add(operationId);
    return RuntimePreparationResult(operationId: operationId, done: true);
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
