import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_agenda.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_date_time_field.dart';
import 'package:floe_client/l10n/app_localizations.dart';

Widget host(Widget child) => MaterialApp(
  theme: FloeTheme.light.copyWith(platform: TargetPlatform.macOS),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: child),
);

void main() {
  testWidgets('time segments support typing, arrow stepping and validation', (
    tester,
  ) async {
    final form = GlobalKey<FormState>();
    var value = DateTime(2026, 9, 8, 9, 30);
    await tester.pumpWidget(
      host(
        Form(
          key: form,
          child: StatefulBuilder(
            builder: (context, setState) => CalendarDateTimeField(
              label: 'Starts',
              value: value,
              onChanged: (next) => setState(() => value = next),
            ),
          ),
        ),
      ),
    );
    await tester.enterText(find.byType(TextFormField).first, '14');
    await tester.pump();
    expect(value.hour, 14);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(value.hour, 15);
    await tester.tap(find.byTooltip('Decrease Starts minute'));
    await tester.pump();
    expect(value.minute, 29);
    await tester.enterText(find.byType(TextFormField).last, '99');
    expect(form.currentState!.validate(), isFalse);
    expect(value.minute, 29);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'drag snaps, cancels with Escape/outside, and menu edits/deletes',
    (tester) async {
      tester.view.physicalSize = const Size(900, 760);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final event = EventItem(
        id: 'event',
        title: 'Focus',
        revision: 1,
        createdAt: DateTime(2026, 9, 8),
        startsAt: DateTime(2026, 9, 8, 9),
        endsAt: DateTime(2026, 9, 8, 10),
        canModify: true,
      );
      final snapshot = DaySnapshot(
        personId: 'person',
        date: DateTime(2026, 9, 8),
        generatedAt: DateTime(2026, 9, 8, 8),
        timezoneOffsetSeconds: event.startsAt.timeZoneOffset.inSeconds,
        items: [event],
      );
      final moved = <DateTime>[];
      var edits = 0;
      var deletes = 0;
      await tester.pumpWidget(
        host(
          CalendarAgenda(
            snapshot: snapshot,
            onConnections: () {},
            canModify: (_) => true,
            onMoveEvent: (_, start) => moved.add(start),
            onEditEvent: (_) => edits++,
            onDeleteEvent: (_) => deletes++,
          ),
        ),
      );
      await tester.pumpAndSettle();
      final origin =
          tester.getCenter(find.byType(CalendarEventCard)) -
          const Offset(50, 0);
      final drag = await tester.startGesture(
        origin,
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(0, 37));
      await tester.pump();
      expect(find.byKey(const Key('calendar-drag-preview')), findsOneWidget);
      await drag.up();
      await tester.pump();
      expect(moved.single.toLocal(), DateTime(2026, 9, 8, 9, 30));
      final cancelled = await tester.startGesture(
        origin,
        kind: PointerDeviceKind.mouse,
      );
      await cancelled.moveBy(const Offset(0, 60));
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await cancelled.up();
      await tester.pump();
      expect(moved, hasLength(1));
      expect(find.byKey(const Key('calendar-drag-preview')), findsNothing);
      final outside = await tester.startGesture(
        origin,
        kind: PointerDeviceKind.mouse,
      );
      await outside.moveTo(const Offset(10, 700));
      await outside.up();
      await tester.pump();
      expect(moved, hasLength(1));
      await tester.tapAt(origin, buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      expect(find.text('Edit event…'), findsOneWidget);
      await tester.tap(find.text('Edit event…'));
      await tester.pumpAndSettle();
      expect(edits, 1);
      await tester.tap(find.byTooltip('Event actions'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Delete event…'));
      await tester.pumpAndSettle();
      expect(deletes, 1);
      final scroll = tester
          .widget<SingleChildScrollView>(
            find.byKey(const Key('calendar-scroll')),
          )
          .controller!;
      final before = scroll.offset;
      final edge = await tester.startGesture(
        origin,
        kind: PointerDeviceKind.mouse,
      );
      await edge.moveTo(const Offset(400, 740));
      await tester.pump(const Duration(milliseconds: 160));
      expect(scroll.offset, greaterThan(before));
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await edge.up();
      await tester.pump();
      expect(moved, hasLength(1));
      scroll.jumpTo(before);
      await tester.pump();
      Focus.of(tester.element(find.byType(CalendarEventCard))).requestFocus();
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.f10);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pumpAndSettle();
      expect(find.text('Edit event…'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      await tester.pumpWidget(
        host(
          CalendarAgenda(
            snapshot: snapshot,
            onConnections: () {},
            canModify: (_) => false,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Event actions'));
      await tester.pumpAndSettle();
      final edit = tester.widget<PopupMenuItem<String>>(
        find.widgetWithText(PopupMenuItem<String>, 'Edit event…'),
      );
      expect(edit.enabled, isFalse);
      expect(find.byType(Draggable<EventItem>), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );
}
