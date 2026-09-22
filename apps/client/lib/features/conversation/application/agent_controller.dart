import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_calendar_experts.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/experts/domain/agent_expert_result.dart';
import 'package:floe_client/features/actions/domain/agent_proposal.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/experts/application/agent_registry_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/experts/application/agent_calendar_expert_controller.dart';
import 'package:floe_client/features/connections/application/agent_connection_controller.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

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
    required AgentConversationGateway gateway,
    required String personId,
    Duration loadTimeout = const Duration(seconds: 10),
    LocalOwnerGateways owners = const LocalOwnerGateways(),
  }) => AgentController._(
    gateway,
    personId,
    loadTimeout,
    LocalOwnerGateways(
      vault:
          owners.vault ??
          (gateway is AgentVaultGateway ? gateway as AgentVaultGateway : null),
      registry:
          owners.registry ??
          (gateway is AgentRegistryGateway
              ? gateway as AgentRegistryGateway
              : null),
      calendarExperts:
          owners.calendarExperts ??
          (gateway is AgentCalendarExpertGateway
              ? gateway as AgentCalendarExpertGateway
              : null),
      memory:
          owners.memory ??
          (gateway is AgentMemoryGateway
              ? gateway as AgentMemoryGateway
              : null),
      memoryReview:
          owners.memoryReview ??
          (gateway is AgentMemoryReviewGateway
              ? gateway as AgentMemoryReviewGateway
              : null),
      connections:
          owners.connections ??
          (gateway is AgentConnectionsGateway
              ? gateway as AgentConnectionsGateway
              : null),
      proposals:
          owners.proposals ??
          (gateway is AgentProposalGateway
              ? gateway as AgentProposalGateway
              : null),
      personalAccess: owners.personalAccess,
    ),
  );

  AgentController._(
    this.gateway,
    this.personId,
    this.loadTimeout,
    this.owners,
  ) {
    registryController = AgentRegistryController(
      gateway: owners.registry,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !_sealed &&
          !_disposed &&
          _locking == null &&
          vaultState == AgentVaultState.ready,
      onFatalFailure: (error) => _failFromError(error, 'storage_unavailable'),
    )..addListener(_notify);
    memoryController = AgentMemoryController(
      memoryGateway: owners.memory,
      reviewGateway: owners.memoryReview,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !registryController.busy &&
          !_sealed &&
          !_disposed &&
          _locking == null &&
          vaultState == AgentVaultState.ready,
      onFatalFailure: (error) => _failFromError(error, 'storage_unavailable'),
    )..addListener(_notify);
    calendarExpertController = AgentCalendarExpertController(
      gateway: owners.calendarExperts,
      registryGateway: owners.registry,
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
      onFatalFailure: (error) => _failFromError(error, 'storage_unavailable'),
    )..addListener(_notify);
    connectionController = AgentConnectionController(
      gateway: owners.connections,
      personId: personId,
      canOperate: () =>
          !_busy &&
          !registryController.busy &&
          !memoryController.busy &&
          !calendarExpertController.busy &&
          !_sealed &&
          !_disposed &&
          _locking == null,
      onFatalFailure: (error) => _failFromError(error, 'storage_unavailable'),
    )..addListener(_notify);
    _conversationRuntime?.readModel.addListener(_notify);
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
  bool _stopRequested = false;
  AgentSession? _runSession;
  String? _lastConversationText;
  String? _lastConversationRunId;
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
      owners.proposals != null &&
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
      final result = await owners.proposals!.inspectProposal(
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
      if (error is AgentVaultException &&
          (error.reloadRequired == true || error.sealSession == true)) {
        _failFromError(error, reason);
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
    required String deviceId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required CalendarSourceAuthority sourceAuthority,
    CalendarSubjectPreview? reviewedPreview,
  }) => calendarExpertController.install(
    setupId: setupId,
    provider: provider,
    deviceId: deviceId,
    calendarIds: calendarIds,
    connectionScope: connectionScope,
    connectionRevision: connectionRevision,
    sourceAuthority: sourceAuthority,
    reviewedPreview: reviewedPreview,
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
    required String deviceId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required CalendarSourceAuthority sourceAuthority,
    CalendarSubjectPreview? reviewedPreview,
  }) => calendarExpertController.changeCalendarAccessScope(
    setupId: setupId,
    provider: provider,
    deviceId: deviceId,
    calendarIds: calendarIds,
    connectionScope: connectionScope,
    connectionRevision: connectionRevision,
    sourceAuthority: sourceAuthority,
    reviewedPreview: reviewedPreview,
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

  bool get usesVault => owners.vault != null;
  bool get isGeneralConversation =>
      session?.scope == null && session?.dataClasses.singleOrNull == 'personal';
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
  ConversationRuntimeGateway? get _conversationRuntime =>
      gateway is ConversationRuntimeProvider
      ? (gateway as ConversationRuntimeProvider).conversationRuntime
      : null;
  bool get _conversationBusy =>
      _busy ||
      registryController.busy ||
      memoryController.busy ||
      calendarExpertController.busy;
  bool get canStartConversation => !_conversationBusy && !_disposed && !running;
  bool get canSend =>
      !_conversationBusy &&
      !_disposed &&
      !_sealed &&
      _locking == null &&
      !needsReload &&
      !needsRecovery &&
      session != null &&
      isGeneralConversation &&
      (_conversationRuntime?.readModel.conversation.canSend(session!.id) ??
          false);
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
      isGeneralConversation &&
      _lastConversationText != null &&
      _lastConversationRunId != null;

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
      if (owners.vault case final vault?) {
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
          throw const AgentVaultException(
            'vault_unavailable',
            reloadRequired: true,
            sealSession: true,
          );
        }
      }
      final conversation = gateway;
      final saved = newSession
          ? await conversation.startConversation(personId).timeout(loadTimeout)
          : await conversation
                .resumeConversation(personId)
                .timeout(loadTimeout);
      _acceptSession(saved);
      await _conversationRuntime
          ?.synchronizeConversation(saved)
          .timeout(loadTimeout);
      needsReload = false;
    } on Object catch (error, stackTrace) {
      _recordError('load', error, stackTrace);
      _failFromError(
        error,
        session != null && _conversationRuntime != null
            ? 'transport_unavailable'
            : 'storage_unavailable',
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

      final saved = await gateway.recoverConversation(original);
      _acceptSession(saved);
      await _conversationRuntime?.synchronizeConversation(saved);
      needsReload = false;
    } on Object catch (error, stackTrace) {
      _recordError('recover', error, stackTrace, sessionId: session?.id);
      _failFromError(
        error,
        _conversationRuntime == null
            ? 'storage_unavailable'
            : 'transport_unavailable',
      );
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
        _conversationRuntime == null ||
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
    await _runConversationCommand(_conversationRuntime!, original, request);
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
      if (_conversationRun case final request?) {
        await _conversationRuntime?.cancelConversationTurn(request);
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
    _clearFailure();
    failure = saved.lastOutcome?.failure;
    final lastUser = messages
        .whereType<AgentTextMessage>()
        .where((message) => message.kind == AgentMessageKind.user)
        .lastOrNull;
    _lastConversationText = lastUser?.text;
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<void> unlock({bool create = false}) async {
    if (busy || _disposed || _locking != null || owners.vault == null) {
      return;
    }
    final vault = owners.vault!;
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
        throw const AgentVaultException(
          'vault_unavailable',
          reloadRequired: true,
          sealSession: true,
        );
      }
      final saved = await gateway.resumeConversation(personId);
      _acceptSession(saved);
      await _conversationRuntime?.synchronizeConversation(saved);
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
    if (owners.vault == null) return stop();
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
    _lastConversationRunId = null;
    vaultState = AgentVaultState.locked;
    _notify();
    await stop();
    await _operationDone?.future;
    _begin();
    try {
      await owners.vault!.lockVault(personId);
      vaultState = AgentVaultState.locked;
      _clearFailure();
      needsReload = false;
    } on Object {
      _fail('vault_unavailable', reloadRequired: true, sealSession: true);
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
      reloadRequired: source?.reloadRequired ?? source == null,
      sealSession: source?.sealSession,
    );
  }

  void _acceptConversationIssue(AppWireIssue issue) {
    _fail(
      issue.metadata['reason_code'] ?? issue.code,
      recoveryAction: issue.metadata['recovery_action'],
      domain: issue.metadata['domain'],
      category: issue.metadata['category'],
      reloadRequired: issue.metadata['reload_required'] == 'true',
      sealSession: issue.metadata['seal_session'] == 'true',
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
    _clearProposals();
    failure = reason;
    this.recoveryAction = recoveryAction;
    failureDomain = domain;
    failureCategory = category;
    failureSafeActions = List.unmodifiable(safeActions ?? const []);
    failureAffectedRefs = List.unmodifiable(affectedRefs ?? const []);
    failureIncidentId = incidentId;
    failureRetryPolicy = retryPolicy;
    needsReload = reloadRequired ?? false;
    if (usesVault && (sealSession ?? false)) {
      _sealed = true;
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
    _conversationRuntime?.readModel.removeListener(_notify);
    registryController.removeListener(_notify);
    memoryController.removeListener(_notify);
    calendarExpertController.removeListener(_notify);
    connectionController.removeListener(_notify);
    if (_conversationRuntime == null) unawaited(stop());
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
