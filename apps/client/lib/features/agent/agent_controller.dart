import 'dart:async';

import 'package:flutter/foundation.dart';

import 'agent_calendar_experts.dart';
import 'agent_connections.dart';
import 'agent_conversation_gateway.dart';
import 'agent_fixture_gateway.dart';
import 'agent_memory_review.dart';
import 'agent_memory.dart';
import 'agent_expert_result.dart';
import 'agent_proposal.dart';
import 'agent_registry.dart';
import 'agent_vault_gateway.dart';
import 'application/agent_registry_controller.dart';
import 'application/agent_memory_controller.dart';
import 'application/agent_calendar_expert_controller.dart';
import 'application/agent_connection_controller.dart';

enum AgentProgress {
  idle,
  loading,
  model,
  expertModel,
  correcting,
  capability,
  stopping,
}

final class AgentController extends ChangeNotifier {
  factory AgentController({
    required AgentFixtureStreamingGateway gateway,
    required String personId,
    Future<void> Function()? beforeInvocation,
  }) => AgentController._(gateway, personId, beforeInvocation);

  AgentController._(this.gateway, this.personId, this._beforeInvocation) {
    registryController = AgentRegistryController(
      gateway: gateway is AgentRegistryGateway
          ? gateway as AgentRegistryGateway
          : null,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !_sealed &&
          !_disposed &&
          _locking == null &&
          vaultState == AgentVaultState.ready,
      onFatalFailure: _fail,
    )..addListener(_notify);
    memoryController = AgentMemoryController(
      memoryGateway: gateway is AgentMemoryGateway
          ? gateway as AgentMemoryGateway
          : null,
      reviewGateway: gateway is AgentMemoryReviewGateway
          ? gateway as AgentMemoryReviewGateway
          : null,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !registryController.busy &&
          !_sealed &&
          !_disposed &&
          _locking == null &&
          vaultState == AgentVaultState.ready,
      onFatalFailure: _fail,
    )..addListener(_notify);
    calendarExpertController = AgentCalendarExpertController(
      gateway: gateway is AgentCalendarExpertGateway
          ? gateway as AgentCalendarExpertGateway
          : null,
      registryGateway: gateway is AgentRegistryGateway
          ? gateway as AgentRegistryGateway
          : null,
      registryController: registryController,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !registryController.busy &&
          !memoryController.busy &&
          !_sealed &&
          !_disposed &&
          _locking == null &&
          vaultState == AgentVaultState.ready,
      onFatalFailure: _fail,
    )..addListener(_notify);
    connectionController = AgentConnectionController(
      gateway: gateway is AgentConnectionsGateway
          ? gateway as AgentConnectionsGateway
          : null,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !registryController.busy &&
          !memoryController.busy &&
          !calendarExpertController.busy &&
          !_sealed &&
          !_disposed &&
          _locking == null,
      onFatalFailure: _fail,
    )..addListener(_notify);
  }

  final AgentFixtureStreamingGateway gateway;
  final String personId;
  final Future<void> Function()? _beforeInvocation;
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
  String? _lastConversationText;
  AgentConversationTurnRequest? _conversationRun;
  AgentVaultState? vaultState;
  bool _sealed = false;
  Completer<void>? _operationDone;
  Future<void>? _locking;
  late final AgentRegistryController registryController;
  AgentRegistryView? get registry => registryController.registry;
  String? get registryFailure => registryController.failure;
  bool get registryLoaded => registryController.loaded;
  late final AgentCalendarExpertController calendarExpertController;
  AgentCalendarExperts? get calendarExperts => calendarExpertController.experts;
  String? get calendarExpertFailure => calendarExpertController.failure;
  late final AgentMemoryController memoryController;
  late final AgentConnectionController connectionController;
  List<AgentConnection>? get connections => connectionController.connections;
  String? get connectionFailure => connectionController.failure;
  bool get hasConnections => connectionController.available;
  bool get canReadConnections => connectionController.canRead;

  Future<void> loadConnections() => connectionController.load();
  List<AgentMemoryCandidate>? get memoryCandidates =>
      memoryController.candidates;
  String? get memoryReviewFailure => memoryController.reviewFailure;
  AgentMemoryOverview? get memoryOverview => memoryController.overview;
  set memoryOverview(AgentMemoryOverview? value) {
    memoryController.overview = value;
  }

  String? get memoryFailure => memoryController.failure;
  final Map<String, AgentProposalInspection> _proposals = {};
  final Map<String, String> _proposalFailures = {};

  AgentProposalInspection? proposalFor(String callId) => _proposals[callId];
  String? proposalFailureFor(String callId) => _proposalFailures[callId];

  AgentExpertResult? expertResult(AgentCapabilityMessage message) {
    final classes = session?.dataClasses ?? const <String>[];
    if (classes.length != 1 ||
        classes.single == 'personal' &&
            (!usesVault || vaultState != AgentVaultState.ready || _sealed)) {
      return null;
    }
    return AgentExpertResult.tryParse(
      message.output,
      callId: message.callId,
      personId: personId,
      allowedDataClasses: classes,
    );
  }

  bool canInspectProposal(AgentCapabilityMessage message) =>
      usesVault &&
      gateway is AgentProposalGateway &&
      !busy &&
      !_sealed &&
      !_disposed &&
      _locking == null &&
      !needsReload &&
      !needsRecovery &&
      vaultState == AgentVaultState.ready &&
      session?.personId == personId &&
      session!.messages
              .whereType<AgentCapabilityMessage>()
              .where(
                (saved) =>
                    saved.callId == message.callId &&
                    saved.output == message.output,
              )
              .length ==
          1 &&
      expertResult(message)?.proposal != null;

  Future<void> inspectProposal(AgentCapabilityMessage message) async {
    if (!canInspectProposal(message)) return;
    final original = session!;
    _begin();
    _proposals.remove(message.callId);
    _proposalFailures.remove(message.callId);
    _notify();
    try {
      final result = await (gateway as AgentProposalGateway).inspectProposal(
        personId: personId,
        sessionId: original.id,
        invocationId: message.callId,
      );
      if (_sealed || _disposed || session?.id != original.id) return;
      if (result.personId != personId ||
          result.sessionId != original.id ||
          result.invocationId != message.callId) {
        throw const FormatException('Proposal inspection scope mismatch');
      }
      _proposals[message.callId] = result;
    } on Object catch (error) {
      if (_sealed || _disposed) return;
      final reason = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      _proposalFailures[message.callId] = reason;
      if (reason == 'vault_unavailable' || reason == 'interrupted') {
        _fail(reason);
      }
    } finally {
      _end();
      _notify();
    }
  }

  void _clearProposals() {
    _proposals.clear();
    _proposalFailures.clear();
  }

  AgentCalendarSetup? get pendingCalendarSetup =>
      calendarExpertController.pendingSetup;
  bool get hasCalendarExpertManagement =>
      hasRegistryManagement && calendarExpertController.available;
  bool get canManageCalendarExperts =>
      hasCalendarExpertManagement && calendarExpertController.canManage;

  Future<void> loadCalendarExperts() => calendarExpertController.load();

  Future<void> installCalendarExpert({
    required String setupId,
    required String provider,
    required List<String> calendarIds,
  }) => calendarExpertController.install(
    setupId: setupId,
    provider: provider,
    calendarIds: calendarIds,
  );

  Future<void> retryCalendarSetup() => calendarExpertController.retrySetup();

  void discardUncommittedCalendarSetup() =>
      calendarExpertController.discardUncommittedSetup();

  Future<void> configureCalendarView(String handle, bool enabled) =>
      calendarExpertController.configureView(handle, enabled);

  Future<void> setCalendarAccessEnabled(String setupId, bool enabled) =>
      calendarExpertController.setCalendarAccessEnabled(setupId, enabled);

  Future<void> changeCalendarAccessScope({
    required String setupId,
    required String provider,
    required List<String> calendarIds,
  }) => calendarExpertController.changeCalendarAccessScope(
    setupId: setupId,
    provider: provider,
    calendarIds: calendarIds,
  );

  Future<void> removeCalendarAccess(String setupId) =>
      calendarExpertController.removeCalendarAccess(setupId);

  bool get hasRegistryManagement => usesVault && registryController.available;

  bool get hasMemoryReview => usesVault && memoryController.hasReview;
  bool get hasMemory => usesVault && memoryController.hasMemory;
  bool get canReadMemory => hasMemory && memoryController.canRead;

  Future<void> loadMemory() => memoryController.load();

  bool get canReviewMemory => hasMemoryReview && memoryController.canReview;

  Future<void> loadMemoryReview() => memoryController.loadReview();

  Future<void> decideMemoryCandidate(
    String candidateId,
    AgentMemoryDecision decision,
  ) => memoryController.decide(candidateId, decision);

  bool get canManageRegistry =>
      hasRegistryManagement && registryController.canManage;

  Future<void> loadRegistry() => registryController.load();

  Future<void> configureRegistry(
    AgentRegistryTarget target,
    String id,
    bool enabled,
  ) => registryController.configure(target, id, enabled);

  Future<void> configureCapability(String installationId, bool enabled) async {
    final changed = await registryController.configureCapability(
      installationId,
      enabled,
    );
    if (changed && hasCalendarExpertManagement && canManageCalendarExperts) {
      await loadCalendarExperts();
    }
  }

  bool get usesVault => gateway is AgentVaultGateway;
  bool get isGeneralConversation =>
      usesVault &&
      session?.scope == null &&
      session?.dataClasses.singleOrNull == 'personal';
  bool get isConnectedConversation => isGeneralConversation;
  bool get isPersonalConversation => isGeneralConversation;

  bool get busy =>
      _busy ||
      registryController.busy ||
      memoryController.busy ||
      calendarExpertController.busy ||
      connectionController.busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  bool get canSend =>
      !busy && !needsReload && !needsRecovery && session != null;
  bool get canContinue =>
      canSend &&
      session?.continuation != null &&
      isGeneralConversation &&
      _lastConversationText != null;
  bool get canRetry =>
      canSend &&
      !canContinue &&
      failure != null &&
      (isGeneralConversation
          ? _lastConversationText != null
          : _lastPrompt != null);

  Future<void> load({bool newSession = false}) async {
    if (_disposed) return;
    if (_locking case final locking?) {
      await locking;
    }
    if (busy || _disposed) return;
    _sealed = false;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      if (gateway case final AgentVaultGateway vault) {
        final state = await vault.vaultStatus(personId);
        if (_sealed) return;
        vaultState = switch (state) {
          AgentVaultState.missing => await vault.createVault(personId),
          AgentVaultState.locked => await vault.unlockVault(personId),
          _ => state,
        };
        if (_sealed) return;
        if (vaultState != AgentVaultState.ready) {
          throw const AgentVaultException('vault_unavailable');
        }
      }
      if (gateway case final AgentConversationGateway conversation) {
        final saved = newSession
            ? await conversation.startConversation(personId)
            : await conversation.resumeConversation(personId);
        _acceptSession(saved);
      } else {
        final result = newSession
            ? await gateway.startAgentFixture(personId)
            : await gateway.resumeAgentFixture(personId);
        _acceptSession(result.session);
      }
      needsReload = false;
    } on Object catch (error) {
      _fail(
        error is AgentVaultException ? error.failure : 'storage_unavailable',
      );
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
      if (isGeneralConversation && gateway is AgentConversationGateway) {
        final saved = await (gateway as AgentConversationGateway)
            .recoverConversation(original);
        _acceptSession(saved);
      } else {
        final result = await gateway.recoverAgentFixture(original);
        _acceptSession(result.session);
      }
      needsReload = false;
    } on Object catch (error) {
      _fail(
        error is AgentVaultException ? error.failure : 'storage_unavailable',
      );
    } finally {
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> retry() async {
    if (!canRetry) return;
    if (isGeneralConversation) {
      await sendText(_lastConversationText!);
    } else {
      await send(_lastPrompt!);
    }
  }

  Future<void> continueTurn() async {
    if (!canContinue) return;
    await _sendConversationText(_lastConversationText!, continuation: true);
  }

  Future<void> sendText(String text) => _sendConversationText(text);

  Future<void> _sendConversationText(
    String text, {
    bool continuation = false,
  }) async {
    final normalized = text.trim();
    if (!canSend ||
        _disposed ||
        !isGeneralConversation ||
        gateway is! AgentConversationGateway ||
        normalized.isEmpty ||
        normalized.length > 8192) {
      return;
    }
    final original = session!;
    final request = AgentConversationTurnRequest(
      session: original,
      text: normalized,
      continuation: continuation,
    );
    _conversationRun = request;
    _runSession = original;
    _lastConversationText = normalized;
    _begin();
    _stopRequested = false;
    failure = null;
    progress = AgentProgress.model;
    _notify();
    var done = false;
    var started = false;
    try {
      await _beforeInvocation?.call();
      var update = await (gateway as AgentConversationGateway)
          .beginConversationTurn(request);
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
            needsReload = false;
          } else {
            _fail(update.failure ?? 'storage_unavailable');
          }
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        update = await (gateway as AgentConversationGateway)
            .pollConversationTurn(request, sequence);
      }
    } on Object catch (error) {
      _fail(
        error is AgentVaultException ? error.failure : 'transport_unavailable',
      );
    } finally {
      try {
        if (!done && started) {
          var update = await (gateway as AgentConversationGateway)
              .stopConversationTurn(request);
          for (var attempt = 0; !update.done && attempt < 25; attempt++) {
            await Future<void>.delayed(const Duration(milliseconds: 80));
            update = await (gateway as AgentConversationGateway)
                .pollConversationTurn(request, 0);
          }
          done = update.done;
        }
        if (done && started) {
          await (gateway as AgentConversationGateway).releaseConversationTurn(
            request,
          );
        }
      } on Object {
        needsReload = true;
        failure ??= 'transport_unavailable';
      }
      _conversationRun = null;
      _runSession = null;
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> send(AgentFixturePrompt prompt) async {
    if (!canSend || _disposed || isGeneralConversation) return;
    final original = session!;
    _runSession = original;
    _lastPrompt = prompt;
    _begin();
    _stopRequested = false;
    failure = null;
    progress = AgentProgress.model;
    _notify();
    var done = false;
    var started = false;
    try {
      await _beforeInvocation?.call();
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
            needsReload = false;
          } else {
            _fail(update.failure ?? 'storage_unavailable');
          }
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        update = await gateway.pollAgentFixtureRun(original, sequence);
      }
    } on Object catch (error) {
      _fail(
        error is AgentVaultException ? error.failure : 'transport_unavailable',
      );
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
      if (_conversationRun case final request?) {
        await (gateway as AgentConversationGateway).stopConversationTurn(
          request,
        );
      } else {
        await gateway.stopAgentFixtureRun(original);
      }
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
    _clearProposals();
    session = saved;
    messages = List.of(saved.messages);
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
    failure = null;
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
      _acceptSession((await vault.resumeAgentFixture(personId)).session);
      needsReload = false;
    } on Object catch (error) {
      _fail(error is AgentVaultException ? error.failure : 'vault_unavailable');
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
    _clearProposals();
    registryController.clear();
    calendarExpertController.clear();
    memoryController.clear();
    connectionController.clear();
    session = null;
    messages = [];
    _lastPrompt = null;
    vaultState = AgentVaultState.locked;
    _notify();
    await stop();
    await _operationDone?.future;
    _begin();
    try {
      await (gateway as AgentVaultGateway).lockVault(personId);
      vaultState = AgentVaultState.locked;
      failure = null;
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

  void _fail(String reason) {
    _clearProposals();
    failure = reason;
    needsReload = true;
    if (usesVault &&
        const {
          'vault_unavailable',
          'storage_unavailable',
          'interrupted',
        }.contains(reason)) {
      registryController.clear();
      calendarExpertController.clear();
      memoryController.clear();
      connectionController.clear();
      session = null;
      messages = [];
      vaultState = AgentVaultState.unavailable;
    }
  }

  @override
  void dispose() {
    _disposed = true;
    registryController.removeListener(_notify);
    memoryController.removeListener(_notify);
    calendarExpertController.removeListener(_notify);
    connectionController.removeListener(_notify);
    unawaited(stop());
    super.dispose();
  }
}
