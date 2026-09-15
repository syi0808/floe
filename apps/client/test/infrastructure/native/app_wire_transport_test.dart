import 'dart:io';

import 'package:floe_client/infrastructure/native/native_transport.dart';
import 'package:floe_client/runtime_client/floe_client.dart';
import 'package:floe_client/runtime_client/transport/app_wire_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'real Dart/C ABI v2 binds host identity and rejects route injection',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!Platform.isMacOS || !library.existsSync()) {
        markTestSkipped('macOS cargo build -p floe-ffi is required.');
        return;
      }
      const personId = '00000000-0000-4000-8000-000000000101';
      final support = await Directory.systemTemp.createTemp(
        'floe-app-wire-v2-',
      );
      final personDirectory = Directory('${support.path}/people/$personId');
      await personDirectory.create(recursive: true);
      await File('${support.path}/local_device_id').writeAsString('mac-local');
      final transport = await NativeTransport.open(
        libraryPath: library.path,
        databasePath: '${personDirectory.path}/floe.db',
      );
      final client = FloeClient(transport);
      addTearDown(() async {
        await client.close();
        await support.delete(recursive: true);
      });

      await expectLater(
        client.getCommand('00000000-0000-4000-8000-000000000103'),
        throwsA(
          isA<NativeTransportException>().having(
            (error) => error.code,
            'code',
            'unavailable',
          ),
        ),
      );
      final initialEvents = await client.readEvents();
      final eventCursor =
          (initialEvents as AppEventsResyncRequired).snapshotCursor;
      expect(eventCursor.runtimeEpoch, greaterThan(0));
      final emptyEvents =
          await client.readEvents(after: eventCursor) as AppEventsPage;
      expect(emptyEvents.events, isEmpty);

      final command = client.prepareStartTurn(
        sessionId: '00000000-0000-4000-8000-000000000106',
        expectedRevision: 0,
        text: 'Use the host-selected model route.',
        retryOf: '00000000-0000-4000-8000-000000000108',
      );
      await expectLater(
        client.submitStartTurn(command),
        throwsA(
          isA<NativeTransportException>().having(
            (error) => error.code,
            'code',
            'unavailable',
          ),
        ),
      );

      final start = <String, dynamic>{
        'schema_version': appWireProtocolVersion,
        'request_id': '00000000-0000-4000-8000-000000000104',
        'command_id': '00000000-0000-4000-8000-000000000105',
        'command': <String, dynamic>{
          'kind': 'conversation.start_turn',
          'session_id': '00000000-0000-4000-8000-000000000106',
          'expected_revision': 0,
          'text': 'Use the host-selected model route.',
          'mode': {'kind': 'new_turn'},
        },
      };
      await expectLater(
        transport.commandV2(start),
        throwsA(
          isA<NativeTransportException>().having(
            (error) => error.code,
            'code',
            'unavailable',
          ),
        ),
      );

      final injected = Map<String, dynamic>.from(start);
      injected['request_id'] = '00000000-0000-4000-8000-000000000107';
      injected['command'] = Map<String, dynamic>.from(start['command'] as Map)
        ..['remote_route'] = {'bearer_token': 'must-not-cross-app-wire'};
      await expectLater(
        transport.commandV2(injected),
        throwsA(
          isA<NativeTransportException>().having(
            (error) => error.code,
            'code',
            'validation',
          ),
        ),
      );
    },
  );
}
