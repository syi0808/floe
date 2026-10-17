import 'dart:convert';

/// Immutable Person-facing history boundaries, never Core turn identifiers.
final class AppSurfaceAnchor {
  AppSurfaceAnchor.fromJson(Map<String, Object?> json)
    : sessionId = _id(json['session_id']),
      sequence = _number(json['sequence']),
      digest = _string(json['digest'], 64) {
    _keys(json, {'session_id', 'sequence', 'digest'});
    if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(digest)) _invalid();
  }
  final String sessionId;
  final int sequence;
  final String digest;
  Map<String, Object?> toJson() => {
    'session_id': sessionId,
    'sequence': sequence,
    'digest': digest,
  };
  @override
  bool operator ==(Object other) =>
      other is AppSurfaceAnchor &&
      sessionId == other.sessionId &&
      sequence == other.sequence &&
      digest == other.digest;
  @override
  int get hashCode => Object.hash(sessionId, sequence, digest);
}

enum AppSurfaceInputKind { text, voice }

enum AppSurfaceResultCompleteness { complete, partial }

enum AppSurfaceOutcomeState {
  blocked,
  failed,
  cancelled,
  timedOut,
  interrupted,
}

sealed class AppSurfaceMessage {
  const AppSurfaceMessage(this.messageId, this.before, this.through);
  final String messageId;
  final AppSurfaceAnchor before;
  final AppSurfaceAnchor through;
  factory AppSurfaceMessage.fromJson(Map<String, Object?> json) {
    _keys(json, {'message_id', 'before', 'through', 'content'});
    final id = _id(json['message_id']);
    final before = AppSurfaceAnchor.fromJson(_object(json['before']));
    final through = AppSurfaceAnchor.fromJson(_object(json['through']));
    if (before.sessionId != through.sessionId ||
        before.sequence + 1 != through.sequence) {
      _invalid();
    }
    final c = _object(json['content']);
    switch (c['kind']) {
      case 'input':
        _keys(c, {'kind', 'text', 'origin'});
        final text = _string(c['text'], 8192);
        if (text.runes.any(
          (r) => (r < 32 && r != 10) || (r >= 127 && r <= 159),
        )) {
          _invalid();
        }
        return AppSurfaceInput(
          id,
          before,
          through,
          text,
          AppSurfaceInputKind.values.byName(c['origin']! as String),
        );
      case 'result':
        _keys(c, {'kind', 'run_id', 'completeness', 'text'});
        return AppSurfaceResult(
          id,
          before,
          through,
          _id(c['run_id']),
          _string(c['text'], 65536),
          AppSurfaceResultCompleteness.values.byName(
            c['completeness']! as String,
          ),
        );
      case 'outcome':
        _keys(c, {'kind', 'run_id', 'state', 'issue', 'interaction_refs'});
        final state = switch (c['state']) {
          'blocked' => AppSurfaceOutcomeState.blocked,
          'failed' => AppSurfaceOutcomeState.failed,
          'cancelled' => AppSurfaceOutcomeState.cancelled,
          'timed_out' => AppSurfaceOutcomeState.timedOut,
          'interrupted' => AppSurfaceOutcomeState.interrupted,
          _ => throw const FormatException('Invalid Surface outcome.'),
        };
        final refs = _list(
          c['interaction_refs'],
          32,
        ).map(_id).toList(growable: false);
        if (refs.toSet().length != refs.length) _invalid();
        final issue = c['issue'] == null
            ? null
            : _string(_object(c['issue'])['message'], 8192);
        if (state == AppSurfaceOutcomeState.blocked) {
          if (issue != null || refs.isEmpty) _invalid();
        } else if (issue == null || refs.isNotEmpty) {
          _invalid();
        }
        return AppSurfaceOutcome(
          id,
          before,
          through,
          _id(c['run_id']),
          state,
          issue,
          List.unmodifiable(refs),
        );
      case 'pending_work_cancelled':
        _keys(c, {'kind', 'work_id', 'input_id'});
        return AppSurfaceCancellation(
          id,
          before,
          through,
          _id(c['work_id']),
          _id(c['input_id']),
        );
      default:
        throw const FormatException('Unknown Surface entry.');
    }
  }
}

final class AppSurfaceInput extends AppSurfaceMessage {
  const AppSurfaceInput(
    super.messageId,
    super.before,
    super.through,
    this.text,
    this.origin,
  );
  final String text;
  final AppSurfaceInputKind origin;
}

final class AppSurfaceResult extends AppSurfaceMessage {
  const AppSurfaceResult(
    super.messageId,
    super.before,
    super.through,
    this.runId,
    this.text,
    this.completeness,
  );
  final String runId, text;
  final AppSurfaceResultCompleteness completeness;
}

final class AppSurfaceOutcome extends AppSurfaceMessage {
  const AppSurfaceOutcome(
    super.messageId,
    super.before,
    super.through,
    this.runId,
    this.state,
    this.issueMessage,
    this.interactionIds,
  );
  final String runId;
  final AppSurfaceOutcomeState state;

  /// Display only; never infers approval, retry policy or source authority.
  final String? issueMessage;
  final List<String> interactionIds;
}

final class AppSurfaceCancellation extends AppSurfaceMessage {
  const AppSurfaceCancellation(
    super.messageId,
    super.before,
    super.through,
    this.workId,
    this.inputId,
  );
  final String workId, inputId;
}

final class AppSurfaceSnapshot {
  AppSurfaceSnapshot.fromJson(Map<String, Object?> json)
    : through = AppSurfaceAnchor.fromJson(_object(json['through'])),
      older = json['older'] == null
          ? null
          : AppSurfaceAnchor.fromJson(_object(json['older'])),
      messages = List.unmodifiable(
        _list(
          json['messages'],
          128,
        ).map((v) => AppSurfaceMessage.fromJson(_object(v))),
      ) {
    _keys(json, {'through', 'messages', 'older'});
    if (utf8.encode(jsonEncode(json)).length > 2 * 1024 * 1024 ||
        (through.sequence == 0) != messages.isEmpty) {
      _invalid();
    }
    if (messages.isEmpty) {
      if (older != null) _invalid();
      return;
    }
    var boundary = messages.first.before;
    final ids = <String>{};
    for (final message in messages) {
      if (!ids.add(message.messageId) ||
          message.before != boundary ||
          message.through.sessionId != through.sessionId) {
        _invalid();
      }
      boundary = message.through;
    }
    if (boundary != through ||
        older !=
            (messages.first.before.sequence == 0
                ? null
                : messages.first.before)) {
      _invalid();
    }
  }
  final AppSurfaceAnchor through;
  final AppSurfaceAnchor? older;
  final List<AppSurfaceMessage> messages;
}

enum AppSurfaceWorkOrigin { text, voice, retry }

final class AppSurfaceWorkReceipt {
  AppSurfaceWorkReceipt.fromJson(Map<String, Object?> json)
    : commandId = _id(json['command_id']),
      workId = _id(json['work_id']),
      executionCommandId = _id(json['execution_command_id']),
      inputId = _id(json['input_id']),
      anchor = AppSurfaceAnchor.fromJson(_object(json['anchor'])),
      runtimeEpoch = _number(json['runtime_epoch'], positive: true),
      origin = AppSurfaceWorkOrigin.values.byName(
        _object(json['origin'])['kind']! as String,
      ),
      sourceRunId = _object(json['origin'])['source_run_id'] == null
          ? null
          : _id(_object(json['origin'])['source_run_id']) {
    _keys(json, {
      'command_id',
      'work_id',
      'execution_command_id',
      'input_id',
      'anchor',
      'origin',
      'runtime_epoch',
    });
    _keys(
      _object(json['origin']),
      origin == AppSurfaceWorkOrigin.retry
          ? {'kind', 'source_run_id'}
          : {'kind'},
    );
    if (anchor.sequence == 0 ||
        (origin == AppSurfaceWorkOrigin.retry) != (sourceRunId != null)) {
      _invalid();
    }
  }
  final String commandId, workId, executionCommandId, inputId;
  final AppSurfaceAnchor anchor;
  final int runtimeEpoch;
  final AppSurfaceWorkOrigin origin;
  final String? sourceRunId;
}

enum AppSurfaceWorkState { queued, linked, cancelled }

final class AppSurfaceWorkSnapshot {
  AppSurfaceWorkSnapshot.fromJson(Map<String, Object?> json)
    : receipt = AppSurfaceWorkReceipt.fromJson(_object(json['receipt'])),
      inputText = _string(json['input_text'], 8192),
      state = AppSurfaceWorkState.values.byName(
        _object(json['state'])['kind']! as String,
      ),
      initialRunId = _object(json['state'])['run_id'] == null
          ? null
          : _id(_object(json['state'])['run_id']),
      currentRunId = json['current_run_id'] == null
          ? null
          : _id(json['current_run_id']) {
    _keys(json, {'receipt', 'input_text', 'state', 'current_run_id'});
    _keys(
      _object(json['state']),
      state == AppSurfaceWorkState.linked ? {'kind', 'run_id'} : {'kind'},
    );
    if ((state == AppSurfaceWorkState.linked) !=
        (initialRunId != null && currentRunId != null)) {
      _invalid();
    }
    if (state != AppSurfaceWorkState.linked &&
        (initialRunId != null || currentRunId != null)) {
      _invalid();
    }
  }
  final AppSurfaceWorkReceipt receipt;
  final String inputText;
  final AppSurfaceWorkState state;
  final String? initialRunId, currentRunId;
}

final class AppSurfaceWorkCursor {
  AppSurfaceWorkCursor.fromJson(Map<String, Object?> json)
    : through = _id(json['through']),
      before = _id(json['before']) {
    _keys(json, {'through', 'before'});
  }
  final String through, before;
  Map<String, Object?> toJson() => {'through': through, 'before': before};
}

final class AppSurfaceWorkPage {
  AppSurfaceWorkPage.fromJson(Map<String, Object?> json)
    : items = List.unmodifiable(
        _list(
          json['items'],
          128,
        ).map((v) => AppSurfaceWorkSnapshot.fromJson(_object(v))),
      ),
      next = json['next'] == null
          ? null
          : AppSurfaceWorkCursor.fromJson(_object(json['next'])) {
    _keys(json, {'items', 'next'});
    if (items.map((v) => v.receipt.commandId).toSet().length != items.length ||
        (items.isEmpty && next != null)) {
      _invalid();
    }
  }
  final List<AppSurfaceWorkSnapshot> items;
  final AppSurfaceWorkCursor? next;
}

Never _invalid() =>
    throw const FormatException('Invalid Conversation Surface custody.');
Map<String, Object?> _object(Object? value) {
  if (value is! Map || value.keys.any((k) => k is! String)) _invalid();
  return Map<String, Object?>.from(value);
}

List<Object?> _list(Object? value, int maximum) {
  if (value is! List || value.length > maximum) _invalid();
  return value;
}

int _number(Object? value, {bool positive = false}) {
  if (value is! int || value < (positive ? 1 : 0) || value > 0x7fffffffffffffff) {
    _invalid();
  }
  return value;
}

String _id(Object? value) {
  if (value is! String ||
      value == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value)) {
    _invalid();
  }
  return value;
}

String _string(Object? value, int maximum) {
  if (value is! String || value.isEmpty || utf8.encode(value).length > maximum) {
    _invalid();
  }
  return value;
}

void _keys(Map<String, Object?> value, Set<String> required) {
  if (value.length != required.length || !required.every(value.containsKey)) {
    _invalid();
  }
}
