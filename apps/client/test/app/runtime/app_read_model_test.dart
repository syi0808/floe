import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const sessionId = '00000000-0000-4000-8000-000000000301';
  const commandId = '00000000-0000-4000-8000-000000000302';
  const runId = '00000000-0000-4000-8000-000000000303';

  AppCommandReceipt receipt() => const AppCommandReceipt(
    commandId: commandId,
    runId: runId,
    sessionRevision: 1,
    runtimeEpoch: 7,
  );

  AppRunSnapshot run(int revision, AppRunState state) => AppRunSnapshot(
    runId: runId,
    sessionId: sessionId,
    revision: revision,
    runtimeEpoch: 7,
    executorGeneration: 1,
    state: state,
    progress: state.name,
    report: null,
  );

  test('ack before event and event before ack reduce to one backend state', () {
    final model = AppReadModel();
    model.requireResync(
      const AppEventsResyncRequired(
        AppEventCursor(runtimeEpoch: 7, cursor: 10),
      ),
    );
    model.bootstrap(cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 10));
    final beforePending = model.conversation;
    model.markCommandPending(commandId);
    expect(beforePending.pendingCommandIds, isEmpty);
    expect(model.applyCommandReceipt(receipt()), isTrue);
    expect(model.conversation.pendingCommandIds, isEmpty);
    expect(model.conversation.canSend(sessionId), isTrue);

    expect(
      model.applyEvents(
        AppEventsPage(
          cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 12),
          events: [
            AppCommandUpdated(
              cursor: 11,
              aggregateRevision: 1,
              receipt: receipt(),
            ),
            AppRunUpdated(
              cursor: 12,
              aggregateRevision: 1,
              run: run(1, AppRunState.executing),
            ),
          ],
        ),
      ),
      isTrue,
    );
    expect(model.conversation.commands, hasLength(1));
    expect(model.conversation.canSend(sessionId), isFalse);

    final eventFirst = AppReadModel()
      ..bootstrap(cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 10))
      ..markCommandPending(commandId);
    expect(
      eventFirst.applyEvents(
        AppEventsPage(
          cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 11),
          events: [
            AppCommandUpdated(
              cursor: 11,
              aggregateRevision: 1,
              receipt: receipt(),
            ),
          ],
        ),
      ),
      isTrue,
    );
    expect(eventFirst.applyCommandReceipt(receipt()), isTrue);
    expect(eventFirst.conversation.commands, hasLength(1));
  });

  test('stale snapshots are ignored and cursor gaps require resync', () {
    final model = AppReadModel()
      ..bootstrap(
        cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 20),
        runs: [run(2, AppRunState.finished)],
      );
    expect(model.applyRunSnapshot(run(1, AppRunState.executing)), isTrue);
    expect(model.conversation.runs[runId]!.revision, 2);
    expect(
      model.applyEvents(
        AppEventsPage(
          cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 22),
          events: [
            AppRunUpdated(
              cursor: 22,
              aggregateRevision: 3,
              run: run(3, AppRunState.finished),
            ),
          ],
        ),
      ),
      isFalse,
    );
    expect(model.conversation.syncState, AppReadSyncState.resyncRequired);
  });

  test('runtime epoch changes seal old projections', () {
    final model = AppReadModel()
      ..bootstrap(
        cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 2),
        commands: [receipt()],
        runs: [run(1, AppRunState.executing)],
      );
    model.requireResync(
      const AppEventsResyncRequired(AppEventCursor(runtimeEpoch: 8, cursor: 0)),
    );
    expect(model.conversation.commands, isEmpty);
    expect(model.conversation.runs, isEmpty);
    expect(model.conversation.syncState, AppReadSyncState.resyncRequired);
    expect(model.applyCommandReceipt(receipt()), isFalse);
    expect(model.conversation.commands, isEmpty);
    expect(model.conversation.cursor!.runtimeEpoch, 8);
  });
}
