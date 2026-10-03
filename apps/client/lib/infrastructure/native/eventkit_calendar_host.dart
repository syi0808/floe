import 'package:flutter/services.dart';

import 'package:floe_client/features/day/domain/day_models.dart';

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

/// Receives only a host-issued acquisition from the registered native pump.
final class EventKitCalendarHost {
  const EventKitCalendarHost({required String deviceId}) : _deviceId = deviceId;
  static const _channel = MethodChannel('floe/calendar');
  final String _deviceId;
  Future<Map<String, dynamic>> readAcquisition(
    Map<String, dynamic> request,
  ) async {
    if (request['device_id'] != _deviceId)
      throw const FormatException('Foreign Calendar acquisition.');
    final response = await _channel.invokeMapMethod<Object?, Object?>(
      'readAcquisition',
      request,
    );
    if (response == null)
      throw const FormatException('Missing EventKit acquisition result.');
    return Map<String, dynamic>.from(response);
  }
}
