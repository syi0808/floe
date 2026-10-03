import 'package:floe_client/features/vault/application/vault_controller.dart';

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';

enum AgentProgress {
  idle,
  loading,
  model,
  expertModel,
  correcting,
  capability,
  stopping,
}

final class ConversationController extends ChangeNotifier {
  factory ConversationController({
    required AgentConversationGateway gateway,
    required String personId,
    Duration loadTimeout = const Duration(seconds: 10),
    required LocalOwnerGateways owners,
  }) => ConversationController._(gateway, personId, loadTimeout, owners);

  ConversationController._(
    this.gateway,
    this.personId,
    this.loadTimeout,
    this.owners,
  ) {
    final vault = owners.vault;
    if (vault == null || vault.personId != personId) {
      throw ArgumentError(
        'Conversation requires the admitted Person’s shared Vault readiness.',
      );
    }
    vaultController = vault..addListener(_vaultChanged);
    _conversationRuntime.readModel.addListener(_notify);
  }

  final AgentConversationGateway gateway;
  final LocalOwnerGateways owners;
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
  bool _viewAttached = true;
  bool _stopRequested = false;
  AgentSession? _runSession;
  String? _lastConversationText;
  String? _lastConversationRunId;
  String? _observedRunId;
  AgentConversationTurnRequest? _conversationRun;
  late final VaultController vaultController;
  bool _sealed = false;
  final Map<String, AgentInteractionSnapshot> _interactions = {};
  final Set<String> _interactionBusy = {};
  final Map<String, String> _interactionFailures = {};

  AgentInteractionSnapshot? interactionFor(String interactionId) =>
      _interactions[interactionId];
  bool interactionBusyFor(String interactionId) =>
      _interactionBusy.contains(interactionId);
  String? interactionFailureFor(String interactionId) =>
      _interactionFailures[interactionId];

  AgentInteractionGateway get _interactionGateway => gateway.interactionGateway;

  bool get hasInteractions => true;

  bool canDecideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) =>
      hasInteractions &&
      !_busy &&
      !_sealed &&
      !_disposed &&
      !needsReload &&
      !needsRecovery &&
      vaultController.ready &&
      session?.id == snapshot.sessionId &&
      session?.personId == personId &&
      !_interactionBusy.contains(snapshot.id) &&
      snapshot.actions.contains(switch (decision) {
        AgentInteractionDecision.approve => AgentInteractionAction.allow,
        AgentInteractionDecision.deny => AgentInteractionAction.deny,
        AgentInteractionDecision.dismiss => AgentInteractionAction.dismiss,
      });

  bool canRefreshInteraction(AgentInteractionSnapshot snapshot) =>
      hasInteractions &&
      !_busy &&
      !_sealed &&
      !_disposed &&
      !needsReload &&
      !needsRecovery &&
      vaultController.ready &&
      session?.id == snapshot.sessionId &&
      session?.personId == personId &&
      !_interactionBusy.contains(snapshot.id) &&
      snapshot.actions.contains(AgentInteractionAction.refresh);

  /// Load one card snapshot when the panel first shows its reference.
  Future<void> ensureInteraction(String interactionId) async {
    final gateway = _interactionGateway;
    final original = session;
    if (original == null ||
        _interactions.containsKey(interactionId) ||
        _interactionBusy.contains(interactionId) ||
        _sealed ||
        _disposed) {
      return;
    }
    _interactionBusy.add(interactionId);
    _interactionFailures.remove(interactionId);
    _notify();
    try {
      final snapshot = await gateway.loadInteraction(interactionId);
      if (_sealed || _disposed || session?.id != original.id) return;
      if (snapshot == null) {
        _interactionFailures[interactionId] = 'interaction_unavailable';
        return;
      }
      _acceptInteractionSnapshot(snapshot, original);
    } on Object catch (error) {
      if (_sealed || _disposed || session?.id != original.id) return;
      _interactionFailures[interactionId] = _interactionReason(error);
    } finally {
      _interactionBusy.remove(interactionId);
      _notify();
    }
  }

  /// Reload every card of the current Session, oldest first.
  Future<void> refreshInteractions() async {
    final gateway = _interactionGateway;
    final original = session;
    if (original == null || _busy || _sealed || _disposed) {
      return;
    }
    _begin();
    _notify();
    try {
      final snapshots = await gateway.loadSessionInteractions(original.id);
      if (_sealed || _disposed || session?.id != original.id) return;
      _interactions.clear();
      _interactionFailures.clear();
      for (final snapshot in snapshots) {
        _acceptInteractionSnapshot(snapshot, original);
      }
    } on Object catch (error, stackTrace) {
      if (_sealed || _disposed) return;
      _recordError(
        'interaction_list',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _failFromError(error, 'transport_unavailable');
    } finally {
      _end();
      _notify();
    }
  }

  Future<void> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) async {
    final gateway = _interactionGateway;
    if (!canDecideInteraction(snapshot, decision)) return;
    final original = session!;
    _begin();
    _interactionBusy.add(snapshot.id);
    _interactionFailures.remove(snapshot.id);
    _notify();
    try {
      final result = await gateway.decideInteraction(snapshot, decision);
      if (_sealed || _disposed || session?.id != original.id) return;
      _acceptInteractionSnapshot(result.snapshot, original);
      switch (result.outcome) {
        case AgentInteractionResolveOutcome.stale:
          // The review moved under this card: the snapshot is current
          // and the person taps again to decide it.
          _interactionFailures[snapshot.id] = 'interaction_stale';
        case AgentInteractionResolveOutcome.wrongDevice:
          _interactionFailures[snapshot.id] = 'interaction_wrong_device';
        case AgentInteractionResolveOutcome.expired:
          _interactionFailures[snapshot.id] = 'interaction_expired';
        case AgentInteractionResolveOutcome.resolving:
          break;
        case AgentInteractionResolveOutcome.resolved:
        case AgentInteractionResolveOutcome.denied:
        case AgentInteractionResolveOutcome.dismissed:
        case AgentInteractionResolveOutcome.superseded:
        case AgentInteractionResolveOutcome.pending:
          break;
      }
      if (result.linkedRun != null &&
          result.outcome == AgentInteractionResolveOutcome.resolved) {
        await _observeLinkedRun(result.linkedRun!, original);
      }
    } on Object catch (error, stackTrace) {
      if (_sealed || _disposed || session?.id != original.id) return;
      _recordError(
        'interaction_decide',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _interactionFailures[snapshot.id] = _interactionReason(error);
      _failFromError(error, 'transport_unavailable');
    } finally {
      _interactionBusy.remove(snapshot.id);
      _end();
      _notify();
    }
  }

  Future<void> refreshInteraction(AgentInteractionSnapshot snapshot) async {
    final gateway = _interactionGateway;
    if (!canRefreshInteraction(snapshot)) return;
    final original = session!;
    _begin();
    _interactionBusy.add(snapshot.id);
    _interactionFailures.remove(snapshot.id);
    _notify();
    try {
      final result = await gateway.refreshInteraction(snapshot);
      if (_sealed || _disposed || session?.id != original.id) return;
      _acceptInteractionSnapshot(result.snapshot, original);
      switch (result.outcome) {
        case AgentInteractionRefreshOutcome.stale:
          _interactionFailures[snapshot.id] = 'interaction_stale';
        case AgentInteractionRefreshOutcome.wrongDevice:
          _interactionFailures[snapshot.id] = 'interaction_wrong_device';
        case AgentInteractionRefreshOutcome.expired:
          _interactionFailures[snapshot.id] = 'interaction_expired';
        case AgentInteractionRefreshOutcome.resolved:
        case AgentInteractionRefreshOutcome.pending:
        case AgentInteractionRefreshOutcome.superseded:
        case AgentInteractionRefreshOutcome.resolving:
        case AgentInteractionRefreshOutcome.denied:
        case AgentInteractionRefreshOutcome.dismissed:
          break;
      }
      if (result.linkedRun != null &&
          result.outcome == AgentInteractionRefreshOutcome.resolved) {
        await _observeLinkedRun(result.linkedRun!, original);
      }
    } on Object catch (error, stackTrace) {
      if (_sealed || _disposed || session?.id != original.id) return;
      _recordError(
        'interaction_refresh',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _interactionFailures[snapshot.id] = _interactionReason(error);
      _failFromError(error, 'transport_unavailable');
    } finally {
      _interactionBusy.remove(snapshot.id);
      _end();
      _notify();
    }
  }

  Future<void> _observeLinkedRun(
    AgentLinkedRun linked,
    AgentSession original,
  ) async {
    final runtime = _conversationRuntime;
    await _observeReceipt(
      AppCommandReceipt(
        commandId: linked.commandId,
        runId: linked.runId,
        sessionRevision: linked.sessionRevision,
        runtimeEpoch: linked.runtimeEpoch,
      ),
      original,
      runtime,
    );
  }

  Future<void> _observeReceipt(
    AppCommandReceipt receipt,
    AgentSession original,
    ConversationRuntimeGateway runtime,
  ) async {
    _runSession = original;
    progress = AgentProgress.model;
    _notify();
    try {
      final completion = await runtime.observeConversationRun(
        receipt,
        session!,
        onRun: (run) {
          _observedRunId = run.runId;
          if (_disposed || _sealed || session?.id != original.id) return;
          _notify();
        },
      );
      if (!_disposed && !_sealed && session?.id == original.id) {
        _acceptSession(completion.session);
        _lastConversationRunId = completion.run.runId;
        needsReload = false;
        final issue = completion.run.report?.issues.firstOrNull;
        if (issue != null) {
          _acceptConversationIssue(issue);
        }
      }
    } finally {
      _runSession = null;
      _observedRunId = null;
    }
  }

  void _acceptInteractionSnapshot(
    AgentInteractionSnapshot snapshot,
    AgentSession original,
  ) {
    if (snapshot.sessionId != original.id ||
        original.personId != personId ||
        session?.id != original.id) {
      throw const FormatException('Interaction scope mismatch.');
    }
    _interactions[snapshot.id] = snapshot;
    _interactionFailures.remove(snapshot.id);
  }

  String _interactionReason(Object error) {
    if (error is AgentVaultException) {
      return error.reasonCode ?? error.failure;
    }
    return 'transport_unavailable';
  }

  void _clearInteractions() {
    _interactions.clear();
    _interactionBusy.clear();
    _interactionFailures.clear();
  }

  bool get isGeneralConversation => session != null;
  bool get isConnectedConversation => isGeneralConversation;
  bool get isPersonalConversation => isGeneralConversation;

  bool get busy => _busy || vaultController.busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  ConversationRuntimeGateway get _conversationRuntime =>
      gateway.conversationRuntime;
  bool get _conversationBusy => _busy || !vaultController.ready;
  bool get canStartConversation => !_conversationBusy && !_disposed && !running;
  bool get canSend =>
      !_conversationBusy &&
      !_disposed &&
      !_sealed &&
      !needsReload &&
      !needsRecovery &&
      session != null &&
      isGeneralConversation &&
      _conversationRuntime.readModel.conversation.canSend(session!.id);
  bool get canContinue =>
      canSend &&
      session?.continuation != null &&
      isGeneralConversation &&
      _lastConversationText != null;
  bool get canRetry =>
      canSend &&
      !canContinue &&
      failure != null &&
      failureSafeActions.contains('retry') &&
      isGeneralConversation &&
      _lastConversationText != null &&
      _lastConversationRunId != null;

  Future<void> load({bool newSession = false}) async {
    if (_disposed) return;
    if (busy || _disposed || !vaultController.ready) return;
    _sealed = false;
    _lastConversationRunId = null;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      final conversation = gateway;
      final resumed = newSession
          ? null
          : await conversation
                .resumeConversation(personId)
                .timeout(loadTimeout);
      if (_sealed || _disposed) return;
      final saved =
          resumed ??
          await conversation.startConversation(personId).timeout(loadTimeout);
      if (_sealed || _disposed || !vaultController.ready) return;
      _acceptSession(saved);
      await _conversationRuntime
          .synchronizeConversation(saved)
          .timeout(loadTimeout);
      needsReload = false;
      if (saved.activeTurn != null) {
        _runSession = saved;
        progress = AgentProgress.model;
        final completion = await _conversationRuntime.observeSessionRun(
          saved,
          onRun: (run) {
            _observedRunId = run.runId;
            if (!_disposed && !_sealed) _notify();
          },
        );
        if (!_disposed && !_sealed) _acceptSession(completion.session);
        _runSession = null;
        _observedRunId = null;
      }
    } on Object catch (error, stackTrace) {
      _recordError('load', error, stackTrace);
      _failFromError(
        error,
        session != null ? 'transport_unavailable' : 'storage_unavailable',
      );
    } finally {
      _runSession = null;
      _observedRunId = null;
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

      final saved = await gateway.recoverConversation(original);
      _acceptSession(saved);
      await _conversationRuntime.synchronizeConversation(saved);
      needsReload = false;
    } on Object catch (error, stackTrace) {
      _recordError('recover', error, stackTrace, sessionId: session?.id);
      _failFromError(error, 'transport_unavailable');
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> retry() async {
    if (!canRetry) return;
    await _sendConversationText(
      _lastConversationText!,
      retryOf: _lastConversationRunId,
    );
  }

  Future<void> continueTurn() async {
    if (!canContinue) return;
    await _sendConversationText(_lastConversationText!, continuation: true);
  }

  bool acceptsConversationText(String text) {
    final normalized = text.trim();
    return normalized.isNotEmpty && utf8.encode(normalized).length <= 8192;
  }

  Future<void> sendText(String text) async {
    if (!acceptsConversationText(text)) {
      failure = 'invalid_input';
      needsReload = false;
      _notify();
      return;
    }
    await _sendConversationText(text);
  }

  Future<void> _sendConversationText(
    String text, {
    bool continuation = false,
    String? retryOf,
  }) async {
    final normalized = text.trim();
    if (!canSend ||
        _disposed ||
        !isGeneralConversation ||
        !acceptsConversationText(normalized)) {
      return;
    }
    final original = session!;
    final request = AgentConversationTurnRequest(
      session: original,
      text: normalized,
      continuation: continuation,
      retryOf: retryOf,
    );
    _conversationRun = request;
    _runSession = original;
    _lastConversationText = normalized;
    _begin();
    _stopRequested = false;
    _clearFailure();
    progress = AgentProgress.model;
    _notify();
    await _runConversationCommand(_conversationRuntime, original, request);
  }

  Future<void> _runConversationCommand(
    ConversationRuntimeGateway runtime,
    AgentSession original,
    AgentConversationTurnRequest request,
  ) async {
    try {
      final completion = await runtime.runConversationTurn(
        request,
        onRun: (run) {
          _observedRunId = run.runId;
          if (_disposed || _sealed || session?.id != original.id) return;
          progress = run.state == AppRunState.cancelling || _stopRequested
              ? AgentProgress.stopping
              : AgentProgress.model;
          _notify();
        },
      );
      _runSession = null;
      if (!_disposed && !_sealed && session?.id == original.id) {
        _acceptSession(completion.session);
        _lastConversationRunId = completion.run.runId;
        needsReload = false;
        final issue = completion.run.report?.issues.firstOrNull;
        if (issue != null) {
          _acceptConversationIssue(issue);
        }
      }
    } on Object catch (error, stackTrace) {
      if (!_disposed && !_sealed) {
        _recordError(
          'conversation_turn',
          error,
          stackTrace,
          sessionId: original.id,
        );
        _failFromError(error, 'transport_unavailable');
      }
    } finally {
      _conversationRun = null;
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
      if (_observedRunId case final runId?) {
        await _conversationRuntime.cancelObservedRun(runId);
      } else if (_conversationRun case final request?) {
        await _conversationRuntime.cancelConversationTurn(request);
      }
    } on Object {
      failure = 'transport_unavailable';
      needsReload = true;
      _notify();
    }
  }

  void _acceptSession(AgentSession saved) {
    if (_sealed) return;
    if (saved.personId != personId) {
      throw const FormatException('Agent Person mismatch.');
    }
    _clearInteractions();
    session = saved;
    messages = List.of(saved.messages);
    _clearFailure();
    failure = saved.lastOutcome?.failure;
    if (saved.lastOutcome?.issue?.ownerFailure case final ownerFailure?) {
      _applyOwnerFailure(ownerFailure);
      if (ownerFailure.reloadRequired || ownerFailure.sealSession) {
        // Preserve the owner's barrier through callers that would otherwise
        // mark a successful session read as ready to send a new turn.
        throw AgentVaultException.fromAppWire(
          ownerFailure.reason,
          ownerFailure: ownerFailure,
        );
      }
    } else if (saved.lastOutcome?.issue case final issue?) {
      _fail(issue.reason, reloadRequired: true);
      throw AgentVaultException(issue.reason, reloadRequired: true);
    }
    final lastUser = messages
        .whereType<AgentTextMessage>()
        .where((message) => message.kind == AgentMessageKind.user)
        .lastOrNull;
    _lastConversationText = lastUser?.text;
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void _vaultChanged() {
    if (_disposed) return;
    if (!vaultController.ready) {
      _sealed = true;
      _clearInteractions();
      session = null;
      messages = [];
    }
    _notify();
  }

  void detachView() {
    if (!_viewAttached) return;
    _viewAttached = false;
    _conversationRuntime.readModel.removeListener(_notify);
  }

  void attachView() {
    if (_disposed || _viewAttached) return;
    _viewAttached = true;
    _conversationRuntime.readModel.addListener(_notify);
    _notify();
  }

  void _begin() {
    _busy = true;
  }

  void _end() {
    _busy = false;
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

  OwnerFailure? _ownerFailure(Object error) => switch (error) {
    AgentVaultException() => error.ownerFailure,
    NativeTransportException() => error.ownerFailure,
    _ => null,
  };

  void _applyOwnerFailure(OwnerFailure owner) {
    if (!_sealed && vaultController.ready) {
      vaultController.reportFailure(
        AgentVaultException.fromAppWire(owner.reason, ownerFailure: owner),
      );
    }
    _fail(
      owner.reason,
      recoveryAction: owner.recovery,
      domain: owner.domain,
      category: owner.category,
      safeActions: owner.safeActions.toList(growable: false),
      incidentId: owner.incidentId,
      reloadRequired: owner.reloadRequired,
      sealSession: owner.sealSession,
    );
  }

  void _failFromError(Object error, String fallback) {
    final owner = _ownerFailure(error);
    if (owner != null) {
      _applyOwnerFailure(owner);
      return;
    }
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
      // Structural errors cannot authorize continued work or model retry.
      reloadRequired: source?.reloadRequired ?? true,
      sealSession: source?.sealSession,
    );
  }

  void _acceptConversationIssue(AppWireIssue issue) {
    final owner = issue.ownerFailure;
    if (owner != null) {
      _applyOwnerFailure(owner);
      return;
    }
    _fail(issue.code, reloadRequired: true);
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
    if (sealSession ?? false) _clearInteractions();
    failure = reason;
    this.recoveryAction = recoveryAction;
    failureDomain = domain;
    failureCategory = category;
    failureSafeActions = List.unmodifiable(safeActions ?? const []);
    failureAffectedRefs = List.unmodifiable(affectedRefs ?? const []);
    failureIncidentId = incidentId;
    failureRetryPolicy = retryPolicy;
    needsReload = reloadRequired ?? false;
    if (sealSession ?? false) {
      _sealed = true;
      session = null;
      messages = [];
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _conversationRuntime.readModel.removeListener(_notify);
    vaultController.removeListener(_vaultChanged);
    super.dispose();
  }

  void _recordError(
    String operation,
    Object error,
    StackTrace stackTrace, {
    String? sessionId,
  }) {
    final vaultError = error is AgentVaultException ? error : null;
    final owner = _ownerFailure(error);
    AppDiagnostics.error(
      component: 'agent',
      operation: vaultError?.stage ?? operation,
      error: error,
      stackTrace: stackTrace,
      failure: owner?.reason ?? vaultError?.failure,
      failureDomain: owner?.domain ?? vaultError?.domain,
      failureCategory: owner?.category ?? vaultError?.category,
      reasonCode: owner?.reason ?? vaultError?.reasonCode,
      incidentId: owner?.incidentId ?? vaultError?.incidentId,
      safeActions:
          owner?.safeActions.toList(growable: false) ??
          vaultError?.safeActions ??
          const [],
      requestId: vaultError?.requestId,
      sessionId: sessionId,
      retryable: vaultError?.retryable,
    );
  }
}
