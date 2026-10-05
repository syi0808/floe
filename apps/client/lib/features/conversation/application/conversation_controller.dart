import 'package:floe_client/features/vault/application/vault_controller.dart';

import 'dart:async';

import 'conversation_observation.dart';

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
  bool loadingEarlier = false;
  String? earlierFailure;
  bool? _historyHasEarlier;
  int _historyGeneration = 0;
  bool get hasEarlierMessages =>
      _historyHasEarlier ?? session?.hasEarlierMessages ?? false;

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
  int _runUiGeneration = 0;
  late final VaultController vaultController;
  bool _sealed = false;
  int _adoptionEpoch = 0;
  ConversationObservation _observation = ConversationObservation();

  bool _acceptsEpoch(int epoch) =>
      !_disposed &&
      !_sealed &&
      vaultController.ready &&
      epoch == _adoptionEpoch;

  void _advanceEpoch() {
    _observation.stop();
    _observation = ConversationObservation();
    _adoptionEpoch++;
    _historyGeneration++;
    loadingEarlier = false;
    _interactionBusy.clear();
    _busy = false;
    _clearRunUi();
    _conversationRun = null;
    progress = AgentProgress.idle;
  }

  /// Keep the Stop target attached to the Run the UI currently observes.
  /// A Stop requested while a new turn is still awaiting its first Run
  /// snapshot belongs to that pending admission and survives its first ID.
  void _trackRunUi(
    AgentSession current, {
    required String? runId,
    AppRunState? state,
    bool newTurn = false,
  }) {
    final previousRunId = _observedRunId ?? _runSession?.activeTurn;
    final firstPendingTurnSnapshot =
        _conversationRun != null && _observedRunId == null && runId != null;
    final transitioned =
        newTurn ||
        (runId != null &&
            previousRunId != runId &&
            !firstPendingTurnSnapshot);
    if (transitioned) {
      _runUiGeneration++;
      _stopRequested = false;
    }
    _runSession = current;
    _observedRunId = runId;
    if (runId != null) {
      progress = state == AppRunState.cancelling || _stopRequested
          ? AgentProgress.stopping
          : AgentProgress.model;
    }
  }

  void _clearRunUi() {
    if (_runSession != null || _observedRunId != null || _stopRequested) {
      _runUiGeneration++;
    }
    _runSession = null;
    _observedRunId = null;
    _stopRequested = false;
  }

  bool _ownsStopOperation(int epoch, int generation) =>
      _acceptsEpoch(epoch) && generation == _runUiGeneration;

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
    final epoch = _adoptionEpoch;
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
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      if (snapshot == null) {
        _interactionFailures[interactionId] = 'interaction_unavailable';
        return;
      }
      _acceptInteractionSnapshot(snapshot, original);
    } on Object catch (error) {
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      _interactionFailures[interactionId] = _interactionReason(error);
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _interactionBusy.remove(interactionId);
        _notify();
      }
    }
  }

  /// Reload every card of the current Session, oldest first.
  Future<void> refreshInteractions() async {
    final gateway = _interactionGateway;
    final original = session;
    final epoch = _adoptionEpoch;
    if (original == null || _busy || _sealed || _disposed) {
      return;
    }
    _begin();
    _notify();
    try {
      final snapshots = await gateway.loadSessionInteractions(original.id);
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      _interactions.clear();
      _interactionFailures.clear();
      for (final snapshot in snapshots) {
        _acceptInteractionSnapshot(snapshot, original);
      }
    } on Object catch (error, stackTrace) {
      if (!_acceptsEpoch(epoch)) return;
      _recordError(
        'interaction_list',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _failFromError(error, 'transport_unavailable');
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _end();
        _notify();
      }
    }
  }

  Future<void> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) async {
    final gateway = _interactionGateway;
    if (!canDecideInteraction(snapshot, decision)) return;
    final original = session!;
    final epoch = _adoptionEpoch;
    _begin();
    _interactionBusy.add(snapshot.id);
    _interactionFailures.remove(snapshot.id);
    _notify();
    try {
      final result = await gateway.decideInteraction(snapshot, decision);
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
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
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      _recordError(
        'interaction_decide',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _interactionFailures[snapshot.id] = _interactionReason(error);
      _failFromError(error, 'transport_unavailable');
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _interactionBusy.remove(snapshot.id);
        _end();
        _notify();
      }
    }
  }

  Future<void> refreshInteraction(AgentInteractionSnapshot snapshot) async {
    final gateway = _interactionGateway;
    if (!canRefreshInteraction(snapshot)) return;
    final original = session!;
    final epoch = _adoptionEpoch;
    _begin();
    _interactionBusy.add(snapshot.id);
    _interactionFailures.remove(snapshot.id);
    _notify();
    try {
      final result = await gateway.refreshInteraction(snapshot);
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
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
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      _recordError(
        'interaction_refresh',
        error,
        stackTrace,
        sessionId: original.id,
      );
      _interactionFailures[snapshot.id] = _interactionReason(error);
      _failFromError(error, 'transport_unavailable');
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _interactionBusy.remove(snapshot.id);
        _end();
        _notify();
      }
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
    final epoch = _adoptionEpoch;
    final observation = _observation;
    _trackRunUi(original, runId: receipt.runId);
    _notify();
    try {
      final completion = await runtime.observeConversationRun(
        receipt,
        session!,
        observation: observation,
        onRun: (run) {
          if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
          _trackRunUi(original, runId: run.runId, state: run.state);
          _notify();
        },
      );
      if (_acceptsEpoch(epoch) && session?.id == original.id) {
        _recordTerminalIssue(completion.run, original.id);
        if (_acceptSession(completion.session) == null) return;
        _lastConversationRunId = completion.run.runId;
        needsReload = false;
        final issue = completion.run.report?.issues.firstOrNull;
        if (issue != null) {
          _acceptConversationIssue(issue);
        }
      }
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _clearRunUi();
      }
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
    if (_disposed || busy || !vaultController.ready) return;
    _advanceEpoch();
    final epoch = _adoptionEpoch;
    final observation = _observation;
    _sealed = false;
    _lastConversationRunId = null;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      await _conversationRuntime.awaitStoppedObservation(
        observation: observation,
      );
      if (!_acceptsEpoch(epoch)) return;
      // Settlement is an explicit command operation. No failed/incomplete
      // settlement falls through to a query or a fresh Start.
      final settled = await gateway
          .settlePendingSessionStart(personId)
          .timeout(loadTimeout);
      if (!_acceptsEpoch(epoch)) return;
      final resumed = newSession
          ? null
          : settled ??
                await observation.read(
                  () =>
                      gateway.resumeConversation(personId).timeout(loadTimeout),
                );
      if (!_acceptsEpoch(epoch)) return;
      // Every New gesture is new; its prior settlement was acknowledgement only.
      final saved =
          resumed ??
          await gateway.startConversation(personId).timeout(loadTimeout);
      if (!_acceptsEpoch(epoch)) return;
      await _observeLoadedSession(saved, epoch, observation);
    } on Object catch (error, stackTrace) {
      if (_acceptsEpoch(epoch)) {
        _recordError('load', error, stackTrace);
        _failFromError(
          error,
          session != null ? 'transport_unavailable' : 'storage_unavailable',
        );
      }
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _clearRunUi();
        _end();
        progress = AgentProgress.idle;
        _notify();
      }
    }
  }

  Future<void> _observeLoadedSession(
    AgentSession saved,
    int epoch,
    ConversationObservation observation,
  ) async {
    if (!_acceptsEpoch(epoch)) return;
    final accepted = _acceptSession(saved);
    if (accepted == null) return;
    await _conversationRuntime
        .synchronizeConversation(accepted, observation: observation)
        .timeout(loadTimeout);
    if (!_acceptsEpoch(epoch)) return;
    needsReload = false;
    if (accepted.activeTurn != null) {
      _trackRunUi(accepted, runId: accepted.activeTurn);
      _notify();
      final completion = await _conversationRuntime.observeSessionRun(
        accepted,
        observation: observation,
        onRun: (run) {
          if (!_acceptsEpoch(epoch)) return;
          _trackRunUi(accepted, runId: run.runId, state: run.state);
          _notify();
        },
      );
      if (_acceptsEpoch(epoch) &&
          session?.id == accepted.id &&
          completion.session.id == accepted.id) {
        _recordTerminalIssue(completion.run, accepted.id);
        _acceptSession(completion.session);
      }
    }
  }

  /// Re-observe owner recovery; no business Recover command or implicit Start.
  Future<void> recover() async {
    if (busy || _disposed || !needsRecovery || !vaultController.ready) return;
    final original = session!;
    _advanceEpoch();
    final epoch = _adoptionEpoch;
    final observation = _observation;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      await _conversationRuntime.awaitStoppedObservation(
        observation: observation,
      );
      if (!_acceptsEpoch(epoch)) return;
      final saved = await observation.read(
        () => gateway
            .loadConversation(personId, original.id)
            .timeout(loadTimeout),
      );
      if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
      await _observeLoadedSession(saved, epoch, observation);
    } on Object catch (error, stackTrace) {
      if (_acceptsEpoch(epoch)) {
        _recordError('reobserve', error, stackTrace, sessionId: original.id);
        _failFromError(error, 'transport_unavailable');
      }
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _clearRunUi();
        _end();
        progress = AgentProgress.idle;
        _notify();
      }
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
    _trackRunUi(original, runId: null, newTurn: true);
    _lastConversationText = normalized;
    _begin();
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
    final epoch = _adoptionEpoch;
    final observation = _observation;
    try {
      final completion = await runtime.runConversationTurn(
        request,
        observation: observation,
        onRun: (run) {
          if (!_acceptsEpoch(epoch) || session?.id != original.id) return;
          _trackRunUi(original, runId: run.runId, state: run.state);
          _notify();
        },
      );
      if (_acceptsEpoch(epoch) && session?.id == original.id) {
        _clearRunUi();
        _recordTerminalIssue(completion.run, original.id);
        if (_acceptSession(completion.session) == null) return;
        _lastConversationRunId = completion.run.runId;
        needsReload = false;
        final issue = completion.run.report?.issues.firstOrNull;
        if (issue != null) {
          _acceptConversationIssue(issue);
        }
      }
    } on Object catch (error, stackTrace) {
      if (_acceptsEpoch(epoch)) {
        _recordError(
          'conversation_turn',
          error,
          stackTrace,
          sessionId: original.id,
        );
        _failFromError(error, 'transport_unavailable');
      }
    } finally {
      if (!_disposed && epoch == _adoptionEpoch) {
        _conversationRun = null;
        _clearRunUi();
        _end();
        progress = AgentProgress.idle;
        _notify();
      }
    }
  }

  Future<void> stop() async {
    final original = _runSession;
    final epoch = _adoptionEpoch;
    final generation = _runUiGeneration;
    if (original == null || _stopRequested) return;
    final runId = _observedRunId ?? original.activeTurn;
    final request = _conversationRun;
    if (runId == null && request == null) return;
    _stopRequested = true;
    progress = AgentProgress.stopping;
    _notify();
    try {
      if (runId != null) {
        final dispatched = await _conversationRuntime.cancelObservedRun(runId);
        if (!_ownsStopOperation(epoch, generation)) return;
        if (!dispatched) {
          _stopRequested = false;
          progress = AgentProgress.model;
          _notify();
        }
      } else if (request != null) {
        await _conversationRuntime.cancelConversationTurn(request);
      }
    } on Object {
      if (!_ownsStopOperation(epoch, generation)) return;
      _stopRequested = false;
      failure = 'transport_unavailable';
      needsReload = true;
      progress = AgentProgress.model;
      _notify();
    }
  }

  Future<void> loadEarlierMessages() async {
    final current = session;
    if (_disposed ||
        _sealed ||
        !vaultController.ready ||
        busy ||
        loadingEarlier ||
        current == null ||
        messages.isEmpty ||
        !hasEarlierMessages)
      return;
    final before = messages.first.messageId;
    final generation = _historyGeneration;
    loadingEarlier = true;
    earlierFailure = null;
    _notify();
    try {
      final page = await gateway.loadEarlierConversation(
        personId,
        current.id,
        before,
      );
      if (_disposed ||
          _sealed ||
          !vaultController.ready ||
          generation != _historyGeneration ||
          session?.id != current.id ||
          messages.isEmpty ||
          messages.first.messageId != before)
        return;
      if (page.id != current.id ||
          page.personId != personId ||
          page.revision < current.revision ||
          page.messages.any((message) => message.messageId == before) ||
          (page.messages.isEmpty && page.hasEarlierMessages)) {
        throw const FormatException('Invalid Conversation history page.');
      }
      final existing = messages.map((message) => message.messageId).toSet();
      final earlier = page.messages
          .where((message) => !existing.contains(message.messageId))
          .toList();
      if (earlier.isEmpty && page.hasEarlierMessages)
        throw const FormatException('History cursor did not advance.');
      messages = [...earlier, ...messages];
      _historyHasEarlier = page.hasEarlierMessages;
    } on Object catch (error) {
      if (!_disposed && generation == _historyGeneration) {
        earlierFailure = 'Earlier messages could not be loaded. Try again.';
        final owner = _ownerFailure(error);
        if (owner != null && (owner.reloadRequired || owner.sealSession))
          _applyOwnerFailure(owner);
      }
    } finally {
      if (!_disposed && generation == _historyGeneration) {
        loadingEarlier = false;
        _notify();
      }
    }
  }

  void _resetHistory() {
    _historyGeneration++;
    _historyHasEarlier = null;
    loadingEarlier = false;
    earlierFailure = null;
  }

  AgentSession? _acceptSession(AgentSession saved) {
    if (_sealed) return null;
    if (saved.personId != personId) {
      throw const FormatException('Agent Person mismatch.');
    }
    final current = session;
    if (current != null && current.id == saved.id) {
      if (saved.revision < current.revision) return null;
      if (saved.revision == current.revision) {
        _applySessionOutcome(current);
        return current;
      }
    }
    _clearInteractions();
    final first = saved.messages.firstOrNull?.messageId;
    final prefixEnd = session?.id == saved.id && first != null
        ? messages.indexWhere((message) => message.messageId == first)
        : -1;
    final earlier = prefixEnd >= 0
        ? messages.take(prefixEnd).toList()
        : <AgentMessage>[];
    if (prefixEnd < 0) _resetHistory();
    session = saved;
    messages = [...earlier, ...saved.messages];
    _applySessionOutcome(saved);
    final lastUser = messages
        .whereType<AgentTextMessage>()
        .where((message) => message.kind == AgentMessageKind.user)
        .lastOrNull;
    _lastConversationText = lastUser?.text;
    return saved;
  }

  void _applySessionOutcome(AgentSession saved) {
    _clearFailure();
    failure = saved.lastOutcome?.failure;
    if (saved.lastOutcome?.issue?.ownerFailure case final ownerFailure?) {
      _applyOwnerFailure(ownerFailure);
      if (ownerFailure.reloadRequired || ownerFailure.sealSession) {
        // Preserve the owner's barrier through callers that would otherwise
        // mark a successful session read as ready to send a new turn.
        throw _SessionOutcomeBarrier.fromOwnerFailure(ownerFailure);
      }
    } else if (saved.lastOutcome?.issue case final issue?) {
      _fail(issue.reason, reloadRequired: true);
      throw _SessionOutcomeBarrier(issue.reason, reloadRequired: true);
    }
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void _vaultChanged() {
    if (_disposed) return;
    if (!vaultController.ready) {
      _advanceEpoch();
      _sealed = true;
      _clearInteractions();
      session = null;
      messages = [];
      _resetHistory();
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
    _notify();
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
      _observation.stop();
      _sealed = true;
      session = null;
      messages = [];
      _resetHistory();
    }
  }

  @override
  void dispose() {
    _advanceEpoch();
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
    if (error is _SessionOutcomeBarrier) return;
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

  void _recordTerminalIssue(AppRunSnapshot run, String sessionId) {
    if (!run.state.terminal ||
        run.sessionId != sessionId ||
        _diagnosticUuid(sessionId) == null ||
        _diagnosticUuid(run.runId) == null) {
      return;
    }
    for (final issue in run.report?.issues ?? const <AppWireIssue>[]) {
      final owner = issue.ownerFailure;
      AppDiagnostics.event(
        component: 'agent',
        operation: 'conversation_terminal_issue',
        level: DiagnosticLevel.warning,
        failure: _diagnosticToken(owner?.reason ?? issue.code),
        failureDomain: _diagnosticToken(owner?.domain),
        failureCategory: _diagnosticToken(owner?.category),
        reasonCode: _diagnosticToken(
          issue.metadata['reason_code'] ?? owner?.reason,
        ),
        incidentId: _diagnosticUuid(owner?.incidentId),
        safeActions: owner?.safeActions
                .where((action) => RegExp(r'^[a-z_]{1,64}$').hasMatch(action))
                .take(16)
                .toList(growable: false) ??
            const [],
        sessionId: sessionId,
        runId: run.runId,
        runState: run.state.name,
      );
    }
  }

  String? _diagnosticToken(String? value) =>
      value != null && RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value)
      ? value
      : null;

  String? _diagnosticUuid(String? value) =>
      value != null &&
          value != '00000000-0000-0000-0000-000000000000' &&
          RegExp(
            r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
          ).hasMatch(value)
      ? value
      : null;
}

/// Marks only Session outcomes turned into a local admission/recovery barrier.
/// Gateway AgentVaultExceptions remain ordinary reportable exceptions.
final class _SessionOutcomeBarrier extends AgentVaultException {
  const _SessionOutcomeBarrier(
    super.failure, {
    super.requestId,
    super.stage,
    super.metadata,
    super.recoveryAction,
    super.affectedRefs,
    super.correlationRequestId,
    super.retryableOverride,
    super.domain,
    super.category,
    super.reasonCode,
    super.safeActions,
    super.incidentId,
    super.retryPolicy,
    super.reloadRequired,
    super.sealSession,
    super.ownerFailure,
  });

  factory _SessionOutcomeBarrier.fromOwnerFailure(OwnerFailure owner) =>
      _SessionOutcomeBarrier(
        owner.reason,
        ownerFailure: owner,
        recoveryAction: owner.recovery,
        domain: owner.domain,
        category: owner.category,
        reasonCode: owner.reason,
        safeActions: owner.safeActions.toList(growable: false),
        incidentId: owner.incidentId,
        correlationRequestId: owner.correlationId,
        reloadRequired: owner.reloadRequired,
        sealSession: owner.sealSession,
      );
}
