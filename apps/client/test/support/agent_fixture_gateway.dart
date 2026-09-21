import 'package:floe_client/features/conversation/domain/agent_session.dart';
export 'package:floe_client/features/conversation/domain/agent_session.dart';

enum AgentFixturePrompt {
  today('today'),
  followUp('follow_up'),
  repeatedCall('repeated_call'),
  unavailable('unavailable');

  const AgentFixturePrompt(this.wireName);
  final String wireName;

  String get sampleText => switch (this) {
    today => 'Show the sample day briefing.',
    followUp => 'What can the sample assistant change?',
    repeatedCall => 'Repeat the sample read without progress.',
    unavailable => 'Show a sample model connection failure.',
  };
}

abstract interface class AgentFixtureGateway {
  Future<AgentFixtureResult> startAgentFixture(String personId);
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  );
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  );
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session);
}

abstract interface class AgentFixtureStreamingGateway
    implements AgentFixtureGateway {
  Future<AgentFixtureResult> resumeAgentFixture(String personId);
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  );
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  );
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session);
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session);
}

final class AgentRunUpdate {
  AgentRunUpdate.fromJson(Map<String, Object?> json)
    : requestId = json['request_id'] as String?,
      sessionId = json['session_id']! as String,
      expectedRevision = json['expected_revision']! as int,
      nextSequence = json['next_sequence']! as int,
      done = json['done']! as bool,
      session = json['session'] == null
          ? null
          : AgentSession.fromJson(_object(json['session'])),
      failure = json['failure'] as String?,
      recoveryAction = json['recovery_action'] as String?,
      failureDomain = json['failure_domain'] as String?,
      failureCategory = json['failure_category'] as String?,
      failureReasonCode = json['failure_reason_code'] as String?,
      failureSafeActions = List.unmodifiable(
        (json['failure_safe_actions'] as List? ?? const []).cast<String>(),
      ),
      failureAffectedRefs = List.unmodifiable(
        (json['failure_affected_refs'] as List? ?? const []).cast<String>(),
      ),
      failureIncidentId = json['failure_incident_id'] as String?,
      failureRetryPolicy = json['failure_retry_policy'] as String?,
      failureReloadRequired = json['failure_reload_required'] as bool?,
      failureSealSession = json['failure_seal_session'] as bool?,
      events = List.unmodifiable(
        (json['events']! as List).map(
          (value) => AgentEvent.fromJson(_object(value)),
        ),
      ) {
    if (nextSequence < events.length ||
        done != (session != null || failure != null) ||
        !done && recoveryAction != null) {
      throw const FormatException('Invalid Agent run state.');
    }
  }

  /// Decided by the owner; the client never re-derives it.
  final bool? failureReloadRequired;

  /// Decided by the owner; the client stops applying results when set.
  final bool? failureSealSession;
  final String? requestId;
  final String sessionId;
  final int expectedRevision;
  final int nextSequence;
  final bool done;
  final AgentSession? session;
  final String? failure;
  final String? recoveryAction;
  final String? failureDomain;
  final String? failureCategory;
  final String? failureReasonCode;
  final List<String> failureSafeActions;
  final List<String> failureAffectedRefs;
  final String? failureIncidentId;
  final String? failureRetryPolicy;
  final List<AgentEvent> events;
}

final class AgentFixtureResult {
  AgentFixtureResult.fromJson(Map<String, Object?> json)
    : session = AgentSession.fromJson(_object(json['session'])),
      events = List.unmodifiable(
        (json['events']! as List).map(
          (value) => AgentEvent.fromJson(_object(value)),
        ),
      );

  final AgentSession session;
  final List<AgentEvent> events;
}

final class AgentEvent {
  AgentEvent.fromJson(Map<String, Object?> json)
    : sessionId = json['session_id']! as String,
      turnId = json['turn_id']! as String,
      event = AgentEventData.fromJson(_object(json['event'])) {
    _checkVersion(json);
  }

  final String sessionId;
  final String turnId;
  final AgentEventData event;
}

sealed class AgentEventData {
  const AgentEventData();

  factory AgentEventData.fromJson(Map<String, Object?> json) =>
      switch (json['kind']) {
        'started' => const AgentStarted(),
        'model_attempt' => AgentModelAttempt.fromJson(_object(json['record'])),
        'model_started' => AgentModelStarted(
          json['iteration']! as int,
          json['placement']! as String,
        ),
        'capability_started' => AgentCapabilityStarted(
          json['call_id']! as String,
          json['capability_id']! as String,
        ),
        'delegation_started' => AgentDelegationStarted(
          json['task_id']! as String,
          json['agent_id']! as String,
        ),
        'message_committed' => AgentMessageCommitted(
          AgentMessage.fromJson(_object(json['message'])),
          json['revision']! as int,
        ),
        'finished' => AgentFinished(
          AgentOutcome.fromJson(_object(json['outcome'])),
          json['revision']! as int,
        ),
        _ => throw const FormatException('Unknown Agent event kind.'),
      };
}

final class AgentStarted extends AgentEventData {
  const AgentStarted();
}

final class AgentDelegationStarted extends AgentEventData {
  AgentDelegationStarted(this.taskId, this.agentId) {
    if (taskId.isEmpty || agentId.isEmpty) {
      throw const FormatException('Invalid Agent delegation event.');
    }
  }

  final String taskId;
  final String agentId;
}

final class AgentModelAttempt extends AgentEventData {
  AgentModelAttempt.fromJson(Map<String, Object?> json)
    : id = json['id']! as String,
      scopeId = json['scope_id']! as String,
      attempt = json['attempt']! as int,
      state = json['state']! as String,
      failure = json['failure'] as String? {
    if (id.isEmpty ||
        scopeId.isEmpty ||
        attempt < 1 ||
        attempt > 2 ||
        !{'started', 'accepted', 'rejected', 'interrupted'}.contains(state)) {
      throw const FormatException('Invalid model attempt.');
    }
  }

  final String id;
  final String scopeId;
  final int attempt;
  final String state;
  final String? failure;
}

final class AgentModelStarted extends AgentEventData {
  const AgentModelStarted(this.iteration, this.placement);
  final int iteration;
  final String placement;
}

final class AgentCapabilityStarted extends AgentEventData {
  const AgentCapabilityStarted(this.callId, this.capabilityId);
  final String callId;
  final String capabilityId;
}

final class AgentMessageCommitted extends AgentEventData {
  const AgentMessageCommitted(this.message, this.revision);
  final AgentMessage message;
  final int revision;
}

final class AgentFinished extends AgentEventData {
  const AgentFinished(this.outcome, this.revision);
  final AgentOutcome outcome;
  final int revision;
}

Map<String, Object?> _object(Object? value) =>
    Map<String, Object?>.from(value! as Map);

void _checkVersion(Map<String, Object?> json) {
  if (json['schema_version'] != agentSchemaVersion) {
    throw const FormatException('Unsupported Agent schema version.');
  }
}
