import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';

final class AppWireRegistryGateway implements AgentRegistryGateway {
  AppWireRegistryGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async {
    return _observe(
      personId,
      {'kind': 'experts.registry.inspect'},
      decode: (result) {
        final raw = result['registry'];
        if (raw == null) return null;
        final overview = AgentRegistryView.fromJson(
          Map<String, dynamic>.from(raw as Map),
        );
        if (overview.personId != personId) {
          throw const FormatException('Registry Person mismatch');
        }
        return overview;
      },
    );
  }

  @override
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  }) async {
    return _observe(
      current.personId,
      {
        'kind': 'experts.registry.configure',
        'change': {
          'instance_id': current.instanceId,
          'expected_revision': current.revision,
          'target': {'kind': target.wireName, 'id': id, 'enabled': enabled},
        },
      },
      decode: (result) {
        final overview = AgentRegistryView.fromJson(
          Map<String, dynamic>.from(result['registry'] as Map),
        );
        if (overview.personId != current.personId ||
            overview.instanceId != current.instanceId ||
            overview.revision != current.revision + 1) {
          throw const FormatException('Registry configuration mismatch');
        }
        return overview;
      },
    );
  }

  @override
  Future<AgentCandidateCatalog> readCandidates(
    String personId, {
    required String assignmentId,
    required String requirementKey,
  }) => _observe(
    personId,
    {
      'kind': 'experts.sources.candidates',
      'assignment_id': assignmentId,
      'requirement_key': requirementKey,
    },
    decode: (result) => AgentCandidateCatalog.fromJson(
      Map<String, dynamic>.from(result['candidates'] as Map),
    ),
  );

  @override
  Future<AgentCandidateCatalog> replaceSelection(
    String personId, {
    required AgentInstallation installation,
    required AgentExpertDefinition definition,
    required AgentAssignment assignment,
    required AgentSourceRequirement requirement,
    required List<String> candidateIds,
  }) => _observe(
    personId,
    {
      'kind': 'experts.binding.replace',
      'selection': {
        'assignment_id': assignment.id,
        'package_id': installation.packageId,
        'package_version': installation.version,
        'definition_revision': definition.definitionRevision,
        'requirement_key': requirement.key,
        'expected_binding_revision': assignment.bindingRevision,
        'candidate_ids': candidateIds,
      },
    },
    decode: (result) => AgentCandidateCatalog.fromJson(
      Map<String, dynamic>.from(result['candidates'] as Map),
    ),
  );

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{
      'experts.registry.configure',
      'experts.binding.replace',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: switch (intent['kind']) {
        'experts.sources.candidates' => 'expert_candidates',
        'experts.binding.replace' => 'expert_binding',
        _ => 'registry',
      },
      resultKind: 'expert_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'experts.read_result', operationId, release),
      decode: decode,
    );
  }
}
