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
  });

  final String address;
  final String token;
  final String clientId;

  Map<String, String> toJson() => {
    'base_url': address,
    'token': token,
    'client_id': clientId,
  };
  Map<String, String> toInferenceJson() => {
    'base_url': address,
    'token': token,
  };
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

  Future<Map<String, dynamic>> inferenceClasses(ServerConnection value) async {
    final response = await request(
      value.address,
      '/v1/inference-classes',
      token: value.token,
    );
    if (response['schema_version'] != 1 ||
        response['inference_classes'] is! Map<String, dynamic>) {
      throw const ServerConnectionException('invalid_response');
    }
    return response['inference_classes'] as Map<String, dynamic>;
  }

  Future<void> openDashboard(String address) => KeychainServerCredentialStore
      .channel
      .invokeMethod('open', '${normalizeAddress(address)}/manage/');
}
