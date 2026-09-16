import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native transport preserves structured Rust error context', () {
    final error = NativeTransportException.fromEnvelope({
      'status': 'error',
      'error': {
        'code': 'storage',
        'message': 'Agent request could not complete',
        'field': 'request_id',
        'metadata': {
          'agent_failure': 'vault_unavailable',
          'request_id': 'request-1',
          'stage': 'conversation_turn',
        },
      },
    });

    expect(error.code, 'storage');
    expect(error.field, 'request_id');
    expect(error.metadata['agent_failure'], 'vault_unavailable');
    expect(error.metadata['request_id'], 'request-1');
    expect(error.metadata['stage'], 'conversation_turn');
  });
}
