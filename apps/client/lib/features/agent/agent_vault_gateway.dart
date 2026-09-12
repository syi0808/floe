import 'dart:convert';

import '../../infrastructure/diagnostics/app_diagnostics.dart';
import 'agent_calendar_experts.dart';
import 'agent_connections.dart';
import 'agent_conversation_gateway.dart';
import 'agent_fixture_gateway.dart';
import 'agent_memory_review.dart';
import 'agent_memory.dart';
import 'agent_proposal.dart';
import 'agent_registry.dart';
import 'agent_request_id.dart';

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
  });

  final String failure;
  final String? requestId;
  final String? stage;
  final Map<String, String> metadata;
  final String? recoveryAction;
  final List<String> affectedRefs;
  final String? correlationRequestId;
  final bool? retryableOverride;

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

abstract interface class AgentVaultGateway
    implements AgentFixtureStreamingGateway {
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
        AgentMemoryGateway,
        AgentMemoryReviewGateway {
  NativeAgentVaultGateway(
    this.request, {
    required this.deviceId,
    this.resolveRemoteRoute,
  });

  final Future<Map<String, dynamic>> Function(Map<String, Object?>) request;
  final String deviceId;
  final Future<Map<String, Object?>?> Function()? resolveRemoteRoute;
  _VaultJob? _pending;
  AgentSession? _run;
  AgentConversationTurnRequest? _conversationRun;

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
      'decision': ?decision,
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
  Future<AgentRunUpdate> beginConversationTurn(
    AgentConversationTurnRequest turn,
  ) async {
    if (_conversationRun != null &&
        !_sameConversationTurn(_conversationRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    if (_conversationRun == null) {
      if (_pending != null) await _drain();
      _pending = _VaultJob(
        turn.session.personId,
        newAgentRequestId(),
        'conversation_turn',
      );
      _run = turn.session;
      _conversationRun = turn;
    }
    final serialized = turn.toJson();
    serialized['device_id'] = deviceId;
    if (resolveRemoteRoute != null) {
      serialized['remote_route'] = await resolveRemoteRoute!();
    }
    return _conversationUpdate(
      turn,
      await _call(_pending!, {
        'kind': 'submit',
        'action': {'kind': 'conversation_turn', 'request': serialized},
      }),
    );
  }

  @override
  Future<AgentRunUpdate> pollConversationTurn(
    AgentConversationTurnRequest turn,
    int afterSequence,
  ) => _conversationCall(turn, {
    'kind': 'poll',
    'after_sequence': afterSequence,
  });

  @override
  Future<AgentRunUpdate> stopConversationTurn(
    AgentConversationTurnRequest turn,
  ) => _conversationCall(turn, {'kind': 'stop'});

  @override
  Future<AgentRunUpdate> releaseConversationTurn(
    AgentConversationTurnRequest turn,
  ) async {
    final result = await _conversationCall(turn, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
    return result;
  }

  Future<AgentRunUpdate> _conversationCall(
    AgentConversationTurnRequest turn,
    Map<String, Object?> operation,
  ) async {
    if (_conversationRun == null ||
        !_sameConversationTurn(_conversationRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    return _conversationUpdate(turn, await _call(_pending!, operation));
  }

  AgentRunUpdate _conversationUpdate(
    AgentConversationTurnRequest turn,
    Map<String, dynamic> result,
  ) => AgentRunUpdate.fromJson({
    ...result,
    'failure': _failureKind(result['failure']),
    'recovery_action': _failureRecoveryAction(result['failure']),
    'session_id': turn.session.id,
    'expected_revision': turn.session.revision,
  });

  bool _sameConversationTurn(
    AgentConversationTurnRequest left,
    AgentConversationTurnRequest right,
  ) =>
      identical(left, right) ||
      left.session.personId == right.session.personId &&
          jsonEncode(left.toJson()) == jsonEncode(right.toJson());

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

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _session(personId, {'kind': 'start'});
  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _session(personId, {'kind': 'resume'});
  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _session(personId, {'kind': 'get', 'session_id': sessionId});
  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _session(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });
  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _session(session.personId, _turn(session, prompt));

  Future<AgentFixtureResult> _session(
    String personId,
    Map<String, Object?> operation,
  ) async {
    final result = await _perform(personId, {
      'kind': 'session',
      'operation': operation,
    });
    final parsed = AgentFixtureResult.fromJson({
      'session': result['session'],
      'events': result['events'],
    });
    if (parsed.session.scope != null) {
      throw const FormatException('Calendar session is not a sample');
    }
    return parsed;
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
      if (_run != null) await _call(job, {'kind': 'stop'});
      await _finish(await _call(job, {'kind': 'poll', 'after_sequence': 0}));
      await _release(job);
    } on AgentVaultException catch (error) {
      if (error.failure != 'not_found') rethrow;
      _pending = null;
      _run = null;
      _conversationRun = null;
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

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) async {
    if (_run != null &&
        (_run!.id != session.id ||
            _run!.revision != session.revision ||
            _run!.personId != session.personId)) {
      throw const AgentVaultException('conflict');
    }
    if (_run == null) {
      if (_pending != null) await _drain();
      _pending = _VaultJob(
        session.personId,
        newAgentRequestId(),
        'fixture_turn',
      );
      _run = session;
    }
    return _update(
      session,
      await _call(_pending!, {
        'kind': 'submit',
        'action': {'kind': 'session', 'operation': _turn(session, prompt)},
      }),
    );
  }

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _runCall(session, {'kind': 'poll', 'after_sequence': afterSequence});
  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _runCall(session, {'kind': 'stop'});
  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) async {
    final result = await _runCall(session, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
    return result;
  }

  Future<AgentRunUpdate> _runCall(
    AgentSession session,
    Map<String, Object?> operation,
  ) async {
    if (_run?.id != session.id ||
        _run?.revision != session.revision ||
        _run?.personId != session.personId) {
      throw const AgentVaultException('conflict');
    }
    return _update(session, await _call(_pending!, operation));
  }

  AgentRunUpdate _update(AgentSession session, Map<String, dynamic> result) =>
      AgentRunUpdate.fromJson({
        ...result,
        'failure': _failureKind(result['failure']),
        'recovery_action': _failureRecoveryAction(result['failure']),
        'session_id': session.id,
        'expected_revision': session.revision,
      });

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
      );
      AppDiagnostics.error(
        component: 'agent_gateway',
        operation: enriched.stage!,
        error: error,
        stackTrace: stackTrace,
        failure: enriched.failure,
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
      envelope.kind,
      requestId: envelope.correlationRequestId,
      stage: envelope.stage,
      recoveryAction: envelope.recoveryAction,
      affectedRefs: envelope.affectedRefs,
      correlationRequestId: envelope.correlationRequestId,
      retryableOverride: envelope.retryable,
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
      'kind',
      'stage',
      'affected_refs',
      'retryable',
      'recovery_action',
      'correlation_request_id',
    };
    if (value.length != fields.length ||
        !value.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid vault failure envelope fields');
    }
    if (value['schema_version'] != agentSchemaVersion ||
        value['kind'] is! String ||
        value['stage'] != job.stage ||
        (value['stage'] as String).isEmpty ||
        (value['stage'] as String).length > 64 ||
        (value['kind'] as String).isEmpty ||
        (value['kind'] as String).length > 128 ||
        value['retryable'] is! bool ||
        value['recovery_action'] is! String ||
        value['correlation_request_id'] != job.id) {
      throw const FormatException('Invalid vault failure envelope');
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
    final retryable = value['retryable']! as bool;
    if (retryable != (action == 'retry_read')) {
      throw const FormatException('Invalid vault recovery retry contract');
    }
    return _VaultFailureEnvelope(
      kind: value['kind']! as String,
      stage: value['stage']! as String,
      affectedRefs: List.unmodifiable(refs.cast<String>()),
      retryable: retryable,
      recoveryAction: action,
      correlationRequestId: value['correlation_request_id']! as String,
    );
  }

  String? _failureKind(Object? raw) {
    if (raw == null) return null;
    if (raw is String) return raw;
    if (raw is Map && raw['kind'] is String) return raw['kind'] as String;
    throw const FormatException('Invalid vault failure envelope');
  }

  String? _failureRecoveryAction(Object? raw) {
    if (raw is! Map) return null;
    return raw['recovery_action'] as String?;
  }

  Future<void> _release(_VaultJob job) async {
    await _call(job, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
  }

  Map<String, Object?> _turn(AgentSession session, AgentFixturePrompt prompt) =>
      {
        'kind': 'turn',
        'session_id': session.id,
        'expected_revision': session.revision,
        'prompt': prompt.wireName,
      };
}

final class _VaultFailureEnvelope {
  const _VaultFailureEnvelope({
    required this.kind,
    required this.stage,
    required this.affectedRefs,
    required this.retryable,
    required this.recoveryAction,
    required this.correlationRequestId,
  });

  final String kind;
  final String stage;
  final List<String> affectedRefs;
  final bool retryable;
  final String recoveryAction;
  final String correlationRequestId;
}

final class _VaultJob {
  _VaultJob(this.personId, this.id, this.stage);
  final String personId;
  final String id;
  final String stage;
}
