import 'dart:convert';

import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

/// Safe Conversation owner projection. Task receipts remain references and
/// artifacts remain metadata; no raw Task payload crosses this boundary.
final class AgentSession {
  AgentSession.fromJson(Map<String, Object?> json)
    : id = _id(json['id']),
      personId = _id(json['person_id']),
      revision = _revision(json['revision']),
      activeTurn = json['active_turn'] == null
          ? null
          : _id(json['active_turn']),
      lastOutcome = json['last_outcome'] == null
          ? null
          : AgentOutcome.fromJson(_object(json['last_outcome'])),
      continuation = json['continuation_ref'] == null
          ? null
          : AgentContinuation.fromJson(_object(json['continuation_ref'])),
      hasEarlierMessages = json['has_earlier_messages']! as bool,
      messages = List.unmodifiable(
        (json['messages']! as List).map(
          (value) => AgentMessage.fromJson(_object(value)),
        ),
      ) {
    _keys(
      json,
      {
        'id',
        'person_id',
        'revision',
        'usage',
        'messages',
        'has_earlier_messages',
      },
      {'active_turn', 'last_outcome', 'continuation_ref'},
    );
    // Revision zero is the valid persisted pre-turn Session revision.
    if (revision > 0x7fffffffffffffff ||
        messages.length > 256 ||
        messages.map((message) => message.messageId).toSet().length !=
            messages.length) {
      throw const FormatException('Invalid Conversation message window.');
    }
    final usage = _object(json['usage']);
    _keys(usage, {
      'unknown_token_attempts',
      'unknown_cost_attempts',
      'model_attempts',
      'estimated_tokens',
      'iterations',
      'capability_calls',
      'tokens',
      'cost_micros',
      'estimated_cost_micros',
    });
    final modelAttempts = _revision(usage['model_attempts']);
    if (_revision(usage['unknown_token_attempts']) > modelAttempts ||
        _revision(usage['unknown_cost_attempts']) > modelAttempts) {
      throw const FormatException('Invalid Conversation usage counters.');
    }
    for (final counter in usage.values) {
      _revision(counter);
    }
  }
  final String id;
  final String personId;
  final int revision;
  final String? activeTurn;
  final AgentOutcome? lastOutcome;
  final AgentContinuation? continuation;
  final bool hasEarlierMessages;
  final List<AgentMessage> messages;
}

final class AgentContinuation {
  AgentContinuation.fromJson(Map<String, Object?> json) : id = _id(json['id']) {
    _keys(json, {'id'});
  }
  final String id;
}

enum AgentMessageKind {
  user,
  assistant,
  preamble,
  compaction,
  capability,
  interaction,
}

sealed class AgentMessage {
  const AgentMessage(this.messageId, this.turnId);
  factory AgentMessage.fromJson(Map<String, Object?> json) {
    final messageId = _id(json['message_id']);
    final turnId = _id(json['turn_id']);
    switch (json['kind']) {
      case 'user':
      case 'assistant':
      case 'preamble':
        _keys(json, {'kind', 'message_id', 'turn_id', 'text'});
        return AgentTextMessage(
          messageId,
          turnId,
          AgentMessageKind.values.byName(json['kind']! as String),
          _text(json['text']),
        );
      case 'compaction':
        _keys(json, {'kind', 'message_id', 'turn_id', 'summary'});
        return AgentTextMessage(
          messageId,
          turnId,
          AgentMessageKind.compaction,
          _text(json['summary']),
        );
      case 'capability':
        return AgentCapabilityMessage.fromJson(json);
      case 'delegation':
        return AgentCapabilityMessage.fromDelegation(json);
      case 'interaction':
        return AgentInteractionMessage.fromJson(json);
      default:
        throw const FormatException('Unknown Conversation message kind.');
    }
  }
  final String messageId;
  final String turnId;
  AgentMessageKind get kind;
}

enum AgentInteractionMessageKind { sourceAccess, assistantFeatureSources }

final class AgentInteractionMessage extends AgentMessage {
  AgentInteractionMessage.fromJson(Map<String, Object?> json)
    : interactionId = _id(json['interaction_id']),
      interactionKind = switch (json['interaction_kind']) {
        'source_access' => AgentInteractionMessageKind.sourceAccess,
        'assistant_feature_sources' =>
          AgentInteractionMessageKind.assistantFeatureSources,
        _ => throw const FormatException('Unknown interaction kind.'),
      },
      super(_id(json['message_id']), _id(json['turn_id'])) {
    _keys(json, {
      'kind',
      'message_id',
      'turn_id',
      'interaction_id',
      'interaction_kind',
    });
  }
  @override
  AgentMessageKind get kind => AgentMessageKind.interaction;
  final String interactionId;
  final AgentInteractionMessageKind interactionKind;
}

final class AgentTextMessage extends AgentMessage {
  const AgentTextMessage(super.messageId, super.turnId, this.kind, this.text);
  @override
  final AgentMessageKind kind;
  final String text;
}

final class AgentCapabilityMessage extends AgentMessage {
  AgentCapabilityMessage._(
    super.messageId,
    super.turnId, {
    required this.callId,
    required this.capabilityId,
    required this.output,
    required this.failure,
    required this.executionReceipt,
    required this.artifacts,
    required this.isDelegation,
  });

  factory AgentCapabilityMessage.fromJson(Map<String, Object?> json) {
    _keys(json, {
      'kind',
      'message_id',
      'turn_id',
      'call_id',
      'capability_id',
      'result',
    });
    final result = _object(json['result']);
    if (result.length != 1 || !{'Ok', 'Err'}.contains(result.keys.single)) {
      throw const FormatException('Invalid capability result.');
    }
    return AgentCapabilityMessage._(
      _id(json['message_id']),
      _id(json['turn_id']),
      executionReceipt: null,
      callId: _id(json['call_id']),
      capabilityId: _text(json['capability_id'], maximum: 256),
      output: result.containsKey('Ok') ? _text(result['Ok']) : null,
      failure: result.containsKey('Err')
          ? AgentSessionIssue.fromJson(_object(result['Err'])).reason
          : null,
      artifacts: const [],
      isDelegation: false,
    );
  }

  factory AgentCapabilityMessage.fromDelegation(Map<String, Object?> json) {
    _keys(json, {'kind', 'message_id', 'turn_id', 'task'});
    final task = _object(json['task']);
    _keys(
      task,
      {'execution_receipt', 'task_id', 'agent_id', 'state', 'artifacts'},
      {'result', 'issue'},
    );
    final state = task['state'];
    if (!{
      'submitted',
      'working',
      'blocked',
      'completed',
      'failed',
      'cancelled',
      'rejected',
      'timed_out',
      'interrupted',
    }.contains(state)) {
      throw const FormatException('Invalid delegation state.');
    }
    final output = task['result'] == null ? null : _text(task['result']);
    final failure = task['issue'] == null
        ? null
        : AgentSessionIssue.fromJson(_object(task['issue'])).reason;
    if (state == 'completed'
        ? output == null || failure != null
        : output != null) {
      throw const FormatException('Invalid delegation outcome.');
    }
    final values = task['artifacts'];
    if (values is! List || values.length > 16)
      throw const FormatException('Invalid artifact summaries.');
    final artifacts = values
        .map((raw) {
          final value = _object(raw);
          _keys(value, {'artifact_id', 'name', 'media_types'});
          final types = value['media_types'];
          if (types is! List || types.length > 16)
            throw const FormatException('Invalid artifact media types.');
          return AgentArtifact(
            _id(value['artifact_id']),
            _text(value['name'], maximum: 256),
            List.unmodifiable(types.map((type) => _text(type, maximum: 256))),
          );
        })
        .toList(growable: false);
    final taskId = _id(task['task_id']);
    final receiptJson = task['execution_receipt'];
    final executionReceipt = receiptJson == null
        ? null
        : TaskExecutionReceiptReference.fromJson(
            Map<String, dynamic>.from(_object(receiptJson)),
          );
    if (executionReceipt == null) {
      if (state != 'rejected' ||
          task['issue'] == null ||
          artifacts.isNotEmpty) {
        throw const FormatException('Invalid rejected Task receipt.');
      }
    } else {
      final execution = _object(executionReceipt.toJson()['execution']);
      if (_id(execution['task_id']) != taskId) {
        throw const FormatException('Task execution receipt mismatch.');
      }
    }
    return AgentCapabilityMessage._(
      _id(json['message_id']),
      _id(json['turn_id']),
      executionReceipt: executionReceipt,
      callId: _id(task['task_id']),
      capabilityId: _text(task['agent_id'], maximum: 256),
      output: output,
      failure: failure,
      artifacts: List.unmodifiable(artifacts),
      isDelegation: true,
    );
  }
  @override
  AgentMessageKind get kind => AgentMessageKind.capability;
  final String callId;
  final String capabilityId;
  final TaskExecutionReceiptReference? executionReceipt;
  final List<AgentArtifact> artifacts;
  final bool isDelegation;
  final String? output;
  final String? failure;
  bool hasArtifactMediaType(String mediaType) =>
      artifacts.any((artifact) => artifact.mediaTypes.contains(mediaType));
}

final class AgentArtifact {
  const AgentArtifact(this.id, this.name, this.mediaTypes);
  final String id;
  final String name;
  final List<String> mediaTypes;
}

final class AgentSessionIssue {
  AgentSessionIssue.fromJson(Map<String, Object?> json)
    : code = _text(json['code'], maximum: 128),
      message = _text(json['message'], maximum: 4096),
      ownerFailure = json['owner_failure'] == null
          ? null
          : OwnerFailure.fromJson(json['owner_failure']),
      metadata = Map.unmodifiable(
        (json['metadata'] == null
                ? <String, Object?>{}
                : _object(json['metadata']))
            .map((key, value) => MapEntry(key, _text(value, maximum: 4096))),
      ) {
    _keys(json, {'code', 'message'}, {'field', 'metadata', 'owner_failure'});
    if (json['field'] != null) _text(json['field'], maximum: 256);
  }
  final String code;
  final String message;
  final Map<String, String> metadata;
  final OwnerFailure? ownerFailure;
  String get reason =>
      ownerFailure?.reason ?? metadata['agent_failure'] ?? code;
}

final class AgentOutcome {
  AgentOutcome.fromJson(Map<String, Object?> json)
    : completed = json['kind'] == 'completed',
      issue = json['reason'] == null
          ? null
          : AgentSessionIssue.fromJson(_object(json['reason'])) {
    switch (json['kind']) {
      case 'completed':
        _keys(json, {'kind'});
      case 'halted':
        _keys(json, {'kind', 'reason'});
      case 'blocked':
        _keys(json, {'kind', 'run_id', 'review_group_id'});
        _id(json['run_id']);
        _id(json['review_group_id']);
      default:
        throw const FormatException('Invalid Conversation outcome.');
    }
  }
  final bool completed;
  final AgentSessionIssue? issue;
  String? get failure => issue?.reason;
}

Map<String, Object?> _object(Object? value) =>
    Map<String, Object?>.from(value! as Map);
void _keys(
  Map<String, Object?> value,
  Set<String> required, [
  Set<String> optional = const {},
]) {
  if (!value.keys.toSet().containsAll(required) ||
      value.keys.toSet().difference({...required, ...optional}).isNotEmpty) {
    throw const FormatException('Invalid Conversation projection fields.');
  }
}

String _id(Object? value) {
  if (value is! String ||
      value == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value)) {
    throw const FormatException('Invalid owner reference.');
  }
  return value;
}

String _text(Object? value, {int maximum = 16384}) {
  if (value is! String || utf8.encode(value).length > maximum)
    throw const FormatException('Invalid owner display text.');
  return value;
}

int _revision(Object? value) {
  if (value is! int || value < 0)
    throw const FormatException('Invalid owner counter.');
  return value;
}
