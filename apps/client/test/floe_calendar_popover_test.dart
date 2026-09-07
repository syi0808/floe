import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_date_picker.dart';
import 'package:floe_client/app/floe_context_menu.dart';
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
          matching: find.byType(AnimatedScale),
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
