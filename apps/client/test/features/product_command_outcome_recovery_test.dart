import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/app_wire_operation_authorization_gateway.dart';
import 'package:floe_client/features/conversation/assistant_features/infrastructure/app_wire_assistant_feature_gateway.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';
import 'package:floe_client/features/knowledge/infrastructure/app_wire_memory_gateway.dart';

void main() {
  test('Assistant feature retries an indeterminate owner conflict with the same ID', () async {
    final transport = _RecordingTransport(_indeterminateConflict);
    final gateway = AppWireAssistantFeatureGateway(transport);

    await expectLater(
      gateway.configure(
        featureRef: '00000000-0000-4000-8000-000000000001',
        expectedRevision: 1,
        enabled: true,
        sourceSelections: [_assistantFeatureSelection()],
      ),
      throwsA(isA<AppOwnerException>()),
    );
    final firstCommand = transport.commands.single;
    await expectLater(
      gateway.retryPendingCommand(),
      throwsA(isA<AppOwnerException>()),
    );

    expect(transport.commands, hasLength(2));
    expect(transport.commands[1]['command_id'], firstCommand['command_id']);
    expect(transport.commands[1]['command'], firstCommand['command']);
    expect(
      transport.commands[1]['request_id'],
      isNot(firstCommand['request_id']),
    );
  });

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

AssistantFeatureSourceSelection _assistantFeatureSelection() {
  final review = AssistantFeatureSourceReview.fromJson({
    'review_ref': {
      'id': '00000000-0000-4000-8000-0000000000aa',
      'digest': '1${List.filled(63, '0').join()}',
    },
    'source_scope_ref': '00000000-0000-4000-8000-000000000002',
    'source_requirement_ref': 'floe.source.calendar',
    'binding_revision': 7,
    'candidates': [
      {
        'candidate_ref': '00000000-0000-4000-8000-00000000000b',
        'label': 'Work calendar',
        'availability': 'available',
        'selected': false,
      },
    ],
    'expires_at_unix_ms': DateTime.now().millisecondsSinceEpoch + 60000,
    'allowed_actions': ['replace'],
  });
  return AssistantFeatureSourceSelection(
    review: review,
    candidateRefs: const ['00000000-0000-4000-8000-00000000000b'],
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
