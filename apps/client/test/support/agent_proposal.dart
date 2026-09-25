import 'dart:async';

import 'package:floe_client/features/actions/domain/agent_proposal.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

import 'agent_vault_gateway.dart';

const proposalPerson = '00000000-0000-4000-8000-000000000001';
const proposalSession = '00000000-0000-4000-8000-000000000002';
const proposalCall = '00000000-0000-4000-8000-000000000003';
const proposalExecution = '00000000-0000-4000-8000-000000000004';

Map<String, Object?> proposalDelegation() => {
  'kind': 'delegation',
  'turn_id': 'turn-1',
  'task': {
    'id': proposalCall,
    'agent_id': 'floe.builtin.schedule',
    'state': 'completed',
    'result': 'One focus window is available for review.',
    'failure': null,
    'artifacts': [
      {
        'artifact_id': proposalExecution,
        'name': 'Calendar proposal',
        'parts': [
          {
            'kind': 'data',
            'media_type': 'application/vnd.floe.actions.calendar-proposal+json;version=1',
            'data': '{}',
          },
        ],
      },
    ],
  },
};

Map<String, dynamic> inspectionJson({String? status = 'pending'}) => {
  'schema_version': 1,
  'person_id': proposalPerson,
  'session_id': proposalSession,
  'invocation_id': proposalCall,
  'action': status == null
      ? null
      : {
          'action_id': proposalCall,
          'execution_id': proposalExecution,
          'status': status,
          'expires_at': '2050-01-01T00:00:00Z',
          'starts_at': '2050-01-02T08:00:00Z',
          'ends_at': '2050-01-02T09:00:00Z',
        },
};

class TestProposalGateway extends TestVaultGateway
    implements AgentProposalGateway {
  TestProposalGateway({String dataClass = 'synthetic'})
    : super(personId: proposalPerson) {
    state = AgentVaultState.ready;
    saved = {
      'schema_version': 1,
      'id': proposalSession,
      'person_id': proposalPerson,
      'revision': 4,
      'active_turn': null,
      'last_outcome': {'status': 'completed'},
      'data_classes': [dataClass],
      'messages': [
        proposalDelegation(),
      ],
    };
  }

  final requests = <(String, String, String)>[];
  Map<String, dynamic> response = inspectionJson();
  String? inspectionFailure;
  Completer<void>? inspectionGate;

  @override
  Future<AgentProposalInspection> inspectProposal({
    required String personId,
    required String sessionId,
    required String invocationId,
  }) async {
    requests.add((personId, sessionId, invocationId));
    await inspectionGate?.future;
    if (inspectionFailure case final reason?) {
      throw AgentVaultException(
        reason,
        reloadRequired: reason == 'vault_unavailable',
        sealSession: reason == 'vault_unavailable',
      );
    }
    return AgentProposalInspection.fromJson(response);
  }
}
