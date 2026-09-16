import 'dart:async';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_fixture_gateway.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';

final class ConversationTurnCompletion {
  const ConversationTurnCompletion({required this.run, required this.session});

  final AppRunSnapshot run;
  final AgentSession session;
}

abstract interface class ConversationRuntimeGateway {
  AppReadModel get readModel;

  Future<void> synchronizeConversation(AgentSession session);

  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  });

  Future<void> cancelConversationTurn(AgentConversationTurnRequest request);
}

abstract interface class ConversationRuntimeProvider {
  ConversationRuntimeGateway? get conversationRuntime;
}

final class NativeConversationRuntimeGateway
    implements ConversationRuntimeGateway {
  factory NativeConversationRuntimeGateway({
    required FloeClient client,
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

  final FloeClient _client;
  @override
  final AppReadModel readModel;
  final Future<AgentSession> Function(String personId, String sessionId)
  _loadSession;
  final Future<void> Function()? _beforeStartTurn;
  final Duration _observationInterval;
  _ActiveConversationTurn? _active;

  @override
  Future<void> synchronizeConversation(AgentSession session) async {
    final projection = readModel.conversation;
    final result = await _client.readEvents(
      after: projection.syncState == AppReadSyncState.synchronized
          ? projection.cursor
          : null,
    );
    switch (result) {
      case AppEventsResyncRequired():
        await _bootstrapAt(result, session);
      case AppEventsPage():
        if (!readModel.applyEvents(result)) {
          throw const FormatException('Conversation event resync required.');
        }
        await _refreshActiveRun(session);
    }
  }

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    if (_active != null) {
      throw StateError('A conversation turn is already active.');
    }
    final active = _ActiveConversationTurn(request);
    _active = active;
    try {
      await synchronizeConversation(request.session);
      await _beforeStartTurn?.call();
      final continuation = await _continuationFor(request);
      final retryOf = await _retryFor(request);
      final command = _client.prepareStartTurn(
        sessionId: request.session.id,
        expectedRevision: request.session.revision,
        text: request.text,
        continuation: continuation,
        retryOf: retryOf,
        profileId: request.profileId,
      );
      active.commandId = command.commandId;
      readModel.markCommandPending(command.commandId);
      active.receiptFuture = _admit(command);
      final receipt = await active.receiptFuture!;
      active.receipt = receipt;
      if (!readModel.applyCommandReceipt(receipt)) {
        final resync = await _client.readEvents();
        if (resync is! AppEventsResyncRequired) {
          throw const FormatException('Conversation receipt resync required.');
        }
        await _bootstrapAt(resync, request.session, receipt: receipt);
      }
      active.receiptReady.complete();
      if (active.cancelRequested) await _cancel(active);

      var lastNotifiedRevision = 0;
      while (true) {
        final result = await _client.readEvents(
          after: readModel.conversation.cursor,
        );
        switch (result) {
          case AppEventsResyncRequired():
            await _bootstrapAt(result, request.session, receipt: receipt);
          case AppEventsPage():
            if (!readModel.applyEvents(result)) {
              throw const FormatException(
                'Conversation event resync required.',
              );
            }
        }
        final durable = await _client.getRun(receipt.runId);
        if (!readModel.applyRunSnapshot(durable)) {
          final resync = await _client.readEvents();
          if (resync is! AppEventsResyncRequired) {
            throw const FormatException('Conversation Run resync required.');
          }
          await _bootstrapAt(resync, request.session, receipt: receipt);
        }
        final run = readModel.conversation.runs[receipt.runId]!;
        if (run.sessionId != request.session.id) {
          throw const FormatException('Conversation Run scope mismatch.');
        }
        if (run.revision > lastNotifiedRevision) {
          lastNotifiedRevision = run.revision;
          onRun(run);
        }
        if (run.state == AppRunState.finished) {
          final session = await _loadSession(
            request.session.personId,
            request.session.id,
          );
          if (session.id != request.session.id ||
              session.personId != request.session.personId ||
              session.activeTurn != null ||
              session.revision < receipt.sessionRevision) {
            throw const FormatException(
              'Conversation terminal snapshot mismatch.',
            );
          }
          return ConversationTurnCompletion(run: run, session: session);
        }
        if (_observationInterval > Duration.zero) {
          await Future<void>.delayed(_observationInterval);
        }
      }
    } finally {
      if (!active.receiptReady.isCompleted) active.receiptReady.complete();
      if (active.commandId != null && active.receipt == null) {
        readModel.sealForResync();
      }
      if (identical(_active, active)) _active = null;
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
    if (reference == null || reference.level >= 3) {
      throw const FormatException('Conversation continuation unavailable.');
    }
    final source = await _client.getRun(reference.turnId);
    if (source.sessionId != request.session.id ||
        source.state != AppRunState.finished) {
      throw const FormatException('Conversation continuation mismatch.');
    }
    return AppContinuationRef(
      runId: source.runId,
      executorGeneration: source.executorGeneration,
      level: reference.level + 1,
    );
  }

  Future<String?> _retryFor(AgentConversationTurnRequest request) async {
    final retryOf = request.retryOf;
    if (retryOf == null) return null;
    final source = await _client.getRun(retryOf);
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
  }) async {
    final projection = readModel.conversation;
    final commandIds = {...projection.commands.keys, ?receipt?.commandId};
    final runIds = {
      ...projection.runs.keys,
      ?receipt?.runId,
      ?session.activeTurn,
      ?session.continuation?.turnId,
    };
    readModel.requireResync(boundary);
    final commands = <AppCommandReceipt>[];
    for (final commandId in commandIds) {
      final command = await _client.getCommand(commandId);
      if (command != null) commands.add(command);
    }
    final runs = <AppRunSnapshot>[];
    for (final runId in runIds) {
      runs.add(await _client.getRun(runId));
    }
    readModel.bootstrap(
      cursor: boundary.snapshotCursor,
      commands: commands,
      runs: runs,
    );
  }

  Future<void> _refreshActiveRun(AgentSession session) async {
    final runId = session.activeTurn;
    if (runId == null) return;
    final run = await _client.getRun(runId);
    if (run.sessionId != session.id || !readModel.applyRunSnapshot(run)) {
      throw const FormatException('Conversation active Run mismatch.');
    }
  }
}

final class _ActiveConversationTurn {
  _ActiveConversationTurn(this.request);

  final AgentConversationTurnRequest request;
  final Completer<void> receiptReady = Completer<void>();
  String? commandId;
  Future<AppCommandReceipt>? receiptFuture;
  AppCommandReceipt? receipt;
  bool cancelRequested = false;
  PreparedCancelRun? cancelCommand;
  Future<AppCancelRunReceipt>? cancelFuture;
}
