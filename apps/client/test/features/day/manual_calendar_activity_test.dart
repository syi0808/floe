import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/infrastructure/app_wire_day_gateway.dart';
import 'package:floe_client/features/day/presentation/manual_calendar_activity.dart';

void main() {
  testWidgets('a late list page cannot replace a newer inspection', (
    tester,
  ) async {
    final delayedPage = Completer<Map<String, dynamic>>();
    final pageStarted = Completer<void>();
    var pageReads = 0;
    final transport = _SyntheticAppWireTransport(
      onQuery: (query) async {
        switch (query['kind']) {
          case 'day.external_calendar_operations':
            pageReads++;
            if (pageReads == 1) return _page([_unknown(revision: 1)]);
            pageStarted.complete();
            return delayedPage.future;
          case 'day.external_calendar_operation.get':
            return _operation(_succeeded(revision: 2, pending: true));
          default:
            throw StateError('Unexpected query: $query');
        }
      },
    );
    final gateway = AppWireDayGateway(transport);

    await tester.pumpWidget(_activityApp(gateway));
    await tester.pump();
    expect(
      find.text(
        'Outcome unknown. The Calendar may have changed; check status before trying again.',
      ),
      findsOneWidget,
    );

    await tester.tap(find.byTooltip('Reload Calendar activity'));
    await tester.pump();
    await pageStarted.future.timeout(const Duration(seconds: 1));
    await tester.tap(find.text('Check status'));
    await tester.pump();
    await tester.pump();
    expect(
      find.text('Calendar change succeeded; Day update is pending.'),
      findsOneWidget,
    );

    delayedPage.complete(_page([_unknown(revision: 1)]));
    await tester.pump();
    await tester.pump();
    expect(
      find.text('Calendar change succeeded; Day update is pending.'),
      findsOneWidget,
    );
  });

  testWidgets('a late reconcile result cannot replace a newer list page', (
    tester,
  ) async {
    final delayedReconcile = Completer<Map<String, dynamic>>();
    final reconcileStarted = Completer<void>();
    var pageReads = 0;
    final transport = _SyntheticAppWireTransport(
      onQuery: (query) async {
        if (query['kind'] != 'day.external_calendar_operations') {
          throw StateError('Unexpected query: $query');
        }
        pageReads++;
        return _page([
          pageReads == 1
              ? _unknown(revision: 1)
              : _succeeded(revision: 2, pending: true),
        ]);
      },
      onCommand: (command) {
        expect(command, {
          'kind': 'day.external_calendar_operation.reconcile',
          'operation_ref': _operationRef,
          'expected_revision': 1,
        });
        reconcileStarted.complete();
        return delayedReconcile.future;
      },
    );
    final gateway = AppWireDayGateway(transport);

    await tester.pumpWidget(_activityApp(gateway));
    await tester.pump();
    await tester.tap(find.text('Reconcile'));
    await tester.pump();
    await reconcileStarted.future.timeout(const Duration(seconds: 1));

    await tester.tap(find.byTooltip('Reload Calendar activity'));
    await tester.pump();
    await tester.pump();
    expect(
      find.text('Calendar change succeeded; Day update is pending.'),
      findsOneWidget,
    );

    delayedReconcile.complete(_operation(_unknown(revision: 1)));
    await tester.pump();
    await tester.pump();
    expect(
      find.text('Calendar change succeeded; Day update is pending.'),
      findsOneWidget,
    );
    expect(transport.commands, hasLength(1));
  });

  testWidgets('same-revision status changes are rejected', (tester) async {
    await _sameRevisionChangeIsRejected(
      tester,
      before: _unknown(revision: 1),
      after: _succeeded(revision: 1, pending: false),
    );
  });

  testWidgets('same-revision collection changes are rejected', (tester) async {
    await _sameRevisionChangeIsRejected(
      tester,
      before: _succeeded(revision: 1, pending: false),
      after: _succeeded(revision: 1, pending: true),
    );
  });

  testWidgets(
    'late updates reject same-revision status and collection conflicts with a newer receipt',
    (tester) async {
      await _cachedUpdateConflictIsRejected(
        tester,
        before: _unknown(revision: 1),
        current: _succeeded(revision: 2, pending: true),
        late: _blocked(revision: 2),
      );
      await _cachedUpdateConflictIsRejected(
        tester,
        before: _unknown(revision: 1),
        current: _succeeded(revision: 2, pending: true),
        late: _succeeded(revision: 2, pending: false),
      );
    },
  );

  testWidgets(
    'a divergent same-revision page preserves rows, cursor, and receipt cache',
    (tester) async {
      final first = _unknown(revision: 1);
      final second = _succeeded(
        ref: _secondOperationRef,
        revision: 1,
        pending: false,
      );
      var pageReads = 0;
      final transport = _SyntheticAppWireTransport(
        onQuery: (query) {
          if (query['kind'] != 'day.external_calendar_operations') {
            throw StateError('Unexpected query: $query');
          }
          pageReads++;
          return Future.value(switch (pageReads) {
            1 => _page([first, second], nextCursor: _secondOperationRef),
            2 => _page([
              _succeeded(revision: 2, pending: true),
              _succeeded(ref: _secondOperationRef, revision: 1, pending: true),
            ]),
            3 => _page([first, second], nextCursor: _secondOperationRef),
            _ => throw StateError('Unexpected page read $pageReads.'),
          });
        },
      );

      await tester.pumpWidget(_activityApp(AppWireDayGateway(transport)));
      await tester.pump();
      expect(find.text(_description(first)), findsOneWidget);
      expect(find.text(_description(second)), findsOneWidget);
      expect(find.text('Next page'), findsOneWidget);

      await tester.tap(find.byTooltip('Reload Calendar activity'));
      await tester.pump();
      await tester.pump();
      expect(
        find.textContaining('Calendar operation status regressed.'),
        findsOneWidget,
      );
      expect(find.text(_description(first)), findsOneWidget);
      expect(find.text(_description(second)), findsOneWidget);
      expect(find.text('Next page'), findsOneWidget);

      await tester.tap(find.byTooltip('Reload Calendar activity'));
      await tester.pump();
      await tester.pump();
      expect(
        find.textContaining('Calendar operation status regressed.'),
        findsNothing,
      );
      expect(find.text(_description(first)), findsOneWidget);
      expect(find.text(_description(second)), findsOneWidget);
      expect(pageReads, 3);
    },
  );

  testWidgets('replacing the gateway reloads and fences its late list page', (
    tester,
  ) async {
    final oldPage = Completer<Map<String, dynamic>>();
    final oldPageStarted = Completer<void>();
    final oldTransport = _SyntheticAppWireTransport(
      onQuery: (query) {
        expect(query['kind'], 'day.external_calendar_operations');
        oldPageStarted.complete();
        return oldPage.future;
      },
    );
    final newTransport = _SyntheticAppWireTransport(
      onQuery: (query) async {
        expect(query['kind'], 'day.external_calendar_operations');
        return _page([_blocked(revision: 1)]);
      },
    );

    await tester.pumpWidget(_activityApp(AppWireDayGateway(oldTransport)));
    await oldPageStarted.future.timeout(const Duration(seconds: 1));
    await tester.pumpWidget(_activityApp(AppWireDayGateway(newTransport)));
    await tester.pump();
    await tester.pump();
    expect(newTransport.queries, hasLength(1));
    expect(
      find.text('Blocked; no Calendar change was confirmed'),
      findsOneWidget,
    );

    oldPage.complete(_page([_unknown(revision: 9)], nextCursor: _operationRef));
    await tester.pump();
    await tester.pump();
    expect(
      find.text('Blocked; no Calendar change was confirmed'),
      findsOneWidget,
    );
    expect(find.text('Next page'), findsNothing);
  });

  testWidgets(
    'gateway replacement clears old cursors and fences late update and load errors',
    (tester) async {
      final delayedInspect = Completer<Map<String, dynamic>>();
      final inspectStarted = Completer<void>();
      final delayedOldPage = Completer<Map<String, dynamic>>();
      final oldPageStarted = Completer<void>();
      var oldPageReads = 0;
      final oldTransport = _SyntheticAppWireTransport(
        onQuery: (query) {
          switch (query['kind']) {
            case 'day.external_calendar_operations':
              oldPageReads++;
              if (oldPageReads == 1) {
                return Future.value(
                  _page([_unknown(revision: 1)], nextCursor: _operationRef),
                );
              }
              if (oldPageReads == 2) {
                expect(query['cursor'], _operationRef);
                return Future.value(
                  _page([_unknown(ref: _secondOperationRef, revision: 9)]),
                );
              }
              oldPageStarted.complete();
              return delayedOldPage.future;
            case 'day.external_calendar_operation.get':
              inspectStarted.complete();
              return delayedInspect.future;
            default:
              throw StateError('Unexpected query: $query');
          }
        },
      );
      var replacementInspections = 0;
      final newTransport = _SyntheticAppWireTransport(
        onQuery: (query) async {
          switch (query['kind']) {
            case 'day.external_calendar_operations':
              return _page([_blocked(ref: _secondOperationRef, revision: 1)]);
            case 'day.external_calendar_operation.get':
              replacementInspections++;
              return _operation(
                _blocked(ref: _secondOperationRef, revision: 1),
              );
            default:
              throw StateError('Unexpected query: $query');
          }
        },
      );

      await tester.pumpWidget(_activityApp(AppWireDayGateway(oldTransport)));
      await tester.pump();
      await tester.tap(find.text('Next page'));
      await tester.pump();
      await tester.pump();
      expect(find.text('Previous page'), findsOneWidget);

      await tester.tap(find.text('Check status'));
      await tester.pump();
      await inspectStarted.future.timeout(const Duration(seconds: 1));
      await tester.tap(find.byTooltip('Reload Calendar activity'));
      await tester.pump();
      await oldPageStarted.future.timeout(const Duration(seconds: 1));

      await tester.pumpWidget(_activityApp(AppWireDayGateway(newTransport)));
      await tester.pump();
      await tester.pump();
      expect(
        find.text('Blocked; no Calendar change was confirmed'),
        findsOneWidget,
      );
      expect(find.text('Next page'), findsNothing);
      expect(find.text('Previous page'), findsNothing);

      await tester.tap(find.text('Check status'));
      await tester.pump();
      await tester.pump();
      expect(replacementInspections, 1);

      delayedInspect.complete(
        _operation(_unknown(ref: _secondOperationRef, revision: 10)),
      );
      await tester.pump();
      await tester.pump();
      expect(
        find.text('Blocked; no Calendar change was confirmed'),
        findsOneWidget,
      );

      delayedOldPage.completeError(StateError('late old-owner failure'));
      await tester.pump();
      await tester.pump();
      expect(find.text('late old-owner failure'), findsNothing);
      expect(
        find.text('Blocked; no Calendar change was confirmed'),
        findsOneWidget,
      );
    },
  );

  testWidgets('duplicate reconcile taps dispatch one command', (tester) async {
    final delayedReconcile = Completer<Map<String, dynamic>>();
    final reconcileStarted = Completer<void>();
    final transport = _SyntheticAppWireTransport(
      onQuery: (query) async {
        expect(query['kind'], 'day.external_calendar_operations');
        return _page([_unknown(revision: 1)]);
      },
      onCommand: (command) {
        expect(command['kind'], 'day.external_calendar_operation.reconcile');
        reconcileStarted.complete();
        return delayedReconcile.future;
      },
    );

    await tester.pumpWidget(_activityApp(AppWireDayGateway(transport)));
    await tester.pump();
    await tester.tap(find.text('Reconcile'));
    await tester.pump();
    await reconcileStarted.future.timeout(const Duration(seconds: 1));
    expect(find.text('Reconcile'), findsOneWidget);
    await tester.tap(find.text('Reconcile'));
    await tester.pump();
    expect(transport.commands, hasLength(1));

    delayedReconcile.complete(_operation(_unknown(revision: 1)));
    await tester.pump();
    await tester.pump();
  });

  testWidgets('late inspect error after Activity disposal is ignored', (
    tester,
  ) async {
    final delayedInspect = Completer<Map<String, dynamic>>();
    final inspectStarted = Completer<void>();
    final transport = _SyntheticAppWireTransport(
      onQuery: (query) {
        switch (query['kind']) {
          case 'day.external_calendar_operations':
            return Future.value(_page([_unknown(revision: 1)]));
          case 'day.external_calendar_operation.get':
            inspectStarted.complete();
            return delayedInspect.future;
          default:
            throw StateError('Unexpected query: $query');
        }
      },
    );

    await tester.pumpWidget(_activityApp(AppWireDayGateway(transport)));
    await tester.pump();
    await tester.tap(find.text('Check status'));
    await tester.pump();
    await inspectStarted.future.timeout(const Duration(seconds: 1));

    await tester.pumpWidget(const SizedBox.shrink());
    delayedInspect.completeError(StateError('disposed owner failure'));
    await tester.pump();
    await tester.pump();
    expect(tester.takeException(), isNull);
  });
}

Future<void> _sameRevisionChangeIsRejected(
  WidgetTester tester, {
  required ManualCalendarOperationReceipt before,
  required ManualCalendarOperationReceipt after,
}) async {
  final transport = _SyntheticAppWireTransport(
    onQuery: (query) async {
      switch (query['kind']) {
        case 'day.external_calendar_operations':
          return _page([before]);
        case 'day.external_calendar_operation.get':
          return _operation(after);
        default:
          throw StateError('Unexpected query: $query');
      }
    },
  );

  await tester.pumpWidget(_activityApp(AppWireDayGateway(transport)));
  await tester.pump();
  final beforeText = _description(before);
  expect(find.text(beforeText), findsOneWidget);

  await tester.tap(find.text('Check status'));
  await tester.pump();
  await tester.pump();
  expect(find.text(beforeText), findsOneWidget);
  expect(
    find.textContaining('Calendar operation status regressed.'),
    findsOneWidget,
  );
}

Future<void> _cachedUpdateConflictIsRejected(
  WidgetTester tester, {
  required ManualCalendarOperationReceipt before,
  required ManualCalendarOperationReceipt current,
  required ManualCalendarOperationReceipt late,
}) async {
  final delayedInspect = Completer<Map<String, dynamic>>();
  final inspectStarted = Completer<void>();
  var pageReads = 0;
  final transport = _SyntheticAppWireTransport(
    onQuery: (query) {
      switch (query['kind']) {
        case 'day.external_calendar_operations':
          pageReads++;
          return Future.value(_page([pageReads == 1 ? before : current]));
        case 'day.external_calendar_operation.get':
          inspectStarted.complete();
          return delayedInspect.future;
        default:
          throw StateError('Unexpected query: $query');
      }
    },
  );

  await tester.pumpWidget(_activityApp(AppWireDayGateway(transport)));
  await tester.pump();
  expect(find.text(_description(before)), findsOneWidget);

  await tester.tap(find.text('Check status'));
  await tester.pump();
  await inspectStarted.future.timeout(const Duration(seconds: 1));
  await tester.tap(find.byTooltip('Reload Calendar activity'));
  await tester.pump();
  await tester.pump();
  expect(find.text(_description(current)), findsOneWidget);

  delayedInspect.complete(_operation(late));
  await tester.pump();
  await tester.pump();
  expect(find.text(_description(current)), findsOneWidget);
  expect(
    find.textContaining('Calendar operation status regressed.'),
    findsOneWidget,
  );
  await tester.pumpWidget(const SizedBox.shrink());
}

Widget _activityApp(AppWireDayGateway gateway) => MaterialApp(
  home: Scaffold(body: ManualCalendarActivity(gateway: gateway)),
);

String _description(ManualCalendarOperationReceipt receipt) =>
    switch (receipt.status) {
      ManualCalendarOperationStatus.pending => 'Waiting to run',
      ManualCalendarOperationStatus.executing => 'In progress',
      ManualCalendarOperationStatus.blocked =>
        'Blocked; no Calendar change was confirmed',
      ManualCalendarOperationStatus.notApplied => 'Not applied',
      ManualCalendarOperationStatus.unknown => 'Outcome unknown. The Calendar may have changed; check status before trying again.',
      ManualCalendarOperationStatus.succeeded =>
        receipt.collectionPending
            ? 'Calendar change succeeded; Day update is pending.'
            : 'Calendar change succeeded.',
    };

const _operationRef = '00000000-0000-4000-8000-000000000101';
const _secondOperationRef = '00000000-0000-4000-8000-000000000102';

ManualCalendarOperationReceipt _unknown({
  String ref = _operationRef,
  required int revision,
}) => ManualCalendarOperationReceipt(
  operationRef: ref,
  revision: revision,
  status: ManualCalendarOperationStatus.unknown,
  collectionPending: false,
);

ManualCalendarOperationReceipt _blocked({
  String ref = _operationRef,
  required int revision,
}) => ManualCalendarOperationReceipt(
  operationRef: ref,
  revision: revision,
  status: ManualCalendarOperationStatus.blocked,
  collectionPending: false,
);

ManualCalendarOperationReceipt _succeeded({
  String ref = _operationRef,
  required int revision,
  required bool pending,
}) => ManualCalendarOperationReceipt(
  operationRef: ref,
  revision: revision,
  status: ManualCalendarOperationStatus.succeeded,
  collectionPending: pending,
);

Map<String, dynamic> _receipt(ManualCalendarOperationReceipt operation) => {
  'operation_ref': operation.operationRef,
  'revision': operation.revision,
  'status': switch (operation.status) {
    ManualCalendarOperationStatus.pending => 'pending',
    ManualCalendarOperationStatus.executing => 'executing',
    ManualCalendarOperationStatus.blocked => 'blocked',
    ManualCalendarOperationStatus.notApplied => 'not_applied',
    ManualCalendarOperationStatus.unknown => 'unknown',
    ManualCalendarOperationStatus.succeeded => 'succeeded',
  },
  'collection_pending': operation.collectionPending,
};

Map<String, dynamic> _operation(ManualCalendarOperationReceipt operation) => {
  'kind': 'day.external_calendar_operation',
  'operation': _receipt(operation),
};

Map<String, dynamic> _page(
  List<ManualCalendarOperationReceipt> operations, {
  String? nextCursor,
}) => {
  'kind': 'day.external_calendar_operations',
  'operations': {
    'operations': operations.map(_receipt).toList(growable: false),
    'next_cursor': nextCursor,
  },
};

final class _SyntheticAppWireTransport implements AppWireTransport {
  _SyntheticAppWireTransport({required this.onQuery, this.onCommand});

  final Future<Map<String, dynamic>> Function(Map<String, dynamic>) onQuery;
  final Future<Map<String, dynamic>> Function(Map<String, dynamic>)? onCommand;
  final List<Map<String, dynamic>> queries = [];
  final List<Map<String, dynamic>> commands = [];

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final query = Map<String, dynamic>.from(request['query'] as Map);
    queries.add(query);
    return onQuery(query);
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final command = Map<String, dynamic>.from(request['command'] as Map);
    commands.add(command);
    final handler = onCommand;
    if (handler == null) {
      return Future.error(StateError('Unexpected command: $command'));
    }
    return handler(command);
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => Future.error(StateError('Unexpected event request: $request'));

  @override
  Future<void> close() async {}
}
