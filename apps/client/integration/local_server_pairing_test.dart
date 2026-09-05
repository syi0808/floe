import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/focus_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';

import '../test/support/server_credentials.dart';

void main() {
  test('paired address and credential route Flutter → Rust → Go without environment token', () async {
    final temporary = await Directory.systemTemp.createTemp('floe-pairing-');
    final socket = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = socket.port;
    await socket.close();
    final address = 'http://127.0.0.1:$port';
    final upstream = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final contexts = <Map<String, dynamic>>[];
    upstream.listen((request) async {
      final payload =
          jsonDecode(await utf8.decoder.bind(request).join()) as Map;
      final context = jsonDecode(
        payload['messages'][1]['content'] as String,
      ) as Map<String, dynamic>;
      contexts.add(context);
      final slot = (context['slots'] as List).last as Map;
      request.response.headers.contentType = ContentType.json;
      request.response.write(
        jsonEncode({
          'choices': [
            {
              'finish_reason': 'stop',
              'message': {
                'content': jsonEncode({
                  'slot_id': slot['id'],
                  'reason': 'A supplied available interval.',
                  'source_ids': context['required_source_ids'],
                }),
              },
            },
          ],
        }),
      );
      await request.response.close();
    });
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
    FfiDayGateway? gateway;
    final http = HttpClient()..findProxy = (_) => 'DIRECT';
    addTearDown(() async {
      await gateway?.close();
      http.close(force: true);
      process.kill(ProcessSignal.sigterm);
      await process.exitCode.timeout(const Duration(seconds: 10));
      await upstream.close(force: true);
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
    await manage('target', {
      'id': 'fixture-focus',
      'provider': 'openai_compatible',
      'model': 'synthetic-model',
      'base_url': 'http://127.0.0.1:${upstream.port}/v1',
      'api_key': '',
    });
    await manage('route', {
      'inference_class': 'high_effort',
      'target': 'fixture-focus',
      'reasoning_effort': 'high',
    });
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
    expect((await client.inferenceClasses(connection)).keys, ['high_effort']);
    final now = DateTime.utc(2026, 9, 5);
    final query = DayQuery(
      personId: localPersonId,
      date: now,
      now: now,
      timezoneOffsetSeconds: 32400,
    );
    gateway = await FfiDayGateway.open(
      libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
      databasePath: '${temporary.path}/focus.db',
      clock: () => now,
      serverClient: client,
    );
    await expectLater(
      gateway.suggestFocus(query),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'external_transfer_denied',
        ),
      ),
    );
    expect(contexts, isEmpty);
    final proposal = await gateway.suggestFocus(query, allowExternal: true);
    expect(proposal.inferenceClass, 'high_effort');
    expect(contexts.length, 1);
    expect(jsonEncode(contexts), isNot(contains(connection.token)));
    expect(jsonEncode(contexts), isNot(contains(localPersonId)));
    expect((await gateway.loadDay(query)).items, isEmpty);
    await manage('client/delete', {'id': connection.clientId});
    await expectLater(
      client.inferenceClasses(connection),
      throwsA(
        isA<ServerConnectionException>().having(
          (error) => error.code,
          'code',
          'authorization_required',
        ),
      ),
    );
  }, timeout: const Timeout(Duration(minutes: 2)));
}
