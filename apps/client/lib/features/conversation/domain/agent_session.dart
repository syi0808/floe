const agentSchemaVersion = 1;

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
      level = json['level']! as int {
    if (level < 0 || level > 3 || json['usage'] is! Map) {
      throw const FormatException('Invalid Agent continuation.');
    }
  }

  final String turnId;
  final int level;
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

enum AgentMessageKind { user, assistant, preamble, capability, interaction }

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
      'preamble' => AgentTextMessage(
        turnId,
        AgentMessageKind.preamble,
        json['text']! as String,
      ),
      'capability' => AgentCapabilityMessage.fromJson(json),
      'delegation' => AgentCapabilityMessage.fromDelegation(json),
      'interaction' => AgentInteractionMessage.fromJson(json),
      _ => throw const FormatException('Unknown Agent message kind.'),
    };
  }

  final String turnId;
  AgentMessageKind get kind;
}

enum AgentInteractionMessageKind { sourceAccess, processingRecipient }

final class AgentInteractionMessage extends AgentMessage {
  AgentInteractionMessage.fromJson(Map<String, Object?> json)
    : interactionId = json['interaction_id']! as String,
      interactionKind = switch (json['interaction_kind']) {
        'source_access' => AgentInteractionMessageKind.sourceAccess,
        'processing_recipient' =>
          AgentInteractionMessageKind.processingRecipient,
        _ => throw const FormatException('Unknown interaction kind.'),
      },
      super(json['turn_id']! as String) {
    if (interactionId.isEmpty) {
      throw const FormatException('Invalid interaction reference.');
    }
  }

  @override
  AgentMessageKind get kind => AgentMessageKind.interaction;
  final String interactionId;
  final AgentInteractionMessageKind interactionKind;
}

final class AgentTextMessage extends AgentMessage {
  const AgentTextMessage(super.turnId, this.kind, this.text);

  @override
  final AgentMessageKind kind;
  final String text;
}

final class AgentCapabilityMessage extends AgentMessage {
  factory AgentCapabilityMessage.fromDelegation(Map<String, Object?> json) {
    final task = _object(json['task']);
    final state = task['state'] as String?;
    final failure = task['failure'] as String?;
    if (task['id'] is! String ||
        task['agent_id'] is! String ||
        !const {
          'submitted',
          'working',
          'completed',
          'failed',
          'cancelled',
          'rejected',
        }.contains(state) ||
        task['artifacts'] is! List) {
      throw const FormatException('Invalid Agent delegation.');
    }
    final outputs = <String>[];
    for (final artifact in task['artifacts']! as List) {
      final parts = _object(artifact)['parts'];
      if (parts is! List) {
        throw const FormatException('Invalid Agent delegation artifact.');
      }
      for (final part in parts) {
        final value = _object(part);
        if (value['kind'] == 'data' &&
            value['media_type'] ==
                'application/vnd.floe.expert-result+json;version=1' &&
            value['data'] is String) {
          outputs.add(value['data']! as String);
        }
      }
    }
    if (state == 'completed' && (failure != null || outputs.length != 1)) {
      throw const FormatException('Invalid completed Agent delegation.');
    }
    return AgentCapabilityMessage.fromJson({
      'turn_id': json['turn_id'],
      'call_id': task['id'],
      'capability_id': 'floe.a2a.delegate',
      'input': task['agent_id'],
      'result': state == 'completed'
          ? {'Ok': outputs.single}
          : {'Err': failure ?? 'invalid_model_output'},
    });
  }

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

Map<String, Object?> _object(Object? value) =>
    Map<String, Object?>.from(value! as Map);

void _checkVersion(Map<String, Object?> json) {
  if (json['schema_version'] != agentSchemaVersion) {
    throw const FormatException('Unsupported Agent schema version.');
  }
}
