import '../../support/app_wire_transport.dart';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/native_calendar_action_gateway.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

void main() {
  test('action policy mutation uses the authoritative vault job', () async {
    final calls = <String>[];
    final gateway = NativeCalendarActionGateway(
      CallbackAppWireTransport((request) async {
        final intent = (request['command'] ?? request['query']) as Map;
        calls.add(intent['kind'] as String);
        expect(request.containsKey('person_id'), isFalse);
        if (intent['kind'] == 'actions.calendar') {
          expect(intent['operation'], {
            'kind': 'set_authority',
            'calendar_create': 'deny',
          });
          return {
            'kind': 'action_operation',
            'operation_id': ownerOperationId(request),
            'done': false,
            'failure': null,
          };
        }
        expect(intent['kind'], 'actions.read_result');
        return {
          'kind': 'action_operation',
          'operation_id': ownerOperationId(request),
          'done': true,
          'failure': null,
          'calendar_actions': {
            'actions': <Object>[],
            'authority': {'calendar_create': 'deny'},
          },
        };
      }),
    );
    final authority = await gateway.setCalendarCreateAuthority(
      'person',
      ActionAuthorityMode.deny,
    );
    expect(authority.calendarCreate, ActionAuthorityMode.deny);
    expect(calls, [
      'actions.calendar',
      'actions.read_result',
      'actions.read_result',
    ]);
  });

  test(
    'vault action failure is released and retains recovery metadata',
    () async {
      final calls = <String>[];
      final gateway = NativeCalendarActionGateway(
        CallbackAppWireTransport((request) async {
          final intent = request['query'] as Map;
          calls.add(intent['kind'] as String);
          return {
            'kind': 'action_operation',
            'operation_id': ownerOperationId(request),
            'done': true,
            'failure': ownerFailure(
              request,
              'vault_unavailable',
              'calendar_action',
              recovery: 'reopen_vault',
            ),
          };
        }),
      );
      await expectLater(
        gateway.loadActionAuthority('person'),
        throwsA(
          isA<AgentVaultException>()
              .having((error) => error.failure, 'failure', 'vault_unavailable')
              .having(
                (error) => error.recoveryAction,
                'recovery',
                'reopen_vault',
              ),
        ),
      );
      expect(calls, ['actions.authority', 'actions.read_result']);
    },
  );

}
