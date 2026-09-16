/// What Rust Connections tells this client to do next for a pairing Operation.
sealed class PairingDirective {
  const PairingDirective();
}

/// Nothing settled yet; observe again after [pollAfter].
final class PairingObserveAgain extends PairingDirective {
  const PairingObserveAgain({required this.pollAfter});

  final Duration pollAfter;
}

/// The Operation settled. [clientId] and [token] are present only when
/// [state] is `approved`.
final class PairingSettled extends PairingDirective {
  const PairingSettled({required this.state, this.clientId, this.token});

  /// One of `approved`, `rejected`, `expired`, `repair_required`.
  final String state;
  final String? clientId;
  final String? token;
}

/// The pairing Operation owner. Implementations forward to Rust Connections.
abstract interface class PairingOperationGateway {
  Future<PairingDirective> observePairing({
    required String personId,
    required String pairingId,
    required String pollingProof,
    required Map<String, Object?> route,
  });

  Future<void> cancelPairing({
    required String personId,
    required String pairingId,
  });
}

/// The message shown for a settled pairing Operation.
String pairingSettlementMessage(String state) => switch (state) {
  'rejected' => 'Pairing was rejected in the server dashboard.',
  'repair_required' => 'The server asked to pair again. Start a new request.',
  _ => 'Pairing expired. Start a new connection request.',
};
