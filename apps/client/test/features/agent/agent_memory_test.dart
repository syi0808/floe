import 'package:floe_client/features/agent/agent_memory.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native gateway reads a bounded saved memory overview', () async {
    Map<String, Object?>? submittedAction;
    final gateway = NativeAgentVaultGateway((request) async {
      final operation = Map<String, Object?>.from(request['operation']! as Map);
      if (operation['kind'] == 'submit') {
        submittedAction = Map<String, Object?>.from(
          operation['action']! as Map,
        );
      }
      return {
        'request_id': request['request_id'],
        'events': <Object?>[],
        'next_sequence': 0,
        'done': true,
        'state': 'ready',
        'memory': _overview,
        'failure': null,
      };
    }, deviceId: 'test-device');

    final overview = await gateway.readMemory('person-1');

    expect(submittedAction, {'kind': 'memory'});
    expect(overview.savedCount, 1);
    expect(overview.pendingCount, 2);
    expect(overview.memories.single.statement, 'Prefers focused mornings');
    expect(overview.memories.single.category, 'Preference');
    expect(overview.memories.single.origin, AgentMemoryOrigin.learned);
  });

  test('saved memory parser rejects duplicates and invalid metadata', () {
    final memory = Map<String, Object?>.from(
      (_overview['memories']! as List).single as Map,
    );
    expect(
      () => AgentMemoryOverview.fromJson({
        ..._overview,
        'memories': [memory, memory],
        'saved_count': 2,
      }),
      throwsFormatException,
    );
    expect(
      () => AgentMemory.fromJson({...memory, 'source_count': 0}),
      throwsFormatException,
    );
  });
}

const _overview = <String, Object?>{
  'schema_version': 1,
  'person_id': 'person-1',
  'saved_count': 1,
  'pending_count': 2,
  'memories': [
    {
      'target_id': 'memory-1',
      'revision': 2,
      'statement': 'Prefers focused mornings',
      'memory_kind': 'preference',
      'epistemic_status': 'fact',
      'confidence_millis': 900,
      'source_count': 1,
      'origin': 'learned',
      'created_at': '2026-09-10T01:00:00Z',
    },
  ],
};
