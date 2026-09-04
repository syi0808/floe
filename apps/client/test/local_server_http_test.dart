import 'dart:io';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/server_credentials.dart';

void main() {
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
        '/v1/targets',
        token: 'a' * 52,
      ),
      throwsA(isA<ServerConnectionException>()),
    );
    expect(calls, 1);
  });
}
