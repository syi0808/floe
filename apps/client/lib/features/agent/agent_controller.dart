import 'dart:async';

import 'package:flutter/foundation.dart';

import 'agent_fixture_gateway.dart';

enum AgentProgress { idle, loading, model, capability, stopping }

final class AgentController extends ChangeNotifier {
  AgentController({required this.gateway, required this.personId});

  final AgentFixtureStreamingGateway gateway;
  final String personId;
  AgentSession? session;
  List<AgentMessage> messages = [];
  AgentProgress progress = AgentProgress.idle;
  String? failure;
  bool needsReload = false;
  bool _busy = false;
  bool _disposed = false;
  bool _stopRequested = false;
  AgentSession? _runSession;
  AgentFixturePrompt? _lastPrompt;

  bool get busy => _busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  bool get canSend =>
      !_busy && !needsReload && !needsRecovery && session != null;
  bool get canRetry => canSend && failure != null && _lastPrompt != null;

  Future<void> load({bool newSession = false}) async {
    if (_busy || _disposed) return;
    _busy = true;
    progress = AgentProgress.loading;
    _notify();
    try {
      final result = newSession
          ? await gateway.startAgentFixture(personId)
          : await gateway.resumeAgentFixture(personId);
      _acceptSession(result.session);
      needsReload = false;
    } on Object {
      failure = 'storage_unavailable';
      needsReload = true;
    } finally {
      _busy = false;
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> recover() async {
    if (_busy || _disposed || !needsRecovery) return;
    _busy = true;
    progress = AgentProgress.loading;
    _notify();
    try {
      final result = await gateway.recoverAgentFixture(session!);
      _acceptSession(result.session);
      needsReload = false;
    } on Object {
      failure = 'storage_unavailable';
      needsReload = true;
    } finally {
      _busy = false;
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> retry() async {
    if (canRetry) await send(_lastPrompt!);
  }

  Future<void> send(AgentFixturePrompt prompt) async {
    if (!canSend || _disposed) return;
    final original = session!;
    _runSession = original;
    _lastPrompt = prompt;
    _busy = true;
    _stopRequested = false;
    failure = null;
    progress = AgentProgress.model;
    _notify();
    var done = false;
    try {
      var update = await gateway.beginAgentFixtureRun(original, prompt);
      var sequence = 0;
      while (true) {
        _validateUpdate(original, update, sequence);
        for (final event in update.events) {
          switch (event.event) {
            case AgentMessageCommitted(:final message):
              messages = [...messages, message];
            case AgentModelStarted():
              progress = AgentProgress.model;
            case AgentCapabilityStarted():
              progress = AgentProgress.capability;
            case AgentStarted() || AgentFinished():
              break;
          }
        }
        sequence = update.nextSequence;
        if (_stopRequested) progress = AgentProgress.stopping;
        _notify();
        if (update.done) {
          done = true;
          _runSession = null;
          if (update.session case final saved?) {
            _acceptSession(saved);
            needsReload = false;
          } else {
            failure = update.failure;
            needsReload = true;
          }
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        update = await gateway.pollAgentFixtureRun(original, sequence);
      }
    } on Object {
      failure = 'transport_unavailable';
      needsReload = true;
    } finally {
      try {
        if (!done) {
          var update = await gateway.stopAgentFixtureRun(original);
          for (var attempt = 0; !update.done && attempt < 25; attempt++) {
            await Future<void>.delayed(const Duration(milliseconds: 80));
            update = await gateway.pollAgentFixtureRun(original, 0);
          }
          done = update.done;
        }
        if (done) await gateway.releaseAgentFixtureRun(original);
      } on Object {
        needsReload = true;
        failure ??= 'transport_unavailable';
      }
      _runSession = null;
      _busy = false;
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> stop() async {
    final original = _runSession;
    if (original == null || _stopRequested) return;
    _stopRequested = true;
    progress = AgentProgress.stopping;
    _notify();
    try {
      await gateway.stopAgentFixtureRun(original);
    } on Object {
      failure = 'transport_unavailable';
      needsReload = true;
      _notify();
    }
  }

  void _acceptSession(AgentSession saved) {
    if (saved.personId != personId) {
      throw const FormatException('Agent Person mismatch.');
    }
    session = saved;
    messages = List.of(saved.messages);
    failure = saved.lastOutcome?.failure;
    final lastUser = messages
        .whereType<AgentTextMessage>()
        .where((message) => message.kind == AgentMessageKind.user)
        .lastOrNull;
    _lastPrompt = AgentFixturePrompt.values
        .where((prompt) => prompt.sampleText == lastUser?.text)
        .firstOrNull;
  }

  void _validateUpdate(
    AgentSession original,
    AgentRunUpdate update,
    int sequence,
  ) {
    if (update.sessionId != original.id ||
        update.expectedRevision != original.revision ||
        update.nextSequence != sequence + update.events.length ||
        update.events.any((event) => event.sessionId != original.id) ||
        update.session != null && update.session!.id != original.id) {
      throw const FormatException('Agent run sequence mismatch.');
    }
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(stop());
    super.dispose();
  }
}
