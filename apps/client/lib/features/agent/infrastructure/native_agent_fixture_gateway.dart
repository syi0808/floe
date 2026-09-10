import '../agent_fixture_gateway.dart';

final class NativeAgentFixtureGateway implements AgentFixtureStreamingGateway {
  NativeAgentFixtureGateway(this._request);

  final Future<Map<String, dynamic>> Function(
    String operation,
    Map<String, dynamic> request,
  )
  _request;

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _session(personId, {'kind': 'start'});

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _session(personId, {'kind': 'resume'});

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _run(session, {'kind': 'begin', 'prompt': prompt.wireName});

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _run(session, {'kind': 'poll', 'after_sequence': afterSequence});

  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _run(session, {'kind': 'stop'});

  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) =>
      _run(session, {'kind': 'release'});

  Future<AgentRunUpdate> _run(
    AgentSession session,
    Map<String, Object?> operation,
  ) async => AgentRunUpdate.fromJson(
    await _request('agent_fixture_run', {
      'schema_version': agentSchemaVersion,
      'person_id': session.personId,
      'session_id': session.id,
      'expected_revision': session.revision,
      'operation': operation,
    }),
  );

  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _session(personId, {'kind': 'get', 'session_id': sessionId});

  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _session(session.personId, {
    'kind': 'turn',
    'session_id': session.id,
    'expected_revision': session.revision,
    'prompt': prompt.wireName,
  });

  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _session(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<AgentFixtureResult> _session(
    String personId,
    Map<String, Object?> operation,
  ) async => AgentFixtureResult.fromJson(
    await _request('agent_fixture', {
      'schema_version': agentSchemaVersion,
      'person_id': personId,
      'operation': operation,
    }),
  );
}
