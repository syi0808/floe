import 'dart:io';

import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_calendar_experts.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/settings/domain/agent_personal_access.dart';
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
    implements AgentConversationGateway, ConversationRuntimeProvider {
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
  }
  late final ConversationRuntimeGateway? _conversationRuntime;
  @override
  ConversationRuntimeGateway? get conversationRuntime => _conversationRuntime;

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

final class NativeCalendarExpertGateway implements AgentCalendarExpertGateway {
  NativeCalendarExpertGateway(this._transport, {required this.deviceId});

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();
  final String deviceId;

  @override
  Future<AgentCalendarExperts> readCalendarExperts(String personId) async {
    return _observe(
      personId,
      {'kind': 'experts.calendar.inspect'},
      decode: (result) {
        return _calendarExperts(personId, result);
      },
    );
  }

  @override
  Future<CalendarSubjectPreview> previewCalendarSubject(
    CalendarSubjectPreviewRequest request,
  ) async {
    if (request.deviceId != deviceId) {
      throw const FormatException('Calendar preview device mismatch');
    }
    return _observe(
      request.personId,
      {'kind': 'access.calendar.preview', 'request': request.toJson()},
      decode: (result) {
        if (result['state'] != 'ready' ||
            result['calendar_subject_preview'] is! Map) {
          throw const FormatException('Missing Calendar subject preview');
        }
        final preview = CalendarSubjectPreview.fromJson(
          result['calendar_subject_preview'],
        );
        final expectedCalendarIds = [...request.calendarIds]..sort();
        if (preview.provider != request.provider ||
            preview.deviceId != request.deviceId ||
            preview.connectionId != request.connectionId ||
            preview.connectionScope != request.connectionScope ||
            !listEquals(preview.calendarIds, expectedCalendarIds) ||
            preview.sourceAuthority != request.sourceAuthority) {
          throw const FormatException('Calendar preview identity mismatch');
        }
        return preview;
      },
    );
  }

  @override
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup setup,
  ) async {
    if (setup.deviceId != deviceId) {
      throw const FormatException('Calendar setup device mismatch');
    }
    final serialized = setup.toJson();
    return _observe(
      setup.personId,
      {'kind': 'experts.calendar.install', 'setup': serialized},
      decode: (result) {
        final overview = _calendarExperts(setup.personId, result);
        if (overview.receiptFor(setup) == null) {
          throw const FormatException('Missing Calendar setup receipt');
        }
        return overview;
      },
    );
  }

  @override
  Future<AgentCalendarExperts> configureCalendarAccess(
    AgentCalendarAccessRequest request,
  ) async {
    if (request.operation == AgentCalendarAccessOperation.setScope &&
        request.deviceId != deviceId) {
      throw const FormatException('Calendar scope device mismatch');
    }
    final serialized = request.toJson();
    return _observe(
      request.personId,
      {'kind': 'access.calendar.configure', 'change': serialized},
      decode: (result) {
        return _calendarExperts(request.personId, result);
      },
    );
  }

  AgentCalendarExperts _calendarExperts(
    String personId,
    Map<String, dynamic> result,
  ) {
    final overview = AgentCalendarExperts.fromJson(
      Map<String, dynamic>.from(result['calendar_experts'] as Map),
    );
    if (overview.registry.personId != personId || result['state'] != 'ready') {
      throw const FormatException('Calendar Expert Person or vault mismatch');
    }
    return overview;
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final access = (intent['kind']! as String).startsWith('access.');
    final command = const <String>{
      'experts.calendar.install',
      'access.calendar.configure',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: const {
        "experts.calendar.inspect": "calendar_experts",
        "experts.calendar.install": "calendar_experts",
        "access.calendar.configure": "calendar_access",
        "access.calendar.preview": "calendar_subject_preview",
      }[intent['kind']]!,
      resultKind: access ? 'local_access_operation' : 'expert_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        access ? 'access.local.read_result' : 'experts.read_result',
        operationId,
        release,
      ),
      decode: decode,
    );
  }
}

final class NativePersonalAccessGateway implements AgentPersonalAccessGateway {
  NativePersonalAccessGateway(this._transport, {required this.deviceId});

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();
  final String deviceId;

  @override
  Future<PersonalAccessOverview> inspectPersonalAttention(
    String personId,
  ) async {
    return _personalAccess(personId, {'kind': 'inspect'});
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalAttention(
    String personId, {
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null) {
      throw const FormatException('Attention preview unavailable');
    }
    if (reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Attention review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    });
  }

  @override
  Future<PersonalAccessOverview> setPersonalAttentionEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    });
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalFeasibility(
    String personId,
  ) async {
    return _personalAccess(personId, {
      'kind': 'inspect',
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalFeasibility(
    String personId, {
    required PersonalFeasibilityQuery query,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null ||
        reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Feasibility review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
      'feasibility_query': query.toJson(),
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> setPersonalFeasibilityEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalWellbeing(
    String personId,
  ) async {
    return _personalAccess(personId, {
      'kind': 'inspect',
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalWellbeing(
    String personId, {
    required PersonalAccessOverview reviewedPreview,
    required String nativeSubjectFingerprint,
  }) async {
    if (reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        !RegExp(r'^[0-9a-f]{64}$').hasMatch(nativeSubjectFingerprint)) {
      throw const FormatException('Wellbeing review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': nativeSubjectFingerprint,
      'consumers': const ['assistant'],
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> setPersonalWellbeingEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalContacts(
    String personId,
    List<String> selectedHandles,
  ) async {
    final handles = _canonicalContactHandles(selectedHandles);
    return _personalContacts(personId, {
      'kind': 'inspect',
      'selected_handles': handles,
    });
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalContacts(
    String personId, {
    required List<String> selectedHandles,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null ||
        reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Contacts review scope changed');
    }
    return _personalContacts(personId, {
      'kind': 'review',
      'selected_handles': _canonicalContactHandles(selectedHandles),
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    });
  }

  Future<PersonalAccessOverview> _personalAccess(
    String personId,
    Map<String, Object?> change, {
    String connector = 'attention.macos',
  }) async {
    return _observe(
      personId,
      {
        'kind': change['kind'] == 'inspect'
            ? 'access.personal.inspect'
            : 'access.personal.configure',
        'connector': connector,
        if (change['kind'] != 'inspect') 'change': change,
      },
      decode: (result) {
        if (result['state'] != 'ready' || result['personal_access'] is! Map) {
          throw const FormatException('Missing personal access overview');
        }
        final overview = PersonalAccessOverview.fromJson(
          result['personal_access'],
        );
        if (overview.personId != personId || overview.deviceId != deviceId) {
          throw const FormatException('Personal access scope mismatch');
        }
        return overview;
      },
    );
  }

  Future<PersonalAccessOverview> _personalContacts(
    String personId,
    Map<String, Object?> change,
  ) async {
    return _observe(
      personId,
      {
        'kind': change['kind'] == 'inspect'
            ? 'access.contacts.inspect'
            : 'access.contacts.configure',
        'connector': 'contacts.$platformContactsConnector',
        if (change['kind'] == 'inspect')
          'selected_handles': change['selected_handles'],
        if (change['kind'] != 'inspect') 'change': change,
      },
      decode: (result) {
        if (result['state'] != 'ready' || result['personal_access'] is! Map) {
          throw const FormatException('Missing Contacts access overview');
        }
        final overview = PersonalAccessOverview.fromJson(
          result['personal_access'],
        );
        if (overview.personId != personId || overview.deviceId != deviceId) {
          throw const FormatException('Contacts access scope mismatch');
        }
        return overview;
      },
    );
  }

  String get platformContactsConnector => Platform.isAndroid
      ? 'android'
      : Platform.isIOS
      ? 'apple'
      : 'unsupported';

  List<String> _canonicalContactHandles(List<String> handles) {
    final value = handles.toSet().toList()..sort();
    if (value.isEmpty ||
        value.length > 64 ||
        value.length != handles.length ||
        value.any(
          (handle) => handle.isEmpty || handle.contains(RegExp(r'\s')),
        )) {
      throw const FormatException('Invalid Contacts selection');
    }
    return List.unmodifiable(value);
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{
      'access.personal.configure',
      'access.contacts.configure',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: const {
        "access.personal.inspect": "personal_access",
        "access.personal.configure": "personal_access",
        "access.contacts.inspect": "contacts_access",
        "access.contacts.configure": "contacts_access",
      }[intent['kind']]!,
      resultKind: 'local_access_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'access.local.read_result',
        operationId,
        release,
      ),
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

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{'experts.registry.configure'}
        .contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: 'registry',
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
