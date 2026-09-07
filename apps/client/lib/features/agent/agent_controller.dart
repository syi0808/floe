import 'dart:async';

import 'package:flutter/foundation.dart';

import 'agent_fixture_gateway.dart';
import 'agent_registry.dart';
import 'agent_vault_gateway.dart';

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
  AgentVaultState? vaultState;
  bool _sealed = false;
  Completer<void>? _operationDone;
  Future<void>? _locking;
  AgentRegistryView? registry;
  String? registryFailure;
  bool registryLoaded = false;

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

  Future<void> _registryOperation(
    Future<AgentRegistryView> Function()? change,
  ) async {
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
              result.revision != previous.revision + 1)) {
        throw const FormatException('Registry configuration mismatch');
      }
      registry = result;
      registryLoaded = true;
    } on Object catch (error) {
      if (_sealed || _disposed) return;
      registry = null;
      registryLoaded = false;
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

  bool get busy => _busy;
  bool get running => _runSession != null;
  bool get needsRecovery => session?.activeTurn != null && !running;
  bool get canSend =>
      !_busy && !needsReload && !needsRecovery && session != null;
  bool get canRetry => canSend && failure != null && _lastPrompt != null;

  Future<void> load({bool newSession = false}) async {
    if (_busy || _disposed || _locking != null) return;
    _sealed = false;
    _begin();
    progress = AgentProgress.loading;
    _notify();
    try {
      if (gateway case final AgentVaultGateway vault) {
        final state = await vault.vaultStatus(personId);
        if (_sealed) return;
        vaultState = state;
        if (state != AgentVaultState.ready) {
          registry = null;
          registryLoaded = false;
          registryFailure = null;
          session = null;
          messages = [];
          needsReload = false;
          failure = null;
          return;
        }
      }
      final result = newSession
          ? await gateway.startAgentFixture(personId)
          : await gateway.resumeAgentFixture(personId);
      _acceptSession(result.session);
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
      final result = await gateway.recoverAgentFixture(session!);
      _acceptSession(result.session);
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
    if (canRetry) await send(_lastPrompt!);
  }

  Future<void> send(AgentFixturePrompt prompt) async {
    if (!canSend || _disposed) return;
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
    registry = null;
    registryFailure = null;
    registryLoaded = false;
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
    failure = reason;
    needsReload = true;
    if (usesVault) {
      registry = null;
      registryLoaded = false;
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
