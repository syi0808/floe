import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

import '../../app/local_identity.dart';

final _uuidPattern = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
);

abstract interface class ServerCredentialStore {
  Future<String?> read();
  Future<void> write(String value);
  Future<void> delete();
}

class KeychainServerCredentialStore implements ServerCredentialStore {
  static const channel = MethodChannel('floe/local-server');

  @override
  Future<String?> read() async {
    try {
      return await channel.invokeMethod<String>('read');
    } on MissingPluginException {
      return null;
    }
  }

  @override
  Future<void> write(String value) => channel.invokeMethod('write', value);

  @override
  Future<void> delete() => channel.invokeMethod('delete');
}

class ServerConnection {
  ServerConnection({
    required this.address,
    required this.token,
    required this.clientId,
    required this.personId,
    required this.deviceId,
    this.allowExternal = false,
    List<String> externalRecipients = const [],
  }) : externalRecipients = List.unmodifiable(externalRecipients);

  final String address;
  final String token;
  final String clientId;
  final String personId;
  final String deviceId;
  final bool allowExternal;
  final List<String> externalRecipients;

  Map<String, Object> toJson() => {
    'base_url': address,
    'token': token,
    'client_id': clientId,
    'person_id': personId,
    'device_id': deviceId,
    'allow_external': allowExternal,
    'external_recipients': externalRecipients,
  };

  bool coversExternalRecipient(String? recipient) =>
      allowExternal &&
      recipient != null &&
      externalRecipients.contains(recipient);

  ServerConnection withExternalConsent(
    bool value, {
    Iterable<String> recipients = const [],
  }) => ServerConnection(
    address: address,
    token: token,
    clientId: clientId,
    personId: personId,
    deviceId: deviceId,
    allowExternal: value && recipients.isNotEmpty,
    externalRecipients: value ? ([...recipients.toSet()]..sort()) : const [],
  );
}

enum InferencePurpose {
  quickResponse('quick_response'),
  everydayAssistance('everyday_assistance'),
  deepWork('deep_work');

  const InferencePurpose(this.wireName);
  final String wireName;
}

final class InferencePurposeAvailability {
  const InferencePurposeAvailability({
    required this.available,
    required this.requiresExternalConsent,
    this.placement,
    this.recipient,
  });
  final bool available;
  final bool requiresExternalConsent;
  final String? placement;
  final String? recipient;
}

final class InferenceAuditRecord {
  const InferenceAuditRecord({
    required this.traceId,
    required this.createdAt,
    required this.purpose,
    required this.dataClasses,
    required this.placement,
    required this.externalTransfer,
    required this.outcome,
  });

  final String traceId;
  final DateTime createdAt;
  final String purpose;
  final List<String> dataClasses;
  final String placement;
  final bool externalTransfer;
  final String outcome;
}

final class RemoteGenerationResult {
  const RemoteGenerationResult({
    required this.output,
    required this.traceId,
    required this.placement,
    required this.externalTransfer,
  });
  final String output;
  final String traceId;
  final String placement;
  final bool externalTransfer;
}

class ServerConnectionException implements Exception {
  const ServerConnectionException(this.code);
  final String code;
  @override
  String toString() => code;
}

enum ServerConnectorStatus {
  available,
  connecting,
  connected,
  error,
  unavailable,
}

final class ServerConnectorCapabilities {
  const ServerConnectorCapabilities({
    required this.connect,
    required this.cancel,
    required this.disconnect,
    required this.scopeUpdate,
  });

  final bool connect;
  final bool cancel;
  final bool disconnect;
  final bool scopeUpdate;
}

final class ServerConnector {
  const ServerConnector({
    required this.id,
    required this.name,
    required this.authKind,
    required this.available,
    required this.status,
    required this.requiredScopes,
    required this.scopeFields,
    required this.capabilities,
    required this.scope,
    this.connectionId,
    this.connectionRevision,
  });

  final String id;
  final String name;
  final String authKind;
  final bool available;
  final ServerConnectorStatus status;
  final List<String> requiredScopes;
  final List<String> scopeFields;
  final ServerConnectorCapabilities capabilities;
  final Map<String, Object?> scope;
  final String? connectionId;
  final int? connectionRevision;

  bool get isSecret => authKind == 'secret';
}

final class ServerConnectorCatalog {
  const ServerConnectorCatalog({
    required this.personId,
    required this.deviceId,
    required this.connectors,
  });

  final String personId;
  final String deviceId;
  final List<ServerConnector> connectors;
}

final class ServerConnectorAttempt {
  const ServerConnectorAttempt({
    required this.id,
    required this.connectorId,
    required this.connectionId,
    required this.status,
    required this.createdAt,
    this.authorizationUrl,
    this.errorCode,
  });

  final String id;
  final String connectorId;
  final String connectionId;
  final ServerConnectorStatus status;
  final DateTime createdAt;
  final String? authorizationUrl;
  final String? errorCode;
}

class LocalServerClient {
  LocalServerClient({
    ServerCredentialStore? store,
    this.personId = defaultLocalPersonId,
    this.deviceId = 'local-client',
  }) : store = store ?? KeychainServerCredentialStore();
  static final shared = LocalServerClient();
  final ServerCredentialStore store;
  final String personId;
  final String deviceId;

  static String normalizeAddress(String source) {
    final address = Uri.tryParse(source.trim());
    if (address == null ||
        address.scheme != 'http' ||
        !['127.0.0.1', 'localhost'].contains(address.host) ||
        address.userInfo.isNotEmpty ||
        address.hasQuery ||
        address.hasFragment ||
        !['', '/'].contains(address.path) ||
        address.port < 1 ||
        address.port > 65535) {
      throw const ServerConnectionException('invalid_address');
    }
    return address.replace(host: '127.0.0.1', path: '').toString();
  }

  Future<ServerConnection?> connection() async {
    final raw = await store.read();
    if (raw == null) return null;
    try {
      final value = jsonDecode(raw) as Map<String, dynamic>;
      final address = normalizeAddress(value['base_url'] as String);
      final token = value['token'] as String;
      if (!RegExp(r'^[A-Za-z0-9_-]{32,256}$').hasMatch(token)) {
        throw const FormatException();
      }
      final recipients = List<String>.from(
        value['external_recipients'] as List,
      );
      final allowExternal = value['allow_external'] as bool;
      if (recipients.length > 16 ||
          recipients.toSet().length != recipients.length ||
          (allowExternal && recipients.isEmpty) ||
          recipients.any(
            (recipient) => recipient.trim().isEmpty || recipient.length > 253,
          )) {
        throw const FormatException();
      }
      final connection = ServerConnection(
        address: address,
        token: token,
        clientId: value['client_id'] as String,
        personId: value['person_id'] as String,
        deviceId: value['device_id'] as String,
        allowExternal: allowExternal,
        externalRecipients: recipients,
      );
      _requireConnectionIdentity(connection);
      return connection;
    } on Object {
      throw const ServerConnectionException('invalid_saved_connection');
    }
  }

  Future<void> save(ServerConnection value) {
    _requireConnectionIdentity(value);
    return store.write(jsonEncode(value.toJson()));
  }

  void _requireConnectionIdentity(ServerConnection connection) {
    if (connection.personId != personId || connection.deviceId != deviceId) {
      throw const ServerConnectionException('connection_identity_mismatch');
    }
  }

  Future<Map<String, dynamic>> request(
    String address,
    String path, {
    Map<String, Object?>? body,
    String? token,
  }) => _request(
    address,
    path,
    method: body == null ? 'GET' : 'POST',
    body: body,
    token: token,
  );

  Future<Map<String, dynamic>> _request(
    String address,
    String path, {
    required String method,
    Map<String, Object?>? body,
    String? token,
    Set<int> acceptedStatuses = const {200},
  }) async {
    final base = normalizeAddress(address);
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 3)
      ..findProxy = (_) => 'DIRECT';
    try {
      return await (() async {
        final request = await client.openUrl(method, Uri.parse('$base$path'));
        request.followRedirects = false;
        request.headers.contentType = ContentType.json;
        if (token != null) {
          request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
        }
        if (body != null) request.write(jsonEncode(body));
        final response = await request.close();
        final bytes = <int>[];
        await for (final chunk in response) {
          if (bytes.length + chunk.length > 65536) {
            throw const ServerConnectionException('invalid_response');
          }
          bytes.addAll(chunk);
        }
        if (!acceptedStatuses.contains(response.statusCode)) {
          final serverCode = _responseErrorCode(bytes);
          throw ServerConnectionException(
            serverCode ??
                switch (response.statusCode) {
                  401 => 'unauthorized',
                  403 => 'external_transfer_denied',
                  429 => 'pairing_in_progress',
                  _ => 'server_rejected',
                },
          );
        }
        return jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
      })().timeout(const Duration(seconds: 8));
    } on ServerConnectionException {
      rethrow;
    } on Object {
      throw const ServerConnectionException('server_unavailable');
    } finally {
      client.close(force: true);
    }
  }

  Future<Map<String, dynamic>> startPairing(String address) => request(
    address,
    '/pair/start',
    body: {'person_id': personId, 'device_id': deviceId},
  );

  Future<ServerConnectorCatalog> connectorCatalog(
    ServerConnection connection,
  ) async {
    _requireConnectionIdentity(connection);
    final value = await _request(
      connection.address,
      '/v1/connectors',
      method: 'GET',
      token: connection.token,
    );
    try {
      if (value['schema_version'] != 1 ||
          value['person_id'] != personId ||
          value['device_id'] != deviceId ||
          value['connectors'] is! List) {
        throw const FormatException();
      }
      final connectors = (value['connectors'] as List)
          .map(_serverConnector)
          .toList(growable: false);
      if (connectors.length > 64 ||
          connectors.map((item) => item.id).toSet().length !=
              connectors.length) {
        throw const FormatException();
      }
      return ServerConnectorCatalog(
        personId: value['person_id'] as String,
        deviceId: value['device_id'] as String,
        connectors: List.unmodifiable(connectors),
      );
    } on Object {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<ServerConnectorAttempt> connectConnector({
    required ServerConnection connection,
    required String connectorId,
    required Map<String, Object?> scope,
    String? secret,
  }) async {
    _requireConnectionIdentity(connection);
    final value = await _request(
      connection.address,
      '/v1/connectors/${Uri.encodeComponent(connectorId)}/connect',
      method: 'POST',
      token: connection.token,
      acceptedStatuses: const {201},
      body: {'schema_version': 1, 'scope': scope, 'secret': ?secret},
    );
    return _connectorAttempt(
      value,
      personId: connection.personId,
      deviceId: connection.deviceId,
    );
  }

  Future<ServerConnectorAttempt> connectorAttempt({
    required ServerConnection connection,
    required String connectorId,
    required String attemptId,
  }) async {
    _requireConnectionIdentity(connection);
    return _connectorAttempt(
      await _request(
        connection.address,
        '/v1/connectors/${Uri.encodeComponent(connectorId)}/connection-attempts/${Uri.encodeComponent(attemptId)}',
        method: 'GET',
        token: connection.token,
      ),
      personId: connection.personId,
      deviceId: connection.deviceId,
    );
  }

  Future<ServerConnectorAttempt> cancelConnectorAttempt({
    required ServerConnection connection,
    required String connectorId,
    required String attemptId,
  }) async {
    _requireConnectionIdentity(connection);
    return _connectorAttempt(
      await _request(
        connection.address,
        '/v1/connectors/${Uri.encodeComponent(connectorId)}/connection-attempts/${Uri.encodeComponent(attemptId)}/cancel',
        method: 'POST',
        token: connection.token,
        body: const {},
      ),
      personId: connection.personId,
      deviceId: connection.deviceId,
    );
  }

  Future<Map<String, Object?>> updateConnectorScope({
    required ServerConnection connection,
    required String connectorId,
    required String connectionId,
    required int connectionRevision,
    required Map<String, Object?> scope,
  }) async {
    _requireConnectionIdentity(connection);
    if (connectionId.isEmpty || connectionRevision <= 0) {
      throw ArgumentError('Invalid connector precondition');
    }
    final value = await _request(
      connection.address,
      '/v1/connectors/${Uri.encodeComponent(connectorId)}/scope',
      method: 'PATCH',
      token: connection.token,
      body: {
        'schema_version': 1,
        'connection_id': connectionId,
        'connection_revision': connectionRevision,
        'scope': scope,
      },
    );
    try {
      if (value['schema_version'] != 1 ||
          value['person_id'] != connection.personId ||
          value['device_id'] != connection.deviceId) {
        throw const FormatException();
      }
      return Map<String, Object?>.unmodifiable(
        Map<String, Object?>.from(value['scope'] as Map),
      );
    } on Object {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<void> disconnectConnector({
    required ServerConnection connection,
    required String connectorId,
    required String connectionId,
    required int connectionRevision,
  }) async {
    _requireConnectionIdentity(connection);
    if (connectionId.isEmpty || connectionRevision <= 0) {
      throw ArgumentError('Invalid connector precondition');
    }
    final value = await _request(
      connection.address,
      '/v1/connectors/${Uri.encodeComponent(connectorId)}',
      method: 'DELETE',
      token: connection.token,
      body: {
        'schema_version': 1,
        'connection_id': connectionId,
        'connection_revision': connectionRevision,
      },
    );
    if (value['schema_version'] != 1 ||
        value['person_id'] != connection.personId ||
        value['device_id'] != connection.deviceId ||
        value['disconnected'] != true) {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<void> checkConnection(ServerConnection value) async {
    _requireConnectionIdentity(value);
    final response = await request(
      value.address,
      '/v1/inference-purposes',
      token: value.token,
    );
    if (response['schema_version'] != 1 ||
        response['purposes'] is! Map<String, dynamic>) {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<Map<InferencePurpose, InferencePurposeAvailability>> purposes(
    ServerConnection connection,
  ) async {
    _requireConnectionIdentity(connection);
    final response = await request(
      connection.address,
      '/v1/inference-purposes',
      token: connection.token,
    );
    if (response['schema_version'] != 1 ||
        response['purposes'] is! Map<String, dynamic>) {
      throw const ServerConnectionException('invalid_response');
    }
    final values = Map<String, dynamic>.from(response['purposes'] as Map);
    try {
      return {
        for (final purpose in InferencePurpose.values)
          purpose: _purposeAvailability(values[purpose.wireName]),
      };
    } on Object {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<List<Map<String, dynamic>>> connections(
    ServerConnection connection,
  ) async {
    _requireConnectionIdentity(connection);
    final response = await request(
      connection.address,
      '/v1/connections',
      token: connection.token,
    );
    if (response['schema_version'] != 1 ||
        response['person_id'] != connection.personId ||
        response['device_id'] != connection.deviceId ||
        response['connections'] is! List ||
        (response['connections'] as List).length > 64) {
      throw const ServerConnectionException('invalid_response');
    }
    try {
      return List.unmodifiable(
        (response['connections'] as List).map(
          (value) => Map<String, dynamic>.from(value as Map),
        ),
      );
    } on Object {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<List<InferenceAuditRecord>> privacyActivity(
    ServerConnection connection,
  ) async {
    _requireConnectionIdentity(connection);
    final response = await request(
      connection.address,
      '/v1/traces',
      token: connection.token,
    );
    if (response['schema_version'] != 1 ||
        response['traces'] is! List ||
        (response['traces'] as List).length > 20) {
      throw const ServerConnectionException('invalid_response');
    }
    try {
      return List.unmodifiable(
        (response['traces'] as List).map((raw) {
          final value = Map<String, dynamic>.from(raw as Map);
          final record = InferenceAuditRecord(
            traceId: value['trace_id'] as String,
            createdAt: DateTime.parse(value['created_at'] as String).toLocal(),
            purpose: value['purpose'] as String,
            dataClasses: List<String>.from(value['data_classes'] as List),
            placement: value['placement'] as String,
            externalTransfer: value['external_transfer'] as bool,
            outcome: value['outcome'] as String,
          );
          if (!RegExp(r'^[0-9a-f]{32}$').hasMatch(record.traceId) ||
              !InferencePurpose.values.any(
                (purpose) => purpose.wireName == record.purpose,
              ) ||
              record.dataClasses.isEmpty ||
              record.dataClasses.length > 4 ||
              record.dataClasses.any(
                (value) => !const {
                  'synthetic',
                  'personal',
                  'highly_sensitive',
                }.contains(value),
              ) ||
              !const {'server_local', 'remote'}.contains(record.placement) ||
              record.externalTransfer != (record.placement == 'remote') ||
              record.outcome.isEmpty ||
              record.outcome.length > 64) {
            throw const FormatException();
          }
          return record;
        }),
      );
    } on Object {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<RemoteGenerationResult> generate({
    required ServerConnection connection,
    required InferencePurpose purpose,
    required List<String> dataClasses,
    required String instructions,
    required Object input,
    required Map<String, Object?> outputSchema,
    String? replayOf,
  }) async {
    _requireConnectionIdentity(connection);
    final response = await request(
      connection.address,
      '/v1/generate',
      token: connection.token,
      body: {
        'schema_version': 1,
        'purpose': purpose.wireName,
        'data_classes': dataClasses,
        'allow_external': connection.allowExternal,
        'instructions': instructions,
        'input': input,
        'output_schema': outputSchema,
        'replay_of': ?replayOf,
      },
    );
    final routing = Map<String, dynamic>.from(response['routing'] as Map);
    if (response['schema_version'] != 1 ||
        response['purpose'] != purpose.wireName ||
        response['output'] is! String ||
        response['trace_id'] is! String ||
        routing['placement'] is! String ||
        routing['external_transfer'] is! bool) {
      throw const ServerConnectionException('invalid_response');
    }
    return RemoteGenerationResult(
      output: response['output'] as String,
      traceId: response['trace_id'] as String,
      placement: routing['placement'] as String,
      externalTransfer: routing['external_transfer'] as bool,
    );
  }

  Future<void> openDashboard(String address) => KeychainServerCredentialStore
      .channel
      .invokeMethod('open', '${normalizeAddress(address)}/manage/');
}

String? _responseErrorCode(List<int> bytes) {
  try {
    final value = jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
    final error = Map<String, dynamic>.from(value['error'] as Map);
    final code = error['code'];
    if (code is String && RegExp(r'^[a-z][a-z0-9_]{1,63}$').hasMatch(code)) {
      return code;
    }
  } on Object {
    return null;
  }
  return null;
}

ServerConnector _serverConnector(Object? raw) {
  final value = Map<String, dynamic>.from(raw as Map);
  final capabilities = Map<String, dynamic>.from(value['capabilities'] as Map);
  final id = value['id'] as String;
  final name = value['name'] as String;
  final authKind = value['auth_kind'] as String;
  final status = _connectorStatus(value['status'] as String);
  final requiredScopes = List<String>.from(value['required_scopes'] as List);
  final scopeFields = List<String>.from(value['scope_fields'] as List);
  final scope = value['scope'] == null
      ? const <String, Object?>{}
      : Map<String, Object?>.from(value['scope'] as Map);
  if (id.isEmpty ||
      name.isEmpty ||
      !const {'oauth_pkce', 'secret'}.contains(authKind) ||
      value['available'] is! bool ||
      requiredScopes.length > 32 ||
      scopeFields.length > 16 ||
      capabilities.values.any((item) => item is! bool)) {
    throw const FormatException();
  }
  final connectionId = value['connection_id'] as String?;
  final connectionRevision = value['connection_revision'] as int?;
  if ((connectionId == null) != (connectionRevision == null) ||
      connectionId != null && !_uuidPattern.hasMatch(connectionId) ||
      connectionRevision != null && connectionRevision <= 0) {
    throw const FormatException();
  }
  return ServerConnector(
    id: id,
    name: name,
    authKind: authKind,
    available: value['available'] as bool,
    status: status,
    requiredScopes: List.unmodifiable(requiredScopes),
    scopeFields: List.unmodifiable(scopeFields),
    capabilities: ServerConnectorCapabilities(
      connect: capabilities['connect'] as bool,
      cancel: capabilities['cancel'] as bool,
      disconnect: capabilities['disconnect'] as bool,
      scopeUpdate: capabilities['scope_update'] as bool,
    ),
    scope: Map.unmodifiable(scope),
    connectionId: connectionId,
    connectionRevision: connectionRevision,
  );
}

ServerConnectorAttempt _connectorAttempt(
  Map<String, dynamic> value, {
  required String personId,
  required String deviceId,
}) {
  try {
    if (value['schema_version'] != 1 ||
        value['person_id'] != personId ||
        value['device_id'] != deviceId) {
      throw const FormatException();
    }
    final error = value['error'] == null
        ? null
        : Map<String, dynamic>.from(value['error'] as Map)['code'] as String;
    return ServerConnectorAttempt(
      id: value['attempt_id'] as String,
      connectorId: value['connector_id'] as String,
      connectionId: value['connection_id'] as String,
      status: _connectorStatus(value['status'] as String),
      createdAt: DateTime.parse(value['created_at'] as String),
      authorizationUrl: value['authorization_url'] as String?,
      errorCode: error,
    );
  } on Object {
    throw const ServerConnectionException('invalid_response');
  }
}

ServerConnectorStatus _connectorStatus(String value) => switch (value) {
  'disconnected' => ServerConnectorStatus.available,
  'pending' || 'connecting' => ServerConnectorStatus.connecting,
  'connected' => ServerConnectorStatus.connected,
  'failed' || 'error' => ServerConnectorStatus.error,
  'cancelled' => ServerConnectorStatus.available,
  'unavailable' => ServerConnectorStatus.unavailable,
  _ => throw const FormatException(),
};

InferencePurposeAvailability _purposeAvailability(Object? raw) {
  final value = Map<String, dynamic>.from(raw as Map);
  final available = value['available'] as bool;
  final requiresConsent = value['requires_external_consent'] as bool;
  final placement = value['placement'] as String?;
  final recipient = value['recipient'] as String?;
  if ((!available && (placement != null || recipient != null)) ||
      (available && !const {'server_local', 'external'}.contains(placement)) ||
      requiresConsent != (placement == 'external') ||
      (recipient != null &&
          (placement != 'external' ||
              recipient.trim().isEmpty ||
              recipient.length > 253)) ||
      (placement == 'external' && recipient == null)) {
    throw const FormatException();
  }
  return InferencePurposeAvailability(
    available: available,
    requiresExternalConsent: requiresConsent,
    placement: placement,
    recipient: recipient,
  );
}
