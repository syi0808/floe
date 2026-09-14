import 'local_server_client.dart';

Future<Map<String, Object?>?> resolveRemoteInferenceRoute(
  LocalServerClient serverClient,
) async {
  try {
    final connection = await serverClient.connection();
    if (connection == null) return null;
    final availability = await serverClient.purposes(connection);
    final route = availability[InferencePurpose.everydayAssistance];
    if (route == null || !route.available) return null;
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
    return {
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
  } on ServerConnectionException {
    return null;
  }
}
