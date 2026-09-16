import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:floe_client/infrastructure/native/attention_acquisition_broker.dart';
import 'package:floe_client/app/runtime/native_transport.dart';

void main() {
  test('completes a trusted inspect result once', () async {
    final transport = _FakeTransport();
    final broker = AttentionAcquisitionBroker(
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
          'mode': request['mode'],
          'native_subject_fingerprint_before': 'a' * 64,
          'native_subject_fingerprint_after': 'a' * 64,
          'permission_class': 'session_observation',
          'view': null,
        },
      ),
      isTrue,
    );
    expect(transport.completed, isNotNull);
    expect(transport.failed, isNull);
    await broker.dispose();
  });

  test(
    'provider failure completes the pending request as a typed failure',
    () async {
      final transport = _FakeTransport();
      final broker = AttentionAcquisitionBroker(
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
      expect(transport.completed, isNull);
      expect(transport.failed, 'permission_denied');
      await broker.dispose();
    },
  );

  test('rejects a queued request from a different host epoch', () async {
    final transport = _FakeTransport()..request['host_epoch'] = 'stale-host';
    final broker = AttentionAcquisitionBroker(
      transport: transport,
      personId: 'person',
      hostEpoch: 'host-a',
    );

    await expectLater(
      broker.pollAndComplete((_) async => <String, dynamic>{}),
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
    'mode': 'inspect_subject',
    'deadline_unix_ms': 9999999999999,
  };
  Map<String, dynamic>? completed;
  String? failed;
  bool registered = false;

  @override
  Future<void> registerAttentionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    registered = true;
  }

  @override
  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async =>
      registered && completed == null && failed == null ? [request] : const [];

  @override
  Future<void> completeAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    completed = result;
  }

  @override
  Future<void> failAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {
    failed = failure;
  }

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
