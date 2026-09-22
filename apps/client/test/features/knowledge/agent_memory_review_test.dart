import 'package:floe_client/app/runtime/local_owner_gateways.dart';

import '../../support/app_wire_transport.dart';

import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native gateway reads and decides pending memory candidates', () async {
    Map<String, Object?>? submittedAction;
    var pending = true;
    final gateway = NativeMemoryGateway(
      CallbackAppWireTransport((request) async {
        final operation = Map<String, Object?>.from(
          (request['command'] ?? request['query']) as Map,
        );
        if (!(operation['kind'] as String).endsWith('.read_result')) {
          submittedAction = operation;
          if (submittedAction!['decision'] != null) pending = false;
        }
        return {
          'kind': 'knowledge_operation',
          'operation_id': ownerOperationId(request),
          'events': <Object?>[],
          'next_sequence': 0,
          'done': true,
          'state': 'ready',
          'memory_review': {
            'schema_version': 1,
            'person_id': 'person-1',
            'candidates': pending ? [_candidate] : <Object?>[],
          },
          'failure': null,
        };
      }),
    );

    final review = await gateway.readMemoryReview('person-1');
    expect(submittedAction, {'kind': 'knowledge.memory.review'});
    expect(review.candidates.single.statement, 'Prefers focused mornings');
    expect(review.candidates.single.sourceCount, 1);

    final decided = await gateway.decideMemoryCandidate(
      personId: 'person-1',
      candidateId: 'candidate-1',
      decision: AgentMemoryDecision.approve,
    );
    expect(submittedAction, {
      'kind': 'knowledge.memory.decide',
      'candidate_id': 'candidate-1',
      'decision': 'approve',
    });
    expect(decided.candidates, isEmpty);
  });

  test('candidate parser rejects non-pending knowledge', () {
    expect(
      () => AgentMemoryCandidate.fromJson({..._candidate, 'state': 'approved'}),
      throwsFormatException,
    );
  });
}

const _candidate = <String, Object?>{
  'id': 'candidate-1',
  'operation': 'create',
  'state': 'pending',
  'payload': {
    'kind': 'memory',
    'value': {
      'kind': 'preference',
      'statement': 'Prefers focused mornings',
      'epistemic_status': 'fact',
      'confidence_millis': 900,
    },
  },
  'source_refs': [
    {'session_id': 'session-1', 'turn_id': 'turn-1'},
  ],
  'created_at': '2026-09-10T01:00:00Z',
};
