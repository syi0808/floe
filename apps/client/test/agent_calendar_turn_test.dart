import 'package:floe_client/features/agent/agent_calendar_turn_gateway.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

const personId = '00000000-0000-4000-8000-000000000001';
const sessionId = '00000000-0000-4000-8000-000000000002';
const setupId = '00000000-0000-4000-8000-000000000003';
const invocationId = '00000000-0000-4000-8000-000000000004';

Map<String, Object?> sessionJson({int revision = 4}) => {
  'schema_version': 1,
  'id': sessionId,
  'person_id': personId,
  'scope': {'kind': 'calendar', 'setup_id': setupId, 'provider': 'fixture'},
  'revision': revision,
  'active_turn': null,
  'last_outcome': revision == 4 ? null : {'status': 'completed'},
  'data_classes': ['synthetic'],
  'messages': <Object?>[],
};

AgentCalendarTurnRequest turnRequest() {
  final date = DateTime(2026, 9, 8);
  return AgentCalendarTurnRequest(
    session: AgentSession.fromJson(sessionJson()),
    day: DayQuery(
      personId: personId,
      date: date,
      now: DateTime.utc(2026, 9, 8),
      timezoneOffsetSeconds: 32400,
      endTimezoneOffsetSeconds: 32400,
    ),
    startsAt: DateTime.utc(2026, 9, 8),
    endsAt: DateTime.utc(2026, 9, 8, 12),
    prompt: AgentCalendarPromptKind.proposeFocus,
    focusMinutes: 60,
    model: AgentCalendarModel.deterministicFixture,
    destination: const AgentCalendarDestination(
      provider: 'fixture',
      calendarId: 'calendar-a',
      connectionRevision: 9,
      timezone: 'Asia/Seoul',
    ),
  );
}

void main() {
  test(
    'Calendar turn streams with explicit scope and validates completion',
    () async {
      final submitted = <Map<String, Object?>>[];
      var complete = false;
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = request['operation']! as Map;
        if (operation['kind'] == 'submit') {
          submitted.add(Map<String, Object?>.from(operation['action']! as Map));
        } else if (operation['kind'] == 'poll') {
          complete = true;
        }
        return response(request['request_id']! as String, complete: complete);
      });
      final turn = turnRequest();
      final started = await gateway.beginCalendarTurn(turn);
      expect(started.run.done, false);
      expect(started.result, isNull);
      final request = submitted.single['request']! as Map;
      expect(submitted.single['kind'], 'calendar_turn');
      expect(request['session_id'], sessionId);
      expect(request['expected_revision'], 4);
      expect(request['model'], 'deterministic_fixture');
      expect(request['prompt'], {'kind': 'propose_focus', 'focus_minutes': 60});
      expect(request['day'], {
        'start_date': '2026-09-08',
        'end_date_exclusive': '2026-09-09',
        'timezone_offset_seconds': 32400,
        'end_timezone_offset_seconds': 32400,
      });
      final finished = await gateway.pollCalendarTurn(turn, 0);
      expect(finished.run.done, true);
      expect(finished.run.session!.revision, 5);
      expect(finished.result!.setupId, setupId);
      expect(finished.result!.model, AgentCalendarModel.deterministicFixture);
      expect(finished.result!.proposals.single.invocationId, invocationId);
      expect(finished.result!.proposals.single.action!.status.name, 'pending');
      expect((await gateway.releaseCalendarTurn(turn)).run.done, true);
    },
  );

  test(
    'Calendar turn rejects changed retries and mismatched native results',
    () async {
      var complete = false;
      var wrongSetup = false;
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = request['operation']! as Map;
        if (operation['kind'] == 'poll') complete = true;
        final result = response(
          request['request_id']! as String,
          complete: complete,
        );
        if (wrongSetup) {
          (result['calendar_turn']! as Map)['setup_id'] = invocationId;
        }
        return result;
      });
      final turn = turnRequest();
      await gateway.beginCalendarTurn(turn);
      final changed = AgentCalendarTurnRequest(
        session: turn.session,
        day: turn.day,
        startsAt: turn.startsAt,
        endsAt: turn.endsAt,
        prompt: turn.prompt,
        focusMinutes: 30,
        model: turn.model,
        destination: turn.destination,
      );
      await expectLater(
        gateway.pollCalendarTurn(changed, 0),
        throwsA(isA<AgentVaultException>()),
      );
      wrongSetup = true;
      await expectLater(
        gateway.pollCalendarTurn(turn, 0),
        throwsFormatException,
      );
    },
  );
}

Map<String, dynamic> response(String requestId, {required bool complete}) => {
  'request_id': requestId,
  'events': <Object?>[],
  'next_sequence': 0,
  'done': complete,
  'state': complete ? 'ready' : null,
  'session': complete ? sessionJson(revision: 5) : null,
  'failure': null,
  'calendar_turn': complete
      ? {
          'schema_version': 1,
          'person_id': personId,
          'session_id': sessionId,
          'setup_id': setupId,
          'model': 'deterministic_fixture',
          'proposals': [
            {
              'invocation_id': invocationId,
              'action': {
                'action_id': invocationId,
                'execution_id': '00000000-0000-4000-8000-000000000005',
                'status': 'pending',
                'expires_at': '2026-09-08T01:00:00Z',
              },
              'failure': null,
            },
          ],
        }
      : null,
};
