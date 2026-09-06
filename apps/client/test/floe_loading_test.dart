import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('loading operations remain visible for at least 500ms', () async {
    final stopwatch = Stopwatch()..start();

    await FloeLoading.run(() async {});

    expect(
      stopwatch.elapsed,
      greaterThanOrEqualTo(const Duration(milliseconds: 500)),
    );
  });

  testWidgets('loading button keeps its layout and blocks presses', (
    tester,
  ) async {
    var pressed = 0;

    Future<void> pumpButton(bool loading) => tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Center(
          child: FloeButton.filled(
            loading: loading,
            onPressed: () => pressed++,
            child: const Text('Save note'),
          ),
        ),
      ),
    );

    await pumpButton(false);
    final readySize = tester.getSize(find.byType(FilledButton));
    await pumpButton(true);

    expect(tester.getSize(find.byType(FilledButton)), readySize);
    expect(find.byType(FloeSpinner), findsOneWidget);
    await tester.tap(find.byType(FilledButton));
    expect(pressed, 0);
  });

  testWidgets('section overlay preserves its child size', (tester) async {
    Future<void> pumpOverlay(bool loading) => tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Center(
          child: FloeLoadingOverlay(
            loading: loading,
            child: const SizedBox(width: 240, height: 120),
          ),
        ),
      ),
    );

    await pumpOverlay(false);
    final readySize = tester.getSize(find.byType(FloeLoadingOverlay));
    await pumpOverlay(true);

    expect(tester.getSize(find.byType(FloeLoadingOverlay)), readySize);
    expect(find.byType(FloeSpinner), findsOneWidget);
  });
}
