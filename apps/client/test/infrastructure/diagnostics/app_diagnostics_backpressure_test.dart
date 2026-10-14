import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory root;
  setUp(() async {
    AppDiagnostics.clear();
    await AppDiagnostics.flush();
    root = await Directory.systemTemp.createTemp('floe-diagnostic-queue-');
  });
  tearDown(() async {
    await AppDiagnostics.deleteJournal();
    await root.delete(recursive: true);
  });

  test(
    'unavailable persistence retains only the latest bounded spool on recovery',
    () async {
      final blocker = await File('${root.path}/blocked')
          .writeAsString('fixture');
      await AppDiagnostics.initialize(directory: Directory(blocker.path));
      _burst(2000);
      expect(AppDiagnostics.records, hasLength(500));
      expect(AppDiagnostics.persistenceStatus.pendingRecords, 500);
      expect(AppDiagnostics.persistenceStatus.droppedRecords, 1500);
      expect(AppDiagnostics.persistenceStatus.available, isFalse);
      final recovered = Directory('${root.path}/recovered');
      await AppDiagnostics.initialize(directory: recovered);
      await AppDiagnostics.flush();
      final rows = await _rows(recovered);
      expect(rows, hasLength(500));
      expect(rows.first['operation'], 'event_1500');
      expect(rows.last['operation'], 'event_1999');
    },
  );

  test('healthy writer coalesces a producer burst instead of retaining one future per record', () async {
    await AppDiagnostics.initialize(directory: root);
    _burst(2000);
    await AppDiagnostics.flush();
    final rows = await _rows(root);
    expect(rows, hasLength(500));
    expect(rows.first['operation'], 'event_1500');
    expect(rows.last['operation'], 'event_1999');
  });

  test(
    'write failure preserves a bounded spool for explicit reinitialization',
    () async {
      await AppDiagnostics.initialize(directory: root);
      final blocker = await Directory('${root.path}/incidents.ndjson').create();
      AppDiagnostics.event(component: 'fixture', operation: 'failed_write');
      await AppDiagnostics.flush();
      _burst(2000);
      await AppDiagnostics.flush();
      await blocker.delete();
      await AppDiagnostics.initialize(directory: root);
      await AppDiagnostics.flush();
      final rows = await _rows(root);
      expect(rows, hasLength(500));
      expect(rows.first['operation'], 'event_1500');
      expect(rows.last['operation'], 'event_1999');
    },
  );
  for (final inFlight in [false, true]) {
    test(
      'clear fences old drain and preserves later records (in-flight=$inFlight)',
      () async {
        await AppDiagnostics.initialize(directory: root);
        _burst(1000);
        if (inFlight) await Future<void>.delayed(Duration.zero);
        AppDiagnostics.clear();
        AppDiagnostics.event(component: 'fixture', operation: 'after_clear');
        await AppDiagnostics.flush();
        final rows = await _rows(root);
        expect(rows.map((row) => row['operation']), ['after_clear']);
        expect(AppDiagnostics.persistenceStatus.droppedRecords, 0);
        expect(AppDiagnostics.persistenceStatus.pendingRecords, 0);
      },
    );
  }

  test(
    'export barrier snapshots drop counts while new producers remain bounded',
    () async {
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      const channel = MethodChannel('plugins.flutter.io/path_provider');
      messenger.setMockMethodCallHandler(channel, (call) async {
        if (call.method == 'getTemporaryDirectory') {
          // The export owns the writer barrier while another burst arrives.
          _burst(2000);
          expect(AppDiagnostics.persistenceStatus.pendingRecords, 500);
          return root.path;
        }
        return null;
      });
      addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
      await AppDiagnostics.initialize(directory: root);
      _burst(2000);
      final file = await AppDiagnostics.exportBundle();
      final bundle = jsonDecode(await file.readAsString()) as Map;
      expect(bundle['persistence']['available'], isTrue);
      expect(bundle['persistence']['dropped_records'], 1500);
      final records = bundle['records'] as List;
      expect(records, hasLength(500));
      expect(records.first['operation'], 'event_1500');
      expect(records.last['operation'], 'event_1999');
      await AppDiagnostics.flush();
      expect(AppDiagnostics.persistenceStatus.pendingRecords, 0);
      expect(
        AppDiagnostics.persistenceStatus.droppedRecords,
        greaterThan(1500),
      );
    },
  );

  test('rotation retains its file-count limit after batched drains', () async {
    await AppDiagnostics.initialize(
      directory: root,
      maxFileBytes: 300,
      maxFiles: 3,
    );
    _burst(20);
    await AppDiagnostics.flush();
    final files = await root
        .list()
        .where((entry) => entry.path.contains('incidents.ndjson'))
        .toList();
    expect(files.length, lessThanOrEqualTo(3));
    final rows = await _rows(root);
    expect(rows.last['operation'], 'event_19');
    expect(AppDiagnostics.persistenceStatus.droppedRecords, 0);
  });
  test('backlog cap does not truncate a healthy journal history', () async {
    await AppDiagnostics.initialize(directory: root);
    for (var batch = 0; batch < 10; batch++) {
      _burst(100);
      await AppDiagnostics.flush();
    }
    expect(await _rows(root), hasLength(1000));
    expect(AppDiagnostics.records, hasLength(500));
    expect(AppDiagnostics.persistenceStatus.pendingRecords, 0);
    expect(AppDiagnostics.persistenceStatus.droppedRecords, 0);
  });
}

void _burst(int count) {
  for (var index = 0; index < count; index++) {
    AppDiagnostics.event(component: 'fixture', operation: 'event_$index');
  }
}

Future<List<Map<String, dynamic>>> _rows(Directory directory) async {
  final file = File('${directory.path}/incidents.ndjson');
  if (!await file.exists()) return [];
  return (await file.readAsLines())
      .map((line) => jsonDecode(line) as Map<String, dynamic>)
      .toList();
}
