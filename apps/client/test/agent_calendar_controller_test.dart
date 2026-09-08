import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_calendar_session_gateway.dart';
import 'package:floe_client/features/agent/agent_calendar_turn_gateway.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_panel.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_calendar_experts.dart';
import 'support/agent_registry.dart';

const _sessionId = '00000000-0000-4000-8000-000000000009';

void main() {
  test(
    'controller resumes scoped Calendar chat and dispatches the current day',
    () async {
      final gateway = _ConnectedCalendarGateway();
      final context = _context();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
        calendarContext: () => context,
      );
      addTearDown(controller.dispose);

      await controller.load();
      expect(controller.isCalendarConversation, isTrue);
      expect(controller.isPersonalConversation, isTrue);
      expect(controller.session!.scope!.setupId, calendarSetupId);

      await controller.sendCalendar(AgentCalendarPromptKind.briefing);
      expect(gateway.turns, hasLength(1));
      expect(gateway.turns.single.day, same(context.day));
      expect(gateway.turns.single.startsAt, context.day.startsAt);
      expect(gateway.turns.single.endsAt, context.day.endsAt);
      expect(gateway.turns.single.model, AgentCalendarModel.foundationModels);
      expect(gateway.turns.single.destination, isNull);
      expect(controller.messages.last, isA<AgentTextMessage>());
      expect(
        (controller.messages.last as AgentTextMessage).text,
        'Your Calendar is clear after 11:00.',
      );
      expect(gateway.releases, 1);
    },
  );

  testWidgets('panel presents connected Calendar requests instead of samples', (
    tester,
  ) async {
    final gateway = _ConnectedCalendarGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
      calendarContext: _context,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: AgentPanel(controller: controller, onClose: () {}),
        ),
      ),
    );

    expect(find.text('Calendar conversation'), findsOneWidget);
    expect(find.text('Message'), findsOneWidget);
    expect(find.text('Send sample'), findsNothing);
    await tester.enterText(
      find.byType(TextFormField),
      'What does the rest of my day look like?',
    );
    await tester.tap(find.text('Ask Floe'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(
      gateway.turns.single.text,
      'What does the rest of my day look like?',
    );
    expect(find.text('Your Calendar is clear after 11:00.'), findsOneWidget);
    expect(find.text('View Calendar source'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  test(
    'focus request uses only the granted destination and current revision',
    () async {
      final gateway = _ConnectedCalendarGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
        calendarContext: _context,
      );
      addTearDown(controller.dispose);
      await controller.load();

      await controller.sendCalendar(AgentCalendarPromptKind.proposeFocus);

      final destination = gateway.turns.single.destination!;
      expect(destination.provider, 'event_kit');
      expect(destination.calendarId, 'home');
      expect(destination.connectionRevision, 7);
      expect(destination.timezone, 'UTC+09:00');
    },
  );

  test('connected stop keeps the encrypted halted session retryable', () async {
    final gateway = _ConnectedCalendarGateway()..hold = true;
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
      calendarContext: _context,
    );
    addTearDown(controller.dispose);
    await controller.load();

    final turn = controller.sendCalendar(AgentCalendarPromptKind.briefing);
    await Future<void>.delayed(Duration.zero);
    expect(controller.running, isTrue);
    await controller.stop();
    await turn;

    expect(gateway.stops, 1);
    expect(gateway.releases, 1);
    expect(controller.session, isNotNull);
    expect(controller.failure, 'cancelled');
    expect(controller.canRetry, isTrue);
  });
}

AgentCalendarConversationContext _context() {
  final date = DateTime(2026, 9, 8);
  return AgentCalendarConversationContext(
    day: DayQuery(
      personId: registryPerson,
      date: date,
      now: DateTime(2026, 9, 8, 9),
      timezoneOffsetSeconds: 32400,
      endTimezoneOffsetSeconds: 32400,
    ),
    connection: const CalendarConnection(
      id: 'work',
      name: 'Work',
      provider: 'event_kit',
      revision: 7,
      calendarIds: ['home', 'work'],
    ),
  );
}

final class _ConnectedCalendarGateway extends TestCalendarExpertGateway
    implements AgentCalendarSessionGateway, AgentCalendarTurnGateway {
  _ConnectedCalendarGateway() {
    final snapshot = calendarExpertsFixture();
    for (final entry
        in (snapshot['registry'] as Map)['installations'] as List) {
      (entry as Map)['enabled'] = true;
    }
    for (final entry in (snapshot['registry'] as Map)['assignments'] as List) {
      (entry as Map)['enabled'] = true;
    }
    ((snapshot['views'] as List).single as Map)['enabled'] = true;
    transport.snapshot = snapshot;
  }

  final turns = <AgentCalendarTurnRequest>[];
  AgentCalendarTurnRequest? _turn;
  bool _done = false;
  bool _halted = false;

  Map<String, Object?> _session({int revision = 0}) => {
    'schema_version': 1,
    'id': _sessionId,
    'person_id': registryPerson,
    'scope': {
      'kind': 'calendar',
      'setup_id': calendarSetupId,
      'provider': 'event_kit',
    },
    'revision': revision,
    'active_turn': null,
    'last_outcome': revision == 0
        ? null
        : _halted
        ? {'status': 'halted', 'reason': 'cancelled'}
        : {'status': 'completed'},
    'data_classes': ['personal'],
    'messages': revision == 0
        ? <Object?>[]
        : [
            {
              'kind': 'user',
              'turn_id': 'turn-1',
              'text':
                  "Brief today's calendar and find a 60-minute focus window.",
            },
            {
              'kind': 'capability',
              'turn_id': 'turn-1',
              'call_id': 'call-1',
              'capability_id': 'calendar.timeline.read',
              'input': '{"kind":"briefing","focus_minutes":60}',
              'result': {'Ok': 'Enabled Calendar: Team sync 10:00–11:00.'},
            },
            {
              'kind': 'assistant',
              'turn_id': 'turn-1',
              'text': 'Your Calendar is clear after 11:00.',
            },
          ],
  };

  @override
  Future<AgentSession> startCalendarSession(String personId, String setupId) =>
      resumeCalendarSession(personId, setupId);

  @override
  Future<AgentSession> resumeCalendarSession(
    String personId,
    String setupId,
  ) async => AgentSession.fromJson(_session());

  @override
  Future<AgentSession> loadCalendarSession(
    String personId,
    String sessionId,
  ) async => AgentSession.fromJson(_session());

  @override
  Future<AgentSession> recoverCalendarSession(AgentSession session) async =>
      AgentSession.fromJson(_session(revision: session.revision + 1));

  @override
  Future<AgentCalendarTurnUpdate> beginCalendarTurn(
    AgentCalendarTurnRequest request,
  ) async {
    turns.add(request);
    _turn = request;
    _done = false;
    _halted = false;
    return _update();
  }

  @override
  Future<AgentCalendarTurnUpdate> pollCalendarTurn(
    AgentCalendarTurnRequest request,
    int afterSequence,
  ) async {
    if (!hold) _done = true;
    return _update();
  }

  @override
  Future<AgentCalendarTurnUpdate> stopCalendarTurn(
    AgentCalendarTurnRequest request,
  ) async {
    stops++;
    _done = true;
    _halted = true;
    return _update();
  }

  @override
  Future<AgentCalendarTurnUpdate> releaseCalendarTurn(
    AgentCalendarTurnRequest request,
  ) async {
    releases++;
    return _update();
  }

  AgentCalendarTurnUpdate _update() => AgentCalendarTurnUpdate.fromJson({
    'session_id': _turn!.session.id,
    'expected_revision': _turn!.session.revision,
    'next_sequence': 0,
    'events': <Object?>[],
    'done': _done,
    'session': _done ? _session(revision: 3) : null,
    'failure': null,
    'calendar_turn': _done
        ? {
            'schema_version': 1,
            'person_id': registryPerson,
            'session_id': _sessionId,
            'setup_id': calendarSetupId,
            'model': _turn!.model.wireName,
            'proposals': <Object?>[],
          }
        : null,
  }, _turn!);
}
