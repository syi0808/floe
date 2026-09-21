import 'package:floe_client/features/connections/application/remote_owner_operation.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';

final class PairingTarget {
  const PairingTarget(this.baseUrl);
  final String baseUrl;
  Map<String, Object?> toJson() => {'base_url': baseUrl};
}

abstract interface class RemotePairingGateway {
  Future<RemoteOwnerPublicKey> prepareRemotePairing();
  Future<RemotePairingStatus> confirmRemotePairing({
    required PairingTarget target,
    required RemotePairingChallenge challenge,
    required String pollingProof,
  });
  Future<RemotePairingStatus> remotePairingStatus({
    required PairingTarget target,
    required String pairingId,
    required String pollingProof,
  });
  Future<RemotePairingStatus> finalizeRemotePairing({
    required PairingTarget target,
    required String pairingId,
    required String pollingProof,
    required RemotePairingChallenge challenge,
  });
  Future<void> releaseApprovedPairing(String pairingId);
}

final class NativeRemotePairingGateway implements RemotePairingGateway {
  NativeRemotePairingGateway(
    RemoteOwnerRequest request, {
    required this.expectedPersonId,
    required this.expectedDeviceId,
    Duration pollInterval = const Duration(milliseconds: 150),
    Duration deadline = const Duration(seconds: 15),
  }) : _operations = RemoteOwnerOperation(
         request,
         resultFields: {'owner', 'pairing'},
         pollInterval: pollInterval,
         deadline: deadline,
       );

  final String expectedPersonId;
  final String expectedDeviceId;
  final RemoteOwnerOperation _operations;
  final Map<String, Set<String>> _approved = {};

  @override
  Future<RemoteOwnerPublicKey> prepareRemotePairing() => _operations.perform({
    'kind': 'prepare',
  }, (result) => RemoteOwnerPublicKey.fromJson(result['owner']));

  @override
  Future<RemotePairingStatus> confirmRemotePairing({
    required PairingTarget target,
    required RemotePairingChallenge challenge,
    required String pollingProof,
  }) => _pairing({
    'kind': 'confirm',
    'target': target.toJson(),
    'challenge': challenge.toJson(),
    'polling_proof': pollingProof,
  }, challenge.pairingId);

  @override
  Future<RemotePairingStatus> remotePairingStatus({
    required PairingTarget target,
    required String pairingId,
    required String pollingProof,
  }) => _pairing({
    'kind': 'status',
    'target': target.toJson(),
    'pairing_id': pairingId,
    'polling_proof': pollingProof,
  }, pairingId);

  @override
  Future<RemotePairingStatus> finalizeRemotePairing({
    required PairingTarget target,
    required String pairingId,
    required String pollingProof,
    required RemotePairingChallenge challenge,
  }) => _pairing({
    'kind': 'finalize',
    'target': target.toJson(),
    'pairing_id': pairingId,
    'polling_proof': pollingProof,
    'challenge': challenge.toJson(),
  }, pairingId);

  Future<RemotePairingStatus> _pairing(
    Map<String, Object?> operation,
    String pairingId,
  ) => _operations.perform(
    operation,
    (result) {
      final report = RemotePairingStatus.fromJson(
        result['pairing'],
        result['operation_id'] as String,
      );
      if (report.pairingId != pairingId ||
          report.personId != expectedPersonId ||
          report.deviceId != expectedDeviceId) {
        throw const FormatException('Pairing identity mismatch');
      }
      return report;
    },
    retain: (report) {
      if (report.status != 'approved') return false;
      _approved.putIfAbsent(pairingId, () => {}).add(report.operationId);
      return true;
    },
  );

  @override
  Future<void> releaseApprovedPairing(String pairingId) async {
    final operations = _approved[pairingId];
    if (operations == null) return;
    for (final operationId in operations.toList()) {
      await _operations.release(operationId);
      operations.remove(operationId);
    }
    if (operations.isEmpty) _approved.remove(pairingId);
  }
}
