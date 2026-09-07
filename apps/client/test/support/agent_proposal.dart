import 'dart:async';
import 'dart:convert';

import 'package:floe_client/features/agent/agent_proposal.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';

import 'agent_vault_gateway.dart';
import 'expert_result.dart';

const proposalPerson = '00000000-0000-4000-8000-000000000001';
const proposalSession = '00000000-0000-4000-8000-000000000002';
const proposalCall = '00000000-0000-4000-8000-000000000003';
const proposalExecution = '00000000-0000-4000-8000-000000000004';

Map<String, Object?> proposalEvidence({String dataClass = 'synthetic'}) =>
    expertResultFixture()..addAll({
      'person_id': proposalPerson,
      'invocation_id': proposalCall,
      'data_class': dataClass,
      'action_proposals': [
        {
          'starts_at_unix_ms': 39600000,
          'ends_at_unix_ms': 43200000,
          'view_handle': 'view-fixture',
        },
      ],
    });

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
        {
          'kind': 'capability',
          'turn_id': 'turn-1',
          'call_id': proposalCall,
          'capability_id': 'expert.schedule',
          'input': '{"kind":"propose_focus","focus_minutes":60}',
          'result': {'Ok': jsonEncode(proposalEvidence(dataClass: dataClass))},
        },
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
    if (inspectionFailure case final reason?) throw AgentVaultException(reason);
    return AgentProposalInspection.fromJson(response);
  }
}
