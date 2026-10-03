import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/actions/domain/agent_proposal.dart';

final class AppWireProposalGateway implements AgentProposalGateway {
  AppWireProposalGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentProposalInspection> inspectProposal({
    required String personId,
    required String sessionId,
    required String invocationId,
  }) async {
    return _observe(
      personId,
      {
        'kind': 'actions.proposal.inspect',
        'session_id': sessionId,
        'invocation_id': invocationId,
      },
      decode: (result) {
        final inspection = AgentProposalInspection.fromJson(
          Map<String, dynamic>.from(result['proposal'] as Map),
        );
        if (result['state'] != 'ready' ||
            inspection.personId != personId ||
            inspection.sessionId != sessionId ||
            inspection.invocationId != invocationId) {
          throw const FormatException('Proposal inspection scope mismatch');
        }
        return inspection;
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{}.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: 'inspect_proposal',
      resultKind: 'action_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'actions.read_result', operationId, release),
      decode: decode,
    );
  }
}
