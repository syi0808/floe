import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_motion.dart';

void main() {
  for (final reduced in [false, true]) {
    testWidgets(
      'rapid swaps retire outgoing focus and semantics, reduced $reduced',
      (tester) async {
        final handle = tester.ensureSemantics();
        try {
          var page = 'A';
          var activations = 0;
          late StateSetter update;
          await tester.pumpWidget(
            MaterialApp(
              home: MediaQuery(
                data: MediaQueryData(disableAnimations: reduced),
                child: Scaffold(
                  body: StatefulBuilder(
                    builder: (context, setState) {
                      update = setState;
                      return FloeAnimatedSwap(
                        beginScale: 1,
                        child: TextButton(
                          key: ValueKey(page),
                          onPressed: () => activations++,
                          child: Text('$page action'),
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
          );
          await tester.pumpAndSettle();
          final oldFocus = Focus.of(tester.element(find.text('A action')));
          oldFocus.requestFocus();
          await tester.pump();
          expect(oldFocus.hasFocus, isTrue);
          update(() => page = 'B');
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 40));
          expect(
            Focus.of(tester.element(find.text('A action'))),
            same(oldFocus),
          );
          expect(oldFocus.hasFocus, isFalse);
          expect(oldFocus.canRequestFocus, isFalse);
          expect(_liveLabels(tester), isNot(contains('A action')));
          expect(
            _liveLabels(tester).where((label) => label == 'B action'),
            hasLength(1),
          );
          await tester.sendKeyEvent(LogicalKeyboardKey.tab);
          await tester.pump();
          expect(oldFocus.hasFocus, isFalse);
          update(() => page = 'A');
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 20));
          expect(find.text('A action'), findsNWidgets(2));
          expect(
            _liveLabels(tester).where((label) => label == 'A action'),
            hasLength(1),
          );
          expect(_liveLabels(tester), isNot(contains('B action')));
          expect(oldFocus.canRequestFocus, isFalse);
          await tester.pumpAndSettle();
          await tester.tap(find.text('A action'));
          expect(activations, 1);
          expect(tester.takeException(), isNull);
        } finally {
          handle.dispose();
        }
      },
    );
  }
}

List<String> _liveLabels(WidgetTester tester) {
  final labels = <String>[];
  void visit(SemanticsNode node) {
    labels.add(node.label);
    node.visitChildren((child) {
      visit(child);
      return true;
    });
  }

  visit(
    tester
        .renderObject(find.byType(Scaffold))
        .owner!
        .semanticsOwner!
        .rootSemanticsNode!,
  );
  return labels;
}
