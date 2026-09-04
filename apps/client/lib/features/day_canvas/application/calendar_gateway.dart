import 'package:flutter/services.dart';

import '../domain/day_models.dart';

abstract interface class CalendarGateway {
  Future<List<CalendarChoice>> calendars();
  Future<DaySnapshot> selectCalendar(CalendarChoice calendar, DayQuery query);
  Future<DaySnapshot> syncCalendar(DayQuery query);
  Future<void> openCalendarSettings();
}

final class CalendarChoice {
  const CalendarChoice(this.id, this.name, {this.provider = 'event_kit'});
  final String id;
  final String name;
  final String provider;
}

abstract interface class CalendarAdapter {
  Future<List<CalendarChoice>> calendars();
  Future<List<Map<String, dynamic>>> read(String calendarId, DayQuery query);
  Future<void> openSettings();
}

final class EventKitCalendarAdapter implements CalendarAdapter {
  const EventKitCalendarAdapter();
  static const _channel = MethodChannel('floe/calendar');

  @override
  Future<List<CalendarChoice>> calendars() async {
    final values = await _channel.invokeListMethod<dynamic>('calendars');
    return values!
        .map(
          (value) =>
              CalendarChoice(value['id'] as String, value['name'] as String),
        )
        .toList();
  }

  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    final start = DateTime.utc(
      query.date.year,
      query.date.month,
      query.date.day,
    ).subtract(Duration(seconds: query.timezoneOffsetSeconds));
    final values = await _channel.invokeListMethod<dynamic>('read', {
      'calendar_id': calendarId,
      'starts_at': start.toIso8601String(),
      'ends_at': start.add(const Duration(days: 1)).toIso8601String(),
    });
    return values!
        .map(
          (value) => Map<String, dynamic>.from(
            value as Map,
          )..['schedule'] = Map<String, dynamic>.from(value['schedule'] as Map),
        )
        .toList();
  }

  @override
  Future<void> openSettings() => _channel.invokeMethod<void>('settings');
}
