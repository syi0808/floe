import 'package:floe_client/app/runtime/local_owner_gateways.dart';

import '../../support/app_wire_transport.dart';

import 'dart:async';

import 'package:floe_client/features/conversation/application/agent_controller.dart';

import 'package:floe_client/features/conversation/domain/agent_session.dart';

import 'package:floe_client/features/actions/domain/agent_proposal.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_proposal.dart';

void main() {
  test('only an Actions artifact marks a delegation as reviewable', () {
    final delegation = proposalDelegation();
    final message = AgentCapabilityMessage.fromDelegation(delegation);
    expect(message.hasArtifactMediaType(
      'application/vnd.floe.actions.calendar-proposal+json;version=1',
    ), true);
    final task = Map<String, Object?>.from(delegation['task'] as Map);
    task['artifacts'] = [
      {
        'artifact_id': proposalExecution,
        'name': 'Package result',
        'parts': [
          {
            'kind': 'data',
            'media_type': 'application/vnd.floe.expert.schedule+json;version=1',
            'data': '{"execute":true}',
          },
        ],
      },
    ];
    final inert = AgentCapabilityMessage.fromDelegation({
      ...delegation,
      'task': task,
    });
    expect(inert.hasArtifactMediaType(
      'application/vnd.floe.actions.calendar-proposal+json;version=1',
    ), false);
  });

  test('native inspection distinguishes explicit absence, malformed response and failures', () async {
    final operations = <Map<String, Object?>>[];
    var response = inspectionJson();
    var includeProposal = true;
    String? failure;
    final gateway = NativeProposalGateway(
      CallbackAppWireTransport((request) async {
        operations.add(Map<String, Object?>.from(request['query'] as Map));
        return {
          'kind': 'action_operation',
          'operation_id': ownerOperationId(request),
          'done': true,
          'events': [],
          'next_sequence': 0,
          'state': 'ready',
          'failure': failure == null
              ? null
              : {
                  'schema_version': 1,
                  'domain': 'app',
                  'category': 'internal',
                  'reason_code': failure,
                  'kind': failure,
                  'stage': 'inspect_proposal',
                  'safe_actions': const <String>['export_diagnostics'],
                  'affected_refs': const <String>[],
                  'incident_id': ownerOperationId(request),
                  'retry_policy': 'never',
                  'retryable': false,
                  'recovery_action': 'none',
                  'reload_required': false,
                  'seal_session': false,
                  'correlation_request_id': ownerOperationId(request),
                },
          if (includeProposal) 'proposal': response,
        };
      }),
    );
    Future<AgentProposalInspection> inspect() => gateway.inspectProposal(
      personId: proposalPerson,
      sessionId: proposalSession,
      invocationId: proposalCall,
    );
    for (final status in [
      null,
      'pending',
      'approved',
      'rejected',
      'executing',
      'blocked',
      'unknown',
      'succeeded',
    ]) {
      response = inspectionJson(status: status);
      expect((await inspect()).action?.status.name, status);
    }
    for (final change in [
      {'person_id': proposalExecution},
      {'session_id': proposalExecution},
      {'invocation_id': proposalExecution},
      {'schema_version': 2},
      {'session_id': 'malformed'},
      {
        'action': {
          ...inspectionJson()['action'] as Map,
          'action_id': proposalExecution,
        },
      },
      {
        'action': {...inspectionJson()['action'] as Map, 'status': 'executed'},
      },
    ]) {
      response = inspectionJson()..addAll(change);
      await expectLater(inspect(), throwsA(anything));
    }
    response = inspectionJson()..remove('action');
    await expectLater(inspect(), throwsA(isA<FormatException>()));
    includeProposal = false;
    await expectLater(inspect(), throwsA(anything));
    failure = 'not_found';
    await expectLater(
      inspect(),
      throwsA(
        isA<AgentVaultException>().having(
          (error) => error.failure,
          'failure',
          'not_found',
        ),
      ),
    );
    expect(
      operations
          .where((entry) => entry['kind'] != 'actions.read_result')
          .every((entry) => entry['kind'] == 'actions.proposal.inspect'),
      true,
    );
    expect(
      operations.every(
        (entry) => [
          'actions.proposal.inspect',
          'actions.read_result',
        ].contains(entry['kind']),
      ),
      true,
    );
  });

  test('controller reads only durable scoped proposals and serializes inspection with chat', () async {
    final gateway = TestProposalGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: proposalPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    final message = controller.messages.single as AgentCapabilityMessage;
    final forged = AgentCapabilityMessage.fromJson({
      'turn_id': message.turnId,
      'call_id': proposalExecution,
      'capability_id': message.capabilityId,
      'input': message.input,
      'result': {'Ok': message.output},
    });
    await controller.inspectProposal(forged);
    expect(gateway.requests, isEmpty);
    gateway.inspectionGate = Completer<void>();
    final operation = controller.inspectProposal(message);
    expect(controller.busy, true);
    expect(controller.canSend, false);
    await controller.inspectProposal(message);
    await controller.load(newSession: true);
    expect(gateway.requests, [(proposalPerson, proposalSession, proposalCall)]);
    gateway.inspectionGate!.complete();
    await operation;
    expect(
      controller.proposalFor(proposalCall)!.action!.executionId,
      proposalExecution,
    );
    expect(gateway.begins, 0);
    expect(controller.messages.single, same(message));
    gateway.response = inspectionJson(status: null);
    await controller.inspectProposal(message);
    expect(controller.proposalFor(proposalCall)!.action, isNull);
    gateway.inspectionFailure = 'storage_unavailable';
    await controller.inspectProposal(message);
    expect(controller.proposalFor(proposalCall), isNull);
    expect(controller.proposalFailureFor(proposalCall), 'storage_unavailable');
    expect(controller.session, isNotNull);
    gateway.inspectionFailure = null;
    gateway.response = inspectionJson()..['session_id'] = proposalExecution;
    await controller.inspectProposal(message);
    expect(controller.proposalFor(proposalCall), isNull);
    gateway.response = inspectionJson();
    await controller.inspectProposal(message);
    await controller.load(newSession: true);
    expect(controller.proposalFor(proposalCall), isNull);
  });

  test('lock hides proposals immediately, drains owned inspection and ignores late results', () async {
    final gateway = TestProposalGateway(dataClass: 'personal');
    final controller = AgentController(
      gateway: gateway,
      personId: proposalPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    final message = controller.messages.single as AgentCapabilityMessage;
    expect(controller.canInspectProposal(message), isTrue);
    await controller.inspectProposal(message);
    gateway.inspectionGate = Completer<void>();
    final inspection = controller.inspectProposal(message);
    final locking = controller.closeView();
    expect(controller.messages, isEmpty);
    expect(controller.proposalFor(proposalCall), isNull);
    expect(controller.canInspectProposal(message), isFalse);
    expect(gateway.locks, 0);
    gateway.inspectionGate!.complete();
    await inspection;
    await locking;
    expect(gateway.locks, 1);
    expect(controller.proposalFor(proposalCall), isNull);
    expect(controller.vaultState, AgentVaultState.locked);
  });

  test('key loss removes the conversation and all proposal metadata', () async {
    final gateway = TestProposalGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: proposalPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    final message = controller.messages.single as AgentCapabilityMessage;
    await controller.inspectProposal(message);
    gateway.inspectionFailure = 'vault_unavailable';
    await controller.inspectProposal(message);
    expect(controller.vaultState, AgentVaultState.unavailable);
    expect(controller.messages, isEmpty);
    expect(controller.proposalFor(proposalCall), isNull);
    expect(controller.proposalFailureFor(proposalCall), isNull);
  });
}
