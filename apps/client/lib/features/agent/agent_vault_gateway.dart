import 'dart:convert';

import 'agent_calendar_experts.dart';
import 'agent_calendar_session_gateway.dart';
import 'agent_calendar_turn_gateway.dart';
import 'agent_conversation_gateway.dart';
import 'agent_fixture_gateway.dart';
import 'agent_proposal.dart';
import 'agent_registry.dart';
import 'agent_request_id.dart';

enum AgentVaultState { missing, locked, ready, unavailable }

class AgentVaultException implements Exception {
  const AgentVaultException(this.failure);
  final String failure;
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
        AgentCalendarSessionGateway,
        AgentCalendarTurnGateway,
        AgentConversationGateway,
        AgentCalendarExpertGateway {
  NativeAgentVaultGateway(this.request, {this.resolveRemoteRoute});

  final Future<Map<String, dynamic>> Function(Map<String, Object?>) request;
  final Future<Map<String, Object?>?> Function()? resolveRemoteRoute;
  _VaultJob? _pending;
  AgentSession? _run;
  AgentCalendarTurnRequest? _calendarRun;
  AgentConversationTurnRequest? _conversationRun;

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
      _pending = _VaultJob(turn.session.personId, newAgentRequestId());
      _run = turn.session;
      _conversationRun = turn;
    }
    final serialized = turn.toJson();
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
  Future<AgentCalendarTurnUpdate> beginCalendarTurn(
    AgentCalendarTurnRequest turn,
  ) async {
    final scope = turn.session.scope;
    if (scope == null || turn.day.personId != turn.session.personId) {
      throw const FormatException('Calendar turn scope mismatch');
    }
    if (_calendarRun != null && !_sameCalendarTurn(_calendarRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    if (_calendarRun == null) {
      if (_pending != null) await _drain();
      _pending = _VaultJob(turn.session.personId, newAgentRequestId());
      _run = turn.session;
      _calendarRun = turn;
    }
    final serialized = turn.toJson();
    if (turn.prompt == AgentCalendarPromptKind.freeText &&
        resolveRemoteRoute != null) {
      serialized['remote_route'] = await resolveRemoteRoute!();
    }
    return _calendarUpdate(
      turn,
      await _call(_pending!, {
        'kind': 'submit',
        'action': {'kind': 'calendar_turn', 'request': serialized},
      }),
    );
  }

  @override
  Future<AgentCalendarTurnUpdate> pollCalendarTurn(
    AgentCalendarTurnRequest turn,
    int afterSequence,
  ) => _calendarCall(turn, {'kind': 'poll', 'after_sequence': afterSequence});

  @override
  Future<AgentCalendarTurnUpdate> stopCalendarTurn(
    AgentCalendarTurnRequest turn,
  ) => _calendarCall(turn, {'kind': 'stop'});

  @override
  Future<AgentCalendarTurnUpdate> releaseCalendarTurn(
    AgentCalendarTurnRequest turn,
  ) async {
    final result = await _calendarCall(turn, {'kind': 'release'});
    _pending = null;
    _run = null;
    _calendarRun = null;
    return result;
  }

  Future<AgentCalendarTurnUpdate> _calendarCall(
    AgentCalendarTurnRequest turn,
    Map<String, Object?> operation,
  ) async {
    if (_calendarRun == null || !_sameCalendarTurn(_calendarRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    return _calendarUpdate(turn, await _call(_pending!, operation));
  }

  AgentCalendarTurnUpdate _calendarUpdate(
    AgentCalendarTurnRequest turn,
    Map<String, dynamic> result,
  ) => AgentCalendarTurnUpdate.fromJson({
    ...result,
    'session_id': turn.session.id,
    'expected_revision': turn.session.revision,
  }, turn);

  bool _sameCalendarTurn(
    AgentCalendarTurnRequest left,
    AgentCalendarTurnRequest right,
  ) =>
      identical(left, right) ||
      left.session.personId == right.session.personId &&
          jsonEncode(left.toJson()) == jsonEncode(right.toJson());

  @override
  Future<AgentSession> startCalendarSession(String personId, String setupId) =>
      _calendarSession(personId, {
        'kind': 'start',
        'setup_id': setupId,
      }, setupId: setupId);

  @override
  Future<AgentSession> resumeCalendarSession(String personId, String setupId) =>
      _calendarSession(personId, {
        'kind': 'resume',
        'setup_id': setupId,
      }, setupId: setupId);

  @override
  Future<AgentSession> loadCalendarSession(String personId, String sessionId) =>
      _calendarSession(personId, {
        'kind': 'get',
        'session_id': sessionId,
      }, sessionId: sessionId);

  @override
  Future<AgentSession> recoverCalendarSession(AgentSession session) async {
    final scope = session.scope;
    if (scope == null) throw const FormatException('Not a Calendar session');
    final saved = await _calendarSession(
      session.personId,
      {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      },
      sessionId: session.id,
      setupId: scope.setupId,
    );
    if (saved.scope!.provider != scope.provider ||
        saved.activeTurn != null ||
        saved.revision !=
            session.revision + (session.activeTurn == null ? 0 : 1)) {
      throw const FormatException('Calendar recovery mismatch');
    }
    return saved;
  }

  Future<AgentSession> _calendarSession(
    String personId,
    Map<String, Object?> operation, {
    String? sessionId,
    String? setupId,
  }) async {
    final result = await _perform(personId, {
      'kind': 'calendar_session',
      'operation': operation,
    });
    final session = AgentSession.fromJson(
      Map<String, Object?>.from(result['session'] as Map),
    );
    if (result['state'] != 'ready' ||
        session.personId != personId ||
        session.scope == null ||
        sessionId != null && session.id != sessionId ||
        setupId != null && session.scope!.setupId != setupId) {
      throw const FormatException('Calendar session scope mismatch');
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
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup setup,
  ) async {
    final result = await _perform(setup.personId, {
      'kind': 'calendar_experts',
      'setup': setup.toJson(),
    });
    final overview = _calendarExperts(setup.personId, result);
    if (overview.receiptFor(setup) == null) {
      throw const FormatException('Missing Calendar setup receipt');
    }
    return overview;
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
    final job = _VaultJob(personId, newAgentRequestId());
    _pending = job;
    final result = await _finish(
      await _call(job, {'kind': 'submit', 'action': action}),
    );
    await _release(job);
    if (result['failure'] case final String failure) {
      throw AgentVaultException(failure);
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
      _calendarRun = null;
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
      _pending = _VaultJob(session.personId, newAgentRequestId());
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
    _calendarRun = null;
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
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<Map<String, dynamic>> _call(
    _VaultJob job,
    Map<String, Object?> operation,
  ) async {
    final result = await request({
      'schema_version': agentSchemaVersion,
      'person_id': job.personId,
      'request_id': job.id,
      'operation': operation,
    });
    if (result['request_id'] != job.id ||
        result['done'] is! bool ||
        result['events'] is! List ||
        result['next_sequence'] is! int) {
      throw const FormatException('Invalid vault response');
    }
    return result;
  }

  Future<void> _release(_VaultJob job) async {
    await _call(job, {'kind': 'release'});
    _pending = null;
    _run = null;
    _calendarRun = null;
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

final class _VaultJob {
  _VaultJob(this.personId, this.id);
  final String personId;
  final String id;
}
