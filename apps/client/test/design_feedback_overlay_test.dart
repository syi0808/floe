import 'package:floe_client/preview/design_feedback_overlay.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> toggleDesignFeedback(WidgetTester tester) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
  await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
  await tester.pump();
}

void main() {
  testWidgets('opens from the global shortcut while a field owns focus', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: DesignFeedbackOverlay(
          child: const Scaffold(body: TextField(autofocus: true)),
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(TextField), findsOneWidget);
    expect(find.text('Inspect'), findsNothing);
    await toggleDesignFeedback(tester);
    expect(find.text('Inspect'), findsOneWidget);
  });

  testWidgets('selects a rendered target and creates an editable pin', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(900, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        home: DesignFeedbackOverlay(
          child: const Scaffold(body: Center(child: Text('Review target'))),
        ),
      ),
    );

    expect(find.byTooltip('Design feedback (⌘⇧F)'), findsNothing);
    await toggleDesignFeedback(tester);
    await tester.tap(find.text('Inspect'));
    await tester.pump();
    await tester.tapAt(tester.getCenter(find.text('Review target')));
    await tester.pump();

    expect(find.text('Add feedback'), findsOneWidget);
    await tester.enterText(
      find.byKey(const Key('design-feedback-comment')),
      'Increase the contrast.',
    );
    await tester.tap(find.text('Save pin'));
    await tester.pump();

    expect(find.byKey(const ValueKey('design-feedback-pin-1')), findsOneWidget);
    expect(find.text('1 pins'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('design-feedback-pin-1')));
    await tester.pump();
    expect(find.text('Edit feedback'), findsOneWidget);
    expect(find.text('Increase the contrast.'), findsOneWidget);

    await tester.tap(find.text('Delete'));
    await tester.pump();
    expect(find.byKey(const ValueKey('design-feedback-pin-1')), findsNothing);
    expect(find.text('0 pins'), findsOneWidget);
  });

  testWidgets('copies Markdown feedback to the clipboard', (tester) async {
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String?;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: DesignFeedbackOverlay(
          child: const Scaffold(body: Center(child: Text('Export target'))),
        ),
      ),
    );

    await toggleDesignFeedback(tester);
    await tester.tap(find.text('Inspect'));
    await tester.pump();
    await tester.tapAt(tester.getCenter(find.text('Export target')));
    await tester.pump();
    await tester.enterText(
      find.byKey(const Key('design-feedback-comment')),
      'Align this with the rail.',
    );
    await tester.tap(find.text('Save pin'));
    await tester.pump();
    await tester.tap(find.byTooltip('Copy Markdown'));
    await tester.pump();

    expect(copiedText, contains('# Floe design feedback'));
    expect(copiedText, contains('Align this with the rail.'));
    expect(tester.takeException(), isNull);
  });
}
