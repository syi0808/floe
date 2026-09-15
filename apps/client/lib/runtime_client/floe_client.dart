import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'transport/app_wire_transport.dart';

final class PreparedStartTurn {
  const PreparedStartTurn({
    required this.commandId,
    required this.sessionId,
    required this.expectedRevision,
    required this.text,
    this.continuation,
    this.retryOf,
  });

  final String commandId;
  final String sessionId;
  final int expectedRevision;
  final String text;
  final AppContinuationRef? continuation;
  final String? retryOf;
}

final class AppContinuationRef {
  const AppContinuationRef({
    required this.runId,
    required this.executorGeneration,
    required this.level,
  });

  final String runId;
  final int executorGeneration;
  final int level;
}

final class AppCommandReceipt {
  const AppCommandReceipt({
    required this.commandId,
    required this.runId,
    required this.sessionRevision,
    required this.runtimeEpoch,
  });

  final String commandId;
  final String runId;
  final int sessionRevision;
  final int runtimeEpoch;
}

final class PreparedCancelRun {
  const PreparedCancelRun({required this.commandId, required this.runId});

  final String commandId;
  final String runId;
}

enum AppCancelRunOutcome { accepted }

final class AppCancelRunReceipt {
  const AppCancelRunReceipt({
    required this.commandId,
    required this.runId,
    required this.outcome,
    required this.runtimeEpoch,
  });

  final String commandId;
  final String runId;
  final AppCancelRunOutcome outcome;
  final int runtimeEpoch;
}

enum AppRunState { accepted, executing, finalizing, cancelling, finished }

final class AppWireIssue {
  const AppWireIssue(this.code, this.message, {this.metadata = const {}});

  final String code;
  final String message;
  final Map<String, String> metadata;
}

final class AppTurnReport {
  const AppTurnReport({
    required this.execution,
    required this.reply,
    required this.issues,
    required this.finalMessageRef,
  });

  final String execution;
  final String reply;
  final List<AppWireIssue> issues;
  final String? finalMessageRef;
}

final class AppRunSnapshot {
  const AppRunSnapshot({
    required this.runId,
    required this.sessionId,
    required this.revision,
    required this.runtimeEpoch,
    required this.executorGeneration,
    required this.state,
    required this.progress,
    required this.report,
  });

  final String runId;
  final String sessionId;
  final int revision;
  final int runtimeEpoch;
  final int executorGeneration;
  final AppRunState state;
  final String progress;
  final AppTurnReport? report;
}

final class AppMessage {
  const AppMessage({
    required this.messageId,
    required this.role,
    required this.text,
  });

  final String messageId;
  final String role;
  final String text;
}

final class AppEventCursor {
  const AppEventCursor({required this.runtimeEpoch, required this.cursor});

  final int runtimeEpoch;
  final int cursor;
}

sealed class AppEventsRead {
  const AppEventsRead();
}

final class AppEventsPage extends AppEventsRead {
  const AppEventsPage({required this.cursor, required this.events});

  final AppEventCursor cursor;
  final List<AppRuntimeEvent> events;
}

final class AppEventsResyncRequired extends AppEventsRead {
  const AppEventsResyncRequired(this.snapshotCursor);

  final AppEventCursor snapshotCursor;
}

sealed class AppRuntimeEvent {
  const AppRuntimeEvent({
    required this.cursor,
    required this.aggregateRevision,
  });

  final int cursor;
  final int aggregateRevision;
}

final class AppCommandUpdated extends AppRuntimeEvent {
  const AppCommandUpdated({
    required super.cursor,
    required super.aggregateRevision,
    required this.receipt,
  });

  final AppCommandReceipt receipt;
}

final class AppRunUpdated extends AppRuntimeEvent {
  const AppRunUpdated({
    required super.cursor,
    required super.aggregateRevision,
    required this.run,
  });

  final AppRunSnapshot run;
}

final class FloeClient {
  FloeClient(this._transport, {String Function()? newId})
    : _newId = newId ?? _uuidV4;

  final AppWireTransport _transport;
  final String Function() _newId;
  final Map<String, Completer<dynamic>> _pending = {};
  bool _closed = false;

  PreparedStartTurn prepareStartTurn({
    required String sessionId,
    required int expectedRevision,
    required String text,
    AppContinuationRef? continuation,
    String? retryOf,
  }) {
    if (_closed) throw StateError('FloeClient is already closed.');
    if (sessionId.isEmpty ||
        expectedRevision < 0 ||
        text.trim().isEmpty ||
        utf8.encode(text).length > 64 * 1024 ||
        continuation != null &&
            (continuation.runId.isEmpty ||
                continuation.executorGeneration <= 0 ||
                continuation.level < 1 ||
                continuation.level > 3)) {
      throw const FormatException('Invalid conversation turn.');
    }
    if (retryOf != null && (retryOf.isEmpty || continuation != null)) {
      throw const FormatException('Invalid conversation retry.');
    }
    return PreparedStartTurn(
      commandId: _newId(),
      sessionId: sessionId,
      expectedRevision: expectedRevision,
      text: text,
      continuation: continuation,
      retryOf: retryOf,
    );
  }

  Future<AppCommandReceipt> submitStartTurn(
    PreparedStartTurn command, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    final requestId = _newId();
    return _correlate(requestId, () async {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'command_id': command.commandId,
        'command': {
          'kind': 'conversation.start_turn',
          'session_id': command.sessionId,
          'expected_revision': command.expectedRevision,
          'text': command.text,
          'mode': switch (command.continuation) {
            null => {'kind': 'new_turn'},
            final continuation => {
              'kind': 'continue',
              'continuation_ref': {
                'run_id': continuation.runId,
                'executor_generation': continuation.executorGeneration,
                'level': continuation.level,
              },
            },
          },
          'retry_of': ?command.retryOf,
        },
      }, timeout: timeout);
      return _commandReceipt(result, expectedCommandId: command.commandId);
    });
  }

  PreparedCancelRun prepareCancelRun(String runId) {
    if (_closed) throw StateError('FloeClient is already closed.');
    if (runId.isEmpty) throw const FormatException('Invalid Run ID.');
    return PreparedCancelRun(commandId: _newId(), runId: runId);
  }

  Future<AppCancelRunReceipt> submitCancelRun(
    PreparedCancelRun command, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    final requestId = _newId();
    return _correlate(requestId, () async {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'command_id': command.commandId,
        'command': {
          'kind': 'conversation.cancel_run',
          'run_id': command.runId,
          'reason': 'user_requested',
        },
      }, timeout: timeout);
      if (result
          case {
            'kind': 'cancel_run_receipt',
            'command_id': final String commandId,
            'run_id': final String runId,
            'runtime_epoch': final int runtimeEpoch,
            'outcome': final String outcome,
          }
          when commandId == command.commandId &&
              runId == command.runId &&
              runtimeEpoch > 0) {
        return AppCancelRunReceipt(
          commandId: commandId,
          runId: runId,
          runtimeEpoch: runtimeEpoch,
          outcome: switch (outcome) {
            'accepted' => AppCancelRunOutcome.accepted,
            _ => throw const FormatException('Invalid cancel Run outcome.'),
          },
        );
      }
      throw const FormatException('Invalid cancel Run receipt.');
    });
  }

  Future<AppCommandReceipt?> getCommand(
    String commandId, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    final requestId = _newId();
    return _correlate(requestId, () async {
      final result = await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': {'kind': 'conversation.get_command', 'command_id': commandId},
      }, timeout: timeout);
      if (result['kind'] == 'unknown_command' &&
          result['command_id'] == commandId) {
        return null;
      }
      return _commandReceipt(result, expectedCommandId: commandId);
    });
  }

  Future<AppRunSnapshot> getRun(
    String runId, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    final requestId = _newId();
    return _correlate(requestId, () async {
      final result = await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': {'kind': 'conversation.get_run', 'run_id': runId},
      }, timeout: timeout);
      return _runSnapshot(result, expectedRunId: runId);
    });
  }

  Future<AppMessage> getMessage(
    String messageId, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    final requestId = _newId();
    return _correlate(requestId, () async {
      final result = await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': {'kind': 'conversation.get_message', 'message_id': messageId},
      }, timeout: timeout);
      if (result
          case {
            'kind': 'message',
            'message_id': final String returnedId,
            'role': final String role,
            'text': final String text,
          }
          when returnedId == messageId) {
        return AppMessage(messageId: returnedId, role: role, text: text);
      }
      throw const FormatException('Invalid app message response.');
    });
  }

  Future<AppEventsRead> readEvents({
    AppEventCursor? after,
    int limit = 64,
    Duration timeout = const Duration(seconds: 3),
  }) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    if (limit <= 0 ||
        limit > 256 ||
        (after != null && (after.runtimeEpoch <= 0 || after.cursor < 0))) {
      return Future.error(const FormatException('Invalid event cursor.'));
    }
    final requestId = _newId();
    return _correlate(requestId, () async {
      final request = <String, dynamic>{
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'limit': limit,
      };
      if (after != null) {
        request['runtime_epoch'] = after.runtimeEpoch;
        request['cursor'] = after.cursor;
      }
      final result = await _transport.eventsV2(request, timeout: timeout);
      final runtimeEpoch = result['runtime_epoch'];
      if (runtimeEpoch is! int || runtimeEpoch <= 0) {
        throw const FormatException('Invalid event runtime epoch.');
      }
      if (result['kind'] == 'resync_required') {
        final snapshotCursor = result['snapshot_cursor'];
        if (snapshotCursor is! int || snapshotCursor < 0) {
          throw const FormatException('Invalid event resync cursor.');
        }
        return AppEventsResyncRequired(
          AppEventCursor(runtimeEpoch: runtimeEpoch, cursor: snapshotCursor),
        );
      }
      final nextCursor = result['next_cursor'];
      final rawEvents = result['events'];
      if (result['kind'] != 'events' ||
          after == null ||
          runtimeEpoch != after.runtimeEpoch ||
          nextCursor is! int ||
          nextCursor < after.cursor ||
          rawEvents is! List ||
          rawEvents.length > limit) {
        throw const FormatException('Invalid event batch.');
      }
      var expectedCursor = after.cursor;
      final events = rawEvents
          .map((raw) {
            final event = _map(raw);
            final cursor = event['cursor'];
            final aggregateRevision = event['aggregate_revision'];
            if (cursor is! int ||
                cursor != ++expectedCursor ||
                aggregateRevision is! int ||
                aggregateRevision <= 0 ||
                event['runtime_epoch'] != runtimeEpoch) {
              throw const FormatException('Invalid app event.');
            }
            final payload = _map(event['event']);
            switch (payload['kind']) {
              case 'command_updated':
                final receiptSource = _map(payload['receipt'])
                  ..['kind'] = 'command_receipt';
                return AppCommandUpdated(
                  cursor: cursor,
                  aggregateRevision: aggregateRevision,
                  receipt: _commandReceipt(
                    receiptSource,
                    expectedCommandId: receiptSource['command_id'] as String,
                  ),
                );
              case 'run_updated':
                final runSource = _map(payload['run'])
                  ..['kind'] = 'run_snapshot';
                final run = _runSnapshot(
                  runSource,
                  expectedRunId: runSource['run_id'] as String,
                );
                if (run.revision != aggregateRevision) {
                  throw const FormatException('Mismatched Run event revision.');
                }
                return AppRunUpdated(
                  cursor: cursor,
                  aggregateRevision: aggregateRevision,
                  run: run,
                );
              default:
                throw const FormatException('Unknown app event.');
            }
          })
          .toList(growable: false);
      if (nextCursor != expectedCursor) {
        throw const FormatException('Mismatched event batch cursor.');
      }
      return AppEventsPage(
        cursor: AppEventCursor(runtimeEpoch: runtimeEpoch, cursor: nextCursor),
        events: events,
      );
    });
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    final pending = _pending.values.toList(growable: false);
    _pending.clear();
    for (final completer in pending) {
      if (!completer.isCompleted) {
        completer.completeError(StateError('FloeClient closed.'));
      }
    }
    await _transport.close();
  }

  Future<T> _correlate<T>(String requestId, Future<T> Function() invoke) {
    if (_closed) return Future.error(StateError('FloeClient is closed.'));
    if (_pending.containsKey(requestId)) {
      return Future.error(StateError('Duplicate app request ID.'));
    }
    final completer = Completer<T>();
    _pending[requestId] = completer;
    Future.sync(invoke)
        .then(
          (value) {
            if (!completer.isCompleted) completer.complete(value);
          },
          onError: (Object error, StackTrace stackTrace) {
            if (!completer.isCompleted) {
              completer.completeError(error, stackTrace);
            }
          },
        )
        .whenComplete(() {
          if (identical(_pending[requestId], completer)) {
            _pending.remove(requestId);
          }
        });
    return completer.future;
  }
}

AppCommandReceipt _commandReceipt(
  Map<String, dynamic> result, {
  required String expectedCommandId,
}) {
  if (result
      case {
        'kind': 'command_receipt',
        'command_id': final String commandId,
        'runtime_epoch': final int runtimeEpoch,
        'admission': 'accepted',
        'run_id': final String runId,
        'session_revision': final int sessionRevision,
      }
      when commandId == expectedCommandId && runtimeEpoch > 0) {
    return AppCommandReceipt(
      commandId: commandId,
      runId: runId,
      sessionRevision: sessionRevision,
      runtimeEpoch: runtimeEpoch,
    );
  }
  throw const FormatException('Invalid app command receipt.');
}

AppRunSnapshot _runSnapshot(
  Map<String, dynamic> result, {
  required String expectedRunId,
}) {
  if (result['kind'] != 'run_snapshot' || result['run_id'] != expectedRunId) {
    throw const FormatException('Invalid app Run snapshot.');
  }
  final state = AppRunState.values.firstWhere(
    (candidate) => candidate.name == result['state'],
    orElse: () => throw const FormatException('Invalid app Run state.'),
  );
  final reportValue = result['report'];
  AppTurnReport? report;
  if (reportValue != null) {
    final source = _map(reportValue);
    final issues = source['issues'];
    if (issues is! List) {
      throw const FormatException('Invalid app turn report.');
    }
    report = AppTurnReport(
      execution: source['execution'] as String,
      reply: source['reply'] as String,
      issues: issues
          .map((issue) {
            final value = _map(issue);
            return AppWireIssue(
              value['code'] as String,
              value['message'] as String,
              metadata: Map.unmodifiable(
                _map(value['metadata'] ?? const <String, Object?>{})
                    .map((key, value) => MapEntry(key, value as String)),
              ),
            );
          })
          .toList(growable: false),
      finalMessageRef: source['final_message_ref'] as String?,
    );
  }
  return AppRunSnapshot(
    runId: result['run_id'] as String,
    sessionId: result['session_id'] as String,
    revision: result['revision'] as int,
    runtimeEpoch: result['runtime_epoch'] as int,
    executorGeneration: result['executor_generation'] as int,
    state: state,
    progress: result['progress'] as String,
    report: report,
  );
}

Map<String, dynamic> _map(Object? value) =>
    Map<String, dynamic>.from(value! as Map);

String _uuidV4() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  final hex = bytes
      .map((value) => value.toRadixString(16).padLeft(2, '0'))
      .join();
  return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-'
      '${hex.substring(12, 16)}-${hex.substring(16, 20)}-${hex.substring(20)}';
}
