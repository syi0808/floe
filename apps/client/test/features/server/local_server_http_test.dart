import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
  test(
    'purpose, connection, trace and generation contracts use only v1',
    () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final paths = <String>[];
      server.listen((request) async {
        paths.add(request.uri.path);
        final response = <String, Object?>{'schema_version': 1};
        switch (request.uri.path) {
          case '/v1/inference-purposes':
            response['purposes'] = <String, Object?>{};
          case '/v1/traces':
            response['traces'] = <Object?>[];
          case '/v1/connections':
            response['person_id'] = '00000000-0000-4000-8000-000000000001';
            response['device_id'] = 'local-client';
            response['connections'] = <Object?>[];
          case '/v1/generate':
            final body =
                jsonDecode(await utf8.decoder.bind(request).join()) as Map;
            expect(body['schema_version'], 1);
            expect(body['purpose'], 'everyday_assistance');
            expect(body.containsKey('inference_class'), isFalse);
            response.addAll({
              'purpose': 'everyday_assistance',
              'output': '{"ok":true}',
              'trace_id': 'fixture-trace',
              'routing': {
                'placement': 'server_local',
                'external_transfer': false,
              },
            });
          default:
            fail('Unexpected endpoint: ${request.uri.path}');
        }
        request.response.write(jsonEncode(response));
        await request.response.close();
      });
      addTearDown(() => server.close(force: true));
      final client = LocalServerClient(store: MemoryServerCredentials());
      final connection = ServerConnection(
        address: 'http://127.0.0.1:${server.port}',
        token: 'a' * 52,
        clientId: 'fixture',
        personId: '00000000-0000-4000-8000-000000000001',
        deviceId: 'local-client',
      );
      await client.checkConnection(connection);
      expect(await client.connections(connection), isEmpty);
      expect(await client.privacyActivity(connection), isEmpty);
      final generated = await client.generate(
        connection: connection,
        purpose: InferencePurpose.everydayAssistance,
        dataClasses: ['synthetic'],
        instructions: 'Return a synthetic result',
        input: {'test': true},
        outputSchema: {'type': 'object'},
      );
      expect(generated.output, '{"ok":true}');
      expect(paths, [
        '/v1/inference-purposes',
        '/v1/connections',
        '/v1/traces',
        '/v1/generate',
      ]);
    },
  );

  test('HTTP client does not follow redirects or forward tokens', () async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    var calls = 0;
    server.listen((request) async {
      calls++;
      request.response.statusCode = 302;
      request.response.headers.set(
        'location',
        'http://127.0.0.1:${server.port}/stolen',
      );
      await request.response.close();
    });
    addTearDown(() => server.close(force: true));
    await expectLater(
      LocalServerClient(store: MemoryServerCredentials()).request(
        'http://127.0.0.1:${server.port}',
        '/v1/inference-purposes',
        token: 'a' * 52,
      ),
      throwsA(isA<ServerConnectionException>()),
    );
    expect(calls, 1);
  });

  test(
    'pairing identity and connector lifecycle follow the paired API',
    () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final calls = <String>[];
      var catalogCalls = 0;
      server.listen((request) async {
        final body = request.method == 'GET'
            ? <String, dynamic>{}
            : jsonDecode(await utf8.decoder.bind(request).join())
                  as Map<String, dynamic>;
        calls.add('${request.method} ${request.uri.path}');
        request.response.headers.contentType = ContentType.json;
        switch (request.uri.path) {
          case '/pair/start':
            expect(body['person_id'], '00000000-0000-4000-8000-000000000001');
            expect(body['device_id'], 'local-persistent-device');
            request.response.write(
              jsonEncode({'proof': 'proof', 'code': 'CODE'}),
            );
          case '/v1/connectors':
            catalogCalls++;
            request.response.write(
              jsonEncode({
                'schema_version': 1,
                'person_id': '00000000-0000-4000-8000-000000000001',
                'device_id': 'local-persistent-device',
                'connectors': [
                  {
                    'id': 'github.issues',
                    'name': 'GitHub Issues',
                    'auth_kind': 'secret',
                    'available': true,
                    'status': catalogCalls == 1 ? 'disconnected' : 'connected',
                    if (catalogCalls > 1) ...{
                      'connection_id': 'not-a-uuid',
                      'connection_revision': 1,
                    },
                    'required_scopes': ['github.issues.read'],
                    'scope_fields': ['owner', 'repository'],
                    'capabilities': {
                      'connect': true,
                      'cancel': false,
                      'disconnect': true,
                      'scope_update': true,
                    },
                  },
                  {
                    'id': 'gmail',
                    'name': 'Gmail',
                    'auth_kind': 'oauth_pkce',
                    'available': false,
                    'status': 'unavailable',
                    'required_scopes': ['gmail.readonly'],
                    'scope_fields': <String>[],
                    'capabilities': {
                      'connect': true,
                      'cancel': true,
                      'disconnect': true,
                      'scope_update': false,
                    },
                  },
                ],
              }),
            );
          case '/v1/connectors/github.issues/connect':
            expect(request.method, 'POST');
            expect(body['secret'], 'one-shot-secret');
            expect(body['scope'], {'owner': 'floe', 'repository': 'client'});
            request.response.statusCode = 201;
            request.response.write(
              jsonEncode({
                'schema_version': 1,
                'person_id': '00000000-0000-4000-8000-000000000001',
                'device_id': 'local-persistent-device',
                'attempt_id': 'attempt-1',
                'connector_id': 'github.issues',
                'connection_id': 'connection-1',
                'status': 'connected',
                'created_at': '2026-09-11T00:00:00Z',
              }),
            );
          case '/v1/connectors/github.issues/scope':
            expect(request.method, 'PATCH');
            expect(
              body['connection_id'],
              '00000000-0000-4000-8000-000000000010',
            );
            expect(body['connection_revision'], 1);
            request.response.write(
              jsonEncode({
                'schema_version': 1,
                'person_id': '00000000-0000-4000-8000-000000000001',
                'device_id': 'local-persistent-device',
                'scope': {'owner': 'floe', 'repository': 'server'},
              }),
            );
          case '/v1/connectors/github.issues':
            expect(request.method, 'DELETE');
            expect(
              body['connection_id'],
              '00000000-0000-4000-8000-000000000010',
            );
            expect(body['connection_revision'], 2);
            request.response.write(
              jsonEncode({
                'schema_version': 1,
                'person_id': '00000000-0000-4000-8000-000000000001',
                'device_id': 'local-persistent-device',
                'disconnected': true,
              }),
            );
          default:
            fail('Unexpected endpoint: ${request.uri.path}');
        }
        await request.response.close();
      });
      addTearDown(() => server.close(force: true));
      final client = LocalServerClient(
        store: MemoryServerCredentials(),
        deviceId: 'local-persistent-device',
      );
      final address = 'http://127.0.0.1:${server.port}';
      await client.startPairing(address);
      final connection = ServerConnection(
        address: address,
        token: 'a' * 52,
        clientId: 'fixture',
        personId: '00000000-0000-4000-8000-000000000001',
        deviceId: 'local-persistent-device',
      );
      final catalog = await client.connectorCatalog(connection);
      expect(catalog.deviceId, 'local-persistent-device');
      expect(catalog.connectors.map((item) => item.name), [
        'GitHub Issues',
        'Gmail',
      ]);
      expect(catalog.connectors.last.status, ServerConnectorStatus.unavailable);
      final attempt = await client.connectConnector(
        connection: connection,
        connectorId: 'github.issues',
        scope: {'owner': 'floe', 'repository': 'client'},
        secret: 'one-shot-secret',
      );
      expect(attempt.status, ServerConnectorStatus.connected);
      expect(
        await client.updateConnectorScope(
          connection: connection,
          connectorId: 'github.issues',
          connectionId: '00000000-0000-4000-8000-000000000010',
          connectionRevision: 1,
          scope: {'owner': 'floe', 'repository': 'server'},
        ),
        {'owner': 'floe', 'repository': 'server'},
      );
      await client.disconnectConnector(
        connection: connection,
        connectorId: 'github.issues',
        connectionId: '00000000-0000-4000-8000-000000000010',
        connectionRevision: 2,
      );
      await expectLater(
        client.connectorCatalog(connection),
        throwsA(
          isA<ServerConnectionException>().having(
            (error) => error.code,
            'code',
            'invalid_response',
          ),
        ),
      );
      expect(calls, [
        'POST /pair/start',
        'GET /v1/connectors',
        'POST /v1/connectors/github.issues/connect',
        'PATCH /v1/connectors/github.issues/scope',
        'DELETE /v1/connectors/github.issues',
        'GET /v1/connectors',
      ]);
    },
  );
}
