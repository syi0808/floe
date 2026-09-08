import 'dart:async';

import 'package:flutter/foundation.dart';

import 'agent_calendar_experts.dart';
import 'agent_calendar_session_gateway.dart';
import 'agent_calendar_turn_gateway.dart';
import 'agent_conversation_gateway.dart';
import 'agent_fixture_gateway.dart';
import 'agent_expert_result.dart';
import 'agent_proposal.dart';
import 'agent_registry.dart';
import 'agent_request_id.dart';
import 'agent_vault_gateway.dart';

enum AgentProgress { idle, loading, model, capability, stopping }

final class AgentController extends ChangeNotifier {
  AgentController({
    required this.gateway,
    required this.personId,
    this.calendarContext,
  });

  final AgentFixtureStreamingGateway gateway;
  final String personId;
  final AgentCalendarConversationContext? Function()? calendarContext;
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
  AgentCalendarPromptKind? _lastCalendarPrompt;
  String? _lastCalendarText;
  String? _lastConversationText;
  int _lastFocusMinutes = 60;
  AgentCalendarTurnRequest? _calendarRun;
  AgentConversationTurnRequest? _conversationRun;
  AgentCalendarConversationContext? _activeCalendarContext;
  AgentVaultState? vaultState;
  bool _sealed = false;
  Completer<void>? _operationDone;
  Future<void>? _locking;
  AgentRegistryView? registry;
  String? registryFailure;
  bool registryLoaded = false;
  AgentCalendarExperts? calendarExperts;
  String? calendarExpertFailure;
  AgentCalendarSetup? _pendingCalendarSetup;
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
      !_busy &&
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

  AgentCalendarSetup? get pendingCalendarSetup => _pendingCalendarSetup;
  bool get hasCalendarExpertManagement =>
      hasRegistryManagement && gateway is AgentCalendarExpertGateway;
  bool get canManageCalendarExperts =>
      hasCalendarExpertManagement && canManageRegistry;

  Future<void> loadCalendarExperts() => _calendarOperation(
    () => (gateway as AgentCalendarExpertGateway).readCalendarExperts(personId),
  );

  Future<void> installCalendarExpert({
    required String provider,
    required List<String> calendarIds,
  }) async {
    final current = calendarExperts;
    if (!canManageCalendarExperts ||
        current == null ||
        _pendingCalendarSetup != null) {
      return;
    }
    try {
      _pendingCalendarSetup = AgentCalendarSetup(
        personId: personId,
        instanceId: current.registry.instanceId,
        expectedRevision: current.registry.revision,
        setupId: newAgentRequestId(),
        provider: provider,
        calendarIds: calendarIds,
      );
    } on FormatException {
      calendarExpertFailure = 'invalid_input';
      _notify();
      return;
    }
    await retryCalendarSetup();
  }

  Future<void> retryCalendarSetup() async {
    final pending = _pendingCalendarSetup;
    if (pending == null) return;
    await _calendarOperation(
      () => (gateway as AgentCalendarExpertGateway).installCalendarExpert(
        pending,
      ),
      submitted: pending,
    );
    if (_pendingCalendarSetup == null && calendarExperts != null) {
      await setCalendarAccessEnabled(pending.setupId, true);
    }
  }

  void discardUncommittedCalendarSetup() {
    final current = calendarExperts;
    final pending = _pendingCalendarSetup;
    if (!canManageCalendarExperts ||
        current == null ||
        pending == null ||
        current.registry.instanceId != pending.instanceId ||
        current.setups.any((entry) => entry.setupId == pending.setupId)) {
      return;
    }
    _pendingCalendarSetup = null;
    _notify();
  }

  Future<void> configureCalendarView(String handle, bool enabled) async {
    final current = calendarExperts;
    if (current == null || _pendingCalendarSetup != null) return;
    final before = current.views
        .where((entry) => entry.handle == handle)
        .singleOrNull;
    if (before == null) return;
    await _calendarOperation(() async {
      final configured = await (gateway as AgentRegistryGateway)
          .configureRegistry(
            current.registry,
            target: AgentRegistryTarget.calendarView,
            id: handle,
            enabled: enabled,
          );
      final next = await (gateway as AgentCalendarExpertGateway)
          .readCalendarExperts(personId);
      final updated = next.views
          .where((entry) => entry.handle == handle)
          .singleOrNull;
      if (configured.instanceId != current.registry.instanceId ||
          configured.revision != current.registry.revision + 1 ||
          next.registry.instanceId != configured.instanceId ||
          next.registry.revision != configured.revision ||
          updated == null ||
          updated.enabled != enabled ||
          updated.provider != before.provider ||
          !listEquals(updated.calendarIds, before.calendarIds)) {
        throw const FormatException('Calendar configuration mismatch');
      }
      return next;
    });
  }

  Future<void> setCalendarAccessEnabled(String setupId, bool enabled) =>
      _configureCalendarAccess(
        setupId,
        operation: AgentCalendarAccessOperation.setEnabled,
        enabled: enabled,
      );

  Future<void> changeCalendarAccessScope({
    required String setupId,
    required String provider,
    required List<String> calendarIds,
  }) async {
    final replacementSetupId = newAgentRequestId();
    await _configureCalendarAccess(
      setupId,
      operation: AgentCalendarAccessOperation.setScope,
      provider: provider,
      replacementSetupId: replacementSetupId,
      calendarIds: calendarIds,
    );
    if (calendarExperts?.setups.any(
          (entry) => entry.setupId == replacementSetupId,
        ) ??
        false) {
      await setCalendarAccessEnabled(replacementSetupId, true);
    }
  }

  Future<void> removeCalendarAccess(String setupId) => _configureCalendarAccess(
    setupId,
    operation: AgentCalendarAccessOperation.remove,
  );

  Future<void> _configureCalendarAccess(
    String setupId, {
    required AgentCalendarAccessOperation operation,
    bool? enabled,
    String? provider,
    String? replacementSetupId,
    List<String>? calendarIds,
  }) async {
    final current = calendarExperts;
    if (current == null ||
        _pendingCalendarSetup != null ||
        !current.setups.any((entry) => entry.setupId == setupId)) {
      return;
    }
    final request = AgentCalendarAccessRequest(
      personId: personId,
      instanceId: current.registry.instanceId,
      expectedRevision: current.registry.revision,
      setupId: setupId,
      operation: operation,
      enabled: enabled,
      provider: provider,
      replacementSetupId: replacementSetupId,
      calendarIds: calendarIds,
    );
    await _calendarOperation(() async {
      final next = await (gateway as AgentCalendarExpertGateway)
          .configureCalendarAccess(request);
      if (next.registry.instanceId != current.registry.instanceId ||
          next.registry.revision != current.registry.revision + 1) {
        throw const FormatException('Calendar access configuration mismatch');
      }
      final setup = next.setups
          .where((entry) => entry.setupId == setupId)
          .singleOrNull;
      switch (operation) {
        case AgentCalendarAccessOperation.setEnabled:
          if (setup == null || next.accessEnabled(setup) != enabled) {
            throw const FormatException('Calendar access state mismatch');
          }
        case AgentCalendarAccessOperation.setScope:
          final replacement = next.setups
              .where((entry) => entry.setupId == replacementSetupId)
              .singleOrNull;
          final view = replacement == null
              ? null
              : next.views
                    .where((entry) => entry.handle == replacement.viewHandle)
                    .singleOrNull;
          if (setup != null ||
              view == null ||
              view.provider != provider ||
              !listEquals(view.calendarIds, [...calendarIds!]..sort())) {
            throw const FormatException('Calendar access scope mismatch');
          }
        case AgentCalendarAccessOperation.remove:
          if (setup != null) {
            throw const FormatException('Calendar access removal mismatch');
          }
      }
      return next;
    });
  }

  Future<void> _calendarOperation(
    Future<AgentCalendarExperts> Function() operation, {
    AgentCalendarSetup? submitted,
  }) async {
    if (!canManageCalendarExperts) return;
    _begin();
    calendarExpertFailure = null;
    _notify();
    try {
      final result = await operation();
      if (_sealed || _disposed) return;
      if (result.registry.personId != personId) {
        throw const FormatException('Calendar Person mismatch');
      }
      if (submitted != null && result.receiptFor(submitted) == null) {
        throw const FormatException('Missing Calendar setup receipt');
      }
      if (_pendingCalendarSetup case final pending?) {
        if (result.receiptFor(pending) != null) _pendingCalendarSetup = null;
      }
      calendarExperts = result;
      registry = result.registry;
      registryLoaded = true;
      registryFailure = null;
    } on Object catch (error) {
      if (_sealed || _disposed) return;
      calendarExperts = null;
      registry = null;
      registryLoaded = false;
      calendarExpertFailure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (calendarExpertFailure == 'vault_unavailable' ||
          calendarExpertFailure == 'interrupted') {
        _fail(calendarExpertFailure!);
      }
    } finally {
      _end();
      _notify();
    }
  }

  bool get hasRegistryManagement =>
      usesVault && gateway is AgentRegistryGateway;
  bool get canManageRegistry =>
      hasRegistryManagement &&
      !_busy &&
      !_sealed &&
      !_disposed &&
      _locking == null &&
      vaultState == AgentVaultState.ready;

  Future<void> loadRegistry() => _registryOperation(null);

  Future<void> configureRegistry(
    AgentRegistryTarget target,
    String id,
    bool enabled,
  ) async {
    final current = registry;
    if (current == null) return;
    await _registryOperation(
      () => (gateway as AgentRegistryGateway).configureRegistry(
        current,
        target: target,
        id: id,
        enabled: enabled,
      ),
    );
  }

  Future<void> configureCapability(String installationId, bool enabled) async {
    final current = registry;
    if (current == null) return;
    final installation = current.installations
        .where((entry) => entry.id == installationId)
        .singleOrNull;
    if (installation == null) return;
    final assignments = current.assignments
        .where((entry) => entry.installationId == installationId)
        .toList();
    final changes = <(AgentRegistryTarget, String)>[
      if (enabled && !installation.enabled)
        (AgentRegistryTarget.installation, installation.id),
      if (enabled)
        for (final assignment in assignments)
          if (!assignment.enabled)
            (AgentRegistryTarget.assignment, assignment.id),
      if (!enabled)
        for (final assignment in assignments)
          if (assignment.enabled)
            (AgentRegistryTarget.assignment, assignment.id),
      if (!enabled && installation.enabled)
        (AgentRegistryTarget.installation, installation.id),
    ];
    if (changes.isEmpty) return;
    await _registryOperation(() async {
      var next = current;
      for (final (target, id) in changes) {
        final configured = await (gateway as AgentRegistryGateway)
            .configureRegistry(next, target: target, id: id, enabled: enabled);
        if (configured.instanceId != next.instanceId ||
            configured.revision != next.revision + 1) {
          throw const FormatException('Registry configuration mismatch');
        }
        next = configured;
      }
      return next;
    }, expectedChanges: changes.length);
    if (hasCalendarExpertManagement && canManageCalendarExperts) {
      await loadCalendarExperts();
    }
  }

  Future<void> _registryOperation(
    Future<AgentRegistryView> Function()? change, {
    int expectedChanges = 1,
  }) async {
    if (!canManageRegistry) return;
    final previous = registry;
    _begin();
    registryFailure = null;
    _notify();
    try {
      final result = change == null
          ? await (gateway as AgentRegistryGateway).readRegistry(personId)
          : await change();
      if (_sealed || _disposed) return;
      if (result != null && result.personId != personId) {
        throw const FormatException('Registry Person mismatch');
      }
      if (change != null &&
          (result == null ||
              previous == null ||
              result.instanceId != previous.instanceId ||
              result.revision != previous.revision + expectedChanges)) {
        throw const FormatException('Registry configuration mismatch');
      }
      registry = result;
      registryLoaded = true;
      calendarExperts = null;
    } on Object catch (error) {
      if (_sealed || _disposed) return;
      registry = null;
      registryLoaded = false;
      calendarExperts = null;
      registryFailure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (registryFailure == 'vault_unavailable' ||
          registryFailure == 'interrupted') {
        _fail(registryFailure!);
      }
    } finally {
      _end();
      _notify();
    }
  }

  bool get usesVault => gateway is AgentVaultGateway;
  bool get isCalendarConversation => session?.scope != null;
  bool get isGeneralConversation =>
      usesVault &&
      session?.scope == null &&
      session?.dataClasses.singleOrNull == 'personal';
  bool get isConnectedConversation =>
      isCalendarConversation || isGeneralConversation;
  bool get isPersonalConversation =>
      isCalendarConversation && session?.dataClasses.singleOrNull == 'personal';

  bool get busy => _busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  bool get canSend =>
      !_busy && !needsReload && !needsRecovery && session != null;
  bool get canContinue =>
      canSend &&
      session?.continuation != null &&
      (isCalendarConversation
          ? _lastCalendarPrompt != null
          : isGeneralConversation && _lastConversationText != null);
  bool get canRetry =>
      canSend &&
      !canContinue &&
      failure != null &&
      (isCalendarConversation
          ? _lastCalendarPrompt != null
          : isGeneralConversation
          ? _lastConversationText != null
          : _lastPrompt != null);

  Future<void> load({bool newSession = false}) async {
    if (_disposed) return;
    if (_locking case final locking?) {
      await locking;
    }
    if (_busy || _disposed) return;
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
      final connected = await _connectedCalendarSetup();
      if (connected case (final context, final setup)) {
        final calendarGateway = gateway as AgentCalendarSessionGateway;
        final saved = newSession
            ? await calendarGateway.startCalendarSession(
                personId,
                setup.setupId,
              )
            : await calendarGateway.resumeCalendarSession(
                personId,
                setup.setupId,
              );
        _activeCalendarContext = context;
        _acceptSession(saved, setupId: setup.setupId);
      } else {
        _activeCalendarContext = null;
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
    if (_busy || _disposed || !needsRecovery) return;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      final original = session!;
      if (original.scope case final scope?) {
        if (gateway is! AgentCalendarSessionGateway) {
          throw const FormatException('Calendar recovery unavailable');
        }
        final saved = await (gateway as AgentCalendarSessionGateway)
            .recoverCalendarSession(original);
        _acceptSession(saved, setupId: scope.setupId);
      } else if (isGeneralConversation && gateway is AgentConversationGateway) {
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
    if (isCalendarConversation) {
      if (_lastCalendarPrompt == AgentCalendarPromptKind.freeText) {
        await sendCalendarText(_lastCalendarText!);
      } else {
        await sendCalendar(
          _lastCalendarPrompt!,
          focusMinutes: _lastFocusMinutes,
        );
      }
    } else if (isGeneralConversation) {
      await sendText(_lastConversationText!);
    } else {
      await send(_lastPrompt!);
    }
  }

  Future<void> continueTurn() async {
    if (!canContinue) return;
    if (isCalendarConversation) {
      await _sendCalendar(
        _lastCalendarPrompt!,
        focusMinutes: _lastFocusMinutes,
        text: _lastCalendarText,
        continuation: true,
      );
    } else {
      await _sendConversationText(_lastConversationText!, continuation: true);
    }
  }

  Future<void> sendText(String text) => isCalendarConversation
      ? sendCalendarText(text)
      : _sendConversationText(text);

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
    try {
      var update = await (gateway as AgentConversationGateway)
          .beginConversationTurn(request);
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
        if (!done) {
          var update = await (gateway as AgentConversationGateway)
              .stopConversationTurn(request);
          for (var attempt = 0; !update.done && attempt < 25; attempt++) {
            await Future<void>.delayed(const Duration(milliseconds: 80));
            update = await (gateway as AgentConversationGateway)
                .pollConversationTurn(request, 0);
          }
          done = update.done;
        }
        if (done) {
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

  Future<(AgentCalendarConversationContext, AgentCalendarSetupReceipt)?>
  _connectedCalendarSetup() async {
    final context = calendarContext?.call();
    if (context == null ||
        !context.usable ||
        context.day.personId != personId ||
        gateway is! AgentCalendarExpertGateway ||
        gateway is! AgentCalendarSessionGateway ||
        gateway is! AgentCalendarTurnGateway) {
      return null;
    }
    final overview = await (gateway as AgentCalendarExpertGateway)
        .readCalendarExperts(personId);
    if (overview.registry.personId != personId) {
      throw const FormatException('Calendar Person mismatch');
    }
    calendarExperts = overview;
    registry = overview.registry;
    registryLoaded = true;
    final candidates = overview.setups.where((setup) {
      final view = overview.views
          .where((entry) => entry.handle == setup.viewHandle)
          .singleOrNull;
      if (view == null ||
          !view.enabled ||
          view.provider != context.connection.provider ||
          !view.calendarIds.every(
            context.connection.selectedCalendarIds.contains,
          )) {
        return false;
      }
      final linkedIds = [setup.toolInstallationId, setup.expertInstallationId];
      final assignmentIds = [setup.toolAssignmentId, setup.expertAssignmentId];
      return linkedIds.every(
            (id) => overview.registry.installations.any(
              (entry) => entry.id == id && entry.enabled,
            ),
          ) &&
          assignmentIds.every(
            (id) => overview.registry.assignments.any(
              (entry) => entry.id == id && entry.enabled,
            ),
          );
    }).toList()..sort((left, right) => left.setupId.compareTo(right.setupId));
    return candidates.isEmpty ? null : (context, candidates.first);
  }

  Future<void> send(AgentFixturePrompt prompt) async {
    if (!canSend || _disposed || isCalendarConversation) return;
    final original = session!;
    _runSession = original;
    _lastPrompt = prompt;
    _begin();
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
        for (final event in _sealed ? <AgentEvent>[] : update.events) {
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
      _end();
      progress = AgentProgress.idle;
      _notify();
    }
  }

  Future<void> sendCalendar(
    AgentCalendarPromptKind prompt, {
    int focusMinutes = 60,
  }) => _sendCalendar(prompt, focusMinutes: focusMinutes);

  Future<void> sendCalendarText(String text) {
    final normalized = text.trim();
    if (normalized.isEmpty || normalized.length > 8192) return Future.value();
    return _sendCalendar(AgentCalendarPromptKind.freeText, text: normalized);
  }

  Future<void> _sendCalendar(
    AgentCalendarPromptKind prompt, {
    int focusMinutes = 60,
    String? text,
    bool continuation = false,
  }) async {
    if (!canSend ||
        _disposed ||
        !isCalendarConversation ||
        gateway is! AgentCalendarTurnGateway ||
        !(1 <= focusMinutes && focusMinutes <= 240)) {
      return;
    }
    final original = session!;
    final scope = original.scope!;
    final context = calendarContext?.call() ?? _activeCalendarContext;
    final overview = calendarExperts;
    final setup = overview?.setups
        .where((entry) => entry.setupId == scope.setupId)
        .singleOrNull;
    final view = setup == null
        ? null
        : overview?.views
              .where((entry) => entry.handle == setup.viewHandle)
              .singleOrNull;
    if (context == null ||
        !context.usable ||
        context.day.personId != personId ||
        context.connection.provider != scope.provider ||
        view == null ||
        !view.enabled ||
        view.provider != context.connection.provider ||
        !view.calendarIds.every(
          context.connection.selectedCalendarIds.contains,
        )) {
      failure = 'stale_context';
      needsReload = true;
      _notify();
      return;
    }
    final request = AgentCalendarTurnRequest(
      session: original,
      day: context.day,
      startsAt: context.day.startsAt,
      endsAt: context.day.endsAt,
      prompt: prompt,
      focusMinutes: focusMinutes,
      text: text,
      continuation: continuation,
      destination: prompt == AgentCalendarPromptKind.proposeFocus
          ? AgentCalendarDestination(
              provider: scope.provider,
              calendarId: view.calendarIds.first,
              connectionRevision: context.connection.revision,
              timezone: _fixedTimezone(context.day.timezoneOffsetSeconds),
            )
          : null,
    );
    _activeCalendarContext = context;
    _calendarRun = request;
    _runSession = original;
    _lastCalendarPrompt = prompt;
    _lastCalendarText = text;
    _lastFocusMinutes = focusMinutes;
    _begin();
    _stopRequested = false;
    failure = null;
    progress = AgentProgress.model;
    _notify();
    var done = false;
    try {
      var update = await (gateway as AgentCalendarTurnGateway)
          .beginCalendarTurn(request);
      var sequence = 0;
      while (true) {
        _validateUpdate(original, update.run, sequence);
        _acceptEvents(update.run.events);
        sequence = update.run.nextSequence;
        if (_stopRequested) progress = AgentProgress.stopping;
        _notify();
        if (update.run.done) {
          done = true;
          _runSession = null;
          if (update.run.session case final saved?) {
            _acceptSession(saved, setupId: scope.setupId);
            for (final outcome in update.result!.proposals) {
              if (outcome.action case final action?) {
                _proposals[outcome
                    .invocationId] = AgentProposalInspection.fromJson({
                  'schema_version': 1,
                  'person_id': personId,
                  'session_id': saved.id,
                  'invocation_id': outcome.invocationId,
                  'action': {
                    'action_id': action.id,
                    'execution_id': action.executionId,
                    'status': action.status.name,
                    'expires_at': action.expiresAt.toUtc().toIso8601String(),
                  },
                });
              } else if (outcome.failure case final reason?) {
                _proposalFailures[outcome.invocationId] = reason;
              }
            }
            needsReload = false;
          } else {
            _fail(update.run.failure ?? 'storage_unavailable');
          }
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        update = await (gateway as AgentCalendarTurnGateway).pollCalendarTurn(
          request,
          sequence,
        );
      }
    } on Object catch (error) {
      _fail(
        error is AgentVaultException ? error.failure : 'transport_unavailable',
      );
    } finally {
      try {
        if (!done) {
          var update = await (gateway as AgentCalendarTurnGateway)
              .stopCalendarTurn(request);
          for (var attempt = 0; !update.run.done && attempt < 25; attempt++) {
            await Future<void>.delayed(const Duration(milliseconds: 80));
            update = await (gateway as AgentCalendarTurnGateway)
                .pollCalendarTurn(request, 0);
          }
          done = update.run.done;
        }
        if (done) {
          await (gateway as AgentCalendarTurnGateway).releaseCalendarTurn(
            request,
          );
        }
      } on Object {
        needsReload = true;
        failure ??= 'transport_unavailable';
      }
      _calendarRun = null;
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
      if (_calendarRun case final request?) {
        await (gateway as AgentCalendarTurnGateway).stopCalendarTurn(request);
      } else if (_conversationRun case final request?) {
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

  void _acceptSession(AgentSession saved, {String? setupId}) {
    if (_sealed) return;
    if (saved.personId != personId ||
        (setupId == null) != (saved.scope == null) ||
        setupId != null && saved.scope?.setupId != setupId) {
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
    if (saved.scope == null) {
      if (saved.dataClasses.singleOrNull == 'personal') {
        _lastPrompt = null;
        _lastConversationText = lastUser?.text;
      } else {
        _lastPrompt = AgentFixturePrompt.values
            .where((prompt) => prompt.sampleText == lastUser?.text)
            .firstOrNull;
        _lastConversationText = null;
      }
      _lastCalendarPrompt = null;
      _lastCalendarText = null;
    } else {
      _lastPrompt = null;
      _lastCalendarPrompt = switch (lastUser?.text) {
        final text? when text.startsWith('Brief today\'s calendar') =>
          AgentCalendarPromptKind.briefing,
        final text? when text.startsWith('Propose a ') =>
          AgentCalendarPromptKind.proposeFocus,
        final text? when text.trim().isNotEmpty =>
          AgentCalendarPromptKind.freeText,
        _ => null,
      };
      _lastCalendarText =
          _lastCalendarPrompt == AgentCalendarPromptKind.freeText
          ? lastUser?.text
          : null;
    }
  }

  void _acceptEvents(List<AgentEvent> events) {
    for (final event in _sealed ? <AgentEvent>[] : events) {
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
    if (_busy ||
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
    registry = null;
    registryFailure = null;
    registryLoaded = false;
    calendarExperts = null;
    calendarExpertFailure = null;
    _pendingCalendarSetup = null;
    session = null;
    messages = [];
    _lastPrompt = null;
    vaultState = AgentVaultState.locked;
    _notify();
    await stop();
    await _operationDone?.future;
    _calendarRun = null;
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
    if (usesVault) {
      registry = null;
      registryLoaded = false;
      calendarExperts = null;
      _pendingCalendarSetup = null;
      session = null;
      messages = [];
      vaultState = AgentVaultState.unavailable;
    }
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(closeView());
    super.dispose();
  }
}

String _fixedTimezone(int offsetSeconds) {
  final duration = Duration(seconds: offsetSeconds);
  final sign = duration.isNegative ? '-' : '+';
  final minutes = duration.inMinutes.abs();
  final hours = (minutes ~/ 60).toString().padLeft(2, '0');
  final remainder = (minutes % 60).toString().padLeft(2, '0');
  return 'UTC$sign$hours:$remainder';
}
