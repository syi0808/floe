import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
  test('purpose, trace and generation contracts use only v1', () async {
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
    );
    await client.checkConnection(connection);
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
    expect(paths, ['/v1/inference-purposes', '/v1/traces', '/v1/generate']);
  });

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
}
