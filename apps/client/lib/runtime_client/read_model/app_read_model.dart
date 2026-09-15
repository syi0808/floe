import 'dart:collection';

import 'package:flutter/foundation.dart';

import '../floe_client.dart';

enum AppReadSyncState { uninitialized, resyncRequired, synchronized }

final class AppConversationProjection {
  AppConversationProjection({
    required this.syncState,
    required this.cursor,
    required Map<String, AppCommandReceipt> commands,
    required Map<String, AppRunSnapshot> runs,
    required Set<String> pendingCommandIds,
  }) : commands = UnmodifiableMapView(Map.of(commands)),
       runs = UnmodifiableMapView(Map.of(runs)),
       pendingCommandIds = UnmodifiableSetView(Set.of(pendingCommandIds));

  final AppReadSyncState syncState;
  final AppEventCursor? cursor;
  final Map<String, AppCommandReceipt> commands;
  final Map<String, AppRunSnapshot> runs;
  final Set<String> pendingCommandIds;

  bool canSend(String sessionId) =>
      syncState == AppReadSyncState.synchronized &&
      runs.values.every(
        (run) =>
            run.sessionId != sessionId || run.state == AppRunState.finished,
      );
}

final class AppReadModel extends ChangeNotifier {
  AppReadSyncState _syncState = AppReadSyncState.uninitialized;
  AppEventCursor? _cursor;
  final Map<String, AppCommandReceipt> _commands = {};
  final Map<String, AppRunSnapshot> _runs = {};
  final Set<String> _pendingCommandIds = {};

  AppConversationProjection get conversation => AppConversationProjection(
    syncState: _syncState,
    cursor: _cursor,
    commands: _commands,
    runs: _runs,
    pendingCommandIds: _pendingCommandIds,
  );

  void markCommandPending(String commandId) {
    if (commandId.isEmpty || !_pendingCommandIds.add(commandId)) return;
    notifyListeners();
  }

  void markCommandFailed(String commandId) {
    if (_pendingCommandIds.remove(commandId)) notifyListeners();
  }

  void requireResync(AppEventsResyncRequired boundary) {
    final previousEpoch = _cursor?.runtimeEpoch;
    if (previousEpoch != null &&
        previousEpoch != boundary.snapshotCursor.runtimeEpoch) {
      _commands.clear();
      _runs.clear();
    }
    _pendingCommandIds.clear();
    _cursor = boundary.snapshotCursor;
    _syncState = AppReadSyncState.resyncRequired;
    notifyListeners();
  }

  void bootstrap({
    required AppEventCursor cursor,
    Iterable<AppCommandReceipt> commands = const [],
    Iterable<AppRunSnapshot> runs = const [],
  }) {
    if (cursor.runtimeEpoch <= 0 || cursor.cursor < 0) {
      throw const FormatException('Invalid read-model cursor.');
    }
    final nextCommands = <String, AppCommandReceipt>{};
    for (final receipt in commands) {
      if (receipt.runtimeEpoch != cursor.runtimeEpoch) {
        throw const FormatException('Command receipt epoch mismatch.');
      }
      nextCommands[receipt.commandId] = receipt;
    }
    final nextRuns = <String, AppRunSnapshot>{};
    for (final run in runs) {
      if (run.runtimeEpoch != cursor.runtimeEpoch) {
        throw const FormatException('Run snapshot epoch mismatch.');
      }
      final existing = nextRuns[run.runId];
      if (existing == null || existing.revision < run.revision) {
        nextRuns[run.runId] = run;
      }
    }
    _commands
      ..clear()
      ..addAll(nextCommands);
    _runs
      ..clear()
      ..addAll(nextRuns);
    _pendingCommandIds.clear();
    _cursor = cursor;
    _syncState = AppReadSyncState.synchronized;
    notifyListeners();
  }

  bool applyCommandReceipt(AppCommandReceipt receipt) {
    final cursor = _cursor;
    if (_syncState != AppReadSyncState.synchronized ||
        cursor == null ||
        receipt.runtimeEpoch != cursor.runtimeEpoch) {
      _sealForResync();
      return false;
    }
    final changed =
        _commands[receipt.commandId] != receipt ||
        _pendingCommandIds.contains(receipt.commandId);
    _commands[receipt.commandId] = receipt;
    _pendingCommandIds.remove(receipt.commandId);
    if (changed) notifyListeners();
    return true;
  }

  bool applyRunSnapshot(AppRunSnapshot run) {
    final cursor = _cursor;
    if (_syncState != AppReadSyncState.synchronized ||
        cursor == null ||
        run.runtimeEpoch != cursor.runtimeEpoch) {
      _sealForResync();
      return false;
    }
    final existing = _runs[run.runId];
    if (existing != null && existing.revision >= run.revision) return true;
    _runs[run.runId] = run;
    notifyListeners();
    return true;
  }

  bool applyEvents(AppEventsPage page) {
    final current = _cursor;
    if (_syncState != AppReadSyncState.synchronized || current == null) {
      return false;
    }
    if (page.cursor.runtimeEpoch != current.runtimeEpoch) {
      _sealForResync();
      return false;
    }
    var expectedCursor = current.cursor;
    for (final event in page.events) {
      if (event.cursor != ++expectedCursor) {
        _syncState = AppReadSyncState.resyncRequired;
        notifyListeners();
        return false;
      }
    }
    if (page.cursor.cursor != expectedCursor) {
      _syncState = AppReadSyncState.resyncRequired;
      notifyListeners();
      return false;
    }
    for (final event in page.events) {
      switch (event) {
        case AppCommandUpdated(:final receipt):
          if (receipt.runtimeEpoch != current.runtimeEpoch) {
            _sealForResync();
            return false;
          }
          _commands[receipt.commandId] = receipt;
          _pendingCommandIds.remove(receipt.commandId);
        case AppRunUpdated(:final run):
          if (run.runtimeEpoch != current.runtimeEpoch) {
            _sealForResync();
            return false;
          }
          final existing = _runs[run.runId];
          if (existing == null || existing.revision < run.revision) {
            _runs[run.runId] = run;
          }
      }
    }
    _cursor = page.cursor;
    notifyListeners();
    return true;
  }

  void sealForResync() => _sealForResync();

  void _sealForResync() {
    _commands.clear();
    _runs.clear();
    _pendingCommandIds.clear();
    _syncState = AppReadSyncState.resyncRequired;
    notifyListeners();
  }
}
