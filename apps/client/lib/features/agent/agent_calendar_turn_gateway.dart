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

enum AgentCalendarPromptKind { briefing, proposeFocus, freeText }

final class AgentCalendarQueryRange {
  AgentCalendarQueryRange({
    required this.startDate,
    required this.endDateExclusive,
    required this.timezoneOffsetSeconds,
    required this.startsAt,
    required this.endsAt,
    this.endTimezoneOffsetSeconds,
  }) : assert(endDateExclusive.isAfter(startDate)),
       assert(endsAt.isAfter(startsAt));

  factory AgentCalendarQueryRange.day(
    DayQuery day, {
    DateTime? startsAt,
    DateTime? endsAt,
  }) => AgentCalendarQueryRange(
    startDate: day.date,
    endDateExclusive: DateTime.utc(
      day.date.year,
      day.date.month,
      day.date.day + 1,
    ),
    timezoneOffsetSeconds: day.timezoneOffsetSeconds,
    endTimezoneOffsetSeconds: day.endTimezoneOffsetSeconds,
    startsAt: startsAt ?? day.startsAt,
    endsAt: endsAt ?? day.endsAt,
  );

  final DateTime startDate;
  final DateTime endDateExclusive;
  final int timezoneOffsetSeconds;
  final int? endTimezoneOffsetSeconds;
  final DateTime startsAt;
  final DateTime endsAt;

  Map<String, Object?> toJson() => {
    'day': {
      'start_date': _date(startDate),
      'end_date_exclusive': _date(endDateExclusive),
      'timezone_offset_seconds': timezoneOffsetSeconds,
      'end_timezone_offset_seconds': endTimezoneOffsetSeconds,
    },
    'starts_at': startsAt.toUtc().toIso8601String(),
    'ends_at': endsAt.toUtc().toIso8601String(),
  };
}

enum AgentCalendarInferenceRoute {
  deterministicFixture('deterministic_fixture'),
  deviceLocal('device_local'),
  remote('remote');

  const AgentCalendarInferenceRoute(this.wireName);
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
    this.text,
    this.destination,
    this.queryRange,
    this.continuation = false,
  }) : assert(
         prompt != AgentCalendarPromptKind.freeText ||
             text != null && text.length > 0 && text.length <= 8192,
       );

  final AgentSession session;
  final DayQuery day;
  final DateTime startsAt;
  final DateTime endsAt;
  final AgentCalendarPromptKind prompt;
  final int focusMinutes;
  final String? text;
  final AgentCalendarDestination? destination;
  final AgentCalendarQueryRange? queryRange;
  final bool continuation;

  Map<String, Object?> toJson() {
    final range =
        queryRange ??
        AgentCalendarQueryRange.day(day, startsAt: startsAt, endsAt: endsAt);
    return {
      'session_id': session.id,
      'expected_revision': session.revision,
      'prompt': {
        'kind': switch (prompt) {
          AgentCalendarPromptKind.briefing => 'briefing',
          AgentCalendarPromptKind.proposeFocus => 'propose_focus',
          AgentCalendarPromptKind.freeText => 'free_text',
        },
        if (prompt == AgentCalendarPromptKind.freeText)
          'text': text
        else
          'focus_minutes': focusMinutes,
      },
      ...range.toJson(),
      'destination': destination?.toJson(),
      if (continuation) 'continuation': true,
    };
  }
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
    AgentCalendarInferenceRoute expectedRoute,
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
                result!.inferenceRoute != expectedRoute) ||
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
      inferenceRoute = AgentCalendarInferenceRoute.values.singleWhere(
        (value) => value.wireName == json['inference_route'],
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
  final AgentCalendarInferenceRoute inferenceRoute;
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
