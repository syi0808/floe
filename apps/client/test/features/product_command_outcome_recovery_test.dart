import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/app_wire_operation_authorization_gateway.dart';
import 'package:floe_client/features/experts/infrastructure/app_wire_registry_gateway.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';
import 'package:floe_client/features/knowledge/infrastructure/app_wire_memory_gateway.dart';

void main() {
  test(
    'Experts retries an indeterminate owner conflict with the same ID',
    () async {
      final transport = _RecordingTransport(_indeterminateConflict);
      final gateway = AppWireRegistryGateway(transport);

      await expectLater(
        gateway.setInstallationEnabled(
          installationRef: 'installation',
          expectedRevision: 1,
          enabled: true,
        ),
        throwsA(isA<AppOwnerException>()),
      );
      final firstCommandId = transport.commands.single['command_id'];
      await expectLater(
        gateway.retryPendingCommand(),
        throwsA(isA<AppOwnerException>()),
      );

      expect(transport.commands, hasLength(2));
      expect(transport.commands[1]['command_id'], firstCommandId);
    },
  );

  test(
    'Memory retries an indeterminate owner conflict with the same ID',
    () async {
      final transport = _RecordingTransport(_indeterminateConflict);
      final gateway = AppWireMemoryGateway(transport);

      await expectLater(
        gateway.decideMemoryCandidate(
          personId: 'person',
          candidateId: 'candidate',
          decision: AgentMemoryDecision.approve,
        ),
        throwsA(isA<AppOwnerException>()),
      );
      final firstCommandId = transport.commands.single['command_id'];
      await expectLater(
        gateway.retryPendingDecision(personId: 'person'),
        throwsA(isA<AppOwnerException>()),
      );

      expect(transport.commands, hasLength(2));
      expect(transport.commands[1]['command_id'], firstCommandId);
    },
  );

  test(
    'Access preserves the router command disposition for its caller',
    () async {
      final transport = _RecordingTransport(_indeterminateConflict);
      final gateway = AppWireOperationAuthorizationGateway(transport);

      await expectLater(
        gateway.setAuthority(
          commandId: '00000000-0000-4000-8000-000000000001',
          mode: ActionAuthorityMode.ask,
          expectedRevision: 1,
        ),
        throwsA(
          isA<AppRuntimeException>().having(
            (error) => error.commandOutcome,
            'commandOutcome',
            CommandOutcome.indeterminate,
          ),
        ),
      );
    },
  );
}

const _indeterminateConflict = AppWireTransportException(
  'conflict',
  'Owner conflict without a definitive command disposition.',
  metadata: {'reason_code': 'conflict'},
  commandOutcome: CommandOutcome.indeterminate,
);

final class _RecordingTransport implements AppWireTransport {
  _RecordingTransport(this.failure);

  final AppWireTransportException failure;
  final List<Map<String, dynamic>> commands = [];

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    commands.add(Map<String, dynamic>.from(request));
    throw failure;
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw UnimplementedError();

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw UnimplementedError();

  @override
  Future<void> close() async {}
}
