import 'dart:io';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  setUp(AppDiagnostics.clear);

  test('diagnostics retain identifiers without error payloads', () {
    AppDiagnostics.error(
      component: 'agent',
      operation: 'conversation_turn',
      error: StateError('private prompt contents'),
      stackTrace: StackTrace.fromString('agent stack'),
      failure: 'invalid_model_output',
      requestId: 'request-1',
      sessionId: 'session-1',
      retryable: false,
    );

    final json = AppDiagnostics.records.single.toJson();
    expect(json['request_id'], 'request-1');
    expect(json['session_id'], 'session-1');
    expect(json['failure'], 'invalid_model_output');
    expect(json['error_type'], 'StateError');
    expect(json.toString(), isNot(contains('private prompt contents')));
  });

  test('diagnostics use a bounded ring buffer', () {
    for (var index = 0; index < 510; index++) {
      AppDiagnostics.event(component: 'test', operation: 'event-$index');
    }

    expect(AppDiagnostics.records, hasLength(500));
    expect(AppDiagnostics.records.first.operation, 'event-10');
    expect(AppDiagnostics.records.last.operation, 'event-509');
  });

  test('diagnostics persist across initialization', () async {
    final directory = await Directory.systemTemp.createTemp(
      'floe-diagnostics-',
    );
    addTearDown(() async {
      await AppDiagnostics.deleteJournal();
      await directory.delete(recursive: true);
    });

    await AppDiagnostics.initialize(directory: directory);
    AppDiagnostics.event(component: 'test', operation: 'persisted');
    await AppDiagnostics.flush();

    await AppDiagnostics.initialize(directory: directory);
    expect(AppDiagnostics.records.single.operation, 'persisted');
  });

  test('diagnostics rotate bounded journal files', () async {
    final directory = await Directory.systemTemp.createTemp(
      'floe-diagnostics-',
    );
    addTearDown(() async {
      await AppDiagnostics.deleteJournal();
      await directory.delete(recursive: true);
    });

    await AppDiagnostics.initialize(
      directory: directory,
      maxFileBytes: 180,
      maxFiles: 2,
    );
    for (var index = 0; index < 20; index++) {
      AppDiagnostics.event(component: 'test', operation: 'event-$index');
    }
    await AppDiagnostics.flush();

    final files = directory
        .listSync()
        .whereType<File>()
        .map((file) => file.uri.pathSegments.last)
        .where((name) => name.startsWith('incidents.ndjson'))
        .toList();
    expect(files, hasLength(2));
  });

  test('sensitive failure text is excluded from the journal', () async {
    final directory = await Directory.systemTemp.createTemp(
      'floe-diagnostics-',
    );
    addTearDown(() async {
      await AppDiagnostics.deleteJournal();
      await directory.delete(recursive: true);
    });

    await AppDiagnostics.initialize(directory: directory);
    AppDiagnostics.error(
      component: 'agent',
      operation: 'request',
      error: StateError('secret prompt'),
      failure: 'secret prompt and model output',
    );
    await AppDiagnostics.flush();
    final text = await File('${directory.path}/incidents.ndjson')
        .readAsString();
    expect(text, isNot(contains('secret prompt')));
  });

  test('journal write failures stay in memory', () async {
    final directory = Directory('/dev/null/floe-diagnostics');

    await AppDiagnostics.initialize(directory: directory);
    expect(
      () => AppDiagnostics.error(
        component: 'test',
        operation: 'write_failure',
        error: StateError('not persisted'),
      ),
      returnsNormally,
    );
    expect(AppDiagnostics.records, hasLength(1));
  });
}
