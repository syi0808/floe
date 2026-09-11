import 'dart:async';
import 'dart:convert';

import 'package:floe_client/features/agent/agent_calendar_experts.dart';
import 'package:floe_client/features/agent/agent_registry.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';

import 'agent_registry.dart';
import 'agent_vault_gateway.dart';

const calendarSetupId = '00000000-0000-4000-8000-000000000005';
const calendarViewId = '00000000-0000-4000-8000-000000000006';
const calendarToolInstallation = '00000000-0000-4000-8000-000000000007';
const calendarToolAssignment = '00000000-0000-4000-8000-000000000008';

AgentCalendarSetup calendarSetupRequest({List<String>? calendarIds}) =>
    AgentCalendarSetup(
      personId: registryPerson,
      instanceId: registryInstance,
      expectedRevision: 0,
      setupId: calendarSetupId,
      provider: 'event_kit',
      calendarIds: calendarIds ?? ['work', 'home'],
    );

Map<String, dynamic> calendarExpertsFixture({bool installed = true}) => {
  'registry': {
    'schema_version': 1,
    'person_id': registryPerson,
    'instance_id': registryInstance,
    'revision': installed ? 1 : 0,
    'installations': [
      if (installed) ...[
        {
          'id': calendarToolInstallation,
          'package': {
            'kind': 'tool',
            'id': 'floe.builtin.schedule.context',
            'version': '1.0.0',
          },
          'enabled': false,
        },
        {
          'id': registryInstallation,
          'package': {
            'kind': 'expert',
            'id': 'floe.builtin.schedule',
            'version': '1.0.0',
          },
          'enabled': false,
        },
      ],
    ],
    'assignments': [
      if (installed) ...[
        {
          'id': calendarToolAssignment,
          'installation_id': calendarToolInstallation,
          'enabled': false,
          'granted_tool_count': 0,
          'granted_view_count': 1,
          'state_revision': 0,
          'completed_invocations': 0,
        },
        {
          'id': registryAssignment,
          'installation_id': registryInstallation,
          'enabled': false,
          'granted_tool_count': 1,
          'granted_view_count': 1,
          'state_revision': 0,
          'completed_invocations': 0,
        },
      ],
    ],
  },
  'views': [
    if (installed)
      {
        'handle': calendarViewId,
        'person_id': registryPerson,
        'provider': 'event_kit',
        'device_id': 'test-device',
        'calendar_ids': ['home', 'work'],
        'enabled': false,
      },
  ],
  'setups': [
    if (installed)
      {
        'setup_id': calendarSetupId,
        'person_id': registryPerson,
        'expected_revision': 0,
        'view_handle': calendarViewId,
        'tool_installation_id': calendarToolInstallation,
        'expert_installation_id': registryInstallation,
        'tool_assignment_id': calendarToolAssignment,
        'expert_assignment_id': registryAssignment,
      },
  ],
};

final class CalendarExpertTransport {
  Map<String, dynamic> snapshot = calendarExpertsFixture(installed: false);
  Map<String, dynamic>? pending;
  Map<String, dynamic>? committedSetup;
  String? loss;
  String? failure;
  int installations = 0;
  int submissions = 0;
  final List<String> operations = [];

  Future<Map<String, dynamic>> call(Map<String, Object?> request) async {
    final operation = request['operation'] as Map;
    final kind = operation['kind'] as String;
    operations.add(kind);
    if (kind == 'submit') {
      if (pending != null) throw const AgentVaultException('conflict');
      final action = operation['action'] as Map;
      final setup = action['setup'] as Map?;
      if (setup != null) {
        submissions++;
        if (committedSetup != null &&
            jsonEncode(committedSetup) != jsonEncode(setup)) {
          throw const AgentVaultException('conflict');
        }
        if (committedSetup == null) {
          committedSetup = Map<String, dynamic>.from(setup);
          installations++;
          snapshot = calendarExpertsFixture();
          ((snapshot['setups'] as List).single as Map)['setup_id'] =
              setup['setup_id'];
          ((snapshot['views'] as List).single as Map)['calendar_ids'] =
              setup['calendar_ids'];
        }
      }
      if (action['kind'] == 'registry') {
        final change = action['change'] as Map;
        if (change['expected_revision'] !=
            (snapshot['registry'] as Map)['revision']) {
          throw const AgentVaultException('conflict');
        }
        final target = change['target'] as Map;
        if (target['kind'] != 'calendar_view' ||
            target['id'] != calendarViewId) {
          throw const AgentVaultException('not_found');
        }
        ((snapshot['views'] as List).single as Map)['enabled'] =
            target['enabled'];
        (snapshot['registry'] as Map)['revision'] =
            (change['expected_revision'] as int) + 1;
      }
      if (action['kind'] == 'calendar_access') {
        final request = action['change'] as Map;
        if (request['expected_revision'] !=
                (snapshot['registry'] as Map)['revision'] ||
            request['setup_id'] !=
                ((snapshot['setups'] as List).single as Map)['setup_id']) {
          throw const AgentVaultException('conflict');
        }
        final change = request['change'] as Map;
        switch (change['kind']) {
          case 'set_enabled':
            ((snapshot['views'] as List).single as Map)['enabled'] =
                change['enabled'];
            for (final installation in snapshot['registry']['installations']) {
              installation['enabled'] = change['enabled'];
            }
            for (final assignment in snapshot['registry']['assignments']) {
              assignment['enabled'] = change['enabled'];
            }
          case 'set_scope':
            ((snapshot['views'] as List).single as Map)['calendar_ids'] = [
              ...change['calendar_ids'] as List,
            ]..sort();
          case 'remove':
            (snapshot['setups'] as List).clear();
            (snapshot['views'] as List).clear();
            (snapshot['registry']['installations'] as List).clear();
            (snapshot['registry']['assignments'] as List).clear();
          default:
            throw const AgentVaultException('invalid_input');
        }
        (snapshot['registry'] as Map)['revision'] =
            (request['expected_revision'] as int) + 1;
      }
      pending = {
        'request_id': request['request_id'],
        'events': <Object>[],
        'next_sequence': 0,
        'done': true,
        'state': failure == null ? 'ready' : 'unavailable',
        'failure': failure,
        if (action['kind'] == 'calendar_experts')
          'calendar_experts': jsonDecode(jsonEncode(snapshot)),
        if (action['kind'] == 'calendar_access')
          'calendar_experts': jsonDecode(jsonEncode(snapshot)),
        if (action['kind'] == 'registry')
          'registry': jsonDecode(jsonEncode(snapshot['registry'])),
      };
      if (setup != null && loss == 'submit') {
        loss = null;
        throw StateError('Lost accepted submit response');
      }
      if (setup != null && loss == 'poll') return {...pending!, 'done': false};
    } else if (pending == null ||
        pending!['request_id'] != request['request_id']) {
      throw const AgentVaultException('not_found');
    }
    if (kind == 'poll' && loss == 'poll') {
      loss = null;
      throw StateError('Lost poll response');
    }
    if (kind == 'release') {
      if (loss == 'release_before') {
        loss = null;
        throw StateError('Release not received');
      }
      final result = pending!;
      pending = null;
      if (loss == 'release_after') {
        loss = null;
        throw StateError('Lost release response');
      }
      return result;
    }
    return pending!;
  }
}

class TestCalendarExpertGateway extends TestVaultGateway
    implements AgentRegistryGateway, AgentCalendarExpertGateway {
  TestCalendarExpertGateway() : super(personId: registryPerson) {
    state = AgentVaultState.ready;
    native = NativeAgentVaultGateway(transport.call, deviceId: 'test-device');
  }

  final transport = CalendarExpertTransport();
  late final NativeAgentVaultGateway native;
  Completer<void>? gate;
  String? error;
  final List<AgentCalendarSetup> requests = [];

  Future<void> _wait() async {
    await gate?.future;
    if (error case final failure?) throw AgentVaultException(failure);
  }

  @override
  Future<AgentCalendarExperts> readCalendarExperts(String personId) async {
    await _wait();
    return native.readCalendarExperts(personId);
  }

  @override
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup request,
  ) async {
    requests.add(request);
    await _wait();
    return native.installCalendarExpert(request);
  }

  @override
  Future<AgentCalendarExperts> configureCalendarAccess(
    AgentCalendarAccessRequest request,
  ) async {
    await _wait();
    return native.configureCalendarAccess(request);
  }

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async =>
      (await readCalendarExperts(personId)).registry;

  @override
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  }) async {
    await _wait();
    return native.configureRegistry(
      current,
      target: target,
      id: id,
      enabled: enabled,
    );
  }
}
