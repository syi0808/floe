import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/features/day/presentation/calendar_event_actions.dart';
import 'package:floe_client/features/day/presentation/calendar_event_details.dart';
import 'package:floe_client/l10n/app_localizations.dart';

final _start = DateTime.utc(2026, 10, 22, 10);
EventItem item({bool writable = false}) => EventItem(
  id: 'event',
  title: 'Review',
  revision: 1,
  createdAt: _start,
  startsAt: _start,
  endsAt: _start.add(const Duration(hours: 1)),
  actionTarget: writable
      ? const DayEventTarget(eventId: 'target', expectedRevision: 1)
      : null,
);
DaySnapshot snapshot(EventItem event) => DaySnapshot(
  personId: 'fixture',
  date: _start,
  generatedAt: _start,
  timezoneOffsetSeconds: 0,
  items: [event],
);
Widget app(Widget child) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: Center(child: SizedBox(width: 300, height: 80, child: child)),
  ),
);
Widget actions(
  EventItem event, {
  VoidCallback? onEdit,
  VoidCallback? onDelete,
  FocusNode? focus,
}) => Builder(
  builder: (context) => CalendarEventActions(
    event: event,
    snapshot: snapshot(event),
    onEdit: onEdit,
    onDelete: onDelete,
    child: TextButton(
      focusNode: focus,
      key: const Key('primary-event'),
      onPressed: () => openCalendarEvent(
        context,
        event,
        snapshot(event),
        onEdit: onEdit,
        onDelete: onDelete,
      ),
      child: Text(event.title),
    ),
  ),
);

void main() {
  testWidgets('a timeline event has one keyboard stop before opening details', (
    tester,
  ) async {
    final event = item();
    await tester.pumpWidget(
      app(
        CalendarEventActions(
          event: event,
          snapshot: snapshot(event),
          child: CalendarEventCard(
            event: event,
            snapshot: snapshot(event),
            height: 80,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pumpAndSettle();
    expect(find.byType(CalendarEventDetails), findsOneWidget);
  });

  testWidgets(
    'read-only long press retains detail access without unavailable actions',
    (tester) async {
      await tester.pumpWidget(app(actions(item())));
      await tester.pumpAndSettle();
      await tester.longPress(find.byKey(const Key('primary-event')));
      await tester.pumpAndSettle();
      expect(find.text('Details'), findsOneWidget);
      expect(find.text('Edit…'), findsNothing);
      expect(find.text('Delete…'), findsNothing);
      await tester.tap(find.text('Details'));
      await tester.pumpAndSettle();
      expect(find.byType(CalendarEventDetails), findsOneWidget);
    },
  );

  testWidgets(
    'read-only events have no visible overflow but retain keyboard details',
    (tester) async {
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final event = item();
      await tester.pumpWidget(app(actions(event, focus: focus)));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Event actions'), findsNothing);
      focus.requestFocus();
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.f10);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pumpAndSettle();
      expect(find.text('Details'), findsOneWidget);
      expect(find.text('Edit…'), findsNothing);
      expect(find.text('Delete…'), findsNothing);
      await tester.tap(find.text('Details'));
      await tester.pumpAndSettle();
      expect(find.byType(CalendarEventDetails), findsOneWidget);
    },
  );
  testWidgets(
    'missing target suppresses mutation affordances even if callbacks exist',
    (tester) async {
      var edits = 0;
      var deletes = 0;
      await tester.pumpWidget(
        app(actions(item(), onEdit: () => edits++, onDelete: () => deletes++)),
      );
      await tester.pumpAndSettle();
      expect(find.byTooltip('Event actions'), findsNothing);
      await tester.tap(find.byKey(const Key('primary-event')));
      await tester.pumpAndSettle();
      expect(find.byType(CalendarEventDetails), findsOneWidget);
      expect(find.text('Edit event'), findsNothing);
      expect(find.text('Delete'), findsNothing);
      expect(edits, 0);
      expect(deletes, 0);
    },
  );
  testWidgets(
    'one available action renders only that action and dispatches it once',
    (tester) async {
      var deletes = 0;
      await tester.pumpWidget(
        app(actions(item(writable: true), onDelete: () => deletes++)),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Event actions'));
      await tester.pumpAndSettle();
      expect(find.text('Details'), findsOneWidget);
      expect(find.text('Edit…'), findsNothing);
      expect(find.text('Delete…'), findsOneWidget);
      expect(deletes, 0);
      await tester.tap(find.text('Delete…'));
      await tester.pumpAndSettle();
      expect(deletes, 1);
    },
  );
  testWidgets('available edit and delete remain separate deliberate choices', (
    tester,
  ) async {
    var edits = 0;
    var deletes = 0;
    await tester.pumpWidget(
      app(
        actions(
          item(writable: true),
          onEdit: () => edits++,
          onDelete: () => deletes++,
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Event actions'));
    await tester.pumpAndSettle();
    expect(find.text('Edit…'), findsOneWidget);
    expect(find.text('Delete…'), findsOneWidget);
    await tester.tap(find.text('Edit…'));
    await tester.pumpAndSettle();
    expect(edits, 1);
    expect(deletes, 0);
  });
  testWidgets('timeline card exposes one enabled labeled semantic action', (
    tester,
  ) async {
    final handle = tester.ensureSemantics();
    try {
      final event = item();
      await tester.pumpWidget(
        app(
          CalendarEventCard(
            event: event,
            snapshot: snapshot(event),
            height: 80,
          ),
        ),
      );
      await tester.pumpAndSettle();
      final labeledCard = find.bySemanticsLabel('Review · 10:00 – 11:00');
      expect(labeledCard, findsOneWidget);
      final node = tester.getSemantics(labeledCard);
      final data = node.getSemanticsData();
      expect(data.label, 'Review · 10:00 – 11:00');
      expect(data.flagsCollection.isButton, isTrue);
      expect(data.flagsCollection.isEnabled, ui.Tristate.isTrue);
      expect(data.hasAction(ui.SemanticsAction.tap), isTrue);
      tester
          .renderObject(find.byType(CalendarEventCard))
          .owner!
          .semanticsOwner!
          .performAction(node.id, ui.SemanticsAction.tap);
      await tester.pumpAndSettle();
      expect(find.byType(CalendarEventDetails), findsOneWidget);
      expect(tester.takeException(), isNull);
    } finally {
      handle.dispose();
    }
  });
}
