import 'package:floe_client/infrastructure/native/calendar_acquisition_broker.dart';
import 'package:floe_client/infrastructure/native/native_transport.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('polls one bounded request and completes exact identity', () async {
    final transport = _FakeTransport();
    final broker = CalendarAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    final completed = await broker.pollAndComplete((request) async {
      final response = Map<String, dynamic>.from(request)
        ..remove('deadline_unix_ms')
        ..remove('expected_native_subject_fingerprint');
      return {
        ...response,
        'native_subject_fingerprint_before': 'a' * 64,
        'native_subject_fingerprint_after': 'a' * 64,
        'available_calendar_ids': ['calendar-a'],
        'permission_class': 'authorized',
        'batches': [
          {'calendar_id': 'calendar-a', 'records': [], 'failure': null},
        ],
      };
    });
    expect(completed, isTrue);
    expect(transport.completed, isNotNull);
  });

  test('disposed broker rejects late polling and completion', () async {
    final transport = _FakeTransport();
    final broker = CalendarAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    await broker.start();
    await broker.dispose();
    expect(
      () => broker.pollAndComplete((_) async => throw StateError('late')),
      throwsStateError,
    );
    expect(transport.disposed, isTrue);
  });

  test('inspects an iOS subject without accepting event batches', () async {
    final transport = _FakeTransport(
      provider: 'event_kit',
      mode: 'inspect_subject',
      includeExpectedFingerprint: false,
    );
    final broker = CalendarAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    await broker.pollAndComplete((request) async {
      expect(request['mode'], 'inspect_subject');
      expect(request['expected_native_subject_fingerprint'], isNull);
      final response = Map<String, dynamic>.from(request)
        ..remove('deadline_unix_ms');
      return {
        ...response,
        'native_subject_fingerprint_before': 'b' * 64,
        'native_subject_fingerprint_after': 'b' * 64,
        'available_calendar_ids': ['calendar-a'],
        'permission_class': 'full_access',
        'batches': const [],
      };
    });
    expect(transport.completed, isNotNull);
  });

  test(
    'rejects a provider subject fingerprint that differs from review',
    () async {
      final transport = _FakeTransport();
      final broker = CalendarAcquisitionBroker(
        transport: transport,
        personId: 'person',
        hostEpoch: 'host-a',
      );
      await expectLater(
        broker.pollAndComplete((request) async {
          final response = Map<String, dynamic>.from(request)
            ..remove('deadline_unix_ms')
            ..remove('expected_native_subject_fingerprint');
          return {
            ...response,
            'native_subject_fingerprint_before': 'b' * 64,
            'native_subject_fingerprint_after': 'b' * 64,
            'available_calendar_ids': ['calendar-a'],
            'permission_class': 'authorized',
            'batches': [
              {'calendar_id': 'calendar-a', 'records': [], 'failure': null},
            ],
          };
        }),
        throwsFormatException,
      );
      expect(transport.completed, isNull);
      expect(transport.failed, 'provider_unavailable');
    },
  );

  test(
    'completes provider denial without holding the queued request',
    () async {
      final transport = _FakeTransport();
      final broker = CalendarAcquisitionBroker(
        transport: transport,
        personId: 'person',
        hostEpoch: 'host-a',
      );
      expect(
        await broker.pollAndComplete((_) async {
          throw PlatformException(code: 'permission_denied');
        }),
        isTrue,
      );
      expect(transport.failed, 'permission_denied');
    },
  );
}

final class _FakeTransport implements LocalContextTransport {
  _FakeTransport({
    String provider = 'android',
    String mode = 'read_events',
    bool includeExpectedFingerprint = true,
  }) : request = {
         'request_id': 'request-a',
         'host_epoch': 'host-a',
         'person_id': 'person',
         'device_id': 'device-a',
         'connection_id': 'connection-a',
         'connection_revision': 2,
         'provider': provider,
         'mode': mode,
         'calendar_ids': ['calendar-a'],
         'range_start_unix_ms': 1000,
         'range_end_unix_ms': 2000,
         'deadline_unix_ms': 3000,
         if (includeExpectedFingerprint)
           'expected_native_subject_fingerprint': 'a' * 64,
       };

  final Map<String, dynamic> request;
  Map<String, dynamic>? completed;
  String? failed;
  bool disposed = false;
  bool registered = false;

  @override
  Future<void> registerAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    registered = true;
  }

  @override
  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async => registered && !disposed ? [request] : const [];

  @override
  Future<void> completeAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    completed = result;
  }

  @override
  Future<void> failAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {
    failed = failure;
  }

  @override
  Future<void> disposeAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    disposed = true;
  }

  @override
  Future<void> registerAttentionHost({
    required String personId,
    required String hostEpoch,
  }) async {}

  @override
  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async => const [];

  @override
  Future<void> completeAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {}

  @override
  Future<void> failAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {}

  @override
  Future<void> disposeAttentionHost({
    required String personId,
    required String hostEpoch,
  }) async {}

  @override
  Future<void> registerPersonalHost({
    required String personId,
    required String hostEpoch,
  }) async {}

  @override
  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async => const [];

  @override
  Future<void> completePersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {}

  @override
  Future<void> failPersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {}

  @override
  Future<void> disposePersonalHost({
    required String personId,
    required String hostEpoch,
  }) async {}

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
  }) async => 0;

  @override
  Future<void> publishCalendarObservation({
    required String personId,
    required String deviceId,
    required String connectionId,
    required int connectionRevision,
    required String provider,
    required List<String> calendarIds,
    required DateTime observedAt,
    required DateTime expiresAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  }) async {}
}
