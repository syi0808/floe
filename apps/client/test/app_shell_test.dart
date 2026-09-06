import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';

void main() {
  testWidgets('task toast preserves undo and survives navigation', (
    tester,
  ) async {
    tester.view.physicalSize = Size(1440, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final now = DateTime(2026, 9, 4);
    final gateway = FakeDayGateway(
      initialItems: [
        TaskItem(
          id: 'toast-task',
          title: 'Toast task',
          revision: 0,
          createdAt: now,
        ),
      ],
    );
    final query = DayQuery(
      personId: 'test',
      date: now,
      now: now,
      timezoneOffsetSeconds: 0,
    );
    await tester.pumpWidget(FloeApp(gateway: gateway, query: query));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(FloeCheckbox).first);
    await tester.pumpAndSettle();
    expect(find.text('Undo'), findsOneWidget);
    expect(find.byType(SnackBar), findsNothing);
    expect(
      (await gateway.loadDay(query)).items
          .whereType<TaskItem>()
          .single
          .isCompleted,
      isTrue,
    );
    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    expect(find.text('Undo'), findsOneWidget);
    await tester.tap(find.text('Undo'));
    await tester.pump(FloeLoading.minimumDuration);
    await tester.pumpAndSettle();
    expect(
      (await gateway.loadDay(query)).items
          .whereType<TaskItem>()
          .single
          .isCompleted,
      isFalse,
    );
    expect(find.text('Undo'), findsNothing);
  });

  testWidgets('calendar has no capture input and refresh uses toast', (
    tester,
  ) async {
    tester.view.physicalSize = Size(1440, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(FloeApp(gateway: FakeDayGateway()));
    await tester.pumpAndSettle();
    expect(find.byKey(Key('capture-field')), findsNothing);
    expect(find.byType(TextField), findsNothing);
    await tester.tap(find.byTooltip('Refresh calendar'));
    await tester.pumpAndSettle();
    expect(find.text('Calendars refreshed'), findsOneWidget);
    await tester.pump(Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('calendar navigation distinguishes selected dates from today', (
    tester,
  ) async {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(),
        query: DayQuery(
          personId: 'test',
          date: today,
          now: now,
          timezoneOffsetSeconds: now.timeZoneOffset.inSeconds,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byTooltip('Calendar'), findsOneWidget);
    expect(find.text('Today'), findsOneWidget);
    await tester.tap(find.byTooltip('Next day'));
    await tester.pumpAndSettle();
    expect(
      find.text(
        DateFormat.MMMEd('en').format(today.add(const Duration(days: 1))),
      ),
      findsOneWidget,
    );
    expect(find.text('Today'), findsNothing);
    expect(find.text('Go to today'), findsOneWidget);
    await tester.tap(find.text('Go to today'));
    await tester.pumpAndSettle();
    expect(find.text(DateFormat.MMMEd('en').format(today)), findsOneWidget);
    expect(find.text('Today'), findsOneWidget);
    expect(find.text('Go to today'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  for (final width in [390.0, 1440.0, 1920.0]) {
    testWidgets('workspace fills the window without a frame at $width', (
      tester,
    ) async {
      final height = width == 390 ? 700.0 : 768.0;
      tester.view.physicalSize = Size(width, height);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        FloeApp(
          gateway: FakeDayGateway(
            initialItems: width == 1920
                ? []
                : [
                    for (var index = 0; index < 6; index++)
                      EventItem(
                        id: 'all-day-$index',
                        title: 'All-day event $index',
                        revision: 0,
                        createdAt: date,
                        startsAt: date,
                        endsAt: date.add(const Duration(days: 1)),
                        isAllDay: true,
                      ),
                    EventItem(
                      id: 'timed',
                      title: 'Timed event',
                      revision: 0,
                      createdAt: date,
                      startsAt: date.add(const Duration(hours: 9)),
                      endsAt: date.add(const Duration(hours: 10)),
                    ),
                  ],
          ),
          query: DayQuery(
            personId: 'test',
            date: date,
            now: date,
            timezoneOffsetSeconds: 0,
          ),
        ),
      );
      await tester.pumpAndSettle();
      final scaffold = tester.widget<Scaffold>(find.byType(Scaffold));
      final safeArea = scaffold.body! as SafeArea;
      expect(safeArea.child, isA<Stack>());
      expect(
        tester.getRect(find.byWidget(safeArea.child)),
        Rect.fromLTWH(0, 0, width, height),
      );
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is FloeSquircle && widget.size == FloeSquircleSize.frame,
        ),
        findsNothing,
      );
      expect(find.byTooltip('Settings'), findsOneWidget);
      final timeline = find.byKey(const Key('timeline-card'));
      expect(find.byTooltip('Create event'), findsOneWidget);
      expect(find.text('Plan a Calendar event'), findsNothing);
      expect(find.byType(TextField), findsNothing);
      expect(
        tester.getRect(timeline).bottom,
        lessThanOrEqualTo(height - (width <= 780 ? 96 : 24)),
      );
      expect(tester.getRect(timeline).top, lessThan(140));
      expect(
        find.ancestor(
          of: timeline,
          matching: find.byType(SingleChildScrollView),
        ),
        findsNothing,
      );
      final settingsRect = tester.getRect(find.byTooltip('Settings'));
      if (width != 1920) {
        final scroll = find.byKey(const Key('calendar-scroll'));
        final controller = tester
            .widget<SingleChildScrollView>(scroll)
            .controller!;
        final previousOffset = controller.offset;
        final previousBounds = tester.getRect(timeline);
        await tester.drag(scroll, const Offset(0, -80));
        await tester.pumpAndSettle();
        expect(controller.offset, greaterThan(previousOffset));
        expect(tester.getRect(timeline), previousBounds);
      }
      if (width > 780) {
        expect(settingsRect.right, lessThan(100));
      } else {
        expect(settingsRect.top, greaterThan(height - 100));
      }
      await tester.tap(find.byTooltip('Settings'));
      await tester.pumpAndSettle();
      expect(find.text('Settings'), findsOneWidget);
      expect(find.text('Remote server'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  }
}
