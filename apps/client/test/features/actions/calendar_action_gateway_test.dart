import '../../support/app_host.dart';
import '../../support/app_wire_transport.dart';

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/native_calendar_action_gateway.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

import '../day/calendar_gateway_test.dart' show FixtureCalendarAdapter, query;

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

  test('action review requires an unlocked vault while proposals persist without writes', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    expect(
      library.existsSync(),
      isTrue,
      reason: 'cargo build -p floe-ffi required',
    );
    final directory = await Directory.systemTemp.createTemp('floe-actions-');
    final adapter = FixtureCalendarAdapter()..records = [];
    Future<TestAppHost> open() => TestAppHost.open(
      libraryPath: library.path,
      databasePath: '${directory.path}/actions.db',
      calendarAdapter: adapter,
      clock: () => DateTime.utc(2000),
      deviceId: 'test-device',
    );
    var gateway = await open();
    try {
      await gateway.day.selectCalendars(adapter.inventory, query);
      expect(
        await gateway.actions.loadCalendarActions(query.personId),
        isEmpty,
      );
      final now = DateTime.now().toUtc();
      Future<CalendarAction> propose() => gateway.actions.proposeCalendarAction(
        personId: query.personId,
        calendarId: 'fixture',
        title: 'Focus',
        startsAt: now.add(const Duration(hours: 1)),
        endsAt: now.add(const Duration(hours: 2)),
        timezone: 'Asia/Seoul',
      );
      final pending = await propose();
      expect(pending.status, CalendarActionStatus.pending);
      expect(pending.createdAt.year, now.year);
      expect(pending.calendarName, 'Test calendar');
      expect(
        pending.expiresAt.difference(pending.createdAt),
        const Duration(minutes: 15),
      );
      await expectLater(
        gateway.actions.decideCalendarAction(
          personId: query.personId,
          actionId: pending.id,
          decision: CalendarActionDecision.approve,
        ),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'vault_unavailable',
          ),
        ),
      );
      await gateway.close();
      gateway = await open();
      final restored = (await gateway.actions.loadCalendarActions(
        query.personId,
      )).single;
      expect(restored.status, CalendarActionStatus.pending);
      expect(restored.executionId, pending.executionId);
      expect((await gateway.day.loadDay(query)).items, isEmpty);
      expect(adapter.records, isEmpty);
    } finally {
      await gateway.close();
      await directory.delete(recursive: true);
    }
  });
}
