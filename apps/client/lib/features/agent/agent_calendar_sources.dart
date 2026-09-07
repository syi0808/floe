import 'dart:convert';

import '../day_canvas/domain/day_models.dart';

final class AgentCalendarSources {
  AgentCalendarSources({
    required this.personId,
    required CalendarConnection connection,
  }) : provider = connection.provider,
       revision = connection.revision,
       calendars = List.unmodifiable(
         connection.selectedCalendarIds.map((identifier) {
           final calendar = connection.calendars
               .where((entry) => entry.id == identifier)
               .singleOrNull;
           return AgentCalendarSource(
             identifier,
             calendar?.name ??
                 (identifier == connection.id ? connection.name : identifier),
             calendar == null ? connection.error : calendar.error,
           );
         }),
       );

  final String personId;
  final String provider;
  final int revision;
  final List<AgentCalendarSource> calendars;

  bool get usable =>
      (provider == 'event_kit' || provider == 'fixture') &&
      calendars.isNotEmpty &&
      calendars.length <= 128 &&
      calendars.map((entry) => entry.id).toSet().length == calendars.length &&
      calendars.every(
        (entry) =>
            entry.id.trim().isNotEmpty && utf8.encode(entry.id).length <= 512,
      );

  String get fingerprint => jsonEncode([
    personId,
    provider,
    revision,
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
