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
}
