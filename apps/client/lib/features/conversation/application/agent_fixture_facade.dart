import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/conversation/application/agent_fixture_gateway.dart';
import 'package:floe_client/features/conversation/infrastructure/native_agent_fixture_gateway.dart';

/// The synthetic sample conversation used for fixture runs.
final class AgentFixtureFacade implements AgentFixtureStreamingGateway {
  AgentFixtureFacade(AppRuntime runtime)
    : _gateway = NativeAgentFixtureGateway(runtime.request);

  final AgentFixtureStreamingGateway _gateway;

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _gateway.startAgentFixture(personId);

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _gateway.resumeAgentFixture(personId);

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _gateway.beginAgentFixtureRun(session, prompt);

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _gateway.pollAgentFixtureRun(session, afterSequence);

  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _gateway.stopAgentFixtureRun(session);

  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) =>
      _gateway.releaseAgentFixtureRun(session);

  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _gateway.loadAgentFixture(personId, sessionId);

  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _gateway.runAgentFixture(session, prompt);

  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _gateway.recoverAgentFixture(session);
}
