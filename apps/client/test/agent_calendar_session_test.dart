import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_proposal.dart';

const setupId = '00000000-0000-4000-8000-000000000005';

Map<String, Object?> sessionJson({
  String provider = 'event_kit',
  int revision = 0,
  String? activeTurn,
}) => {
  'schema_version': 1,
  'id': proposalSession,
  'person_id': proposalPerson,
  'scope': {'kind': 'calendar', 'setup_id': setupId, 'provider': provider},
  'revision': revision,
  'active_turn': activeTurn,
  'last_outcome': null,
  'data_classes': [provider == 'event_kit' ? 'personal' : 'synthetic'],
  'messages': <Map<String, Object?>>[],
};

void main() {
  test(
    'Calendar session scope is explicit, typed and classification matched',
    () {
      for (final provider in ['fixture', 'event_kit']) {
        final parsed = AgentSession.fromJson(sessionJson(provider: provider));
        expect(parsed.scope!.setupId, setupId);
        expect(parsed.scope!.provider, provider);
        expect(parsed.scope!.dataClass, parsed.dataClasses.single);
      }
      for (final scope in [
        {'kind': 'calendar', 'setup_id': 'invalid', 'provider': 'fixture'},
        {'kind': 'calendar', 'setup_id': setupId, 'provider': 'remote'},
        {
          'kind': 'calendar',
          'setup_id': setupId,
          'provider': 'fixture',
          'calendar_ids': ['extra'],
        },
      ]) {
        expect(
          () => AgentSession.fromJson(sessionJson()..['scope'] = scope),
          throwsFormatException,
        );
      }
      expect(
        () => AgentSession.fromJson(
          sessionJson()..['data_classes'] = ['synthetic'],
        ),
        throwsFormatException,
      );
      expect(
        () => AgentSession.fromJson(
          sessionJson()..['data_classes'] = ['personal', 'synthetic'],
        ),
        throwsFormatException,
      );
      expect(
        AgentSession.fromJson(sessionJson()..remove('scope')).scope,
        isNull,
      );
    },
  );

  test('native Calendar session jobs carry references only and verify resumed identity', () async {
    var saved = sessionJson();
    final submitted = <Map<String, Object?>>[];
    final gateway = NativeAgentVaultGateway((request) async {
      final operation = request['operation'] as Map<String, Object?>;
      if (operation['kind'] == 'submit') {
        submitted.add(operation['action'] as Map<String, Object?>);
      }
      return {
        'request_id': request['request_id'],
        'done': true,
        'events': [],
        'next_sequence': 0,
        'state': 'ready',
        'failure': null,
        'session': saved,
      };
    });
    expect(
      (await gateway.startCalendarSession(
        proposalPerson,
        setupId,
      )).scope!.setupId,
      setupId,
    );
    expect(
      (await gateway.resumeCalendarSession(proposalPerson, setupId)).id,
      proposalSession,
    );
    expect(
      (await gateway.loadCalendarSession(proposalPerson, proposalSession)).id,
      proposalSession,
    );
    expect(submitted, [
      {
        'kind': 'calendar_session',
        'operation': {'kind': 'start', 'setup_id': setupId},
      },
      {
        'kind': 'calendar_session',
        'operation': {'kind': 'resume', 'setup_id': setupId},
      },
      {
        'kind': 'calendar_session',
        'operation': {'kind': 'get', 'session_id': proposalSession},
      },
    ]);
    for (final change in [
      {'person_id': proposalCall},
      {'scope': null},
      {
        'scope': {
          'kind': 'calendar',
          'setup_id': proposalCall,
          'provider': 'event_kit',
        },
      },
    ]) {
      saved = sessionJson()..addAll(change);
      await expectLater(
        gateway.resumeCalendarSession(proposalPerson, setupId),
        throwsFormatException,
      );
    }
    saved = sessionJson()..['id'] = proposalCall;
    await expectLater(
      gateway.loadCalendarSession(proposalPerson, proposalSession),
      throwsFormatException,
    );
  });

  test('Calendar recovery validates revision, settled state and immutable provider', () async {
    var saved = sessionJson(revision: 2);
    final gateway = NativeAgentVaultGateway(
      (request) async => {
        'request_id': request['request_id'],
        'done': true,
        'events': [],
        'next_sequence': 0,
        'state': 'ready',
        'failure': null,
        'session': saved,
      },
    );
    final original = AgentSession.fromJson(
      sessionJson(revision: 1, activeTurn: 'interrupted'),
    );
    expect((await gateway.recoverCalendarSession(original)).revision, 2);
    for (final invalid in [
      sessionJson(revision: 1),
      sessionJson(revision: 2, activeTurn: 'still-running'),
      sessionJson(revision: 2, provider: 'fixture'),
    ]) {
      saved = invalid;
      await expectLater(
        gateway.recoverCalendarSession(original),
        throwsFormatException,
      );
    }
  });

  test('sample controller rejects Calendar sessions instead of silently switching context', () async {
    final gateway = TestProposalGateway()
      ..saved = sessionJson(provider: 'fixture');
    final controller = AgentController(
      gateway: gateway,
      personId: proposalPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    expect(controller.session, isNull);
    expect(controller.messages, isEmpty);
    expect(controller.needsReload, true);
    expect(gateway.begins, 0);
  });
}
