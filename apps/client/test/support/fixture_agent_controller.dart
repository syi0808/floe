import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

import 'agent_fixture_gateway.dart';

import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

enum AgentProgress {
  idle,
  loading,
  model,
  expertModel,
  correcting,
  capability,
  stopping,
}

final class FixtureAgentController extends ChangeNotifier {
  factory FixtureAgentController({
    required AgentFixtureStreamingGateway gateway,
    required String personId,
    Duration loadTimeout = const Duration(seconds: 10),
  }) => FixtureAgentController._(gateway, personId, loadTimeout);

  FixtureAgentController._(this.gateway, this.personId, this.loadTimeout);

  final AgentFixtureStreamingGateway gateway;
  final String personId;
  final Duration loadTimeout;
  AgentSession? session;
  List<AgentMessage> messages = [];
  AgentProgress progress = AgentProgress.idle;
  String? failure;
  String? recoveryAction;
  String? failureDomain;
  String? failureCategory;
  List<String> failureSafeActions = const [];
  List<String> failureAffectedRefs = const [];
  String? failureIncidentId;
  String? failureRetryPolicy;
  bool needsReload = false;
  bool _busy = false;
  bool _disposed = false;
  bool _stopRequested = false;
  AgentSession? _runSession;
  AgentFixturePrompt? _lastPrompt;
  String? _lastConversationText;
  String? _lastConversationRunId;
  AgentVaultState? vaultState;
  bool _sealed = false;
  Completer<void>? _operationDone;
  Future<void>? _locking;
  bool get usesVault => gateway is AgentVaultGateway;
  bool get isGeneralConversation => false;
  bool get isConnectedConversation => isGeneralConversation;
  bool get isPersonalConversation => isGeneralConversation;

  bool get busy => _busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  bool get _conversationBusy => _busy;
  bool get canStartConversation => !_conversationBusy && !_disposed && !running;
  bool get canSend =>
      !_conversationBusy &&
      !_disposed &&
      !_sealed &&
      _locking == null &&
      !needsReload &&
      !needsRecovery &&
      session != null &&
      !isGeneralConversation;
  bool get canContinue =>
      canSend &&
      session?.continuation != null &&
      isGeneralConversation &&
      _lastConversationText != null;
  bool get canRetry =>
      canSend &&
      !canContinue &&
      failure != null &&
      recoveryAction == 'retry_read' &&
      (isGeneralConversation
          ? _lastConversationText != null && _lastConversationRunId != null
          : _lastPrompt != null);

  Future<void> load({bool newSession = false}) async {
    if (_disposed) return;
    if (_locking case final locking?) {
      await locking;
    }
    if (busy || _disposed) return;
    _sealed = false;
    _lastConversationRunId = null;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      if (gateway case final AgentVaultGateway vault) {
        final state = await vault.vaultStatus(personId).timeout(loadTimeout);
        if (_sealed) return;
        vaultState = switch (state) {
          AgentVaultState.missing =>
            await vault.createVault(personId).timeout(loadTimeout),
          AgentVaultState.locked =>
            await vault.unlockVault(personId).timeout(loadTimeout),
          _ => state,
        };
        if (_sealed) return;
        if (vaultState != AgentVaultState.ready) {
          throw const AgentVaultException('vault_unavailable');
        }
      }

      final result = newSession
          ? await gateway.startAgentFixture(personId).timeout(loadTimeout)
          : await gateway.resumeAgentFixture(personId).timeout(loadTimeout);
      _acceptSession(result.session);
      needsReload = false;
    } on Object catch (error, stackTrace) {
      _recordError('load', error, stackTrace);
      _failFromError(error, 'storage_unavailable');
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> recover() async {
    if (busy || _disposed || !needsRecovery) return;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      final original = session!;

      final result = await gateway.recoverAgentFixture(original);
      _acceptSession(result.session);
      needsReload = false;
    } on Object catch (error, stackTrace) {
      _recordError('recover', error, stackTrace, sessionId: session?.id);
      _failFromError(error, 'storage_unavailable');
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> retry() async {
    if (canRetry) await send(_lastPrompt!);
  }

  Future<void> send(AgentFixturePrompt prompt) async {
    if (!canSend || _disposed || isGeneralConversation) return;
    final original = session!;
    _runSession = original;
    _lastPrompt = prompt;
    _begin();
    _stopRequested = false;
    _clearFailure();
    progress = AgentProgress.model;
    _notify();
    var done = false;
    var started = false;
    try {
      var update = await gateway.beginAgentFixtureRun(original, prompt);
      started = true;
      var sequence = 0;
      while (true) {
        _validateUpdate(original, update, sequence);
        _acceptEvents(update.events);
        sequence = update.nextSequence;
        if (_stopRequested) progress = AgentProgress.stopping;
        _notify();
        if (update.done) {
          done = true;
          _runSession = null;
          if (update.session case final saved?) {
            _acceptSession(saved);
            if (update.failure == null) {
              needsReload = false;
            } else {
              _failFromUpdate(update);
            }
          } else {
            _failFromUpdate(update, fallback: 'storage_unavailable');
          }
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        update = await gateway.pollAgentFixtureRun(original, sequence);
      }
    } on Object catch (error, stackTrace) {
      _recordError('fixture_turn', error, stackTrace, sessionId: original.id);
      _failFromError(error, 'transport_unavailable');
    } finally {
      try {
        if (!done && started) {
          var update = await gateway.stopAgentFixtureRun(original);
          for (var attempt = 0; !update.done && attempt < 25; attempt++) {
            await Future<void>.delayed(const Duration(milliseconds: 80));
            update = await gateway.pollAgentFixtureRun(original, 0);
          }
          done = update.done;
        }
        if (done && started) await gateway.releaseAgentFixtureRun(original);
      } on Object {
        needsReload = true;
        failure ??= 'transport_unavailable';
      }
      _runSession = null;
      _end();
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
    if (_sealed) return;
    if (saved.personId != personId || saved.scope != null) {
      throw const FormatException('Agent Person mismatch.');
    }
    session = saved;
    messages = List.of(saved.messages);
    _clearFailure();
    failure = saved.lastOutcome?.failure;
    final lastUser = messages
        .whereType<AgentTextMessage>()
        .where((message) => message.kind == AgentMessageKind.user)
        .lastOrNull;
    if (saved.dataClasses.singleOrNull == 'personal') {
      _lastPrompt = null;
      _lastConversationText = lastUser?.text;
    } else {
      _lastPrompt = AgentFixturePrompt.values
          .where((prompt) => prompt.sampleText == lastUser?.text)
          .firstOrNull;
      _lastConversationText = null;
    }
  }

  void _acceptEvents(List<AgentEvent> events) {
    for (final event in _sealed ? <AgentEvent>[] : events) {
      switch (event.event) {
        case AgentMessageCommitted(:final message):
          messages = [...messages, message];
        case AgentModelAttempt(:final scopeId, :final state, :final attempt):
          if (state == 'started') {
            progress = attempt == 2
                ? AgentProgress.correcting
                : scopeId == event.sessionId
                ? AgentProgress.model
                : AgentProgress.expertModel;
          }
        case AgentModelStarted():
          progress = AgentProgress.model;
        case AgentCapabilityStarted():
          progress = AgentProgress.capability;
        case AgentDelegationStarted():
          progress = AgentProgress.expertModel;
        case AgentStarted() || AgentFinished():
          break;
      }
    }
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

  Future<void> unlock({bool create = false}) async {
    if (busy ||
        _disposed ||
        _locking != null ||
        gateway is! AgentVaultGateway) {
      return;
    }
    final vault = gateway as AgentVaultGateway;
    _sealed = false;
    _begin();
    progress = AgentProgress.loading;
    _clearFailure();
    _notify();
    try {
      final state = create
          ? await vault.createVault(personId)
          : await vault.unlockVault(personId);
      if (_sealed) return;
      vaultState = state;
      if (state != AgentVaultState.ready) {
        throw const AgentVaultException('vault_unavailable');
      }
      _acceptSession((await gateway.resumeAgentFixture(personId)).session);
      needsReload = false;
    } on Object catch (error) {
      _failFromError(error, 'vault_unavailable');
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> closeView() {
    if (gateway is! AgentVaultGateway) return stop();
    return _locking ??= _lock().whenComplete(() => _locking = null);
  }

  Future<void> _lock() async {
    _sealed = true;
    session = null;
    messages = [];
    _lastPrompt = null;
    _lastConversationRunId = null;
    vaultState = AgentVaultState.locked;
    _notify();
    await stop();
    await _operationDone?.future;
    _begin();
    try {
      await (gateway as AgentVaultGateway).lockVault(personId);
      vaultState = AgentVaultState.locked;
      _clearFailure();
      needsReload = false;
    } on Object {
      _fail('vault_unavailable');
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  void _begin() {
    _busy = true;
    _operationDone = Completer<void>();
  }

  void _end() {
    _busy = false;
    _operationDone?.complete();
    _operationDone = null;
  }

  void _clearFailure() {
    failure = null;
    recoveryAction = null;
    failureDomain = null;
    failureCategory = null;
    failureSafeActions = const [];
    failureAffectedRefs = const [];
    failureIncidentId = null;
    failureRetryPolicy = null;
  }

  void _failFromError(Object error, String fallback) {
    final source = error is AgentVaultException ? error : null;
    _fail(
      source?.reasonCode ?? source?.failure ?? fallback,
      recoveryAction: source?.recoveryAction,
      domain: source?.domain,
      category: source?.category,
      safeActions: source?.safeActions,
      affectedRefs: source?.affectedRefs,
      incidentId: source?.incidentId,
      retryPolicy: source?.retryPolicy,
      reloadRequired: source?.reloadRequired,
      sealSession: source?.sealSession,
    );
  }

  void _failFromUpdate(AgentRunUpdate update, {String? fallback}) {
    _fail(
      update.failureReasonCode ?? update.failure ?? fallback ?? 'unknown',
      recoveryAction: update.recoveryAction,
      domain: update.failureDomain,
      category: update.failureCategory,
      safeActions: update.failureSafeActions,
      affectedRefs: update.failureAffectedRefs,
      incidentId: update.failureIncidentId,
      retryPolicy: update.failureRetryPolicy,
      reloadRequired: update.failureReloadRequired,
      sealSession: update.failureSealSession,
    );
  }

  void _fail(
    String reason, {
    String? recoveryAction,
    String? domain,
    String? category,
    List<String>? safeActions,
    List<String>? affectedRefs,
    String? incidentId,
    String? retryPolicy,
    bool? reloadRequired,
    bool? sealSession,
  }) {
    failure = reason;
    this.recoveryAction = recoveryAction;
    failureDomain = domain;
    failureCategory = category;
    failureSafeActions = List.unmodifiable(safeActions ?? const []);
    failureAffectedRefs = List.unmodifiable(affectedRefs ?? const []);
    failureIncidentId = incidentId;
    failureRetryPolicy = retryPolicy;
    // Reload and seal are decided by the owner; this controller only applies
    // what the failure envelope reported.
    needsReload = reloadRequired ?? false;
    if (usesVault && (sealSession ?? false)) {
      session = null;
      messages = [];
      vaultState = AgentVaultState.unavailable;
    }
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(stop());
    super.dispose();
  }

  void _recordError(
    String operation,
    Object error,
    StackTrace stackTrace, {
    String? sessionId,
  }) {
    final vaultError = error is AgentVaultException ? error : null;
    AppDiagnostics.error(
      component: 'agent',
      operation: vaultError?.stage ?? operation,
      error: error,
      stackTrace: stackTrace,
      failure: vaultError?.failure,
      failureDomain: vaultError?.domain,
      failureCategory: vaultError?.category,
      reasonCode: vaultError?.reasonCode,
      incidentId: vaultError?.incidentId,
      safeActions: vaultError?.safeActions ?? const [],
      requestId: vaultError?.requestId,
      sessionId: sessionId,
      retryable: vaultError?.retryable,
    );
  }
}
