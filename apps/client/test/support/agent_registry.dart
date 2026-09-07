import 'dart:async';

import 'package:floe_client/features/agent/agent_registry.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';

import 'agent_vault_gateway.dart';

const registryPerson = '00000000-0000-4000-8000-000000000001';
const registryInstance = '00000000-0000-4000-8000-000000000002';
const registryInstallation = '00000000-0000-4000-8000-000000000003';
const registryAssignment = '00000000-0000-4000-8000-000000000004';

Map<String, dynamic> registryFixture() => {
  'schema_version': 1,
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
  'assignments': [
    {
      'id': registryAssignment,
      'installation_id': registryInstallation,
      'enabled': true,
      'granted_tool_count': 1,
      'granted_view_count': 1,
      'state_revision': 2,
      'completed_invocations': 2,
    },
  ],
};

final class TestRegistryGateway extends TestVaultGateway
    implements AgentRegistryGateway {
  TestRegistryGateway() : super(personId: registryPerson) {
    state = AgentVaultState.ready;
  }
  Map<String, dynamic>? snapshot = registryFixture();
  Completer<void>? registryGate;
  String? registryError;
  int reads = 0;
  int changes = 0;

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async {
    reads++;
    await registryGate?.future;
    if (registryError case final String error) throw AgentVaultException(error);
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
    if (registryError case final String error) throw AgentVaultException(error);
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
}
