import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

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
    this.allowExternal = false,
    List<String> externalRecipients = const [],
  }) : externalRecipients = List.unmodifiable(externalRecipients);

  final String address;
  final String token;
  final String clientId;
  final bool allowExternal;
  final List<String> externalRecipients;

  Map<String, Object> toJson() => {
    'base_url': address,
    'token': token,
    'client_id': clientId,
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

class LocalServerClient {
  LocalServerClient({ServerCredentialStore? store})
    : store = store ?? KeychainServerCredentialStore();
  static final shared = LocalServerClient();
  final ServerCredentialStore store;

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
      return ServerConnection(
        address: address,
        token: token,
        clientId: value['client_id'] as String,
        allowExternal: allowExternal,
        externalRecipients: recipients,
      );
    } on Object {
      throw const ServerConnectionException('invalid_saved_connection');
    }
  }

  Future<void> save(ServerConnection value) =>
      store.write(jsonEncode(value.toJson()));

  Future<Map<String, dynamic>> request(
    String address,
    String path, {
    Map<String, Object?>? body,
    String? token,
  }) async {
    final base = normalizeAddress(address);
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 3)
      ..findProxy = (_) => 'DIRECT';
    try {
      return await (() async {
        final request = await client.openUrl(
          body == null ? 'GET' : 'POST',
          Uri.parse('$base$path'),
        );
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
        if (response.statusCode != 200) {
          throw ServerConnectionException(switch (response.statusCode) {
            401 => 'authorization_required',
            403 => 'external_transfer_denied',
            429 => 'pairing_in_progress',
            _ => 'server_rejected',
          });
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

  Future<void> checkConnection(ServerConnection value) async {
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
    final response = await request(
      connection.address,
      '/v1/connections',
      token: connection.token,
    );
    if (response['schema_version'] != 1 ||
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
