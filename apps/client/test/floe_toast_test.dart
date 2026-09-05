import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_toast.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late GlobalKey<FloeToastHostState> host;

  Future<void> mount(
    WidgetTester tester, {
    double width = 1440,
    double scale = 1,
    bool reduced = false,
    bool accessible = false,
    double keyboard = 0,
  }) async {
    tester.view.physicalSize = Size(width, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    host = GlobalKey<FloeToastHostState>();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context).copyWith(
            textScaler: TextScaler.linear(scale),
            disableAnimations: reduced,
            accessibleNavigation: accessible,
            viewInsets: EdgeInsets.only(bottom: keyboard),
          ),
          child: child!,
        ),
        home: FloeToastHost(
          key: host,
          child: Scaffold(
            body: TextButton(onPressed: () {}, child: Text('Workspace')),
          ),
        ),
      ),
    );
  }

  testWidgets('retains newest three and expires without blocking workspace', (
    tester,
  ) async {
    await mount(tester);
    for (var index = 0; index < 4; index++) {
      host.currentState!.show(title: 'Saved $index');
    }
    await tester.pumpAndSettle();
    expect(find.text('Saved 0'), findsNothing);
    expect(find.text('Saved 3'), findsOneWidget);
    expect(find.byTooltip('Close'), findsNWidgets(3));
    await tester.tap(find.text('Workspace'));
    await tester.pump(Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Close'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('hover expands and pauses remaining lifetime', (tester) async {
    await mount(tester);
    host.currentState!.show(title: 'First');
    host.currentState!.show(title: 'Second');
    await tester.pumpAndSettle();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(Offset(tester.getCenter(find.text('Second')).dx, 100));
    await tester.pumpAndSettle();
    expect(
      tester.getTopLeft(find.text('Second')).dy -
          tester.getTopLeft(find.text('First')).dy,
      lessThan(30),
    );
    await mouse.moveTo(tester.getCenter(find.text('Second')));
    await tester.pumpAndSettle();
    await tester.pump(Duration(seconds: 6));
    expect(find.text('First'), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('First')).dy,
      lessThan(tester.getTopLeft(find.text('Second')).dy - 30),
    );
    final firstCard = find.byKey(ValueKey('toast-card-0'));
    final secondCard = find.byKey(ValueKey('toast-card-1'));
    final gap = Offset(
      tester.getCenter(secondCard).dx,
      (tester.getBottomRight(firstCard).dy + tester.getTopLeft(secondCard).dy) /
          2,
    );
    await mouse.moveTo(gap);
    await tester.pump(Duration(seconds: 6));
    expect(
      tester.getTopLeft(secondCard).dy - tester.getBottomRight(firstCard).dy,
      closeTo(10, 0.1),
    );
    await mouse.moveTo(Offset.zero);
    await tester.pumpAndSettle();
    await tester.pump(Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(find.text('Second'), findsNothing);
    await mouse.removePointer();
  });

  testWidgets('keyboard focus pauses and Escape dismisses', (tester) async {
    await mount(tester);
    host.currentState!.show(title: 'Keyboard');
    await tester.pumpAndSettle();
    final buttonContext = tester.element(find.byType(IconButton));
    Focus.of(buttonContext).requestFocus();
    await tester.pump();
    await tester.pump(Duration(seconds: 6));
    expect(find.text('Keyboard'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    await tester.pump(Duration(milliseconds: 300));
    expect(find.text('Keyboard'), findsNothing);
  });

  for (final tallFirst in [true, false]) {
    testWidgets(
      'mixed heights normalize collapsed and restore expanded: $tallFirst',
      (tester) async {
        await mount(tester);
        void show(bool tall) => host.currentState!.show(
          title: tall ? 'Long notification' : 'Task completed',
          description: tall
              ? 'First line\nSecond line\nThird line\nFourth line'
              : null,
          actionLabel: tall ? null : 'Undo',
          onAction: tall ? null : () {},
        );
        show(tallFirst);
        show(!tallFirst);
        await tester.pumpAndSettle();
        final first = find.byKey(ValueKey('toast-card-0'));
        final front = find.byKey(ValueKey('toast-card-1'));
        final collapsedHeight = tester.getSize(front).height;
        expect(tester.getSize(first).height, collapsedHeight);
        expect(
          tester.getBottomRight(first).dy,
          lessThan(tester.getBottomRight(front).dy),
        );

        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(location: Offset.zero);
        await mouse.moveTo(tester.getCenter(front));
        await tester.pumpAndSettle();
        final tall = tallFirst ? first : front;
        final short = tallFirst ? front : first;
        expect(
          tester.getSize(tall).height,
          greaterThan(tester.getSize(short).height + 40),
        );
        expect(
          tester.getTopLeft(front).dy - tester.getBottomRight(first).dy,
          closeTo(10, 0.1),
        );
        await mouse.moveTo(Offset.zero);
        await tester.pumpAndSettle();
        expect(tester.getSize(first).height, closeTo(collapsedHeight, 0.1));
        expect(tester.getSize(front).height, closeTo(collapsedHeight, 0.1));

        show(false);
        await tester.pumpAndSettle();
        final newest = find.byKey(ValueKey('toast-card-2'));
        for (final card in [first, front]) {
          expect(tester.getSize(card).height, tester.getSize(newest).height);
        }
        await mouse.removePointer();
        await tester.pumpWidget(SizedBox());
        expect(tester.takeException(), isNull);
      },
    );
  }

  for (final scale in [1.0, 2.0]) {
    testWidgets('Undo sits to the right of the message at text scale $scale', (
      tester,
    ) async {
      await mount(tester, width: 390, scale: scale);
      host.currentState!.show(
        title: 'Task completed',
        actionLabel: 'Undo',
        onAction: () {},
      );
      await tester.pumpAndSettle();
      final message = tester.getRect(find.text('Task completed'));
      final undo = tester.getRect(find.widgetWithText(TextButton, 'Undo'));
      final close = tester.getRect(find.byType(IconButton));
      expect(undo.left, greaterThan(message.right));
      expect(undo.right, lessThan(close.left));
      expect(undo.center.dy, closeTo(message.center.dy, 0.1));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(SizedBox());
    });
  }

  testWidgets('background pauses expiry and dispose cancels timers', (
    tester,
  ) async {
    await mount(tester);
    host.currentState!.show(title: 'Background');
    await tester.pumpAndSettle();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pump(Duration(seconds: 6));
    expect(find.text('Background'), findsOneWidget);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump(Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(find.text('Background'), findsNothing);
    host.currentState!.show(title: 'Unmount');
    await tester.pump();
    await tester.pumpWidget(SizedBox());
    await tester.pump(Duration(seconds: 10));
    expect(tester.takeException(), isNull);
  });

  testWidgets('accessible action remains available and invokes undo once', (
    tester,
  ) async {
    await mount(tester, accessible: true);
    var calls = 0;
    host.currentState!.show(
      title: 'Completed',
      actionLabel: 'Undo',
      onAction: () => calls++,
    );
    await tester.pumpAndSettle();
    await tester.pump(Duration(seconds: 10));
    expect(find.text('Undo'), findsOneWidget);
    await tester.tap(find.text('Undo'));
    await tester.pumpAndSettle();
    await tester.pump(Duration(milliseconds: 300));
    expect(calls, 1);
    expect(find.text('Completed'), findsNothing);
  });

  for (final width in [390.0, 1440.0]) {
    testWidgets('fits $width with large text, keyboard and reduced motion', (
      tester,
    ) async {
      await mount(tester, width: width, scale: 2, reduced: true, keyboard: 300);
      host.currentState!.show(
        title: 'A longer notification title that wraps naturally',
        description: 'Your local tasks and notes are unchanged.',
      );
      await tester.pump();
      final rect = tester.getRect(find.byType(IconButton));
      expect(rect.right, lessThanOrEqualTo(width - 16));
      expect(rect.bottom, lessThanOrEqualTo(600));
      expect(tester.takeException(), isNull);
      await tester.tap(find.byTooltip('Close'));
      await tester.pump(Duration(milliseconds: 100));
      expect(find.byTooltip('Close'), findsNothing);
    });
  }
}
