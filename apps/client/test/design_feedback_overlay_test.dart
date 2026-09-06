import 'dart:convert';
import 'dart:io';

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

Future<String> captureTestScreenshot(Rect bounds) async {
  final file = File(
    '${Directory.systemTemp.path}/floe-feedback-test-${DateTime.now().microsecondsSinceEpoch}.png',
  );
  file.writeAsBytesSync(const [137, 80, 78, 71]);
  return file.path;
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
    await tester.pumpAndSettle();

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
          captureScreenshot: captureTestScreenshot,
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
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Copy Markdown'));
    await tester.pumpAndSettle();

    expect(copiedText, contains('# Floe design feedback'));
    expect(copiedText, contains('Align this with the rail.'));
    expect(copiedText, contains('- Selector: `Text[text="Export target"]`'));
    expect(
      copiedText,
      contains('- Identifiers: type `Text`; text `Export target`'),
    );
    expect(copiedText, contains('- Screenshot: `'));
    final screenshotPath = RegExp(r'- Screenshot: `([^`]+)`')
        .firstMatch(copiedText!)!
        .group(1)!;
    expect(File(screenshotPath).existsSync(), isTrue);
    expect(tester.takeException(), isNull);
  });

  testWidgets('exports stable control and page identification as JSON', (
    tester,
  ) async {
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
          captureScreenshot: captureTestScreenshot,
          child: Scaffold(
            body: Center(
              child: IconButton(
                key: const Key('refresh-control'),
                tooltip: 'Refresh calendar',
                onPressed: () {},
                icon: const Icon(Icons.refresh),
              ),
            ),
          ),
        ),
      ),
    );

    await toggleDesignFeedback(tester);
    await tester.tap(find.text('Inspect'));
    await tester.pump();
    await tester.tapAt(
      tester.getCenter(find.byKey(const Key('refresh-control'))),
    );
    await tester.pump();
    await tester.enterText(
      find.byKey(const Key('design-feedback-comment')),
      'Move this control.',
    );
    await tester.tap(find.text('Save pin'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Copy JSON'));
    await tester.pumpAndSettle();

    final payload = jsonDecode(copiedText!) as Map<String, Object?>;
    expect(payload['version'], 2);
    final annotation = (payload['annotations'] as List).single as Map;
    expect(annotation['selector'], contains('refresh-control'));
    expect(annotation['renderObject'], isNotEmpty);
    expect(annotation['creatorChain'], contains('IconButton'));
    final identifiers = annotation['identifiers'] as Map;
    expect(identifiers['widgetType'], 'IconButton');
    expect(identifiers['key'], contains('refresh-control'));
    expect(identifiers['tooltip'], 'Refresh calendar');
    expect((annotation['screenshot'] as Map)['path'], isNotEmpty);
    expect(tester.takeException(), isNull);
  });
}
