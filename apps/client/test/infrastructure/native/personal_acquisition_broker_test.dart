import 'package:flutter_test/flutter_test.dart';

import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/infrastructure/native/personal_acquisition_broker.dart';

void main() {
  test('Wellbeing acquisition accepts no selected handles', () async {
    final transport = _FakeTransport();
    transport.request['domain'] = 'wellbeing';
    transport.request['selected_handles'] = <String>[];
    final broker = PersonalAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    expect(
      await broker.pollAndComplete(
        (request) async => {
          'request_id': request['request_id'],
          'host_epoch': request['host_epoch'],
          'person_id': request['person_id'],
          'device_id': request['device_id'],
          'domain': request['domain'],
          'native_subject_fingerprint_before': 'a' * 64,
          'native_subject_fingerprint_after': 'a' * 64,
          'permission_class': 'authorized',
          'provider': 'apple_health',
          'view': {'view_id': 'wellbeing.derived'},
        },
      ),
      isTrue,
    );
    expect(transport.completed, isNotNull);
    await broker.dispose();
  });

  test(
    'invalid selection or unknown request shape never reaches the reader',
    () async {
      for (final change in [
        {'domain': 'people', 'selected_handles': <String>[]},
        {'domain': 'wellbeing'},
        {'domain': 'unknown'},
        {'unexpected_field': null},
      ]) {
        final transport = _FakeTransport();
        transport.request.addAll(change);
        final broker = PersonalAcquisitionBroker(
          transport: transport,
          personId: 'person',
          hostEpoch: 'host-a',
        );
        var reads = 0;
        await expectLater(
          broker.pollAndComplete((request) async {
            reads += 1;
            return {};
          }),
          throwsFormatException,
        );
        expect(reads, 0);
        expect(transport.completed, isNull);
        await broker.dispose();
      }
    },
  );

  test('completes exact selected People acquisition', () async {
    final transport = _FakeTransport();
    final broker = PersonalAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    expect(
      await broker.pollAndComplete(
        (request) async => {
          'request_id': request['request_id'],
          'host_epoch': request['host_epoch'],
          'person_id': request['person_id'],
          'device_id': request['device_id'],
          'domain': request['domain'],
          'native_subject_fingerprint_before': 'a' * 64,
          'native_subject_fingerprint_after': 'a' * 64,
          'permission_class': 'authorized',
          'provider': 'contacts',
          'view': {
            'view_id': 'people.identity',
            'schema_version': 1,
            'source_handle': 'people:opaque',
            'observed_at_unix_ms': 100,
            'expires_at_unix_ms': 200,
            'coverage_complete': true,
            'identities': [],
          },
        },
      ),
      isTrue,
    );
    expect(transport.completed, isNotNull);
    await broker.dispose();
  });

  test('rejects a result whose view domain changes', () async {
    final transport = _FakeTransport();
    final broker = PersonalAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );
    await expectLater(
      broker.pollAndComplete(
        (request) async => {
          'request_id': request['request_id'],
          'host_epoch': request['host_epoch'],
          'person_id': request['person_id'],
          'device_id': request['device_id'],
          'domain': request['domain'],
          'native_subject_fingerprint_before': 'a' * 64,
          'native_subject_fingerprint_after': 'a' * 64,
          'permission_class': 'authorized',
          'provider': 'contacts',
          'view': {'view_id': 'wellbeing.derived'},
        },
      ),
      throwsFormatException,
    );
    expect(transport.completed, isNull);
    await broker.dispose();
  });
}

final class _FakeTransport implements LocalContextTransport {
  final request = <String, dynamic>{
    'request_id': 'request-a',
    'host_epoch': 'host-a',
    'person_id': 'person',
    'device_id': 'device-a',
    'domain': 'people',
    'selected_handles': ['person.identity:a'],
    'deadline_unix_ms': 9999999999999,
  };
  Map<String, dynamic>? completed;
  bool registered = false;

  @override
  Future<void> registerPersonalHost({
    required String personId,
    required String hostEpoch,
  }) async {
    registered = true;
  }

  @override
  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async => registered && completed == null ? [request] : const [];

  @override
  Future<void> completePersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    completed = result;
  }

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
  Future<void> registerAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {}
  @override
  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async => const [];
  @override
  Future<void> completeAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {}
  @override
  Future<void> failAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {}
  @override
  Future<void> disposeAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {}
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
