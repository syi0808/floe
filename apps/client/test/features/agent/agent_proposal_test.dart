import 'dart:async';
import 'dart:convert';

import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_expert_result.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_proposal.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_proposal.dart';

void main() {
  test('proposal evidence matches one exact focus insight and explicit session class', () {
    AgentExpertResult? parse(
      Map<String, Object?> evidence, {
      List<String> classes = const ['synthetic'],
    }) => AgentExpertResult.tryParse(
      jsonEncode(evidence),
      callId: proposalCall,
      personId: proposalPerson,
      allowedDataClasses: classes,
    );
    expect(parse(proposalEvidence())!.proposal!.start!.hour, 11);
    expect(parse(proposalEvidence(dataClass: 'personal')), isNull);
    expect(
      parse(
        proposalEvidence(dataClass: 'personal'),
        classes: ['personal'],
      )!.proposal,
      isNotNull,
    );
    expect(
      parse(
        proposalEvidence(dataClass: 'raw_sensitive'),
        classes: ['raw_sensitive'],
      ),
      isNull,
    );
    final original =
        (proposalEvidence()['action_proposals'] as List).single as Map;
    for (final proposals in [
      [original, original],
      [
        {...original, 'view_handle': 'foreign'},
      ],
      [
        {...original, 'ends_at_unix_ms': 43200001},
      ],
      [
        {...original, 'starts_at_unix_ms': 39600001},
      ],
      [
        {...original, 'execute': true},
      ],
    ]) {
      expect(
        parse(proposalEvidence()..['action_proposals'] = proposals),
        isNull,
      );
    }
  });

  test('native inspection distinguishes explicit absence, malformed response and failures', () async {
    final operations = <Map<String, Object?>>[];
    var response = inspectionJson();
    var includeProposal = true;
    String? failure;
    final gateway = NativeAgentVaultGateway((request) async {
      operations.add(request['operation']! as Map<String, Object?>);
      return {
        'request_id': request['request_id'],
        'done': true,
        'events': [],
        'next_sequence': 0,
        'state': 'ready',
        'failure': failure,
        if (includeProposal) 'proposal': response,
      };
    });
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
          .where((entry) => entry['kind'] == 'submit')
          .every(
            (entry) => (entry['action'] as Map)['kind'] == 'inspect_proposal',
          ),
      true,
    );
    expect(
      operations.every(
        (entry) => ['submit', 'release'].contains(entry['kind']),
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
    expect(controller.expertResult(message)!.proposal, isNotNull);
    await controller.inspectProposal(message);
    gateway.inspectionGate = Completer<void>();
    final inspection = controller.inspectProposal(message);
    final locking = controller.closeView();
    expect(controller.messages, isEmpty);
    expect(controller.proposalFor(proposalCall), isNull);
    expect(controller.expertResult(message), isNull);
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
