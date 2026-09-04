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
    await mouse.moveTo(tester.getCenter(find.text('Second')));
    await tester.pumpAndSettle();
    await tester.pump(Duration(seconds: 6));
    expect(find.text('First'), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('First')).dy,
      lessThan(tester.getTopLeft(find.text('Second')).dy - 30),
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
