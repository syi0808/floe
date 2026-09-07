import 'package:floe_client/app/floe_switch.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'custom switch supports pointer keyboard semantics and disabled state',
    (tester) async {
      var value = false;
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          home: StatefulBuilder(
            builder: (context, setState) => Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  FloeSwitch(
                    key: const Key('enabled-switch'),
                    value: value,
                    onChanged: (next) => setState(() => value = next),
                    label: const Text('Calendar access'),
                  ),
                  const FloeSwitch(
                    key: Key('disabled-switch'),
                    value: false,
                    onChanged: null,
                    label: Text('Unavailable integration'),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      final enabled = find.byKey(const Key('enabled-switch'));
      expect(tester.getSemantics(enabled), isNotNull);
      await tester.tap(enabled);
      await tester.pumpAndSettle();
      expect(value, true);
      await tester.sendKeyEvent(LogicalKeyboardKey.space);
      await tester.pumpAndSettle();
      expect(value, false);

      await tester.tap(find.byKey(const Key('disabled-switch')));
      await tester.pumpAndSettle();
      expect(value, false);
    },
  );
}
