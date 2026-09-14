import 'local_server_client.dart';

enum RemoteInferenceRouteStatus {
  notConfigured,
  available,
  unavailable,
  denied,
  consentRequired,
}

final class RemoteInferenceRouteObservation {
  const RemoteInferenceRouteObservation._({
    required this.status,
    this.route,
    this.code,
  });

  const RemoteInferenceRouteObservation.notConfigured()
    : this._(status: RemoteInferenceRouteStatus.notConfigured);

  const RemoteInferenceRouteObservation.available(Map<String, Object?> route)
    : this._(status: RemoteInferenceRouteStatus.available, route: route);

  const RemoteInferenceRouteObservation.failure(
    RemoteInferenceRouteStatus status, {
    String? code,
  }) : this._(status: status, code: code);

  final RemoteInferenceRouteStatus status;
  final Map<String, Object?>? route;
  final String? code;

  bool get isAvailable => status == RemoteInferenceRouteStatus.available;
  bool get isNotConfigured =>
      status == RemoteInferenceRouteStatus.notConfigured;
}

final class RemoteInferenceRouteException implements Exception {
  const RemoteInferenceRouteException(this.status, this.code);

  final RemoteInferenceRouteStatus status;
  final String code;

  String get agentFailure => switch (status) {
    RemoteInferenceRouteStatus.unavailable => 'server_model_unavailable',
    RemoteInferenceRouteStatus.denied => switch (code) {
      'unauthorized' ||
      'credential_expired' ||
      'invalid_token' => 'credential_expired',
      _ => 'policy_denied',
    },
    RemoteInferenceRouteStatus.consentRequired => 'consent_required',
    RemoteInferenceRouteStatus.notConfigured ||
    RemoteInferenceRouteStatus.available => 'server_model_unavailable',
  };

  @override
  String toString() => 'RemoteInferenceRouteException($status, $code)';
}

Future<RemoteInferenceRouteObservation> observeRemoteInferenceRoute(
  LocalServerClient serverClient,
) async {
  final ServerConnection? connection;
  try {
    connection = await _savedConnection(serverClient);
  } on RemoteInferenceRouteException catch (error) {
    return RemoteInferenceRouteObservation.failure(
      error.status,
      code: error.code,
    );
  }
  if (connection == null) {
    return const RemoteInferenceRouteObservation.notConfigured();
  }

  final Map<InferencePurpose, InferencePurposeAvailability> availability;
  try {
    availability = await _purposeAvailability(serverClient, connection);
  } on RemoteInferenceRouteException catch (error) {
    return RemoteInferenceRouteObservation.failure(
      error.status,
      code: error.code,
    );
  }
  final route = availability[InferencePurpose.everydayAssistance];
  if (route == null || !route.available) {
    return const RemoteInferenceRouteObservation.failure(
      RemoteInferenceRouteStatus.unavailable,
      code: 'purpose_unavailable',
    );
  }
  final consentCoversRoute =
      !route.requiresExternalConsent ||
      connection.coversExternalRecipient(route.recipient);

  var calendarConnections = <Map<String, Object?>>[];
  try {
    final catalog = await serverClient.connectorCatalog(connection);
    calendarConnections = catalog.connectors
        .where(
          (connector) =>
              connector.status == ServerConnectorStatus.connected &&
              const {
                'calendar.google',
                'calendar.microsoft',
              }.contains(connector.id),
        )
        .map(
          (connector) => {
            'connector_id': connector.id,
            'connection_id': connector.connectionId,
            'connection_revision': connector.connectionRevision,
          },
        )
        .toList(growable: false);
  } on ServerConnectionException {
    calendarConnections = [];
  }
  final routeValue = <String, Object?>{
    'base_url': connection.address,
    'bearer_token': connection.token,
    'purpose': InferencePurpose.everydayAssistance.wireName,
    'external': route.requiresExternalConsent,
    'allow_external': consentCoversRoute,
    'recipient': route.recipient,
    'pairing': {
      'client_id': connection.clientId,
      'person_id': connection.personId,
      'device_id': connection.deviceId,
    },
    'calendar_connections': calendarConnections,
  };
  if (!consentCoversRoute) {
    return RemoteInferenceRouteObservation._(
      status: RemoteInferenceRouteStatus.consentRequired,
      route: routeValue,
      code: 'consent_required',
    );
  }
  return RemoteInferenceRouteObservation.available(routeValue);
}

Future<Map<String, Object?>?> resolveRemoteInferenceRoute(
  LocalServerClient serverClient,
) async {
  final observation = await observeRemoteInferenceRoute(serverClient);
  if (observation.isNotConfigured) return null;
  if (observation.route case final route?) {
    if (observation.status == RemoteInferenceRouteStatus.available ||
        observation.status == RemoteInferenceRouteStatus.consentRequired) {
      return route;
    }
  }
  if (observation.route == null || !observation.isAvailable) {
    throw RemoteInferenceRouteException(
      observation.status,
      observation.code ?? 'route_unavailable',
    );
  }
  return observation.route!;
}

Future<ServerConnection?> _savedConnection(
  LocalServerClient serverClient,
) async {
  try {
    return await serverClient.connection();
  } on ServerConnectionException catch (error) {
    throw RemoteInferenceRouteException(
      _statusForServerError(error.code),
      error.code,
    );
  }
}

Future<Map<InferencePurpose, InferencePurposeAvailability>>
_purposeAvailability(
  LocalServerClient serverClient,
  ServerConnection connection,
) async {
  try {
    return await serverClient.purposes(connection);
  } on ServerConnectionException catch (error) {
    throw RemoteInferenceRouteException(
      _statusForServerError(error.code),
      error.code,
    );
  }
}

RemoteInferenceRouteStatus _statusForServerError(String code) => switch (code) {
  'unauthorized' ||
  'credential_expired' ||
  'invalid_token' => RemoteInferenceRouteStatus.denied,
  'external_transfer_denied' ||
  'policy_denied' ||
  'access_denied' => RemoteInferenceRouteStatus.denied,
  _ => RemoteInferenceRouteStatus.unavailable,
};
