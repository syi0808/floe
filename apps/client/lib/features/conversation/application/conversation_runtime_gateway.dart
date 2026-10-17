import 'dart:async';

import 'conversation_observation.dart';

import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';

final class CalendarProposalSubmission {
  const CalendarProposalSubmission({required this.operation, this.interaction});
  final CalendarAction operation;
  final AgentInteractionSnapshot? interaction;
}

final class ConversationTurnCompletion {
  const ConversationTurnCompletion({required this.run, required this.session});

  final AppRunSnapshot run;
  final AgentSession session;
}

abstract interface class ConversationRuntimeGateway {
  AppReadModel get readModel;

  PreparedSurfaceWork? get pendingWorkAdmission;
  Future<AppSurfaceWorkReceipt> submitTextWork({
    required AppSurfaceAnchor expected,
    required String text,
    String? retryOf,
  });

  /// Explicit command recovery. Ordinary page polling never calls this.
  Future<AppSurfaceWorkReceipt?> settlePendingWorkAdmission();
  Future<AppSurfaceSnapshot> readSurface(
    String sessionId, {
    AppSurfaceAnchor? through,
  });
  Future<AppSurfaceWorkPage> listWork(
    String sessionId, {
    AppSurfaceWorkCursor? cursor,
  });
  Future<AppRunSnapshot> readWorkRun(String runId);
  Future<void> cancelPendingWork(String workCommandId);

  Future<ActionProposalPreview> previewCalendarProposal({
    required Map<String, Object?> receipt,
    required String artifactId,
  });

  Future<CalendarProposalSubmission> submitCalendarProposal({
    required String commandId,
    required String sessionId,
    required String originRunId,
    required Map<String, Object?> receipt,
    required String artifactId,
    required String destinationRef,
  });

  /// Wait for a prior stopped observer's bounded admission handshake before
  /// choosing a Session snapshot. This cannot dispatch or cancel owner work.
  Future<void> awaitStoppedObservation({
    required ConversationObservation observation,
  });

  Future<void> synchronizeConversation(
    AgentSession session, {
    required ConversationObservation observation,
  });

  Future<ConversationTurnCompletion> continueConversationTurn(
    AgentContinuationRequest request, {
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

  /// Requests explicit cancellation and reports uncertainty when admission or
  /// a positive Start receipt cannot be established for this turn.
  Future<void> cancelContinuation(AgentContinuationRequest request);

  /// Returns true only after a cancellation command for a known active Run
  /// receives and validates its positive receipt.
  Future<bool> cancelObservedRun(String runId);
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
    Future<void> Function()? beforeAdmission,
    Duration observationInterval = const Duration(milliseconds: 500),
  }) => NativeConversationRuntimeGateway._withFields(
    client,
    readModel,
    loadSession,
    beforeAdmission,
    observationInterval,
  );

  NativeConversationRuntimeGateway._withFields(
    this._client,
    this.readModel,
    this._loadSession,
    this._beforeAdmission,
    this._observationInterval,
  );

  final AppWireConversationClient _client;
  @override
  final AppReadModel readModel;
  final Future<AgentSession> Function(String personId, String sessionId)
  _loadSession;
  final Future<void> Function()? _beforeAdmission;
  final Duration _observationInterval;
  _ActiveConversationTurn? _active;
  PreparedSurfaceWork? _pendingWork;
  Future<AppSurfaceWorkReceipt>? _pendingWorkFuture;
  bool _pendingWorkUncertain = false;
  final Map<String, PreparedCancelPendingWork> _pendingCancellations = {};

  @override
  PreparedSurfaceWork? get pendingWorkAdmission => _pendingWork;

  @override
  Future<AppSurfaceWorkReceipt> submitTextWork({
    required AppSurfaceAnchor expected,
    required String text,
    String? retryOf,
  }) async {
    if (_pendingWork != null) {
      throw StateError('A Work admission is still unresolved.');
    }
    // Pin the immutable command before awaiting readiness or transport.
    _pendingWork = _client.prepareSurfaceWork(
      expected: expected,
      text: text,
      retryOf: retryOf,
    );
    _pendingWorkUncertain = false;
    return _settleWork();
  }

  @override
  Future<AppSurfaceWorkReceipt?> settlePendingWorkAdmission() async =>
      _pendingWork == null ? null : await _settleWork();

  Future<AppSurfaceWorkReceipt> _settleWork() {
    if (_pendingWorkFuture != null) return _pendingWorkFuture!;
    final command = _pendingWork!;
    late final Future<AppSurfaceWorkReceipt> future;
    future =
        Future<AppSurfaceWorkReceipt>.sync(
          () => _sendRetainedWork(command),
        ).whenComplete(() {
          if (identical(_pendingWorkFuture, future)) _pendingWorkFuture = null;
        });
    _pendingWorkFuture = future;
    return future;
  }

  Future<AppSurfaceWorkReceipt> _sendRetainedWork(
    PreparedSurfaceWork command,
  ) async {
    try {
      await _beforeAdmission?.call();
      if (_pendingWorkUncertain) {
        final retained = await _client.getWork(command.commandId);
        if (retained != null) {
          _client.validateWorkReceipt(command, retained.receipt);
          if (identical(_pendingWork, command)) _pendingWork = null;
          return retained.receipt;
        }
      }
      final receipt = await _client.submitSurfaceWork(command);
      if (identical(_pendingWork, command)) _pendingWork = null;
      return receipt;
    } on Object catch (error) {
      final disposition = switch (error) {
        AppOwnerException() => error.commandOutcome,
        AppWireTransportException() => error.commandOutcome,
        _ => null,
      };
      if (disposition == CommandOutcome.notApplied ||
          (!_pendingWorkUncertain &&
              disposition == CommandOutcome.notAdmitted)) {
        if (identical(_pendingWork, command)) _pendingWork = null;
      } else {
        _pendingWorkUncertain = true;
      }
      rethrow;
    }
  }

  @override
  Future<AppSurfaceSnapshot> readSurface(
    String sessionId, {
    AppSurfaceAnchor? through,
  }) => _client.getSurface(sessionId, through: through);
  @override
  Future<AppSurfaceWorkPage> listWork(
    String sessionId, {
    AppSurfaceWorkCursor? cursor,
  }) => _client.listWork(sessionId, cursor: cursor);
  @override
  Future<AppRunSnapshot> readWorkRun(String runId) async {
    final run = await _client.getRun(runId);
    if (!readModel.applyRunSnapshot(run)) {
      throw const FormatException('Work Run epoch mismatch.');
    }
    return run;
  }

  @override
  Future<void> cancelPendingWork(String workCommandId) async {
    final command = _pendingCancellations.putIfAbsent(
      workCommandId,
      () => _client.prepareCancelPendingWork(workCommandId),
    );
    await _client.submitCancelPendingWork(command);
    if (identical(_pendingCancellations[workCommandId], command)) {
      _pendingCancellations.remove(workCommandId);
    }
  }

  @override
  Future<ActionProposalPreview> previewCalendarProposal({
    required Map<String, Object?> receipt,
    required String artifactId,
  }) =>
      _client.previewCalendarProposal(receipt: receipt, artifactId: artifactId);

  @override
  Future<CalendarProposalSubmission> submitCalendarProposal({
    required String commandId,
    required String sessionId,
    required String originRunId,
    required Map<String, Object?> receipt,
    required String artifactId,
    required String destinationRef,
  }) async {
    final result = await _client.submitCalendarProposal(
      commandId: commandId,
      sessionId: sessionId,
      originRunId: originRunId,
      receipt: receipt,
      artifactId: artifactId,
      destinationRef: destinationRef,
    );
    return CalendarProposalSubmission(
      operation: result.operation,
      interaction: result.interaction,
    );
  }

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
    AgentContinuationRequest? request,
    ConversationObservation observation,
  ) async {
    await awaitStoppedObservation(observation: observation);
    observation.check();
    if (_active != null) {
      throw StateError('A conversation turn is already active.');
    }
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
  Future<ConversationTurnCompletion> continueConversationTurn(
    AgentContinuationRequest request, {
    required void Function(AppRunSnapshot run) onRun,
    required ConversationObservation observation,
  }) async {
    final active = await _claim(request, observation);
    var receiptSettled = false;
    try {
      await synchronizeConversation(request.session, observation: observation);
      observation.check();
      await _beforeAdmission?.call();
      observation.check();
      final continuation = await _continuationFor(request);
      observation.check();
      final command = _client.prepareContinueTurn(
        sessionId: request.session.id,
        expectedRevision: request.session.revision,
        continuation: continuation,
      );
      active.commandId = command.commandId;
      readModel.markCommandPending(command.commandId);
      active.receiptFuture = _admit(command);
      final receipt = await active.receiptFuture!;
      active.receipt = receipt;
      active.receiptReady.complete();
      // Explicit cancellation survives a stopped observation, but it cannot
      // prevent a same-epoch positive receipt from settling the read model.
      if (observation.stopped) {
        if (active.cancelRequested) await _cancel(active);
        observation.check();
      }
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
      receiptSettled = true;
      if (active.cancelRequested) {
        // The Stop caller observes this failure. Keep the admitted Run's
        // observation alive so its terminal state can still be reconciled.
        try {
          await _cancel(active);
        } on Object {
          // _cancel retains the exact command identity for an explicit retry.
        }
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
      try {
        if (active.commandId != null &&
            (!receiptSettled || observation.stopped)) {
          readModel.sealForResync();
        }
      } finally {
        if (identical(_active, active)) _active = null;
        if (!active.finished.isCompleted) active.finished.complete();
      }
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
    if (runId == null) {
      throw StateError('No detached session Run is available.');
    }
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
        // Another queued Work may already own Session.activeTurn. Observe
        // only this explicit Run; current Work lineage comes from Work queries.
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
  Future<void> cancelContinuation(AgentContinuationRequest request) async {
    final active = _active;
    if (active == null || !identical(active.request, request)) {
      throw StateError('Conversation turn admission is unresolved.');
    }
    active.cancelRequested = true;
    await active.receiptReady.future;
    if (active.receipt == null) {
      throw StateError('Conversation turn admission is unresolved.');
    }
    await _cancel(active);
  }

  @override
  Future<bool> cancelObservedRun(String runId) async {
    final run = readModel.conversation.runs[runId];
    if (run == null || run.state.terminal) return false;
    final command = _client.prepareCancelRun(runId);
    final receipt = await _submitCancel(command);
    if (receipt.runId != runId || receipt.runtimeEpoch != run.runtimeEpoch) {
      throw const FormatException('Observed Run cancellation mismatch.');
    }
    return true;
  }

  Future<void> _cancel(_ActiveConversationTurn active) async {
    final receipt = active.receipt!;
    active.cancelCommand ??= _client.prepareCancelRun(receipt.runId);
    final future = active.cancelFuture ??= _submitCancel(active.cancelCommand!);
    try {
      final cancellation = await future;
      if (cancellation.runId != receipt.runId ||
          cancellation.runtimeEpoch != receipt.runtimeEpoch) {
        throw const FormatException('Conversation cancellation mismatch.');
      }
    } on Object {
      // Keep the prepared command (and ID) for a later user retry. A waiter
      // from an older attempt must not clear a newer Future.
      if (identical(active.cancelFuture, future)) active.cancelFuture = null;
      rethrow;
    }
  }

  Future<AppCancelRunReceipt> _submitCancel(PreparedCancelRun command) async {
    try {
      return await _client.submitCancelRun(command);
    } on Object {
      return _client.submitCancelRun(command);
    }
  }

  Future<AppCommandReceipt> _admit(PreparedContinueTurn command) async {
    try {
      return await _client.submitContinueTurn(command);
    } on Object {
      final recovered = await _client.getCommand(command.commandId);
      if (recovered != null) return recovered;
      return _client.submitContinueTurn(command);
    }
  }

  Future<AppContinuationRef> _continuationFor(
    AgentContinuationRequest request,
  ) async {
    final reference = request.session.continuation;
    if (reference == null) {
      throw const FormatException('Conversation continuation unavailable.');
    }
    return AppContinuationRef(id: reference.id);
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

  final AgentContinuationRequest? request;
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
