import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_context_rail.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final disabled in [false, true]) {
    testWidgets('task title uses a pointer when disabled is $disabled', (
      tester,
    ) async {
      final date = DateTime.utc(2026, 9, 5);
      final task = TaskItem(
        id: 'task',
        title: 'Open task',
        revision: 1,
        createdAt: date,
      );
      TaskItem? openedTask;
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: CalendarContextRail(
                snapshot: DaySnapshot(
                  personId: 'test',
                  date: date,
                  generatedAt: date,
                  timezoneOffsetSeconds: 0,
                  items: [task],
                ),
                disabled: disabled,
                complete: (_, _) async {},
                onTasks: () {},
                onOpenTask: (task) => openedTask = task,
              ),
            ),
          ),
        ),
      );

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);
      addTearDown(mouse.removePointer);
      await mouse.moveTo(tester.getCenter(find.text(task.title)));
      await tester.pump();
      expect(
        RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1),
        SystemMouseCursors.click,
      );
      await tester.tap(find.text(task.title));
      expect(openedTask, same(task));

      await mouse.moveTo(tester.getCenter(find.byType(Checkbox)));
      await tester.pump();
      expect(
        RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1),
        disabled ? SystemMouseCursors.basic : SystemMouseCursors.click,
      );
    });
  }
}
