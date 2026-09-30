import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/actions/domain/agent_proposal.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';

import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';

final class NativeConnectionsGateway implements AgentConnectionsGateway {
  NativeConnectionsGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<List<AgentConnection>> readConnections(String personId) async {
    return _observe(
      personId,
      {'kind': 'connections.overview'},
      decode: (result) {
        final raw = result['connections'];
        if (raw is! List || raw.length > 64) {
          throw const FormatException('Invalid connection overview');
        }
        return List.unmodifiable(
          raw.map(
            (entry) => AgentConnection.fromJson(
              Map<String, dynamic>.from(entry as Map),
            ),
          ),
        );
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
      stage: 'connections',
      resultKind: 'connections_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'connections.read_result',
        operationId,
        release,
      ),
      decode: decode,
    );
  }
}

final class NativeMemoryGateway
    implements AgentMemoryGateway, AgentMemoryReviewGateway {
  NativeMemoryGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentMemoryOverview> readMemory(String personId) async {
    return _observe(
      personId,
      {'kind': 'knowledge.memory.overview'},
      decode: (result) {
        final memory = AgentMemoryOverview.fromJson(
          Map<String, Object?>.from(result['memory'] as Map),
        );
        if (result['state'] != 'ready' || memory.personId != personId) {
          throw const FormatException('Memory overview scope mismatch');
        }
        return memory;
      },
    );
  }

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId) =>
      _memoryReview(personId, null);

  @override
  Future<AgentMemoryReviewOverview> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  }) => _memoryReview(personId, {
    'candidate_id': candidateId,
    'decision': decision.name,
  });

  Future<AgentMemoryReviewOverview> _memoryReview(
    String personId,
    Map<String, Object?>? decision,
  ) async {
    return _observe(
      personId,
      {
        'kind': decision == null
            ? 'knowledge.memory.review'
            : 'knowledge.memory.decide',
        ...?decision,
      },
      decode: (result) {
        final review = AgentMemoryReviewOverview.fromJson(
          Map<String, Object?>.from(result['memory_review'] as Map),
        );
        if (result['state'] != 'ready' || review.personId != personId) {
          throw const FormatException('Memory review scope mismatch');
        }
        return review;
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{'knowledge.memory.decide'}
        .contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: const {
        "knowledge.memory.overview": "memory",
        "knowledge.memory.review": "memory_review",
        "knowledge.memory.decide": "memory_review",
      }[intent['kind']]!,
      resultKind: 'knowledge_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'knowledge.read_result',
        operationId,
        release,
      ),
      decode: decode,
    );
  }
}

final class NativeConversationSessionGateway
    implements
        AgentConversationGateway,
        ConversationRuntimeProvider,
        AgentInteractionProvider {
  NativeConversationSessionGateway(
    this._transport, {
    FloeClient? runtimeClient,
    AppReadModel? readModel,
    Future<void> Function()? beforeConversationStart,
  }) {
    if ((runtimeClient == null) != (readModel == null)) {
      throw ArgumentError('Runtime client and read model must be paired.');
    }
    if (beforeConversationStart != null && runtimeClient == null) {
      throw ArgumentError(
        'Conversation start guards require a runtime client.',
      );
    }
    _conversationRuntime = runtimeClient == null
        ? null
        : NativeConversationRuntimeGateway(
            client: runtimeClient,
            readModel: readModel!,
            loadSession: loadConversation,
            beforeStartTurn: beforeConversationStart,
          );
    _interactionGateway = runtimeClient == null
        ? null
        : NativeAgentInteractionGateway(runtimeClient);
  }
  late final ConversationRuntimeGateway? _conversationRuntime;
  @override
  ConversationRuntimeGateway? get conversationRuntime => _conversationRuntime;
  late final AgentInteractionGateway? _interactionGateway;
  @override
  AgentInteractionGateway? get interactionGateway => _interactionGateway;

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentSession> startConversation(String personId) =>
      _conversationSession(personId, {'kind': 'start'});

  @override
  Future<AgentSession> resumeConversation(String personId) =>
      _conversationSession(personId, {'kind': 'resume'});

  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      _conversationSession(personId, {'kind': 'get', 'session_id': sessionId});

  @override
  Future<AgentSession> recoverConversation(AgentSession session) =>
      _conversationSession(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<AgentSession> _conversationSession(
    String personId,
    Map<String, Object?> operation,
  ) async {
    return _observe(
      personId,
      {...operation, 'kind': 'conversation.session.${operation['kind']}'},
      decode: (result) {
        final session = AgentSession.fromJson(
          Map<String, Object?>.from(result['session'] as Map),
        );
        if (result['state'] != 'ready' ||
            session.personId != personId ||
            session.scope != null ||
            session.dataClasses.singleOrNull != 'personal') {
          throw const FormatException('Conversation session mismatch');
        }
        return session;
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{
      'conversation.session.start',
      'conversation.session.resume',
      'conversation.session.recover',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: 'conversation_session',
      resultKind: 'conversation_session_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'conversation.session.read_result',
        operationId,
        release,
      ),
      decode: decode,
    );
  }
}

final class NativeProposalGateway implements AgentProposalGateway {
  NativeProposalGateway(this._transport);

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

final class NativeRegistryGateway implements AgentRegistryGateway {
  NativeRegistryGateway(this._transport);

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

final class NativeVaultLifecycleGateway implements AgentVaultGateway {
  NativeVaultLifecycleGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentVaultState> vaultStatus(String personId) =>
      _access(personId, 'status');
  @override
  Future<AgentVaultState> createVault(String personId) =>
      _access(personId, 'create');
  @override
  Future<AgentVaultState> unlockVault(String personId) =>
      _access(personId, 'unlock');
  @override
  Future<void> lockVault(String personId) async {
    await _access(personId, 'lock');
  }

  Future<AgentVaultState> _access(String personId, String kind) async {
    return _observe(
      personId,
      {'kind': 'vault.$kind'},
      decode: (result) {
        return AgentVaultState.values.byName(result['state'] as String);
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{
      'vault.create',
      'vault.unlock',
      'vault.lock',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: (intent['kind']! as String).split('.').last,
      resultKind: 'vault_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'vault.read_result', operationId, release),
      decode: decode,
    );
  }
}
