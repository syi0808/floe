import 'dart:async';

import 'conversation_observation.dart';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';

final class ConversationTurnCompletion {
  const ConversationTurnCompletion({required this.run, required this.session});

  final AppRunSnapshot run;
  final AgentSession session;
}

abstract interface class ConversationRuntimeGateway {
  AppReadModel get readModel;

  /// Wait for a prior stopped observer's bounded admission handshake before
  /// choosing a Session snapshot. This cannot dispatch or cancel owner work.
  Future<void> awaitStoppedObservation({
    required ConversationObservation observation,
  });

  Future<void> synchronizeConversation(
    AgentSession session, {
    required ConversationObservation observation,
  });

  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  });

  /// Observe an already-admitted Run (for example a linked resume child)
  /// to its terminal snapshot with the normal event/read-model machinery.
  Future<ConversationTurnCompletion> observeConversationRun(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  });

  Future<void> cancelConversationTurn(AgentConversationTurnRequest request);
  Future<void> cancelObservedRun(String runId);
  Future<ConversationTurnCompletion> observeSessionRun(
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  });
}

final class NativeConversationRuntimeGateway
    implements ConversationRuntimeGateway {
  factory NativeConversationRuntimeGateway({
    required AppWireConversationClient client,
    required AppReadModel readModel,
    required Future<AgentSession> Function(String personId, String sessionId)
    loadSession,
    Future<void> Function()? beforeStartTurn,
    Duration observationInterval = const Duration(milliseconds: 80),
  }) => NativeConversationRuntimeGateway._withFields(
    client,
    readModel,
    loadSession,
    beforeStartTurn,
    observationInterval,
  );

  NativeConversationRuntimeGateway._withFields(
    this._client,
    this.readModel,
    this._loadSession,
    this._beforeStartTurn,
    this._observationInterval,
  );

  final AppWireConversationClient _client;
  @override
  final AppReadModel readModel;
  final Future<AgentSession> Function(String personId, String sessionId)
  _loadSession;
  final Future<void> Function()? _beforeStartTurn;
  final Duration _observationInterval;
  _ActiveConversationTurn? _active;

  @override
  Future<void> awaitStoppedObservation({
    required ConversationObservation observation,
  }) async {
    observation.check();
    final previous = _active;
    if (previous != null && previous.observation.stopped) {
      await observation.read(() => previous.finished.future);
    }
    observation.check();
  }

  Future<_ActiveConversationTurn> _claim(
    AgentConversationTurnRequest? request,
    ConversationObservation observation,
  ) async {
    await awaitStoppedObservation(observation: observation);
    observation.check();
    if (_active != null)
      throw StateError('A conversation turn is already active.');
    final active = _ActiveConversationTurn(request, observation);
    _active = active;
    return active;
  }

  @override
  Future<void> synchronizeConversation(
    AgentSession session, {
    required ConversationObservation observation,
  }) async {
    observation.check();
    final projection = readModel.conversation;
    final result = await observation.read(
      () => _client.readEvents(
        after: projection.syncState == AppReadSyncState.synchronized
            ? projection.cursor
            : null,
      ),
    );
    observation.check();
    switch (result) {
      case AppEventsResyncRequired():
        await _bootstrapAt(result, session, observation: observation);
      case AppEventsPage():
        if (!readModel.applyEvents(result)) {
          throw const FormatException('Conversation event resync required.');
        }
        await _refreshActiveRun(session, observation);
    }
  }

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  }) async {
    final active = await _claim(request, observation);
    try {
      await synchronizeConversation(request.session, observation: observation);
      observation.check();
      await _beforeStartTurn?.call();
      observation.check();
      final continuation = await _continuationFor(request);
      final retryOf = await _retryFor(request, observation);
      observation.check();
      final command = _client.prepareStartTurn(
        sessionId: request.session.id,
        expectedRevision: request.session.revision,
        text: request.text,
        continuation: continuation,
        retryOf: retryOf,
      );
      active.commandId = command.commandId;
      readModel.markCommandPending(command.commandId);
      active.receiptFuture = _admit(command);
      final receipt = await active.receiptFuture!;
      active.receipt = receipt;
      active.receiptReady.complete();
      if (active.cancelRequested) await _cancel(active);
      // Admission remains exact and bounded even after a view detaches. Only
      // observation stops; the original owner command is never cancelled here.
      observation.check();
      if (!readModel.applyCommandReceipt(receipt)) {
        final resync = await observation.read(() => _client.readEvents());
        observation.check();
        if (resync is! AppEventsResyncRequired) {
          throw const FormatException('Conversation receipt resync required.');
        }
        await _bootstrapAt(
          resync,
          request.session,
          receipt: receipt,
          observation: observation,
        );
      }
      return await _observeReceipt(
        receipt,
        request.session,
        onRun: onRun,
        observation: observation,
      );
    } finally {
      if (!active.receiptReady.isCompleted) active.receiptReady.complete();
      // A stopped observer cannot leave this command's old projection available
      // to the next epoch. Seal before releasing its activity slot.
      if (active.commandId != null &&
          (active.receipt == null || observation.stopped)) {
        readModel.sealForResync();
      }
      if (identical(_active, active)) _active = null;
      if (!active.finished.isCompleted) active.finished.complete();
    }
  }

  @override
  Future<ConversationTurnCompletion> observeConversationRun(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  }) async {
    final active = await _claim(null, observation);
    try {
      await synchronizeConversation(session, observation: observation);
      if (!readModel.applyCommandReceipt(receipt)) {
        final resync = await observation.read(() => _client.readEvents());
        observation.check();
        if (resync is! AppEventsResyncRequired) {
          throw const FormatException('Conversation receipt resync required.');
        }
        await _bootstrapAt(
          resync,
          session,
          receipt: receipt,
          observation: observation,
        );
      }
      return await _observeReceipt(
        receipt,
        session,
        onRun: onRun,
        observation: observation,
      );
    } finally {
      if (identical(_active, active)) _active = null;
      if (!active.finished.isCompleted) active.finished.complete();
    }
  }

  @override
  Future<ConversationTurnCompletion> observeSessionRun(
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  }) async {
    final runId = session.activeTurn;
    if (runId == null)
      throw StateError('No detached session Run is available.');
    final active = await _claim(null, observation);
    try {
      await synchronizeConversation(session, observation: observation);
      return await _observeRun(
        runId,
        session.revision,
        session,
        onRun: onRun,
        observation: observation,
      );
    } finally {
      if (identical(_active, active)) _active = null;
      if (!active.finished.isCompleted) active.finished.complete();
    }
  }

  Future<ConversationTurnCompletion> _observeReceipt(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  }) => _observeRun(
    receipt.runId,
    receipt.sessionRevision,
    session,
    onRun: onRun,
    observation: observation,
    receipt: receipt,
  );

  Future<ConversationTurnCompletion> _observeRun(
    String runId,
    int minimumSessionRevision,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
    AppCommandReceipt? receipt,
  }) async {
    var lastNotifiedRevision = 0;
    while (true) {
      final result = await observation.read(
        () => _client.readEvents(after: readModel.conversation.cursor),
      );
      observation.check();
      switch (result) {
        case AppEventsResyncRequired():
          await _bootstrapAt(
            result,
            session,
            receipt: receipt,
            observation: observation,
          );
        case AppEventsPage():
          if (!readModel.applyEvents(result)) {
            throw const FormatException('Conversation event resync required.');
          }
      }
      final durable = await observation.read(() => _client.getRun(runId));
      observation.check();
      if (!readModel.applyRunSnapshot(durable)) {
        final resync = await observation.read(() => _client.readEvents());
        observation.check();
        if (resync is! AppEventsResyncRequired) {
          throw const FormatException('Conversation Run resync required.');
        }
        await _bootstrapAt(
          resync,
          session,
          receipt: receipt,
          observation: observation,
        );
      }
      final run = readModel.conversation.runs[runId]!;
      if (run.sessionId != session.id) {
        throw const FormatException('Conversation Run scope mismatch.');
      }
      if (run.revision > lastNotifiedRevision) {
        lastNotifiedRevision = run.revision;
        onRun(run);
      }
      if (run.state.terminal) {
        final reloaded = await observation.read(
          () => _loadSession(session.personId, session.id),
        );
        observation.check();
        if (reloaded.id != session.id ||
            reloaded.personId != session.personId ||
            reloaded.activeTurn == runId ||
            reloaded.revision < minimumSessionRevision) {
          throw const FormatException(
            'Conversation terminal snapshot mismatch.',
          );
        }
        if (reloaded.activeTurn case final linkedRunId?) {
          runId = linkedRunId;
          minimumSessionRevision = reloaded.revision;
          receipt = null;
          lastNotifiedRevision = 0;
          continue;
        }
        return ConversationTurnCompletion(run: run, session: reloaded);
      }
      if (_observationInterval > Duration.zero) {
        await observation.read(
          () => Future<void>.delayed(_observationInterval),
        );
      }
    }
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    final active = _active;
    if (active == null || !identical(active.request, request)) return;
    active.cancelRequested = true;
    await active.receiptReady.future;
    if (active.receipt != null) await _cancel(active);
  }

  @override
  Future<void> cancelObservedRun(String runId) async {
    final run = readModel.conversation.runs[runId];
    if (run == null || run.state.terminal) return;
    final command = _client.prepareCancelRun(runId);
    final receipt = await _submitCancel(command);
    if (receipt.runId != runId || receipt.runtimeEpoch != run.runtimeEpoch) {
      throw const FormatException('Observed Run cancellation mismatch.');
    }
  }

  Future<void> _cancel(_ActiveConversationTurn active) async {
    final receipt = active.receipt!;
    active.cancelCommand ??= _client.prepareCancelRun(receipt.runId);
    active.cancelFuture ??= _submitCancel(active.cancelCommand!);
    final cancellation = await active.cancelFuture!;
    if (cancellation.runId != receipt.runId ||
        cancellation.runtimeEpoch != receipt.runtimeEpoch) {
      throw const FormatException('Conversation cancellation mismatch.');
    }
  }

  Future<AppCancelRunReceipt> _submitCancel(PreparedCancelRun command) async {
    try {
      return await _client.submitCancelRun(command);
    } on Object {
      return _client.submitCancelRun(command);
    }
  }

  Future<AppCommandReceipt> _admit(PreparedStartTurn command) async {
    try {
      return await _client.submitStartTurn(command);
    } on Object {
      final recovered = await _client.getCommand(command.commandId);
      if (recovered != null) return recovered;
      return _client.submitStartTurn(command);
    }
  }

  Future<AppContinuationRef?> _continuationFor(
    AgentConversationTurnRequest request,
  ) async {
    if (!request.continuation) return null;
    final reference = request.session.continuation;
    if (reference == null) {
      throw const FormatException('Conversation continuation unavailable.');
    }
    return AppContinuationRef(id: reference.id);
  }

  Future<String?> _retryFor(
    AgentConversationTurnRequest request,
    ConversationObservation observation,
  ) async {
    final retryOf = request.retryOf;
    if (retryOf == null) return null;
    final source = await observation.read(() => _client.getRun(retryOf));
    observation.check();
    if (source.sessionId != request.session.id ||
        source.state != AppRunState.finished) {
      throw const FormatException('Conversation retry mismatch.');
    }
    return source.runId;
  }

  Future<void> _bootstrapAt(
    AppEventsResyncRequired boundary,
    AgentSession session, {
    AppCommandReceipt? receipt,
    required ConversationObservation observation,
  }) async {
    observation.check();
    final projection = readModel.conversation;
    final commandIds = {...projection.commands.keys, ?receipt?.commandId};
    final runIds = {
      ...projection.runs.keys,
      ?receipt?.runId,
      ?session.activeTurn,
    };
    readModel.requireResync(boundary);
    final commands = <AppCommandReceipt>[];
    for (final commandId in commandIds) {
      final command = await observation.read(
        () => _client.getCommand(commandId),
      );
      observation.check();
      if (command != null) commands.add(command);
    }
    final runs = <AppRunSnapshot>[];
    for (final runId in runIds) {
      runs.add(await observation.read(() => _client.getRun(runId)));
      observation.check();
    }
    readModel.bootstrap(
      cursor: boundary.snapshotCursor,
      commands: commands,
      runs: runs,
    );
  }

  Future<void> _refreshActiveRun(
    AgentSession session,
    ConversationObservation observation,
  ) async {
    final runId = session.activeTurn;
    if (runId == null) return;
    final run = await observation.read(() => _client.getRun(runId));
    observation.check();
    if (run.sessionId != session.id || !readModel.applyRunSnapshot(run)) {
      throw const FormatException('Conversation active Run mismatch.');
    }
  }
}

final class _ActiveConversationTurn {
  _ActiveConversationTurn(this.request, this.observation);

  final AgentConversationTurnRequest? request;
  final ConversationObservation observation;
  final Completer<void> finished = Completer<void>();
  final Completer<void> receiptReady = Completer<void>();
  String? commandId;
  Future<AppCommandReceipt>? receiptFuture;
  AppCommandReceipt? receipt;
  bool cancelRequested = false;
  PreparedCancelRun? cancelCommand;
  Future<AppCancelRunReceipt>? cancelFuture;
}
