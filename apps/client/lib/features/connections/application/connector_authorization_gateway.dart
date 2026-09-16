import 'package:floe_client/features/connections/application/local_server_client.dart';

/// What Rust Connections tells this client to do next for an authorization
/// Operation. The client never decides the transition, the deadline or the
/// retry cadence for itself.
sealed class AuthorizationDirective {
  const AuthorizationDirective();
}

/// Open the page, then observe again after [pollAfter].
final class OpenAuthorizationPage extends AuthorizationDirective {
  const OpenAuthorizationPage({
    required this.authorizationUrl,
    required this.pollAfter,
  });

  final String authorizationUrl;
  final Duration pollAfter;
}

/// Nothing changed; observe again after [pollAfter].
final class ObserveAgain extends AuthorizationDirective {
  const ObserveAgain({required this.pollAfter});

  final Duration pollAfter;
}

/// The Operation settled. [errorCode] is set only for a failed Operation.
final class AuthorizationSettled extends AuthorizationDirective {
  const AuthorizationSettled({required this.state, this.errorCode});

  /// One of `connected`, `failed`, `cancelled`, `timed_out`.
  final String state;
  final String? errorCode;
}

/// The Operation owner. Implementations forward to Rust Connections; the
/// attempt identity and its generation live there, not in a widget field.
abstract interface class ConnectorAuthorizationGateway {
  /// Start an Operation from the attempt the producer just reported.
  Future<AuthorizationDirective> startAuthorization({
    required ServerConnection connection,
    required String connectorId,
    required ServerConnectorAttempt attempt,
  });

  /// Relay one observation of the attempt.
  Future<AuthorizationDirective> observeAuthorization({
    required ServerConnection connection,
    required String connectorId,
    required ServerConnectorAttempt attempt,
  });

  /// Cancel the Operation so an observation already in flight cannot settle it.
  Future<void> cancelAuthorization({
    required ServerConnection connection,
    required String connectorId,
  });
}
