import 'agent_fixture_gateway.dart';

abstract interface class AgentCalendarSessionGateway {
  Future<AgentSession> startCalendarSession(String personId, String setupId);
  Future<AgentSession> resumeCalendarSession(String personId, String setupId);
  Future<AgentSession> loadCalendarSession(String personId, String sessionId);
  Future<AgentSession> recoverCalendarSession(AgentSession session);
}
