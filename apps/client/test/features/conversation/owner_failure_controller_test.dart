import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

void main() {
  final operations = <String, Future<void> Function(AgentController)>{
    'registry': (controller) => controller.loadRegistry(),
    'connections': (controller) => controller.connectionController.load(),
    'memory': (controller) => controller.memoryController.load(),
    'memory review': (controller) => controller.memoryController.loadReview(),
  };
  for (final operation in operations.entries) {
    for (final flags in [(false, false), (true, false), (true, true)]) {
      test('${operation.key} preserves owner reload/seal $flags', () async {
        final failure = AgentVaultException(
          'interrupted',
          domain: 'session',
          category: 'integrity',
          reasonCode: 'owner_reason',
          recoveryAction: 'refresh_session',
          safeActions: const ['export_diagnostics'],
          affectedRefs: const ['session-reference'],
          incidentId: 'incident-reference',
          retryPolicy: 'explicit_read',
          reloadRequired: flags.$1,
          sealSession: flags.$2,
        );
        final gateway = _FailureGateway(failure);
        final controller = AgentController(gateway: gateway, personId: 'test');
        addTearDown(() {
          controller.dispose();
          gateway.conversationRuntime.readModel.dispose();
        });
        await controller.load();
        final original = controller.session;

        await operation.value(controller);

        expect(gateway.reads, 1);
        expect(controller.needsReload, flags.$1);
        expect(controller.session, flags.$2 ? isNull : same(original));
        expect(
          controller.vaultState,
          flags.$2 ? AgentVaultState.unavailable : AgentVaultState.ready,
        );
        expect(gateway.stops, 0);
        expect(gateway.begins, 0);
        if (flags.$1 || flags.$2) {
          expect(controller.failure, failure.reasonCode);
          expect(controller.failureDomain, failure.domain);
          expect(controller.failureCategory, failure.category);
          expect(controller.recoveryAction, failure.recoveryAction);
          expect(controller.failureSafeActions, failure.safeActions);
          expect(controller.failureAffectedRefs, failure.affectedRefs);
          expect(controller.failureIncidentId, failure.incidentId);
          expect(controller.failureRetryPolicy, failure.retryPolicy);
          expect(controller.canSend, isFalse);
        } else {
          expect(controller.failure, isNull);
          expect(controller.canSend, isTrue);
        }
      });
    }
  }
}

final class _FailureGateway extends TestVaultGateway
    implements
        AgentRegistryGateway,
        AgentConnectionsGateway,
        AgentMemoryGateway,
        AgentMemoryReviewGateway {
  _FailureGateway(this.failure) : super(personal: true) {
    state = AgentVaultState.ready;
  }

  final AgentVaultException failure;
  int reads = 0;

  Future<Result> _read<Result>() async {
    reads++;
    throw failure;
  }

  @override
  Future<AgentRegistryView?> readRegistry(String personId) => _read();

  @override
  @override
  Future<List<AgentConnection>> readConnections(String personId) => _read();

  @override
  Future<AgentMemoryOverview> readMemory(String personId) => _read();

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId) =>
      _read();

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnsupportedError('Unexpected mutation');
}
