import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';

final class CalendarObservationPublisher {
  factory CalendarObservationPublisher({
    required LocalContextTransport transport,
    required String deviceId,
  }) => CalendarObservationPublisher._(transport, deviceId);

  const CalendarObservationPublisher._(this._transport, this._deviceId);

  static const freshness = Duration(minutes: 4);
  static const maxCalendarCount = 128;
  static const _supportedProviders = {'event_kit', 'android'};

  final LocalContextTransport _transport;
  final String _deviceId;

  bool supports(String provider) => _supportedProviders.contains(provider);

  Future<void> publish({
    required String personId,
    required CalendarSourceConnection source,
    required DateTime observedAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  }) async {
    if (!supports(source.provider)) return;
    if (source.revision <= 0) {
      throw StateError('Calendar connection revision must be positive.');
    }
    if (source.selectedCalendarIds.length > maxCalendarCount) {
      await revoke(personId: personId);
      return;
    }
    await _transport.publishCalendarObservation(
      personId: personId,
      deviceId: _deviceId,
      connectionId: source.connectionId,
      connectionRevision: source.revision,
      provider: source.provider,
      calendarIds: source.selectedCalendarIds,
      observedAt: observedAt,
      expiresAt: observedAt.add(freshness),
      rangeStart: rangeStart,
      rangeEnd: rangeEnd,
      batches: batches,
    );
  }

  Future<void> revoke({required String personId}) =>
      _transport.revokeLocalContext(
        personId: personId,
        deviceId: _deviceId,
        viewId: 'calendar.timeline',
      );
}
