import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/features/actions/presentation/calendar_action_proposal.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/infrastructure/app_wire_day_gateway.dart';
import 'package:floe_client/features/day/presentation/calendar_date_time_field.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  group('AppWireDayGateway Calendar command retention', () {
    for (final operation in _operationCases) {
      test(
        '${operation.name} may use a new ID after first-attempt NotAdmitted',
        () async {
          var attempts = 0;
          final transport = _RecordingTransport(
            onCommand: (_) async {
              attempts++;
              if (attempts == 1) {
                throw const AppWireTransportException(
                  'not_admitted',
                  'The first delivery was not admitted.',
                  commandOutcome: CommandOutcome.notAdmitted,
                );
              }
              return _operationResponse();
            },
          );
          final gateway = AppWireDayGateway(transport);

          await expectLater(
            gateway.executeExternalCalendarOperation(operation.intent),
            throwsA(isA<AppRuntimeException>()),
          );
          await gateway.executeExternalCalendarOperation(operation.changed);

          expect(transport.commands, hasLength(2));
          expect(
            transport.commands[0]['command_id'],
            isNot(transport.commands[1]['command_id']),
          );
          expect(
            _operationBody(transport.commands[0]),
            isNot(equals(_operationBody(transport.commands[1]))),
          );
        },
      );

      for (final outcome in [
        CommandOutcome.admitted,
        CommandOutcome.indeterminate,
      ]) {
        test(
          '${operation.name} replays the same ID and body after ${outcome.name}',
          () async {
            var attempts = 0;
            final transport = _RecordingTransport(
              onCommand: (_) async {
                attempts++;
                if (attempts == 1) {
                  throw AppWireTransportException(
                    outcome.name,
                    'The Calendar command acknowledgement is uncertain.',
                    commandOutcome: outcome,
                  );
                }
                return _operationResponse();
              },
            );
            final gateway = AppWireDayGateway(transport);

            await expectLater(
              gateway.executeExternalCalendarOperation(operation.intent),
              throwsA(isA<AppRuntimeException>()),
            );
            await expectLater(
              gateway.executeExternalCalendarOperation(operation.changed),
              throwsA(isA<StateError>()),
            );
            expect(transport.commands, hasLength(1));

            final receipt = await gateway.executeExternalCalendarOperation(
              operation.intent,
            );

            expect(receipt.status, ManualCalendarOperationStatus.succeeded);
            expect(transport.commands, hasLength(2));
            expect(
              transport.commands[1]['command_id'],
              transport.commands[0]['command_id'],
            );
            expect(
              _operationBody(transport.commands[1]),
              equals(_operationBody(transport.commands[0])),
            );
          },
        );
      }
    }

    test(
      'a late first NotAdmitted cannot release an uncertain exact retry',
      () async {
        final lateFirstAttempt = Completer<Map<String, dynamic>>();
        final firstStarted = Completer<void>();
        var attempts = 0;
        final transport = _RecordingTransport(
          onCommand: (_) {
            attempts++;
            if (attempts == 1) {
              firstStarted.complete();
              return lateFirstAttempt.future;
            }
            if (attempts == 2) {
              return Future.error(
                const AppWireTransportException(
                  'indeterminate',
                  'The replay may have reached the owner.',
                  commandOutcome: CommandOutcome.indeterminate,
                ),
              );
            }
            return Future.value(_operationResponse());
          },
        );
        final gateway = AppWireDayGateway(transport);
        final original = _createIntent();
        final first = gateway.executeExternalCalendarOperation(original);
        await firstStarted.future;

        await expectLater(
          gateway.executeExternalCalendarOperation(original),
          throwsA(isA<AppRuntimeException>()),
        );
        lateFirstAttempt.completeError(
          const AppWireTransportException(
            'not_admitted',
            'The first delivery was not admitted.',
            commandOutcome: CommandOutcome.notAdmitted,
          ),
        );
        await expectLater(first, throwsA(isA<AppRuntimeException>()));

        await expectLater(
          gateway.executeExternalCalendarOperation(
            _createIntent(title: 'Changed'),
          ),
          throwsA(isA<StateError>()),
        );
        expect(transport.commands, hasLength(2));
        expect(
          transport.commands[1]['command_id'],
          transport.commands[0]['command_id'],
        );
      },
    );

    for (final lateResult in [true, false]) {
      test(
        'a late ${lateResult ? 'receipt' : 'NotApplied'} cannot clear a newer retained command',
        () async {
          final lateFirstAttempt = Completer<Map<String, dynamic>>();
          final firstStarted = Completer<void>();
          var attempts = 0;
          final transport = _RecordingTransport(
            onCommand: (_) {
              attempts++;
              if (attempts == 1) {
                firstStarted.complete();
                return lateFirstAttempt.future;
              }
              if (attempts == 2) {
                return Future.value(_operationResponse());
              }
              if (attempts == 3) {
                return Future.error(
                  const AppWireTransportException(
                    'indeterminate',
                    'The newer command may have reached the owner.',
                    commandOutcome: CommandOutcome.indeterminate,
                  ),
                );
              }
              return Future.value(_operationResponse());
            },
          );
          final gateway = AppWireDayGateway(transport);
          final create = _createIntent();
          final first = gateway.executeExternalCalendarOperation(create);
          await firstStarted.future;
          await gateway.executeExternalCalendarOperation(create);

          await expectLater(
            gateway.executeExternalCalendarOperation(_updateIntent()),
            throwsA(isA<AppRuntimeException>()),
          );

          if (lateResult) {
            lateFirstAttempt.complete(_operationResponse());
            await first;
          } else {
            lateFirstAttempt.completeError(
              const AppWireTransportException(
                'not_applied',
                'The old delivery had no effect.',
                commandOutcome: CommandOutcome.notApplied,
              ),
            );
            await expectLater(first, throwsA(isA<AppRuntimeException>()));
          }

          await expectLater(
            gateway.executeExternalCalendarOperation(_deleteIntent()),
            throwsA(isA<StateError>()),
          );
          expect(transport.commands, hasLength(3));
          expect(
            transport.commands[0]['command_id'],
            transport.commands[1]['command_id'],
          );
          expect(
            transport.commands[0]['command_id'],
            isNot(transport.commands[2]['command_id']),
          );
        },
      );
    }
  });

  group('CalendarEventComposer lifecycle', () {
    testWidgets('a retry shows loading and prevents an overlapping reload', (
      tester,
    ) async {
      final retry = Completer<Map<String, dynamic>>();
      var reads = 0;
      final transport = _RecordingTransport(
        onQuery: (_) {
          reads++;
          if (reads == 1) {
            return Future.error(StateError('Initial destination read failed.'));
          }
          return retry.future;
        },
      );
      final gateway = AppWireDayGateway(transport);
      final config = ValueNotifier(
        _ComposerConfig(gateway: gateway, initialStart: _fixedStart),
      );

      await tester.pumpWidget(_composerApp(config));
      await _openComposer(tester);
      expect(
        find.textContaining('Initial destination read failed.'),
        findsOneWidget,
      );
      expect(find.text('Reload reviews'), findsOneWidget);

      await tester.tap(find.text('Reload reviews'));
      await tester.pump();
      final showsLoading = find
          .text('Checking writable Calendar destinations…')
          .evaluate()
          .isNotEmpty;
      final reloadIsHidden = find.text('Reload reviews').evaluate().isEmpty;
      expect(find.text('This event can’t be edited here.'), findsNothing);
      retry.complete(_destinationsResponse('calendar-a', 'Calendar A'));
      await tester.pumpAndSettle();

      expect(showsLoading, isTrue);
      expect(reloadIsHidden, isTrue);
      expect(reads, 2);
      expect(find.text('Calendar A'), findsOneWidget);
    });

    testWidgets(
      'a replaced gateway reloads destinations and fences the old response',
      (tester) async {
        final oldResult = Completer<Map<String, dynamic>>();
        final oldStarted = Completer<void>();
        final oldTransport = _RecordingTransport(
          onQuery: (_) {
            oldStarted.complete();
            return oldResult.future;
          },
        );
        final newTransport = _RecordingTransport(
          onQuery: (_) async =>
              _destinationsResponse('calendar-new', 'New calendar'),
        );
        final oldGateway = AppWireDayGateway(oldTransport);
        final newGateway = AppWireDayGateway(newTransport);
        final config = ValueNotifier(
          _ComposerConfig(gateway: oldGateway, initialStart: _fixedStart),
        );

        await tester.pumpWidget(_composerApp(config));
        await tester.tap(find.text('Open composer'));
        await tester.pump();
        await oldStarted.future;

        config.value = _ComposerConfig(
          gateway: newGateway,
          initialStart: _fixedStart,
        );
        await tester.pump();
        await tester.pump();
        final newReadCount = newTransport.queries.length;
        oldResult.complete(
          _destinationsResponse('calendar-old', 'Old calendar'),
        );
        await tester.pumpAndSettle();

        expect(newReadCount, 1);
        expect(find.text('New calendar'), findsOneWidget);
        expect(find.text('Old calendar'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      'a fresh equal event instance preserves edits and a pending save',
      (tester) async {
        final commandResult = Completer<Map<String, dynamic>>();
        final commandStarted = Completer<void>();
        final transport = _RecordingTransport(
          onCommand: (_) {
            commandStarted.complete();
            return commandResult.future;
          },
        );
        final gateway = AppWireDayGateway(transport);
        final firstEvent = _event('event-same', 'Original event', 3);
        final refreshedEvent = _event('event-same', 'Original event', 3);
        expect(identical(firstEvent, refreshedEvent), isFalse);
        final config = ValueNotifier(
          _ComposerConfig(
            gateway: gateway,
            initialStart: _fixedStart,
            event: firstEvent,
          ),
        );

        await tester.pumpWidget(_composerApp(config));
        await _openComposer(tester);
        await tester.enterText(
          find.byType(TextFormField).first,
          'Edited event',
        );
        await tester.tap(find.text('Save changes'));
        await tester.pump();
        await commandStarted.future;

        config.value = _ComposerConfig(
          gateway: gateway,
          initialStart: _fixedStart,
          event: refreshedEvent,
        );
        await tester.pump();
        final titleField = tester.widget<TextFormField>(
          find.byType(TextFormField).first,
        );
        expect(titleField.controller!.text, 'Edited event');
        expect(transport.commands, hasLength(1));

        commandResult.complete(_operationResponse());
        await tester.pumpAndSettle();

        expect(find.byTooltip('Close'), findsNothing);
        expect(
          _operationBody(transport.commands.single)['event_ref'],
          'event-same',
        );
        expect(
          _operationBody(transport.commands.single)['title'],
          'Edited event',
        );
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('a different target on the same gateway retires an old save', (
      tester,
    ) async {
      final commandResult = Completer<Map<String, dynamic>>();
      final commandStarted = Completer<void>();
      final transport = _RecordingTransport(
        onCommand: (_) {
          commandStarted.complete();
          return commandResult.future;
        },
      );
      final gateway = AppWireDayGateway(transport);
      final firstEvent = _event('event-first', 'First event', 3);
      final secondEvent = _event('event-second', 'Second event', 4);
      final config = ValueNotifier(
        _ComposerConfig(gateway: gateway, event: firstEvent),
      );

      await tester.pumpWidget(_composerApp(config));
      await _openComposer(tester);
      await tester.enterText(find.byType(TextFormField).first, 'Edited first');
      await tester.tap(find.text('Save changes'));
      await tester.pump();
      await commandStarted.future;

      config.value = _ComposerConfig(gateway: gateway, event: secondEvent);
      await tester.pump();
      commandResult.complete(_operationResponse());
      await tester.pumpAndSettle();

      expect(find.text('Edit event'), findsOneWidget);
      final titleField = tester.widget<TextFormField>(
        find.byType(TextFormField).first,
      );
      expect(titleField.controller!.text, 'Second event');
      expect(
        _operationBody(transport.commands.single)['event_ref'],
        'event-first',
      );
      expect(tester.takeException(), isNull);
    });

    testWidgets('a newer event revision retires an old save', (tester) async {
      final commandResult = Completer<Map<String, dynamic>>();
      final commandStarted = Completer<void>();
      final transport = _RecordingTransport(
        onCommand: (_) {
          commandStarted.complete();
          return commandResult.future;
        },
      );
      final gateway = AppWireDayGateway(transport);
      final firstEvent = _event('event-same', 'First revision', 3);
      final refreshedEvent = _event('event-same', 'Newer revision', 4);
      final config = ValueNotifier(
        _ComposerConfig(gateway: gateway, event: firstEvent),
      );

      await tester.pumpWidget(_composerApp(config));
      await _openComposer(tester);
      await tester.enterText(find.byType(TextFormField).first, 'Edited first');
      await tester.tap(find.text('Save changes'));
      await tester.pump();
      await commandStarted.future;

      config.value = _ComposerConfig(gateway: gateway, event: refreshedEvent);
      await tester.pump();
      commandResult.complete(_operationResponse());
      await tester.pumpAndSettle();

      expect(find.text('Edit event'), findsOneWidget);
      final titleField = tester.widget<TextFormField>(
        find.byType(TextFormField).first,
      );
      expect(titleField.controller!.text, 'Newer revision');
      final operation = _operationBody(transport.commands.single);
      expect(operation['event_ref'], 'event-same');
      expect(operation['expected_revision'], 3);
      expect(tester.takeException(), isNull);
    });

    testWidgets(
      'a gateway replacement retires a pending save for the same event',
      (tester) async {
        final commandResult = Completer<Map<String, dynamic>>();
        final commandStarted = Completer<void>();
        final oldTransport = _RecordingTransport(
          onCommand: (_) {
            commandStarted.complete();
            return commandResult.future;
          },
        );
        final newTransport = _RecordingTransport(
          onCommand: (_) async => _operationResponse(),
        );
        final oldGateway = AppWireDayGateway(oldTransport);
        final newGateway = AppWireDayGateway(newTransport);
        final event = _event('event-same', 'Same event', 3);
        final config = ValueNotifier(
          _ComposerConfig(gateway: oldGateway, event: event),
        );

        await tester.pumpWidget(_composerApp(config));
        await _openComposer(tester);
        await tester.enterText(
          find.byType(TextFormField).first,
          'Edited event',
        );
        await tester.tap(find.text('Save changes'));
        await tester.pump();
        await commandStarted.future;

        config.value = _ComposerConfig(gateway: newGateway, event: event);
        await tester.pump();
        commandResult.complete(_operationResponse());
        await tester.pumpAndSettle();

        expect(find.text('Edit event'), findsOneWidget);
        final titleField = tester.widget<TextFormField>(
          find.byType(TextFormField).first,
        );
        expect(titleField.controller!.text, 'Same event');
        expect(oldTransport.commands, hasLength(1));
        expect(newTransport.commands, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('a changed initialStart retires the old create save', (
      tester,
    ) async {
      final commandResult = Completer<Map<String, dynamic>>();
      final commandStarted = Completer<void>();
      final transport = _RecordingTransport(
        onQuery: (_) async => _destinationsResponse('calendar-a', 'Calendar A'),
        onCommand: (_) {
          commandStarted.complete();
          return commandResult.future;
        },
      );
      final gateway = AppWireDayGateway(transport);
      final laterStart = DateTime(2026, 10, 11, 11);
      final config = ValueNotifier(
        _ComposerConfig(gateway: gateway, initialStart: _fixedStart),
      );

      await tester.pumpWidget(_composerApp(config));
      await _openComposer(tester);
      await tester.enterText(find.byType(TextFormField).first, 'Planning');
      await tester.tap(find.text('Create event'));
      await tester.pump();
      await commandStarted.future;
      final submittedStart = DateTime.parse(
        ((_operationBody(transport.commands.single)['schedule']
                as Map)['starts_at'])
            as String,
      );

      config.value = _ComposerConfig(
        gateway: gateway,
        initialStart: laterStart,
      );
      await tester.pumpAndSettle();

      final titleField = tester.widget<TextFormField>(
        find.byType(TextFormField).first,
      );
      expect(titleField.controller!.text, isEmpty);
      expect(
        tester
            .widgetList<CalendarDateTimeField>(
              find.byType(CalendarDateTimeField),
            )
            .first
            .value,
        laterStart,
      );
      expect(transport.queries, hasLength(2));

      commandResult.complete(_operationResponse());
      await tester.pumpAndSettle();
      expect(find.byTooltip('Close'), findsOneWidget);
      expect(submittedStart, _fixedStart.toUtc());
      expect(submittedStart, isNot(laterStart.toUtc()));
      expect(transport.commands, hasLength(1));
      expect(tester.takeException(), isNull);
    });

    testWidgets(
      'closing and reopening retries the retained command exactly once',
      (tester) async {
        final firstResult = Completer<Map<String, dynamic>>();
        final firstStarted = Completer<void>();
        var commandAttempts = 0;
        final transport = _RecordingTransport(
          onQuery: (_) async =>
              _destinationsResponse('calendar-a', 'Calendar A'),
          onCommand: (_) {
            commandAttempts++;
            if (commandAttempts == 1) {
              firstStarted.complete();
              return firstResult.future;
            }
            return Future.value(_operationResponse());
          },
        );
        final gateway = AppWireDayGateway(transport);
        final config = ValueNotifier(
          _ComposerConfig(gateway: gateway, initialStart: _fixedStart),
        );

        await tester.pumpWidget(_composerApp(config));
        await _openComposer(tester);
        await tester.enterText(find.byType(TextFormField).first, 'Planning');
        await tester.tap(find.text('Create event'), warnIfMissed: false);
        await tester.pump();
        await firstStarted.future;
        final firstRequest = transport.commands.single;

        await tester.tap(find.byTooltip('Close'));
        await tester.pumpAndSettle();
        await _openComposer(tester);
        await tester.enterText(find.byType(TextFormField).first, 'Planning');
        await tester.tap(find.text('Create event'));
        await tester.pumpAndSettle();

        expect(transport.commands, hasLength(2));
        expect(transport.commands[1]['command_id'], firstRequest['command_id']);
        expect(
          _operationBody(transport.commands[1]),
          equals(_operationBody(firstRequest)),
        );

        firstResult.complete(_operationResponse());
        await tester.pumpAndSettle();
        expect(transport.commands, hasLength(2));
        expect(find.byTooltip('Close'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      'repeated save taps send one command and a receipt ends the flow',
      (tester) async {
        final commandResult = Completer<Map<String, dynamic>>();
        final commandStarted = Completer<void>();
        final transport = _RecordingTransport(
          onQuery: (_) async =>
              _destinationsResponse('calendar-a', 'Calendar A'),
          onCommand: (_) {
            commandStarted.complete();
            return commandResult.future;
          },
        );
        final gateway = AppWireDayGateway(transport);
        final config = ValueNotifier(
          _ComposerConfig(gateway: gateway, initialStart: _fixedStart),
        );

        await tester.pumpWidget(_composerApp(config));
        await _openComposer(tester);
        await tester.enterText(find.byType(TextFormField).first, 'Planning');
        await tester.tap(find.text('Create event'), warnIfMissed: false);
        await tester.pump();
        await commandStarted.future;
        await tester.tap(find.text('Create event'), warnIfMissed: false);
        await tester.pump();

        expect(transport.commands, hasLength(1));
        commandResult.complete(_operationResponse());
        await tester.pumpAndSettle();

        expect(transport.commands, hasLength(1));
        expect(find.byTooltip('Close'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );
  });
}

final class _OperationCase {
  const _OperationCase(this.name, this.intent, this.changed);

  final String name;
  final ManualCalendarOperationIntent intent;
  final ManualCalendarOperationIntent changed;
}

final _operationCases = <_OperationCase>[
  _OperationCase('create', _createIntent(), _createIntent(title: 'Changed')),
  _OperationCase('update', _updateIntent(), _updateIntent(expectedRevision: 5)),
  _OperationCase('delete', _deleteIntent(), _deleteIntent(expectedRevision: 5)),
];

final _fixedStart = DateTime(2026, 10, 11, 10);

CreateManualCalendarEvent _createIntent({String title = 'Planning'}) =>
    CreateManualCalendarEvent(
      destinationRef: 'calendar-a',
      title: title,
      startsAt: DateTime.utc(2026, 10, 11, 10),
      endsAt: DateTime.utc(2026, 10, 11, 10, 45),
      timezone: 'UTC+00:00',
    );

UpdateManualCalendarEvent _updateIntent({int expectedRevision = 4}) =>
    UpdateManualCalendarEvent(
      eventRef: 'event-update',
      expectedRevision: expectedRevision,
      title: 'Planning',
      startsAt: DateTime.utc(2026, 10, 11, 10),
      endsAt: DateTime.utc(2026, 10, 11, 10, 45),
      timezone: 'UTC+00:00',
    );

DeleteManualCalendarEvent _deleteIntent({int expectedRevision = 7}) =>
    DeleteManualCalendarEvent(
      eventRef: 'event-delete',
      expectedRevision: expectedRevision,
    );

EventItem _event(String eventId, String title, int revision) => EventItem(
  id: eventId,
  title: title,
  revision: revision,
  createdAt: DateTime.utc(2026, 10, 1),
  source: const CalendarDayItemSource(
    sourceRef: 'source-a',
    calendarRef: 'calendar-a',
    calendarLabel: 'Calendar A',
  ),
  startsAt: DateTime(2026, 10, 11, 10),
  endsAt: DateTime(2026, 10, 11, 10, 45),
  timezone: 'UTC+00:00',
  actionTarget: DayEventTarget(eventId: eventId, expectedRevision: revision),
);

Map<String, dynamic> _operationBody(Map<String, dynamic> request) {
  final command = request['command'] as Map<String, dynamic>;
  return Map<String, dynamic>.from(command['operation'] as Map);
}

Map<String, dynamic> _destinationsResponse(String ref, String label) => {
  'kind': 'day.calendar_destinations',
  'destinations': [
    {'destination_ref': ref, 'label': label},
  ],
};

Map<String, dynamic> _operationResponse() => {
  'kind': 'day.external_calendar_operation',
  'operation': {
    'operation_ref': '00000000-0000-4000-8000-000000000101',
    'revision': 1,
    'status': 'succeeded',
    'collection_pending': false,
  },
};

final class _ComposerConfig {
  const _ComposerConfig({required this.gateway, this.initialStart, this.event});

  final DayGateway gateway;
  final DateTime? initialStart;
  final EventItem? event;
}

Widget _composerApp(ValueListenable<_ComposerConfig> config) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: Builder(
      builder: (context) => Center(
        child: TextButton(
          onPressed: () {
            unawaited(
              showFloeDialog<ManualCalendarOperationReceipt>(
                context,
                (_) => ValueListenableBuilder<_ComposerConfig>(
                  valueListenable: config,
                  builder: (context, value, _) => CalendarEventComposer(
                    gateway: value.gateway,
                    initialStart: value.initialStart,
                    event: value.event,
                  ),
                ),
              ),
            );
          },
          child: const Text('Open composer'),
        ),
      ),
    ),
  ),
);

Future<void> _openComposer(WidgetTester tester) async {
  await tester.tap(find.text('Open composer'));
  await tester.pumpAndSettle();
}

final class _RecordingTransport implements AppWireTransport {
  _RecordingTransport({this.onQuery, this.onCommand});

  final Future<Map<String, dynamic>> Function(Map<String, dynamic>)? onQuery;
  final Future<Map<String, dynamic>> Function(Map<String, dynamic>)? onCommand;
  final List<Map<String, dynamic>> queries = [];
  final List<Map<String, dynamic>> commands = [];

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final copy = _copy(request);
    queries.add(copy);
    final handler = onQuery;
    if (handler == null) {
      return Future.error(StateError('Unexpected query: $copy'));
    }
    return handler(copy);
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final copy = _copy(request);
    commands.add(copy);
    final handler = onCommand;
    if (handler == null) {
      return Future.error(StateError('Unexpected command: $copy'));
    }
    return handler(copy);
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => Future.error(StateError('Unexpected event request: $request'));

  @override
  Future<void> close() async {}
}

Map<String, dynamic> _copy(Map<String, dynamic> value) =>
    jsonDecode(jsonEncode(value)) as Map<String, dynamic>;
