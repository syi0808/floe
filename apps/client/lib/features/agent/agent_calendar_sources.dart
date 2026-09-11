import 'dart:convert';

import '../day_canvas/domain/day_models.dart';

final class AgentCalendarSources {
  AgentCalendarSources({
    required this.personId,
    required CalendarConnection connection,
  }) : provider = connection.provider,
       connectionId = connection.connectionId,
       deviceId = connection.deviceId,
       revision = connection.revision,
       connectionScope = connection.includeAll ? 'all' : 'selected',
       calendars = List.unmodifiable(
         connection.calendars.map((calendar) {
           return AgentCalendarSource(
             calendar.id,
             calendar.name,
             calendar.error,
           );
         }),
       );

  final String personId;
  final String provider;
  final String connectionId;
  final String deviceId;
  final int revision;
  final String connectionScope;
  final List<AgentCalendarSource> calendars;

  bool get usable =>
      const {
        'event_kit',
        'google_calendar',
        'microsoft_calendar',
        'android',
        'fixture',
      }.contains(provider) &&
      calendars.isNotEmpty &&
      calendars.length <= 128 &&
      calendars.map((entry) => entry.id).toSet().length == calendars.length &&
      calendars.every(
        (entry) =>
            entry.id.trim().isNotEmpty && utf8.encode(entry.id).length <= 512,
      );

  String get fingerprint => jsonEncode([
    personId,
    connectionId,
    deviceId,
    provider,
    revision,
    connectionScope,
    for (final calendar in calendars)
      [calendar.id, calendar.name, calendar.error],
  ]);

  bool containsScope(String source, Iterable<String> identifiers) =>
      usable &&
      provider == source &&
      identifiers.every(
        (identifier) => calendars.any((entry) => entry.id == identifier),
      );
}

final class AgentCalendarSource {
  const AgentCalendarSource(this.id, this.name, this.error);
  final String id;
  final String name;
  final String? error;
}
