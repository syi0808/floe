const agentSchemaVersion = 1;

enum AgentFixturePrompt {
  today('today'),
  followUp('follow_up'),
  repeatedCall('repeated_call'),
  unavailable('unavailable');

  const AgentFixturePrompt(this.wireName);
  final String wireName;

  String get sampleText => switch (this) {
    today => 'Give me a day briefing.',
    followUp => 'What can Floe change?',
    repeatedCall => 'Repeat the last read.',
    unavailable => 'Check model availability.',
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
    : sessionId = json['session_id']! as String,
      expectedRevision = json['expected_revision']! as int,
      nextSequence = json['next_sequence']! as int,
      done = json['done']! as bool,
      session = json['session'] == null
          ? null
          : AgentSession.fromJson(_object(json['session'])),
      failure = json['failure'] as String?,
      events = List.unmodifiable(
        (json['events']! as List).map(
          (value) => AgentEvent.fromJson(_object(value)),
        ),
      ) {
    if (nextSequence < events.length ||
        done != (session != null || failure != null) ||
        session != null && failure != null) {
      throw const FormatException('Invalid Agent run state.');
    }
  }

  final String sessionId;
  final int expectedRevision;
  final int nextSequence;
  final bool done;
  final AgentSession? session;
  final String? failure;
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

final class AgentSession {
  AgentSession.fromJson(Map<String, Object?> json)
    : id = json['id']! as String,
      personId = json['person_id']! as String,
      scope = json['scope'] == null
          ? null
          : AgentSessionScope.fromJson(_object(json['scope'])),
      revision = json['revision']! as int,
      activeTurn = json['active_turn'] as String?,
      lastOutcome = json['last_outcome'] == null
          ? null
          : AgentOutcome.fromJson(_object(json['last_outcome'])),
      continuation = json['continuation'] == null
          ? null
          : AgentContinuation.fromJson(_object(json['continuation'])),
      dataClasses = List.unmodifiable(
        (json['data_classes']! as List).cast<String>(),
      ),
      messages = List.unmodifiable(
        (json['messages']! as List).map(
          (value) => AgentMessage.fromJson(_object(value)),
        ),
      ) {
    _checkVersion(json);
    if (scope != null &&
        (dataClasses.length != 1 || dataClasses.single != scope!.dataClass)) {
      throw const FormatException('Calendar session classification mismatch');
    }
  }

  final String id;
  final String personId;
  final AgentSessionScope? scope;
  final int revision;
  final String? activeTurn;
  final AgentOutcome? lastOutcome;
  final AgentContinuation? continuation;
  final List<String> dataClasses;
  final List<AgentMessage> messages;
}

final class AgentContinuation {
  AgentContinuation.fromJson(Map<String, Object?> json)
    : turnId = json['turn_id']! as String,
      level = json['level']! as int,
      placement = json['placement']! as String {
    if (level < 0 || level > 3 || json['usage'] is! Map) {
      throw const FormatException('Invalid Agent continuation.');
    }
  }

  final String turnId;
  final int level;
  final String placement;
}

final class AgentSessionScope {
  AgentSessionScope.fromJson(Map<String, Object?> json)
    : setupId = json['setup_id']! as String,
      provider = json['provider']! as String {
    if (json.length != 3 ||
        json['kind'] != 'calendar' ||
        !['fixture', 'event_kit'].contains(provider) ||
        !RegExp(
          r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
        ).hasMatch(setupId)) {
      throw const FormatException('Invalid Calendar session scope');
    }
  }

  final String setupId;
  final String provider;
  String get dataClass => provider == 'fixture' ? 'synthetic' : 'personal';
}

enum AgentMessageKind { user, assistant, capability }

sealed class AgentMessage {
  const AgentMessage(this.turnId);

  factory AgentMessage.fromJson(Map<String, Object?> json) {
    final turnId = json['turn_id']! as String;
    return switch (json['kind']) {
      'user' => AgentTextMessage(
        turnId,
        AgentMessageKind.user,
        json['text']! as String,
      ),
      'assistant' => AgentTextMessage(
        turnId,
        AgentMessageKind.assistant,
        json['text']! as String,
      ),
      'capability' => AgentCapabilityMessage.fromJson(json),
      _ => throw const FormatException('Unknown Agent message kind.'),
    };
  }

  final String turnId;
  AgentMessageKind get kind;
}

final class AgentTextMessage extends AgentMessage {
  const AgentTextMessage(super.turnId, this.kind, this.text);

  @override
  final AgentMessageKind kind;
  final String text;
}

final class AgentCapabilityMessage extends AgentMessage {
  AgentCapabilityMessage.fromJson(Map<String, Object?> json)
    : callId = json['call_id']! as String,
      capabilityId = json['capability_id']! as String,
      input = json['input']! as String,
      output = _object(json['result'])['Ok'] as String?,
      failure = _object(json['result'])['Err'] as String?,
      super(json['turn_id']! as String) {
    if ((output == null) == (failure == null)) {
      throw const FormatException('Invalid Agent capability result.');
    }
  }

  @override
  AgentMessageKind get kind => AgentMessageKind.capability;
  final String callId;
  final String capabilityId;
  final String input;
  final String? output;
  final String? failure;
}

final class AgentOutcome {
  AgentOutcome.fromJson(Map<String, Object?> json)
    : completed = json['status'] == 'completed',
      failure = json['reason'] as String? {
    if (json['status'] != 'completed' && json['status'] != 'halted' ||
        completed == (failure != null)) {
      throw const FormatException('Invalid Agent outcome.');
    }
  }

  final bool completed;
  final String? failure;
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
        'model_started' => AgentModelStarted(
          json['iteration']! as int,
          json['placement']! as String,
        ),
        'capability_started' => AgentCapabilityStarted(
          json['call_id']! as String,
          json['capability_id']! as String,
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
