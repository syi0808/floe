import '../day_canvas/domain/day_models.dart';
import 'agent_fixture_gateway.dart';
import 'agent_proposal.dart';

final class AgentCalendarConversationContext {
  const AgentCalendarConversationContext({
    required this.day,
    required this.connection,
  });

  final DayQuery day;
  final CalendarConnection connection;

  bool get usable =>
      day.personId.isNotEmpty &&
      connection.error == null &&
      (connection.provider == 'fixture' ||
          connection.provider == 'event_kit') &&
      connection.selectedCalendarIds.isNotEmpty;
}

enum AgentCalendarPromptKind { briefing, proposeFocus }

enum AgentCalendarModel {
  deterministicFixture('deterministic_fixture'),
  foundationModels('foundation_models');

  const AgentCalendarModel(this.wireName);
  final String wireName;
}

final class AgentCalendarDestination {
  const AgentCalendarDestination({
    required this.provider,
    required this.calendarId,
    required this.connectionRevision,
    required this.timezone,
  });

  final String provider;
  final String calendarId;
  final int connectionRevision;
  final String timezone;

  Map<String, Object?> toJson() => {
    'provider': provider,
    'calendar_id': calendarId,
    'connection_revision': connectionRevision,
    'timezone': timezone,
  };
}

final class AgentCalendarTurnRequest {
  const AgentCalendarTurnRequest({
    required this.session,
    required this.day,
    required this.startsAt,
    required this.endsAt,
    required this.prompt,
    required this.focusMinutes,
    required this.model,
    this.destination,
  });

  final AgentSession session;
  final DayQuery day;
  final DateTime startsAt;
  final DateTime endsAt;
  final AgentCalendarPromptKind prompt;
  final int focusMinutes;
  final AgentCalendarModel model;
  final AgentCalendarDestination? destination;

  Map<String, Object?> toJson() => {
    'session_id': session.id,
    'expected_revision': session.revision,
    'prompt': {
      'kind': switch (prompt) {
        AgentCalendarPromptKind.briefing => 'briefing',
        AgentCalendarPromptKind.proposeFocus => 'propose_focus',
      },
      'focus_minutes': focusMinutes,
    },
    'model': model.wireName,
    'day': {
      'start_date': _date(day.date),
      'end_date_exclusive': _date(
        DateTime.utc(day.date.year, day.date.month, day.date.day + 1),
      ),
      'timezone_offset_seconds': day.timezoneOffsetSeconds,
      'end_timezone_offset_seconds': day.endTimezoneOffsetSeconds,
    },
    'starts_at': startsAt.toUtc().toIso8601String(),
    'ends_at': endsAt.toUtc().toIso8601String(),
    'destination': destination?.toJson(),
  };
}

abstract interface class AgentCalendarTurnGateway {
  Future<AgentCalendarTurnUpdate> beginCalendarTurn(
    AgentCalendarTurnRequest request,
  );
  Future<AgentCalendarTurnUpdate> pollCalendarTurn(
    AgentCalendarTurnRequest request,
    int afterSequence,
  );
  Future<AgentCalendarTurnUpdate> stopCalendarTurn(
    AgentCalendarTurnRequest request,
  );
  Future<AgentCalendarTurnUpdate> releaseCalendarTurn(
    AgentCalendarTurnRequest request,
  );
}

final class AgentCalendarTurnUpdate {
  AgentCalendarTurnUpdate.fromJson(
    Map<String, Object?> json,
    AgentCalendarTurnRequest request,
  ) : run = AgentRunUpdate.fromJson(json),
      result = json['calendar_turn'] == null
          ? null
          : AgentCalendarTurnResult.fromJson(
              Map<String, Object?>.from(json['calendar_turn']! as Map),
            ) {
    final scope = request.session.scope;
    if (scope == null ||
        run.sessionId != request.session.id ||
        run.expectedRevision != request.session.revision ||
        result != null &&
            (result!.personId != request.session.personId ||
                result!.sessionId != request.session.id ||
                result!.setupId != scope.setupId ||
                result!.model != request.model) ||
        run.done && run.failure == null && result == null ||
        !run.done && result != null) {
      throw const FormatException('Calendar turn response mismatch');
    }
  }

  final AgentRunUpdate run;
  final AgentCalendarTurnResult? result;
}

final class AgentCalendarTurnResult {
  AgentCalendarTurnResult.fromJson(Map<String, Object?> json)
    : personId = json['person_id']! as String,
      sessionId = json['session_id']! as String,
      setupId = json['setup_id']! as String,
      model = AgentCalendarModel.values.singleWhere(
        (value) => value.wireName == json['model'],
      ),
      proposals = List.unmodifiable(
        (json['proposals']! as List).map(
          (value) => AgentCalendarProposalOutcome.fromJson(
            Map<String, Object?>.from(value! as Map),
          ),
        ),
      ) {
    if (json['schema_version'] != agentSchemaVersion) {
      throw const FormatException('Unsupported Calendar turn version');
    }
  }

  final String personId;
  final String sessionId;
  final String setupId;
  final AgentCalendarModel model;
  final List<AgentCalendarProposalOutcome> proposals;
}

final class AgentCalendarProposalOutcome {
  AgentCalendarProposalOutcome.fromJson(Map<String, Object?> json)
    : invocationId = json['invocation_id']! as String,
      action = json['action'] == null
          ? null
          : AgentProposalAction.fromJson(
              Map<String, dynamic>.from(json['action']! as Map),
            ),
      failure = json['failure'] as String? {
    if ((action == null) == (failure == null) ||
        action != null && action!.id != invocationId) {
      throw const FormatException('Invalid Calendar proposal outcome');
    }
  }

  final String invocationId;
  final AgentProposalAction? action;
  final String? failure;
}

String _date(DateTime value) =>
    '${value.year.toString().padLeft(4, '0')}-'
    '${value.month.toString().padLeft(2, '0')}-'
    '${value.day.toString().padLeft(2, '0')}';
