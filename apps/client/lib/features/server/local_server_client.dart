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
  const ServerConnection({
    required this.address,
    required this.token,
    required this.clientId,
    this.allowExternal = false,
  });

  final String address;
  final String token;
  final String clientId;
  final bool allowExternal;

  Map<String, Object> toJson() => {
    'base_url': address,
    'token': token,
    'client_id': clientId,
    'allow_external': allowExternal,
  };

  ServerConnection withExternalConsent(bool value) => ServerConnection(
    address: address,
    token: token,
    clientId: clientId,
    allowExternal: value,
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
  });
  final bool available;
  final bool requiresExternalConsent;
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
      return ServerConnection(
        address: address,
        token: token,
        clientId: value['client_id'] as String,
        allowExternal: value['allow_external'] == true,
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
      '/v2/inference-purposes',
      token: value.token,
    );
    if (response['schema_version'] != 2 ||
        response['purposes'] is! Map<String, dynamic>) {
      throw const ServerConnectionException('invalid_response');
    }
  }

  Future<Map<InferencePurpose, InferencePurposeAvailability>> purposes(
    ServerConnection connection,
  ) async {
    final response = await request(
      connection.address,
      '/v2/inference-purposes',
      token: connection.token,
    );
    final values = Map<String, dynamic>.from(response['purposes'] as Map);
    return {
      for (final purpose in InferencePurpose.values)
        purpose: InferencePurposeAvailability(
          available: (values[purpose.wireName] as Map?)?['available'] == true,
          requiresExternalConsent:
              (values[purpose.wireName]
                  as Map?)?['requires_external_consent'] ==
              true,
        ),
    };
  }

  Future<RemoteGenerationResult> generate({
    required ServerConnection connection,
    required InferencePurpose purpose,
    required String instructions,
    required Object input,
    required Map<String, Object?> outputSchema,
    String? replayOf,
  }) async {
    final response = await request(
      connection.address,
      '/v2/generate',
      token: connection.token,
      body: {
        'schema_version': 2,
        'purpose': purpose.wireName,
        'allow_external': connection.allowExternal,
        'instructions': instructions,
        'input': input,
        'output_schema': outputSchema,
        'replay_of': ?replayOf,
      },
    );
    final routing = Map<String, dynamic>.from(response['routing'] as Map);
    if (response['schema_version'] != 2 ||
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
