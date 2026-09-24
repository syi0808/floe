import 'package:floe_client/features/connections/application/remote_owner_operation.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';

abstract interface class RemoteAccessGateway {
  Future<RemoteProducerInspection> inspectRemoteProducer();
  Future<RemoteEnrollmentStatus> reviewAndEnrollRemoteProducer({
    required RemoteProducerIdentity producer,
  });
  Future<RemoteEnrollmentStatus> remoteEnrollmentStatus({
    required String enrollmentId,
  });
  Future<String> connectionObserve({
    required String connectorId,
    required String connectionId,
    String? resource,
    bool? enabled,
    bool disconnecting = false,
    ConnectionObserveBundle? expected,
  });
  Future<ConnectionObserveBundle> connectionObserveReview({
    required String connectorId,
    required String connectionId,
    String? resource,
  });
}

final class NativeRemoteAccessGateway implements RemoteAccessGateway {
  NativeRemoteAccessGateway(RemoteOwnerRequest request)
    : _operations = RemoteOwnerOperation(
        request,
        resultFields: {
          'producer',
          'owner',
          'enrollment',
          'connection_observe_status',
          'reviewed_bundle',
        },
      );

  final RemoteOwnerOperation _operations;

  @override
  Future<RemoteProducerInspection> inspectRemoteProducer() =>
      _operations.perform({'kind': 'inspect_producer'}, (result) {
        final owner = result['owner'];
        final ownerKey = owner == null
            ? null
            : RemoteOwnerPublicKey.fromJson(owner);
        return RemoteProducerInspection(
          producer: RemoteProducerIdentity.fromJson(result['producer']),
          ownerFingerprint: ownerKey?.fingerprint,
        );
      });

  @override
  Future<RemoteEnrollmentStatus> reviewAndEnrollRemoteProducer({
    required RemoteProducerIdentity producer,
  }) => _operations.perform({
    'kind': 'review_and_enroll',
    'producer': producer.toJson(),
  }, (result) => RemoteEnrollmentStatus.fromJson(result['enrollment']));

  @override
  Future<RemoteEnrollmentStatus> remoteEnrollmentStatus({
    required String enrollmentId,
  }) => _operations.perform({
    'kind': 'enrollment_status',
    'enrollment_id': enrollmentId,
  }, (result) => RemoteEnrollmentStatus.fromJson(result['enrollment']));

  @override
  Future<String> connectionObserve({
    required String connectorId,
    required String connectionId,
    String? resource,
    bool? enabled,
    bool disconnecting = false,
    ConnectionObserveBundle? expected,
  }) => _operations.perform({
    'kind': 'connection_observe',
    'connector_id': connectorId,
    'connection_id': connectionId,
    'resource': resource,
    'enabled': enabled,
    'disconnecting': disconnecting,
    'expected': expected?.toJson(),
  }, (result) => result['connection_observe_status']! as String);

  @override
  Future<ConnectionObserveBundle> connectionObserveReview({
    required String connectorId,
    required String connectionId,
    String? resource,
  }) => _operations.perform(
    {
      'kind': 'connection_observe_review',
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
    },
    (result) {
      final bundle = result['reviewed_bundle'];
      if (bundle == null) throw const FormatException('Missing observe bundle');
      return ConnectionObserveBundle.fromJson(bundle);
    },
  );
}
