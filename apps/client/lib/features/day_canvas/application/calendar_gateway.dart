import 'package:flutter/services.dart';

import '../domain/day_models.dart';

abstract interface class CalendarGateway {
  Future<List<CalendarChoice>> calendars();
  Future<DaySnapshot> selectCalendar(CalendarChoice calendar, DayQuery query);
  Future<DaySnapshot> selectCalendars(
    List<CalendarChoice> calendars,
    DayQuery query, {
    bool includeAll = false,
  });
  Future<DaySnapshot> syncCalendar(DayQuery query);
  Future<DaySnapshot> disconnectCalendar(DayQuery query);
  Future<void> openCalendarSettings();
}

final class CalendarChoice {
  const CalendarChoice(this.id, this.name, {this.provider = 'event_kit'});
  final String id;
  final String name;
  final String provider;
}

abstract interface class CalendarAdapter {
  Future<List<CalendarChoice>> calendars({bool requestAccess = true});
  Future<List<Map<String, dynamic>>> read(String calendarId, DayQuery query);
  Future<void> openSettings();
}

final class EventKitCalendarAdapter implements CalendarAdapter {
  const EventKitCalendarAdapter({String? deviceId}) : _deviceId = deviceId;
  static const _channel = MethodChannel('floe/calendar');
  final String? _deviceId;

  Map<String, Object> _arguments([Map<String, Object>? values]) => {
    if (_deviceId != null) 'device_id': _deviceId,
    ...?values,
  };

  @override
  Future<List<CalendarChoice>> calendars({bool requestAccess = true}) async {
    final values = await _channel.invokeListMethod<dynamic>(
      'calendars',
      _arguments({'request_access': requestAccess}),
    );
    return values!
        .map(
          (value) => CalendarChoice(
            value['id'] as String,
            value['name'] as String,
            provider: value['provider'] as String? ?? 'event_kit',
          ),
        )
        .toList();
  }

  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    final values = await _channel.invokeListMethod<dynamic>(
      'read',
      _arguments({
        'calendar_id': calendarId,
        'starts_at': query.startsAt.toIso8601String(),
        'ends_at': query.endsAt.toIso8601String(),
      }),
    );
    return values!
        .map(
          (value) => Map<String, dynamic>.from(
            value as Map,
          )..['schedule'] = Map<String, dynamic>.from(value['schedule'] as Map),
        )
        .toList();
  }

  @override
  Future<void> openSettings() =>
      _channel.invokeMethod<void>('settings', _arguments());
}
