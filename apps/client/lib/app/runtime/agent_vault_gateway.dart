import 'dart:io';

import 'package:flutter/foundation.dart';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
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
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

enum AgentVaultState { missing, locked, ready, unavailable }

class AgentVaultException implements Exception {
  const AgentVaultException(
    this.failure, {
    this.requestId,
    this.stage,
    this.metadata = const {},
    this.recoveryAction,
    this.affectedRefs = const [],
    this.correlationRequestId,
    this.retryableOverride,
    this.domain,
    this.category,
    this.reasonCode,
    this.safeActions = const [],
    this.incidentId,
    this.retryPolicy,
    this.reloadRequired,
    this.sealSession,
  });

  final String failure;
  final String? requestId;
  final String? stage;
  final Map<String, String> metadata;
  final String? recoveryAction;
  final List<String> affectedRefs;
  final String? correlationRequestId;
  final bool? retryableOverride;
  final String? domain;
  final String? category;
  final String? reasonCode;
  final List<String> safeActions;
  final String? incidentId;
  final String? retryPolicy;

  /// Decided by the owner; never re-derived from [recoveryAction].
  final bool? reloadRequired;

  /// Decided by the owner; the client stops applying results when set.
  final bool? sealSession;

  bool get retryable =>
      retryableOverride ??
      const {
        'model_unavailable',
        'local_model_unavailable',
        'server_model_unavailable',
        'server_model_timeout',
        'quota_exceeded',
        'stalled',
        'deadline_exceeded',
        'interrupted',
        'transport_unavailable',
      }.contains(failure);

  @override
  String toString() => requestId == null
      ? 'AgentVaultException($failure)'
      : 'AgentVaultException($failure, requestId: $requestId)';
}

abstract interface class AgentVaultGateway implements AgentConversationGateway {
  Future<AgentVaultState> vaultStatus(String personId);
  Future<AgentVaultState> createVault(String personId);
  Future<AgentVaultState> unlockVault(String personId);
  Future<void> lockVault(String personId);
}

final class NativeAgentVaultGateway
    implements
        AgentVaultGateway,
        AgentRegistryGateway,
        AgentProposalGateway,
        AgentConversationGateway,
        AgentCalendarExpertGateway,
        AgentConnectionsGateway,
        AgentPersonalAccessGateway,
        AgentMemoryGateway,
        AgentMemoryReviewGateway,
        ConversationRuntimeProvider {
  NativeAgentVaultGateway(
    this.request, {
    required this.deviceId,
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

  final Future<Map<String, dynamic>> Function(Map<String, Object?>) request;
  final String deviceId;
  late final ConversationRuntimeGateway? _conversationRuntime;
  @override
  ConversationRuntimeGateway? get conversationRuntime => _conversationRuntime;
  _VaultJob? _pending;

  @override
  Future<List<AgentConnection>> readConnections(String personId) async {
    final result = await _perform(personId, {'kind': 'connections'});
    final raw = result['connections'];
    if (raw is! List || raw.length > 64) {
      throw const FormatException('Invalid connection overview');
    }
    return List.unmodifiable(
      raw.map(
        (entry) =>
            AgentConnection.fromJson(Map<String, dynamic>.from(entry as Map)),
      ),
    );
  }

  @override
  Future<AgentMemoryOverview> readMemory(String personId) async {
    final result = await _perform(personId, {'kind': 'memory'});
    final memory = AgentMemoryOverview.fromJson(
      Map<String, Object?>.from(result['memory'] as Map),
    );
    if (result['state'] != 'ready' || memory.personId != personId) {
      throw const FormatException('Memory overview scope mismatch');
    }
    return memory;
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
    final result = await _perform(personId, {
      'kind': 'memory_review',
      'decision': decision,
    });
    final review = AgentMemoryReviewOverview.fromJson(
      Map<String, Object?>.from(result['memory_review'] as Map),
    );
    if (result['state'] != 'ready' || review.personId != personId) {
      throw const FormatException('Memory review scope mismatch');
    }
    return review;
  }

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
    final result = await _perform(personId, {
      'kind': 'conversation_session',
      'operation': operation,
    });
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
  }

  @override
  Future<AgentProposalInspection> inspectProposal({
    required String personId,
    required String sessionId,
    required String invocationId,
  }) async {
    final result = await _perform(personId, {
      'kind': 'inspect_proposal',
      'session_id': sessionId,
      'invocation_id': invocationId,
    });
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
  }

  @override
  Future<AgentCalendarExperts> readCalendarExperts(String personId) async {
    final result = await _perform(personId, {
      'kind': 'calendar_experts',
      'setup': null,
    });
    return _calendarExperts(personId, result);
  }

  @override
  Future<CalendarSubjectPreview> previewCalendarSubject(
    CalendarSubjectPreviewRequest request,
  ) async {
    if (request.deviceId != deviceId) {
      throw const FormatException('Calendar preview device mismatch');
    }
    final result = await _perform(request.personId, {
      'kind': 'calendar_subject_preview',
      'request': request.toJson(),
    });
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
  }

  @override
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup setup,
  ) async {
    if (setup.deviceId != deviceId) {
      throw const FormatException('Calendar setup device mismatch');
    }
    final serialized = setup.toJson();
    final result = await _perform(setup.personId, {
      'kind': 'calendar_experts',
      'setup': serialized,
    });
    final overview = _calendarExperts(setup.personId, result);
    if (overview.receiptFor(setup) == null) {
      throw const FormatException('Missing Calendar setup receipt');
    }
    return overview;
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
    final result = await _perform(request.personId, {
      'kind': 'calendar_access',
      'change': serialized,
    });
    return _calendarExperts(request.personId, result);
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
    final result = await _perform(personId, {
      'kind': 'personal_access',
      'change': {
        'connector': connector,
        'device_id': deviceId,
        'change': change,
      },
    });
    if (result['state'] != 'ready' || result['personal_access'] is! Map) {
      throw const FormatException('Missing personal access overview');
    }
    final overview = PersonalAccessOverview.fromJson(result['personal_access']);
    if (overview.personId != personId || overview.deviceId != deviceId) {
      throw const FormatException('Personal access scope mismatch');
    }
    return overview;
  }

  Future<PersonalAccessOverview> _personalContacts(
    String personId,
    Map<String, Object?> change,
  ) async {
    final result = await _perform(personId, {
      'kind': 'contacts_access',
      'change': {
        'connector': 'contacts.$platformContactsConnector',
        'device_id': deviceId,
        'change': change,
      },
    });
    if (result['state'] != 'ready' || result['personal_access'] is! Map) {
      throw const FormatException('Missing Contacts access overview');
    }
    final overview = PersonalAccessOverview.fromJson(result['personal_access']);
    if (overview.personId != personId || overview.deviceId != deviceId) {
      throw const FormatException('Contacts access scope mismatch');
    }
    return overview;
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

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async {
    final result = await _perform(personId, {
      'kind': 'registry',
      'change': null,
    });
    final raw = result['registry'];
    if (raw == null) return null;
    final overview = AgentRegistryView.fromJson(
      Map<String, dynamic>.from(raw as Map),
    );
    if (overview.personId != personId) {
      throw const FormatException('Registry Person mismatch');
    }
    return overview;
  }

  @override
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  }) async {
    final result = await _perform(current.personId, {
      'kind': 'registry',
      'change': {
        'instance_id': current.instanceId,
        'expected_revision': current.revision,
        'target': {'kind': target.wireName, 'id': id, 'enabled': enabled},
      },
    });
    final overview = AgentRegistryView.fromJson(
      Map<String, dynamic>.from(result['registry'] as Map),
    );
    if (overview.personId != current.personId ||
        overview.instanceId != current.instanceId ||
        overview.revision != current.revision + 1) {
      throw const FormatException('Registry configuration mismatch');
    }
    return overview;
  }

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
    final result = await _perform(personId, {'kind': kind});
    return AgentVaultState.values.byName(result['state'] as String);
  }

  Future<Map<String, dynamic>> _perform(
    String personId,
    Map<String, Object?> action,
  ) async {
    if (_pending != null && _pending!.personId != personId) {
      throw const AgentVaultException('conflict');
    }
    if (_pending != null) await _drain();
    final job = _VaultJob(
      personId,
      newAgentRequestId(),
      action['kind']?.toString() ?? 'unknown',
    );
    _pending = job;
    final result = await _finish(
      await _call(job, {'kind': 'submit', 'action': action}),
    );
    await _release(job);
    if (result['failure'] != null) {
      throw _failureException(result['failure'], job);
    }
    return result;
  }

  Future<void> _drain() async {
    final job = _pending!;
    try {
      await _finish(await _call(job, {'kind': 'poll', 'after_sequence': 0}));
      await _release(job);
    } on AgentVaultException catch (error) {
      if (error.failure != 'not_found') rethrow;
      _pending = null;
    }
  }

  Future<Map<String, dynamic>> _finish(Map<String, dynamic> result) async {
    final elapsed = Stopwatch()..start();
    while (result['done'] != true) {
      if (elapsed.elapsed > const Duration(seconds: 35)) {
        throw const AgentVaultException('deadline_exceeded');
      }
      await Future<void>.delayed(const Duration(milliseconds: 80));
      result = await _call(_pending!, {'kind': 'poll', 'after_sequence': 0});
    }
    return result;
  }

  Future<Map<String, dynamic>> _call(
    _VaultJob job,
    Map<String, Object?> operation,
  ) async {
    late final Map<String, dynamic> result;
    try {
      result = await request({
        'schema_version': agentSchemaVersion,
        'person_id': job.personId,
        'request_id': job.id,
        'operation': operation,
      });
    } on Object catch (error, stackTrace) {
      final source = error is AgentVaultException ? error : null;
      final enriched = AgentVaultException(
        source?.failure ?? 'transport_unavailable',
        requestId: source?.requestId ?? job.id,
        stage: source?.stage ?? job.stage,
        metadata: source?.metadata ?? const {},
        recoveryAction: source?.recoveryAction,
        affectedRefs: source?.affectedRefs ?? const [],
        correlationRequestId: source?.correlationRequestId,
        retryableOverride: source?.retryableOverride,
        domain: source?.domain,
        category: source?.category,
        reasonCode: source?.reasonCode,
        safeActions: source?.safeActions ?? const [],
        incidentId: source?.incidentId,
        retryPolicy: source?.retryPolicy,
        reloadRequired: source?.reloadRequired,
        sealSession: source?.sealSession,
      );
      AppDiagnostics.error(
        component: 'agent_gateway',
        operation: enriched.stage!,
        error: error,
        stackTrace: stackTrace,
        failure: enriched.failure,
        failureDomain: enriched.domain,
        failureCategory: enriched.category,
        reasonCode: enriched.reasonCode,
        incidentId: enriched.incidentId,
        safeActions: enriched.safeActions,
        requestId: enriched.requestId,
        retryable: enriched.retryable,
      );
      Error.throwWithStackTrace(source == null ? error : enriched, stackTrace);
    }
    if (result['request_id'] != job.id ||
        result['done'] is! bool ||
        result['events'] is! List ||
        result['next_sequence'] is! int) {
      throw const FormatException('Invalid vault response');
    }
    _validateFailure(result['failure'], job);
    if (operation['kind'] != 'release' && result['done'] == true) {
      if (result['failure'] != null) {
        final completedError = _failureException(result['failure'], job);
        AppDiagnostics.error(
          component: 'agent_gateway',
          operation: job.stage,
          error: completedError,
          stackTrace: StackTrace.current,
          failure: completedError.failure,
          failureDomain: completedError.domain,
          failureCategory: completedError.category,
          reasonCode: completedError.reasonCode,
          incidentId: completedError.incidentId,
          safeActions: completedError.safeActions,
          requestId: job.id,
          retryable: completedError.retryable,
        );
      }
    }
    return result;
  }

  AgentVaultException _failureException(Object? raw, _VaultJob job) {
    final envelope = _failureEnvelope(raw, job);
    return AgentVaultException(
      envelope.reasonCode,
      requestId: envelope.correlationRequestId,
      stage: envelope.stage,
      recoveryAction: envelope.recoveryAction,
      affectedRefs: envelope.affectedRefs,
      correlationRequestId: envelope.correlationRequestId,
      retryableOverride: envelope.retryable,
      domain: envelope.domain,
      category: envelope.category,
      reasonCode: envelope.reasonCode,
      safeActions: envelope.safeActions,
      incidentId: envelope.incidentId,
      retryPolicy: envelope.retryPolicy,
      reloadRequired: envelope.reloadRequired,
      sealSession: envelope.sealSession,
    );
  }

  void _validateFailure(Object? raw, _VaultJob job) {
    if (raw != null) _failureEnvelope(raw, job);
  }

  _VaultFailureEnvelope _failureEnvelope(Object? raw, _VaultJob job) {
    if (raw is! Map) {
      throw const FormatException('Invalid vault failure envelope');
    }
    late final Map<String, Object?> value;
    try {
      value = Map<String, Object?>.from(raw);
    } on Object {
      throw const FormatException('Invalid vault failure envelope');
    }
    const fields = {
      'schema_version',
      'domain',
      'category',
      'reason_code',
      'kind',
      'stage',
      'safe_actions',
      'affected_refs',
      'incident_id',
      'retry_policy',
      'retryable',
      'recovery_action',
      'correlation_request_id',
      'reload_required',
      'seal_session',
    };
    if (value.length != fields.length ||
        !value.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid vault failure envelope fields');
    }
    if (value['schema_version'] != agentSchemaVersion ||
        value['domain'] is! String ||
        value['category'] is! String ||
        value['reason_code'] is! String ||
        value['kind'] is! String ||
        value['stage'] != job.stage ||
        (value['stage'] as String).isEmpty ||
        (value['stage'] as String).length > 64 ||
        (value['kind'] as String).trim().isEmpty ||
        (value['kind'] as String).length > 128 ||
        (value['reason_code'] as String).trim().isEmpty ||
        (value['reason_code'] as String).length > 128 ||
        value['incident_id'] is! String ||
        (value['incident_id'] as String).isEmpty ||
        (value['incident_id'] as String).length > 128 ||
        value['retryable'] is! bool ||
        value['reload_required'] is! bool ||
        value['seal_session'] is! bool ||
        value['retry_policy'] is! String ||
        value['recovery_action'] is! String ||
        value['correlation_request_id'] != job.id) {
      throw const FormatException('Invalid vault failure envelope');
    }
    if (!const {
      'source',
      'capability',
      'turn',
      'session',
      'vault',
      'app',
    }.contains(value['domain'])) {
      throw const FormatException('Invalid vault failure domain');
    }
    if (!const {
      'user_configuration',
      'transient',
      'integrity',
      'security',
      'internal',
    }.contains(value['category'])) {
      throw const FormatException('Invalid vault failure category');
    }
    final refs = value['affected_refs'];
    if (refs is! List ||
        refs.length > 32 ||
        refs.any((ref) => ref is! String || ref.isEmpty || ref.length > 256)) {
      throw const FormatException('Invalid vault failure references');
    }
    final action = value['recovery_action']! as String;
    if (!const {
      'none',
      'retry_read',
      'refresh_session',
      'refresh_context',
      'review_source',
      'reopen_vault',
      'reconcile',
    }.contains(action)) {
      throw const FormatException('Invalid vault recovery action');
    }
    final safeActions = value['safe_actions'];
    if (safeActions is! List ||
        safeActions.length > 16 ||
        safeActions.any(
          (item) =>
              item is! String ||
              !const {
                'continue_without_source',
                'review_source',
                'retry',
                'refresh_session',
                'start_new_session',
                'reopen_vault',
                'reset_local_agent_state',
                'export_diagnostics',
              }.contains(item),
        ) ||
        safeActions.toSet().length != safeActions.length) {
      throw const FormatException('Invalid vault failure safe actions');
    }
    final retryPolicy = value['retry_policy']! as String;
    if (!const {'never', 'immediate', 'backoff'}.contains(retryPolicy)) {
      throw const FormatException('Invalid vault failure retry policy');
    }
    final retryable = value['retryable']! as bool;
    if (retryable != (action == 'retry_read') ||
        retryable != (retryPolicy != 'never') ||
        retryable != safeActions.contains('retry')) {
      throw const FormatException('Invalid vault recovery retry contract');
    }
    return _VaultFailureEnvelope(
      domain: value['domain']! as String,
      category: value['category']! as String,
      reasonCode: value['reason_code']! as String,
      kind: value['kind']! as String,
      stage: value['stage']! as String,
      safeActions: List.unmodifiable(safeActions.cast<String>()),
      affectedRefs: List.unmodifiable(refs.cast<String>()),
      retryable: retryable,
      incidentId: value['incident_id']! as String,
      retryPolicy: retryPolicy,
      recoveryAction: action,
      reloadRequired: value['reload_required']! as bool,
      sealSession: value['seal_session']! as bool,
      correlationRequestId: value['correlation_request_id']! as String,
    );
  }

  Future<void> _release(_VaultJob job) async {
    await _call(job, {'kind': 'release'});
    _pending = null;
  }
}

final class _VaultFailureEnvelope {
  const _VaultFailureEnvelope({
    required this.domain,
    required this.category,
    required this.reasonCode,
    required this.kind,
    required this.stage,
    required this.safeActions,
    required this.affectedRefs,
    required this.retryable,
    required this.incidentId,
    required this.retryPolicy,
    required this.recoveryAction,
    required this.reloadRequired,
    required this.sealSession,
    required this.correlationRequestId,
  });

  final String domain;
  final String category;
  final String reasonCode;
  final String kind;
  final String stage;
  final List<String> safeActions;
  final List<String> affectedRefs;
  final bool retryable;
  final String incidentId;
  final String retryPolicy;
  final String recoveryAction;
  final bool reloadRequired;
  final bool sealSession;
  final String correlationRequestId;
}

final class _VaultJob {
  _VaultJob(this.personId, this.id, this.stage);
  final String personId;
  final String id;
  final String stage;
}
