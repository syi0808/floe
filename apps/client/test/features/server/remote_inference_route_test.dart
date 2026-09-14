import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/features/server/remote_inference_route.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
  test(
    'catalog failure preserves model route and exact recipient consent',
    () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));
      final requests = <String>[];
      var catalogStatus = 403;
      server.listen((request) async {
        requests.add('${request.method} ${request.uri.path}');
        request.response.headers.contentType = ContentType.json;
        if (request.uri.path == '/v1/inference-purposes') {
          request.response.write(
            jsonEncode({
              'schema_version': 1,
              'purposes': {
                for (final purpose in InferencePurpose.values)
                  purpose.wireName: {
                    'available': true,
                    'requires_external_consent': true,
                    'placement': 'external',
                    'recipient': 'approved.example',
                  },
              },
            }),
          );
        } else {
          expect(request.uri.path, '/v1/connectors');
          request.response.statusCode = catalogStatus;
          request.response.write('{}');
        }
        await request.response.close();
      });
      final credentials = MemoryServerCredentials();
      final client = LocalServerClient(store: credentials);
      final connection = ServerConnection(
        address: 'http://127.0.0.1:${server.port}',
        token: 'fixture_token_that_is_long_enough_for_validation',
        clientId: '00000000-0000-4000-8000-000000000002',
        personId: client.personId,
        deviceId: client.deviceId,
      );
      for (final status in [403, 500, 200]) {
        catalogStatus = status;
        for (final recipient in ['approved.example', 'other.example']) {
          await client.save(
            connection.withExternalConsent(true, recipients: [recipient]),
          );
          final saved = credentials.value;
          final route = await resolveRemoteInferenceRoute(client);
          expect(route, isNotNull);
          expect(route!['base_url'], connection.address);
          expect(route['bearer_token'], connection.token);
          expect(route['external'], isTrue);
          expect(route['allow_external'], recipient == 'approved.example');
          expect(route['calendar_connections'], isEmpty);
          expect(credentials.value, saved);
        }
      }
      expect(requests, [
        for (var attempt = 0; attempt < 6; attempt++) ...[
          'GET /v1/inference-purposes',
          'GET /v1/connectors',
        ],
      ]);
    },
  );
}
