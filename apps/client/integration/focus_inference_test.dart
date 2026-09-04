import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/focus_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/domain/focus_models.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/server/local_server_client.dart';

import '../test/support/server_credentials.dart';

void main() {
  test('Flutter → Rust → Go → fixture model → validated proposal', () async {
    final token = Platform.environment['FLOE_INFERENCE_TOKEN'];
    expect(
      token,
      isNotNull,
      reason: 'Run with a dedicated fixture gateway token.',
    );
    final temporary = await Directory.systemTemp.createTemp('floe-inference-');
    final upstream = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final contexts = <Map<String, dynamic>>[];
    var invalid = false;
    upstream.listen((request) async {
      expect(request.uri.path, '/v1/chat/completions');
      final payload =
          jsonDecode(await utf8.decoder.bind(request).join()) as Map;
      final context = jsonDecode(
        payload['messages'][1]['content'] as String,
      ) as Map<String, dynamic>;
      contexts.add(context);
      final slot = (context['slots'] as List).last as Map;
      final candidate = {
        'slot_id': invalid ? 'invented-slot' : slot['id'],
        'reason': 'This supplied interval avoids the loaded busy times.',
        'source_ids': context['required_source_ids'],
      };
      request.response.headers.contentType = ContentType.json;
      request.response.write(
        jsonEncode({
          'choices': [
            {
              'finish_reason': 'stop',
              'message': {'content': jsonEncode(candidate)},
            },
          ],
        }),
      );
      await request.response.close();
    });
    final config = File('${temporary.path}/config.json');
    await config.writeAsString(
      jsonEncode({
        'targets': {
          'fixture': {
            'provider': 'openai_compatible',
            'base_url': 'http://127.0.0.1:${upstream.port}/v1',
            'model': 'synthetic-model',
          },
        },
      }),
    );
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
        'FLOE_INFERENCE_CONFIG': config.path,
        'FLOE_INFERENCE_TOKEN': token!,
      },
    );
    final stderr = process.stderr
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .asBroadcastStream();
    FfiDayGateway? gateway;
    addTearDown(() async {
      await gateway?.close();
      process.kill(ProcessSignal.sigterm);
      await process.exitCode.timeout(const Duration(seconds: 10));
      await upstream.close(force: true);
      await temporary.delete(recursive: true);
    });
    await stderr
        .firstWhere((line) => line.contains('listening'))
        .timeout(const Duration(seconds: 10));
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
      serverClient: LocalServerClient(store: MemoryServerCredentials()),
    );
    await gateway.saveFocusPreference(
      query,
      0,
      const FocusPreferenceValue(
        startMinute: 600,
        endMinute: 900,
        durationMinutes: 45,
      ),
    );
    await expectLater(
      gateway.suggestFocus(query, 'fixture'),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'external_transfer_denied',
        ),
      ),
    );
    expect(contexts, isEmpty);
    final proposal = await gateway.suggestFocus(
      query,
      'fixture',
      allowExternal: true,
    );
    expect(proposal.personId, localPersonId);
    expect(proposal.endsAt.difference(proposal.startsAt).inMinutes, 45);
    expect(
      proposal.evidence.any((source) => source.id == 'preference'),
      isTrue,
    );
    expect(contexts.single['preference']['duration_minutes'], 45);
    expect(jsonEncode(contexts.single), isNot(contains(localPersonId)));
    expect(jsonEncode(contexts.single), isNot(contains(token)));
    expect((await gateway.loadDay(query)).items, isEmpty);
    await gateway.saveFocusPreference(query, 1, null);
    final withoutPreference = await gateway.suggestFocus(
      query,
      'fixture',
      allowExternal: true,
    );
    expect(contexts.last['preference'], isNull);
    expect(
      withoutPreference.evidence.any((source) => source.id == 'preference'),
      isFalse,
    );
    invalid = true;
    await expectLater(
      gateway.suggestFocus(query, 'fixture', allowExternal: true),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'invalid_proposal',
        ),
      ),
    );
    expect((await gateway.loadDay(query)).items, isEmpty);
  }, timeout: const Timeout(Duration(minutes: 2)));
}
