import 'dart:async';

import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

import 'agent_vault_gateway.dart';

const registryPerson = '00000000-0000-4000-8000-000000000001';
const registryInstance = '00000000-0000-4000-8000-000000000002';
const registryInstallation = '00000000-0000-4000-8000-000000000003';
const registryAssignment = '00000000-0000-4000-8000-000000000004';

Map<String, dynamic> registryFixture() => {
  'schema_version': 3,
  'person_id': registryPerson,
  'instance_id': registryInstance,
  'revision': 10,
  'installations': [
    {
      'id': registryInstallation,
      'package': {'id': 'floe.schedule', 'version': '1.0.0', 'kind': 'expert'},
      'enabled': true,
    },
  ],
  'definitions': [
    {
      'package': {'id': 'floe.schedule', 'version': '1.0.0', 'kind': 'expert'},
      'definition_revision': 1,
      'name': 'Schedule planning',
      'description':
          'Allows Floe to prepare schedule suggestions for you to review.',
      'domain_tags': ['schedule'],
      'skills': ['planning'],
    },
  ],
  'assignments': [
    {
      'id': registryAssignment,
      'installation_id': registryInstallation,
      'enabled': true,
      'state_revision': 2,
      'completed_invocations': 2,
      'binding_revision': 1,
      'requirements': [],
    },
  ],
};

final class TestRegistryGateway extends TestVaultGateway
    implements AgentRegistryGateway {
  TestRegistryGateway() : super(personId: registryPerson, personal: true) {
    state = AgentVaultState.ready;
  }
  Map<String, dynamic>? snapshot = registryFixture();
  Completer<void>? registryGate;
  String? registryError;
  int reads = 0;
  int changes = 0;
  final candidateId = 'a' * 64;
  List<String> selectedCandidateIds = [];

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async {
    reads++;
    await registryGate?.future;
    if (registryError case final String error) {
      throw AgentVaultException(
        error,
        reloadRequired: error == 'vault_unavailable' || error == 'interrupted',
        sealSession: error == 'vault_unavailable' || error == 'interrupted',
      );
    }
    return snapshot == null ? null : AgentRegistryView.fromJson(snapshot!);
  }

  @override
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  }) async {
    changes++;
    await registryGate?.future;
    if (registryError case final String error) {
      throw AgentVaultException(
        error,
        reloadRequired: error == 'vault_unavailable' || error == 'interrupted',
        sealSession: error == 'vault_unavailable' || error == 'interrupted',
      );
    }
    if (current.revision != snapshot!['revision']) {
      throw const AgentVaultException('conflict');
    }
    final entries =
        snapshot![target == AgentRegistryTarget.assignment
                ? 'assignments'
                : 'installations']
            as List;
    (entries.singleWhere((entry) => (entry as Map)['id'] == id)
            as Map)['enabled'] =
        enabled;
    snapshot!['revision'] = current.revision + 1;
    return AgentRegistryView.fromJson(snapshot!);
  }

  @override
  Future<AgentCandidateCatalog> readCandidates(
    String personId, {
    required String assignmentId,
    required String requirementKey,
  }) async => AgentCandidateCatalog.fromJson({
    'assignment_id': assignmentId,
    'requirement_key': requirementKey,
    'binding_revision':
        ((snapshot!['assignments'] as List).single as Map)['binding_revision'],
    'candidates': [
      {
        'candidate_id': candidateId,
        'title': 'Attention',
        'detail': 'This device',
        'availability': 'available',
        'selected': selectedCandidateIds.contains(candidateId),
      },
    ],
  });

  @override
  Future<AgentCandidateCatalog> replaceSelection(
    String personId, {
    required AgentInstallation installation,
    required AgentExpertDefinition definition,
    required AgentAssignment assignment,
    required AgentSourceRequirement requirement,
    required List<String> candidateIds,
  }) async {
    if (candidateIds.any((id) => id != candidateId) ||
        assignment.bindingRevision !=
            ((snapshot!['assignments'] as List).single
                as Map)['binding_revision']) {
      throw const AgentVaultException('conflict');
    }
    selectedCandidateIds = List.of(candidateIds);
    final entry = (snapshot!['assignments'] as List).single as Map;
    entry['binding_revision'] = assignment.bindingRevision + 1;
    ((entry['requirements'] as List).single as Map)['selected_count'] =
        candidateIds.length;
    snapshot!['revision'] = (snapshot!['revision'] as int) + 1;
    return readCandidates(
      personId,
      assignmentId: assignment.id,
      requirementKey: requirement.key,
    );
  }
}
