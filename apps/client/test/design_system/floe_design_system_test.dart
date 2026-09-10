import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_states.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('foundation and interaction tokens match the component contract', () {
    expect(
      [
        FloeRadius.xs,
        FloeRadius.sm,
        FloeRadius.md,
        FloeRadius.lg,
        FloeRadius.xl,
        FloeRadius.frame,
      ],
      [8, 12, 16, 20, 28, 32],
    );
    expect(
      [
        FloeControlSize.compact,
        FloeControlSize.standard,
        FloeControlSize.field,
      ],
      [36, 44, 48],
    );
    expect(
      FloeStates.outlinedBackground({WidgetState.hovered}),
      FloeColor.neutralHover,
    );
    expect(
      FloeStates.quietBackground({WidgetState.hovered}),
      FloeColor.quietHover,
    );
    expect(
      FloeStates.outlinedSide({WidgetState.focused}).color,
      FloeColor.focus,
    );
    expect(FloeType.controlLabel.fontSize, 13);
    expect(FloeType.controlLabel.fontWeight, FontWeight.w500);
    expect(FloeType.button.fontWeight, FontWeight.w600);
  });

  testWidgets('status is rendered by the semantic badge component', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: const Scaffold(
          body: FloeBadge(label: 'Connected', tone: FloeBadgeTone.success),
        ),
      ),
    );

    expect(find.text('Connected'), findsOneWidget);
    expect(tester.getSemantics(find.byType(FloeBadge)).label, 'Connected');
    expect(
      tester.widget<Text>(find.text('Connected')).style?.fontWeight,
      FontWeight.w400,
    );
    expect(
      tester
          .widget<FloeSquircle>(
            find.descendant(
              of: find.byType(FloeBadge),
              matching: find.byType(FloeSquircle),
            ),
          )
          .borderWidth,
      0,
    );
  });

  testWidgets('related controls enforce the shared size contract', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const SizedBox(width: 320, child: FloeInput(label: 'Name')),
              SizedBox(
                width: 320,
                child: FloeSelect<String>(
                  label: 'Calendar',
                  value: null,
                  options: const [
                    FloeSelectOption(value: 'home', label: 'Home'),
                  ],
                  onChanged: (_) {},
                ),
              ),
              FloeButton.filled(onPressed: () {}, child: const Text('Save')),
              FloeButton.icon(
                size: FloeButtonSize.compact,
                tooltip: 'More',
                onPressed: () {},
                icon: const Icon(Icons.more_horiz),
              ),
            ],
          ),
        ),
      ),
    );

    expect(
      tester.getSize(find.byType(TextFormField)).height,
      greaterThanOrEqualTo(FloeControlSize.field),
    );
    expect(
      tester
          .getSize(find.byKey(const ValueKey('floe-selection-input-decorator')))
          .height,
      tester.getSize(find.byType(TextFormField)).height,
    );
    expect(
      tester.getSize(find.byType(FilledButton)).height,
      FloeControlSize.standard,
    );
    expect(
      tester.getSize(find.byType(IconButton)).height,
      FloeControlSize.compact,
    );
  });
}
