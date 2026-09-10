import 'package:floe_client/features/agent/agent_memory_review.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native gateway reads and decides pending memory candidates', () async {
    Map<String, Object?>? submittedAction;
    var pending = true;
    final gateway = NativeAgentVaultGateway((request) async {
      final operation = Map<String, Object?>.from(request['operation']! as Map);
      if (operation['kind'] == 'submit') {
        submittedAction = Map<String, Object?>.from(
          operation['action']! as Map,
        );
        if (submittedAction!['decision'] != null) pending = false;
      }
      return {
        'request_id': request['request_id'],
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
    });

    final review = await gateway.readMemoryReview('person-1');
    expect(submittedAction, {'kind': 'memory_review'});
    expect(review.candidates.single.statement, 'Prefers focused mornings');
    expect(review.candidates.single.sourceCount, 1);

    final decided = await gateway.decideMemoryCandidate(
      personId: 'person-1',
      candidateId: 'candidate-1',
      decision: AgentMemoryDecision.approve,
    );
    expect(submittedAction, {
      'kind': 'memory_review',
      'decision': {'candidate_id': 'candidate-1', 'decision': 'approve'},
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
