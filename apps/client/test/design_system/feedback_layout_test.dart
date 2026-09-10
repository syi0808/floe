import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final width in [360.0, 1440.0]) {
    for (final simple in [false, true]) {
      testWidgets('dialog width is bounded at $width, simple: $simple', (
        tester,
      ) async {
        tester.view.physicalSize = Size(width, 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        await tester.pumpWidget(
          MaterialApp(
            theme: FloeTheme.light,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () => showFloeDialog<void>(
                    context,
                    (_) => simple
                        ? SimpleDialog(
                            title: const Text('Choose a calendar'),
                            children: [Text('Calendar name ' * 30)],
                          )
                        : AlertDialog(
                            title: const Text('Connect Calendar'),
                            content: Text('Calendar disclosure text ' * 30),
                          ),
                  ),
                  child: const Text('Open'),
                ),
              ),
            ),
          ),
        );
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        final surface = find
            .descendant(
              of: find.byType(Dialog),
              matching: find.byType(Material),
            )
            .first;
        final bounds = tester.getRect(surface);
        expect(bounds.width, lessThanOrEqualTo(540));
        expect(bounds.left, greaterThanOrEqualTo(0));
        expect(bounds.right, lessThanOrEqualTo(width));
        expect(tester.takeException(), isNull);
      });
    }
  }

  for (final scale in [1.0, 2.0]) {
    for (final text in [
      'First line\nSecond line\nThird line',
      '첫 번째 줄\n두 번째 줄',
    ]) {
      testWidgets('icon centers on first line at $scale: $text', (
        tester,
      ) async {
        await tester.pumpWidget(
          MaterialApp(
            theme: FloeTheme.light,
            home: MediaQuery(
              data: MediaQueryData(textScaler: TextScaler.linear(scale)),
              child: Scaffold(body: FloeInfoNote(text: text)),
            ),
          ),
        );
        final textBounds = tester.getRect(find.text(text));
        final lineHeight = textBounds.height / text.split('\n').length;
        expect(
          tester.getCenter(find.byType(Icon)).dy,
          closeTo(textBounds.top + lineHeight / 2, 0.1),
        );
        expect(tester.takeException(), isNull);
      });
    }
  }
}
