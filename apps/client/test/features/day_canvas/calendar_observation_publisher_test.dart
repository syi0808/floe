import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day_canvas/application/calendar_observation_publisher.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/infrastructure/native/native_transport.dart';

void main() {
  test('publishes connection and device bound calendar batches', () async {
    final transport = _RecordingTransport();
    final publisher = CalendarObservationPublisher(
      transport: transport,
      deviceId: 'device-1',
    );
    final observedAt = DateTime.utc(2026, 9, 11, 1);
    final rangeStart = DateTime.utc(2026, 9, 10, 15);
    final rangeEnd = DateTime.utc(2026, 9, 11, 15);
    final batches = <Map<String, dynamic>>[
      {'calendar_id': 'home', 'records': <Object>[], 'failure': null},
    ];

    await publisher.publish(
      personId: 'person-1',
      connection: const CalendarConnection(
        connectionId: '00000000-0000-4000-8000-000000000010',
        deviceId: 'test-device',
        provider: 'event_kit',
        revision: 8,
        calendars: [ConnectedCalendar(id: 'home', name: 'Home')],
      ),
      observedAt: observedAt,
      rangeStart: rangeStart,
      rangeEnd: rangeEnd,
      batches: batches,
    );

    expect(transport.calendarPublications, hasLength(1));
    final publication = transport.calendarPublications.single;
    expect(publication.personId, 'person-1');
    expect(publication.deviceId, 'device-1');
    expect(publication.connectionRevision, 8);
    expect(publication.provider, 'event_kit');
    expect(publication.calendarIds, ['home']);
    expect(publication.observedAt, observedAt);
    expect(
      publication.expiresAt,
      observedAt.add(CalendarObservationPublisher.freshness),
    );
    expect(publication.rangeStart, rangeStart);
    expect(publication.rangeEnd, rangeEnd);
    expect(publication.batches, same(batches));
  });

  test('preserves partial failure and empty batch records', () async {
    final transport = _RecordingTransport();
    final publisher = CalendarObservationPublisher(
      transport: transport,
      deviceId: 'android-device',
    );
    final batches = <Map<String, dynamic>>[
      {
        'calendar_id': 'work',
        'records': <Object>[],
        'failure': 'permission_denied',
      },
      {'calendar_id': 'home', 'records': <Object>[], 'failure': null},
    ];

    await publisher.publish(
      personId: 'person-1',
      connection: const CalendarConnection(
        connectionId: '00000000-0000-4000-8000-000000000011',
        deviceId: 'android-device',
        provider: 'android',
        revision: 3,
        calendars: [
          ConnectedCalendar(id: 'work', name: 'Work'),
          ConnectedCalendar(id: 'home', name: 'Home'),
        ],
      ),
      observedAt: DateTime.utc(2026, 9, 11),
      rangeStart: DateTime.utc(2026, 9, 10),
      rangeEnd: DateTime.utc(2026, 9, 12),
      batches: batches,
    );

    expect(transport.calendarPublications.single.batches, batches);
  });

  test('ignores server provider and revokes device observation', () async {
    final transport = _RecordingTransport();
    final publisher = CalendarObservationPublisher(
      transport: transport,
      deviceId: 'device-1',
    );

    await publisher.publish(
      personId: 'person-1',
      connection: const CalendarConnection(
        connectionId: '00000000-0000-4000-8000-000000000012',
        deviceId: 'server-device',
        provider: 'google_calendar',
        revision: 2,
        calendars: [ConnectedCalendar(id: 'primary', name: 'Primary')],
      ),
      observedAt: DateTime.utc(2026, 9, 11),
      rangeStart: DateTime.utc(2026, 9, 10),
      rangeEnd: DateTime.utc(2026, 9, 12),
      batches: const [],
    );
    await publisher.revoke(personId: 'person-1');

    expect(transport.calendarPublications, isEmpty);
    expect(transport.revocations.single, (
      'person-1',
      'device-1',
      'calendar.timeline',
    ));
  });
}

final class _RecordingTransport implements LocalContextTransport {
  final List<_CalendarPublication> calendarPublications = [];
  final List<(String, String, String?)> revocations = [];

  @override
  Future<void> publishCalendarObservation({
    required String personId,
    required String deviceId,
    required int connectionRevision,
    required String provider,
    required List<String> calendarIds,
    required DateTime observedAt,
    required DateTime expiresAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  }) async {
    calendarPublications.add(
      _CalendarPublication(
        personId,
        deviceId,
        connectionRevision,
        provider,
        calendarIds,
        observedAt,
        expiresAt,
        rangeStart,
        rangeEnd,
        batches,
      ),
    );
  }

  @override
  Future<void> publishLocalContext({
    required String personId,
    required String deviceId,
    required Map<String, dynamic> view,
  }) async {}

  @override
  Future<int> revokeLocalContext({
    required String personId,
    required String deviceId,
    String? viewId,
  }) async {
    revocations.add((personId, deviceId, viewId));
    return 1;
  }
}

final class _CalendarPublication {
  const _CalendarPublication(
    this.personId,
    this.deviceId,
    this.connectionRevision,
    this.provider,
    this.calendarIds,
    this.observedAt,
    this.expiresAt,
    this.rangeStart,
    this.rangeEnd,
    this.batches,
  );

  final String personId;
  final String deviceId;
  final int connectionRevision;
  final String provider;
  final List<String> calendarIds;
  final DateTime observedAt;
  final DateTime expiresAt;
  final DateTime rangeStart;
  final DateTime rangeEnd;
  final List<Map<String, dynamic>> batches;
}
