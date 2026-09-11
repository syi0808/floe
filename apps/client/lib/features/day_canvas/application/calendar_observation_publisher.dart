import '../../../infrastructure/native/native_transport.dart';
import '../domain/day_models.dart';

final class CalendarObservationPublisher {
  factory CalendarObservationPublisher({
    required LocalContextTransport transport,
    required String deviceId,
  }) => CalendarObservationPublisher._(transport, deviceId);

  const CalendarObservationPublisher._(this._transport, this._deviceId);

  static const freshness = Duration(minutes: 4);
  static const _supportedProviders = {'event_kit', 'android'};

  final LocalContextTransport _transport;
  final String _deviceId;

  bool supports(String provider) => _supportedProviders.contains(provider);

  Future<void> publish({
    required String personId,
    required CalendarConnection connection,
    required DateTime observedAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  }) async {
    if (!supports(connection.provider)) return;
    if (connection.revision <= 0) {
      throw StateError('Calendar connection revision must be positive.');
    }
    await _transport.publishCalendarObservation(
      personId: personId,
      deviceId: _deviceId,
      connectionRevision: connection.revision,
      provider: connection.provider,
      calendarIds: connection.selectedCalendarIds,
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
