import 'package:floe_client/features/agent/agent_calendar_experts.dart';
import 'package:floe_client/features/agent/agent_registry.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_calendar_experts.dart';
import '../../support/agent_registry.dart';

void main() {
  test('setup intent is immutable canonical and byte bounded without enabling flags', () {
    final ids = ['work', 'home'];
    final request = calendarSetupRequest(calendarIds: ids);
    ids.clear();
    expect(request.calendarIds, ['home', 'work']);
    expect(() => request.calendarIds.clear(), throwsUnsupportedError);
    expect(request.toJson().keys, [
      'instance_id',
      'expected_revision',
      'setup_id',
      'provider',
      'calendar_ids',
    ]);
    final unicode = calendarSetupRequest(calendarIds: ['\u{10000}', '\ue000']);
    expect(unicode.calendarIds, ['\ue000', '\u{10000}']);
    for (final invalid in [
      <String>[],
      ['same', 'same'],
      [' '],
      ['한' * 171],
      ['a', 'b', 'c', 'd', 'e'],
      ['\ud800'],
    ]) {
      expect(
        () => calendarSetupRequest(calendarIds: invalid),
        throwsFormatException,
      );
    }
  });

  test(
    'management snapshot validates ownership scope packages and receipt links',
    () {
      final parsed = AgentCalendarExperts.fromJson(calendarExpertsFixture());
      expect(
        parsed.receiptFor(calendarSetupRequest())!.viewHandle,
        calendarViewId,
      );
      expect(() => parsed.views.clear(), throwsUnsupportedError);
      expect(() => parsed.setups.clear(), throwsUnsupportedError);
      for (var mode = 0; mode < 14; mode++) {
        final fixture = calendarExpertsFixture();
        final registry = fixture['registry'] as Map;
        final view = (fixture['views'] as List).single as Map;
        final setup = (fixture['setups'] as List).single as Map;
        switch (mode) {
          case 0:
            view['person_id'] = registryInstance;
          case 1:
            setup['person_id'] = registryInstance;
          case 2:
            setup['view_handle'] = registryInstance;
          case 3:
            setup['expert_assignment_id'] = calendarToolAssignment;
          case 4:
            setup['expected_revision'] = registry['revision'];
          case 5:
            view['calendar_ids'] = ['work', 'home'];
          case 6:
            view['calendar_ids'] = ['same', 'same'];
          case 7:
            view['provider'] = 'unknown';
          case 8:
            view['enabled'] = 1;
          case 9:
            (fixture['setups'] as List).add(Map<String, Object>.from(setup));
          case 10:
            (((registry['installations'] as List).last as Map)['package']
                    as Map)['id'] =
                'floe.schedule';
          case 11:
            setup['setup_id'] = '00000000-0000-0000-0000-000000000000';
          case 12:
            (fixture['views'] as List).add(Map<String, Object>.from(view));
          case 13:
            view['device_id'] = '';
        }
        expect(
          () => AgentCalendarExperts.fromJson(fixture),
          throwsFormatException,
          reason: 'mode $mode',
        );
      }
    },
  );

  test('read-only inspection supplies instance and revision zero before the first sample', () async {
    final transport = CalendarExpertTransport();
    final gateway = NativeAgentVaultGateway(
      transport.call,
      deviceId: 'test-device',
    );
    final empty = await gateway.readCalendarExperts(registryPerson);
    expect(empty.registry.instanceId, registryInstance);
    expect(empty.registry.revision, 0);
    expect(empty.setups, isEmpty);
    expect(transport.installations, 0);
    expect(transport.pending, isNull);
  });

  test('setup uses explicit stable identity and retries tolerate later revocation or state changes', () async {
    final transport = CalendarExpertTransport();
    final gateway = NativeAgentVaultGateway(
      transport.call,
      deviceId: 'test-device',
    );
    final request = calendarSetupRequest();
    final installed = await gateway.installCalendarExpert(request);
    expect(installed.views.single.enabled, false);
    expect(transport.committedSetup, {
      ...request.toJson(),
      'device_id': 'test-device',
    });
    var registry = await gateway.configureRegistry(
      installed.registry,
      target: AgentRegistryTarget.calendarView,
      id: calendarViewId,
      enabled: true,
    );
    registry = await gateway.configureRegistry(
      registry,
      target: AgentRegistryTarget.calendarView,
      id: calendarViewId,
      enabled: false,
    );
    final replay = await gateway.installCalendarExpert(request);
    expect(replay.registry.revision, registry.revision);
    expect(replay.views.single.enabled, false);
    expect(replay.receiptFor(request)!.setupId, request.setupId);
    expect(transport.installations, 1);
  });

  test('uncertain submit poll and release are drained before exact setup reconciliation', () async {
    for (final loss in ['submit', 'poll', 'release_before', 'release_after']) {
      final transport = CalendarExpertTransport()..loss = loss;
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
      final request = calendarSetupRequest();
      await expectLater(
        gateway.installCalendarExpert(request),
        throwsStateError,
      );
      final result = await gateway.installCalendarExpert(request);
      expect(result.receiptFor(request), isNotNull);
      expect(transport.installations, 1, reason: loss);
      expect(transport.submissions, 2, reason: loss);
      expect(transport.pending, isNull);
    }
  });

  test(
    'refresh reconciles accepted installation without submitting it again',
    () async {
      final transport = CalendarExpertTransport()..loss = 'submit';
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
      final request = calendarSetupRequest();
      await expectLater(
        gateway.installCalendarExpert(request),
        throwsStateError,
      );
      final result = await gateway.readCalendarExperts(registryPerson);
      expect(result.receiptFor(request), isNotNull);
      expect(transport.submissions, 1);
    },
  );

  test(
    'foreign or mismatched setup results and unavailable vaults fail closed',
    () async {
      for (var mode = 0; mode < 7; mode++) {
        final transport = CalendarExpertTransport();
        final gateway = NativeAgentVaultGateway((request) async {
          final result = await transport.call(request);
          final overview = result['calendar_experts'] as Map?;
          if (overview != null && (overview['setups'] as List).isNotEmpty) {
            switch (mode) {
              case 0:
                (overview['registry'] as Map)['person_id'] = registryInstance;
              case 1:
                (overview['registry'] as Map)['instance_id'] = registryPerson;
              case 2:
                ((overview['views'] as List).single as Map)['calendar_ids'] = [
                  'changed',
                ];
              case 3:
                (overview['setups'] as List).clear();
              case 4:
                result['state'] = 'locked';
              case 5:
                result['failure'] = 'vault_unavailable';
              case 6:
                ((overview['views'] as List).single as Map)['device_id'] =
                    'other-device';
            }
          }
          return result;
        }, deviceId: 'test-device');
        await expectLater(
          gateway.installCalendarExpert(calendarSetupRequest()),
          mode == 5
              ? throwsA(isA<AgentVaultException>())
              : throwsFormatException,
        );
      }
    },
  );
}
