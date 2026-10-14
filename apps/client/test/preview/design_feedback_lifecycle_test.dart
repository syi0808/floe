import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/preview/design_feedback_overlay.dart';

void main() {
  for (final action in ['Save pin', 'Delete']) {
    testWidgets('clear during edit invalidates the $action callback', (
      tester,
    ) async {
      await _open(tester);
      await _addPin(tester);
      await tester.tap(find.byKey(const ValueKey('design-feedback-pin-1')));
      await tester.pumpAndSettle();
      final button = tester.widget<ButtonStyleButton>(
        find
            .ancestor(
              of: find.text(action),
              matching: find.bySubtype<ButtonStyleButton>(),
            )
            .first,
      );
      await tester.tap(find.byTooltip('Clear feedback'));
      await tester.pumpAndSettle();
      expect(find.text('0 pins'), findsOneWidget);

      // A callback already taken by an input event must not access a removed row.
      button.onPressed!();
      expect(find.text('Edit feedback'), findsNothing);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('0 pins'), findsOneWidget);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  testWidgets('removed pin callback cannot edit a newly numbered pin', (
    tester,
  ) async {
    await _open(tester);
    await _addPin(tester);
    final oldPin = tester.widget<InkWell>(
      find.byKey(const ValueKey('design-feedback-pin-1')),
    );
    await tester.tap(find.byTooltip('Clear feedback'));
    await tester.pumpAndSettle();
    await _addPin(tester);
    oldPin.onTap!();
    await tester.pumpAndSettle();
    expect(find.text('Edit feedback'), findsNothing);
    expect(find.text('1 pins'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('older screenshot cannot replace a newer saved revision', (
    tester,
  ) async {
    final captures = <Completer<String?>>[];
    String? copied;
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') {
        copied = (call.arguments as Map)['text'] as String;
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );
    await _open(
      tester,
      captureScreenshot: (_) {
        final capture = Completer<String?>();
        captures.add(capture);
        return capture.future;
      },
    );
    await _addPin(tester);
    await tester.tap(find.byKey(const ValueKey('design-feedback-pin-1')));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Updated feedback');
    await tester.tap(find.text('Save pin'));
    await tester.pumpAndSettle();
    expect(captures, hasLength(2));
    captures[1].complete('/tmp/current.png');
    await tester.pump();
    captures[0].complete('/tmp/obsolete.png');
    await tester.pump();
    await tester.tap(find.byTooltip('Copy JSON'));
    await tester.pumpAndSettle();
    final annotation =
        (jsonDecode(copied!)['annotations'] as List).single as Map;
    expect(annotation['comment'], 'Updated feedback');
    expect(annotation['screenshot']['path'], '/tmp/current.png');
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  for (final format in ['Markdown', 'JSON']) {
    testWidgets(
      'dispose while copying $format does not update detached state',
      (tester) async {
        final started = Completer<void>();
        final release = Completer<void>();
        final messenger =
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
        messenger.setMockMethodCallHandler(SystemChannels.platform, (
          call,
        ) async {
          if (call.method == 'Clipboard.setData') {
            started.complete();
            await release.future;
          }
          return null;
        });
        addTearDown(
          () =>
              messenger.setMockMethodCallHandler(SystemChannels.platform, null),
        );
        await _open(tester);
        await _addPin(tester);
        await tester.tap(find.byTooltip('Copy $format'));
        await tester.pump();
        expect(started.isCompleted, isTrue);
        await tester.pumpWidget(const SizedBox.shrink());
        release.complete();
        await tester.pump();
        expect(tester.takeException(), isNull);
      },
    );
  }
}

Future<void> _open(
  WidgetTester tester, {
  Future<String?> Function(Rect)? captureScreenshot,
}) async {
  tester.view.physicalSize = const Size(1200, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: DesignFeedbackOverlay(
        captureScreenshot: captureScreenshot ?? (_) async => null,
        child: const Scaffold(
          body: Center(child: Text('Target', key: ValueKey('target'))),
        ),
      ),
    ),
  );
  await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
  await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
  await tester.pumpAndSettle();
}

Future<void> _addPin(WidgetTester tester) async {
  await tester.tap(find.text('Inspect'));
  await tester.pumpAndSettle();
  await tester.tapAt(tester.getCenter(find.byKey(const ValueKey('target'))));
  await tester.pumpAndSettle();
  await tester.enterText(find.byType(TextField), 'Test feedback');
  await tester.tap(find.text('Save pin'));
  await tester.pumpAndSettle();
  expect(find.text('1 pins'), findsOneWidget);
}
