import 'package:flutter/gestures.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_date_picker.dart';
import 'package:floe_client/app/floe_context_menu.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/app/floe_popover.dart';
import 'package:floe_client/app/floe_time_picker.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_date_time_field.dart';
import 'package:floe_client/l10n/app_localizations.dart';

Widget host(Widget child, {bool reducedMotion = false}) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: MediaQuery(
    data: MediaQueryData(
      size: const Size(800, 600),
      disableAnimations: reducedMotion,
    ),
    child: Scaffold(body: child),
  ),
);

void main() {
  testWidgets('menu hover transfers immediately without a trailing highlight', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(
        const Center(
          child: SizedBox(
            width: 248,
            child: FloeContextMenu<String>(
              entries: [
                FloeMenuEntry(
                  value: 'open',
                  label: 'Open',
                  icon: Icons.open_in_new,
                ),
                FloeMenuEntry(value: 'edit', label: 'Edit', icon: Icons.edit),
                FloeMenuEntry(
                  value: 'delete',
                  label: 'Delete',
                  icon: Icons.delete,
                  destructive: true,
                  separator: true,
                ),
                FloeMenuEntry(
                  value: 'disabled',
                  label: 'Disabled',
                  icon: Icons.block,
                  enabled: false,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    void expectHighlight(String? label) {
      for (final title in ['Open', 'Edit', 'Delete', 'Disabled']) {
        final row = find.ancestor(
          of: find.text(title),
          matching: find.byType(PressableScale),
        );
        final surface = tester.widget<DecoratedBox>(
          find.descendant(of: row, matching: find.byType(DecoratedBox)),
        );
        expect(
          (surface.decoration as BoxDecoration).color,
          title == label ? FloePalette.primary50 : Colors.transparent,
          reason: '$title must update its painted hover background immediately',
        );
      }
    }

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('Open')));
    await tester.pumpAndSettle();
    expectHighlight('Open');

    for (final title in ['Edit', 'Delete', 'Open', 'Disabled', 'Edit']) {
      await mouse.moveTo(tester.getCenter(find.text(title)));
      await tester.pump();
      expectHighlight(title == 'Disabled' ? null : title);
      await tester.pump(const Duration(milliseconds: 16));
      expectHighlight(title == 'Disabled' ? null : title);
    }
    await mouse.moveTo(tester.getCenter(find.byType(Divider)));
    await tester.pump();
    expectHighlight(null);
    await mouse.moveTo(tester.getCenter(find.text('Delete')));
    await tester.pump();
    expectHighlight('Delete');
    await mouse.moveTo(Offset.zero);
    await tester.pump();
    expectHighlight(null);
  });

  testWidgets(
    'custom date popover handles leap months, keyboard and dismissal',
    (tester) async {
      var value = DateTime(2028, 1, 31, 14, 30);
      await tester.pumpWidget(
        host(
          StatefulBuilder(
            builder: (context, setState) => Center(
              child: SizedBox(
                width: 320,
                child: CalendarDateTimeField(
                  label: 'Starts',
                  value: value,
                  onChanged: (next) => setState(() => value = next),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Jan 31, 2028'));
      await tester.pumpAndSettle();
      expect(find.byType(DatePickerDialog), findsNothing);
      expect(find.byType(FloeDatePicker), findsOneWidget);
      await tester.tap(find.byTooltip('Next month'));
      await tester.pumpAndSettle();
      expect(find.text('February 2028'), findsOneWidget);
      await tester.tap(find.bySemanticsLabel('Tuesday, February 29, 2028'));
      await tester.pumpAndSettle();
      expect(value, DateTime(2028, 2, 29, 14, 30));
      await tester.tap(find.text('Feb 29, 2028'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(value, DateTime(2028, 3, 1, 14, 30));
      await tester.tap(find.text('Mar 1, 2028'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byType(FloeDatePicker), findsNothing);
      expect(value, DateTime(2028, 3, 1, 14, 30));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('date-time controls keep compact hover targets', (tester) async {
    await tester.pumpWidget(
      host(
        Center(
          child: SizedBox(
            width: 500,
            child: CalendarDateTimeField(
              label: 'Ends',
              value: DateTime(2026, 9, 8, 15, 45),
              onChanged: (_) {},
            ),
          ),
        ),
      ),
    );

    final dateButton = find.ancestor(
      of: find.text('Sep 8, 2026'),
      matching: find.byType(TextButton),
    );
    final field = find.byType(InputDecorator);
    expect(
      tester.getSize(dateButton).width,
      lessThan(tester.getSize(field).width / 2),
    );
  });

  testWidgets('time picker overlay keeps selected values visible', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(
        Center(
          child: FloeTimePicker(
            initialTime: const TimeOfDay(hour: 15, minute: 28),
          ),
        ),
      ),
    );

    final overlays = tester.widgetList<CupertinoPickerDefaultSelectionOverlay>(
      find.byType(CupertinoPickerDefaultSelectionOverlay),
    );
    expect(overlays, isNotEmpty);
    for (final overlay in overlays) {
      expect(overlay.background.a, lessThan(1));
    }
  });

  testWidgets('calendar hover moves without leaving the previous date styled', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(Center(child: FloeDatePicker(initialDate: DateTime(2028, 2, 14)))),
    );
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);

    Color background(String label) {
      final container = tester.widget<Container>(
        find
            .descendant(
              of: find.bySemanticsLabel(label),
              matching: find.byType(Container),
            )
            .first,
      );
      return (container.decoration! as BoxDecoration).color!;
    }

    const first = 'Tuesday, February 15, 2028';
    const second = 'Wednesday, February 16, 2028';
    await mouse.moveTo(tester.getCenter(find.bySemanticsLabel(first)));
    await tester.pump();
    expect(background(first), FloeColor.selectionHover);
    await mouse.moveTo(tester.getCenter(find.bySemanticsLabel(second)));
    await tester.pump();
    expect(background(first), Colors.transparent);
    expect(background(second), FloeColor.selectionHover);
  });

  testWidgets('popover centers on its trigger and uses directional origins', (
    tester,
  ) async {
    Future<void> openAt(Alignment alignment) async {
      await tester.pumpWidget(
        host(
          Align(
            alignment: alignment,
            child: Builder(
              builder: (context) => TextButton(
                onPressed: () => showFloeDatePicker(
                  context: context,
                  anchor: floeAnchorRect(context),
                  initialDate: DateTime(2028, 2, 14),
                ),
                child: const Text('Calendar'),
              ),
            ),
          ),
          reducedMotion: true,
        ),
      );
      await tester.tap(find.text('Calendar'));
      await tester.pumpAndSettle();
    }

    await openAt(Alignment.topCenter);
    var trigger = tester.getRect(find.text('Calendar'));
    var picker = tester.getRect(find.byType(FloeDatePicker));
    var transition = tester.widget<FloeFadeScaleTransition>(
      find.ancestor(
        of: find.byType(FloeDatePicker),
        matching: find.byType(FloeFadeScaleTransition),
      ),
    );
    expect(picker.center.dx, closeTo(trigger.center.dx, 1));
    expect(picker.top, greaterThan(trigger.bottom));
    expect(transition.alignment.y, -1);

    await tester.tapAt(const Offset(4, 300));
    await tester.pumpAndSettle();
    await openAt(Alignment.bottomCenter);
    trigger = tester.getRect(find.text('Calendar'));
    picker = tester.getRect(find.byType(FloeDatePicker));
    transition = tester.widget<FloeFadeScaleTransition>(
      find.ancestor(
        of: find.byType(FloeDatePicker),
        matching: find.byType(FloeFadeScaleTransition),
      ),
    );
    expect(picker.center.dx, closeTo(trigger.center.dx, 1));
    expect(picker.bottom, lessThan(trigger.top));
    expect(transition.alignment.y, 1);
  });

  testWidgets(
    'menu stays in viewport, skips disabled entries and restores focus',
    (tester) async {
      tester.view.physicalSize = const Size(320, 600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final trigger = FocusNode();
      addTearDown(trigger.dispose);
      String? result;
      await tester.pumpWidget(
        host(
          Builder(
            builder: (context) => Align(
              alignment: Alignment.bottomRight,
              child: TextButton(
                focusNode: trigger,
                onPressed: () async {
                  result = await showFloeContextMenu<String>(
                    context: context,
                    anchor: const Rect.fromLTWH(310, 590, 0, 0),
                    entries: const [
                      FloeMenuEntry(
                        value: 'open',
                        label: 'Open',
                        icon: Icons.open_in_new,
                      ),
                      FloeMenuEntry(
                        value: 'edit',
                        label: 'Edit',
                        icon: Icons.edit,
                        enabled: false,
                      ),
                      FloeMenuEntry(
                        value: 'delete',
                        label: 'Delete',
                        icon: Icons.delete,
                        destructive: true,
                        separator: true,
                      ),
                    ],
                  );
                },
                child: const Text('Menu'),
              ),
            ),
          ),
          reducedMotion: true,
        ),
      );
      trigger.requestFocus();
      await tester.tap(find.text('Menu'));
      await tester.pumpAndSettle();
      final bounds = tester.getRect(find.byType(FloeContextMenu<String>));
      expect(bounds.right, lessThanOrEqualTo(308));
      expect(bounds.bottom, lessThanOrEqualTo(588));
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(result, 'delete');
      expect(trigger.hasFocus, isTrue);
      await tester.tap(find.text('Menu'));
      await tester.pumpAndSettle();
      await tester.tapAt(const Offset(10, 10));
      await tester.pumpAndSettle();
      expect(result, isNull);
      expect(tester.takeException(), isNull);
    },
  );
}
