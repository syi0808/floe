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

  Future<RemoteCalendarGrantPreview> previewRemoteCalendarGrant({
    required String connectorId,
    required String connectionId,
    required String resource,
  });

  Future<RemoteCalendarGrantOverview> reviewRemoteCalendarGrant({
    required String connectorId,
    required String connectionId,
    required String resource,
    required String expectedProducerFingerprint,
    required Map<String, Object?> expectedSourceAuthority,
    String? expectedGrantId,
    Map<String, Object?>? expectedGrantAuthority,
    Map<String, Object?>? expectedConsumerPolicy,
  });

  Future<RemoteCalendarGrantOverview> pauseRemoteCalendarGrant({
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  });

  Future<RemoteViewGrantPreview> previewRemoteViewGrant({
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
  });

  Future<RemoteViewGrantOverview> reviewRemoteViewGrant({
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
    required String expectedProducerFingerprint,
    required Map<String, Object?> expectedSourceAuthority,
    required int expectedConnectionRevision,
    required String expectedProviderIdentity,
    required String expectedRecipient,
  });

  Future<RemoteViewGrantOverview> pauseRemoteViewGrant({
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  });

  Future<RemoteCalendarGrantOverview> remoteCalendarGrantStatus({
    required String grantId,
  });

  Future<RemoteViewGrantOverview> remoteViewGrantStatus({
    required String grantId,
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
          'calendar_grant',
          'calendar_preview',
          'view_grant',
          'view_preview',
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
  }) => _operations.perform(
    {'kind': 'review_and_enroll', 'producer': producer.toJson()},
    (result) {
      return RemoteEnrollmentStatus.fromJson(result['enrollment']);
    },
  );

  @override
  Future<RemoteEnrollmentStatus> remoteEnrollmentStatus({
    required String enrollmentId,
  }) => _operations.perform(
    {'kind': 'enrollment_status', 'enrollment_id': enrollmentId},
    (result) {
      return RemoteEnrollmentStatus.fromJson(result['enrollment']);
    },
  );

  @override
  Future<RemoteCalendarGrantPreview> previewRemoteCalendarGrant({
    required String connectorId,
    required String connectionId,
    required String resource,
  }) => _operations.perform(
    {
      'kind': 'calendar_grant_preview',
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
    },
    (result) {
      return RemoteCalendarGrantPreview.fromJson(result['calendar_preview']);
    },
  );

  @override
  Future<RemoteCalendarGrantOverview> reviewRemoteCalendarGrant({
    required String connectorId,
    required String connectionId,
    required String resource,
    required String expectedProducerFingerprint,
    required Map<String, Object?> expectedSourceAuthority,
    String? expectedGrantId,
    Map<String, Object?>? expectedGrantAuthority,
    Map<String, Object?>? expectedConsumerPolicy,
  }) => _operations.perform(
    {
      'kind': 'calendar_grant_review',
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'expected_producer_fingerprint': expectedProducerFingerprint,
      'expected_source_authority': expectedSourceAuthority,
      'expected_grant_id': expectedGrantId,
      'expected_grant_authority': expectedGrantAuthority,
      'expected_consumer_policy': expectedConsumerPolicy,
    },
    (result) {
      return RemoteCalendarGrantOverview.fromJson(result['calendar_grant']);
    },
  );

  @override
  Future<RemoteCalendarGrantOverview> pauseRemoteCalendarGrant({
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  }) => _operations.perform(
    {
      'kind': 'calendar_grant_pause',
      'grant_id': grantId,
      'expected_authority': expectedAuthority,
    },
    (result) {
      return RemoteCalendarGrantOverview.fromJson(result['calendar_grant']);
    },
  );

  @override
  Future<RemoteViewGrantPreview> previewRemoteViewGrant({
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
  }) => _operations.perform(
    {
      'kind': 'view_grant_preview',
      'view_id': viewId,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'consumer': consumer,
    },
    (result) {
      return RemoteViewGrantPreview.fromJson(result['view_preview']);
    },
  );

  @override
  Future<RemoteViewGrantOverview> reviewRemoteViewGrant({
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
    required String expectedProducerFingerprint,
    required Map<String, Object?> expectedSourceAuthority,
    required int expectedConnectionRevision,
    required String expectedProviderIdentity,
    required String expectedRecipient,
  }) => _operations.perform(
    {
      'kind': 'view_grant_review',
      'view_id': viewId,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'consumer': consumer,
      'expected_producer_fingerprint': expectedProducerFingerprint,
      'expected_source_authority': expectedSourceAuthority,
      'expected_connection_revision': expectedConnectionRevision,
      'expected_provider_identity': expectedProviderIdentity,
      'expected_recipient': expectedRecipient,
    },
    (result) {
      return RemoteViewGrantOverview.fromJson(result['view_grant']);
    },
  );

  @override
  Future<RemoteViewGrantOverview> pauseRemoteViewGrant({
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  }) => _operations.perform(
    {
      'kind': 'view_grant_pause',
      'grant_id': grantId,
      'expected_authority': expectedAuthority,
    },
    (result) {
      return RemoteViewGrantOverview.fromJson(result['view_grant']);
    },
  );

  @override
  Future<RemoteCalendarGrantOverview> remoteCalendarGrantStatus({
    required String grantId,
  }) => _operations.perform(
    {'kind': 'calendar_grant_status', 'grant_id': grantId},
    (result) => RemoteCalendarGrantOverview.fromJson(result['calendar_grant']),
  );

  @override
  Future<RemoteViewGrantOverview> remoteViewGrantStatus({
    required String grantId,
  }) => _operations.perform({
    'kind': 'view_grant_status',
    'grant_id': grantId,
  }, (result) => RemoteViewGrantOverview.fromJson(result['view_grant']));
}
