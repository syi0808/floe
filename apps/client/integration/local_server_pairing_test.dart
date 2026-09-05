import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';

import '../test/support/server_credentials.dart';

void main() {
  test(
    'paired address and credential authorize the local server client',
    () async {
      final temporary = await Directory.systemTemp.createTemp('floe-pairing-');
      final socket = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      final port = socket.port;
      await socket.close();
      final address = 'http://127.0.0.1:$port';
      final binary = '${temporary.path}/floe-server';
      final build = await Process.run('go', [
        'build',
        '-o',
        binary,
        './cmd/floe-server',
      ], workingDirectory: '../../server');
      expect(build.exitCode, 0, reason: build.stderr.toString());
      final process = await Process.start(
        binary,
        [],
        environment: {
          'FLOE_SERVER_DATA': '${temporary.path}/node',
          'FLOE_SERVER_ADDRESS': '127.0.0.1:$port',
          'FLOE_INFERENCE_CONFIG': '',
        },
      );
      final http = HttpClient()..findProxy = (_) => 'DIRECT';
      addTearDown(() async {
        http.close(force: true);
        process.kill(ProcessSignal.sigterm);
        await process.exitCode.timeout(const Duration(seconds: 10));
        await temporary.delete(recursive: true);
      });
      await process.stderr
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .firstWhere((line) => line.contains('listening'))
          .timeout(const Duration(seconds: 10));
      Cookie? cookie;
      var csrf = '';
      Future<Map<String, dynamic>> manage(
        String path, [
        Map<String, Object?>? body,
      ]) async {
        final request = await http.openUrl(
          body == null ? 'GET' : 'POST',
          Uri.parse('$address/manage/api/$path'),
        );
        request.headers.contentType = ContentType.json;
        request.headers.set('Origin', address);
        request.headers.set('X-Floe-CSRF', csrf);
        if (cookie != null) request.cookies.add(cookie!);
        if (body != null) request.write(jsonEncode(body));
        final response = await request.close();
        expect(response.statusCode, 200);
        if (response.cookies.isNotEmpty) cookie = response.cookies.first;
        return jsonDecode(await utf8.decoder.bind(response).join())
            as Map<String, dynamic>;
      }

      final admin = await File('${temporary.path}/node/admin-token')
          .readAsString();
      await manage('login', {'token': admin});
      csrf = (await manage('state'))['csrf'] as String;
      final store = MemoryServerCredentials();
      final client = LocalServerClient(store: store);
      final pair = await client.request(address, '/pair/start', body: {});
      final pending = await client.request(
        address,
        '/pair/poll',
        body: {'proof': pair['proof']},
      );
      expect(pending['status'], 'pending');
      expect(pending.containsKey('token'), false);
      await manage('pair/approve', {'id': pair['id']});
      final approved = await client.request(
        address,
        '/pair/poll',
        body: {'proof': pair['proof']},
      );
      final connection = ServerConnection(
        address: address,
        token: approved['token'] as String,
        clientId: approved['client_id'] as String,
      );
      await client.save(connection);
      await client.checkConnection(connection);
      await manage('client/delete', {'id': connection.clientId});
      await expectLater(
        client.checkConnection(connection),
        throwsA(
          isA<ServerConnectionException>().having(
            (error) => error.code,
            'code',
            'authorization_required',
          ),
        ),
      );
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
