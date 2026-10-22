import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/l10n/app_localizations.dart';

Widget _host(Widget child) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: Center(child: child)),
);

void main() {
  testWidgets('busy buttons announce generic loading and prevent activation', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      var loading = true;
      var activations = 0;
      late StateSetter update;
      await tester.pumpWidget(
        _host(
          StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              return FloeButton.filled(
                loading: loading,
                onPressed: () => activations++,
                child: const Text('Save note'),
              );
            },
          ),
        ),
      );
      final button = find.byType(FilledButton);
      final busy = tester.getSemantics(button).getSemanticsData();
      expect(busy.label, 'Loading');
      expect(busy.flagsCollection.isLiveRegion, isTrue);
      expect(busy.flagsCollection.isEnabled, ui.Tristate.isFalse);
      expect(find.bySemanticsLabel('Save note'), findsNothing);
      expect(find.bySemanticsLabel('Loading calendar'), findsNothing);
      await tester.tap(button);
      expect(activations, 0);
      update(() => loading = false);
      await tester.pumpAndSettle();
      expect(tester.getSemantics(button).label, 'Save note');
      await tester.tap(button);
      expect(activations, 1);
    } finally {
      semantics.dispose();
    }
  });

  testWidgets('busy buttons can opt into a meaningful action label', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      await tester.pumpWidget(
        _host(
          FloeButton.filled(
            loading: true,
            loadingLabel: 'Stop voice',
            onPressed: () {},
            child: const Text('Stop voice'),
          ),
        ),
      );
      final busy = tester
          .getSemantics(find.byType(FilledButton))
          .getSemanticsData();
      expect(busy.label, 'Stop voice');
      expect(busy.flagsCollection.isLiveRegion, isTrue);
      expect(busy.flagsCollection.isEnabled, ui.Tristate.isFalse);
      expect(find.bySemanticsLabel('Loading'), findsNothing);
    } finally {
      semantics.dispose();
    }
  });

  testWidgets('explicit spinner labels retain their context', (tester) async {
    final semantics = tester.ensureSemantics();
    try {
      await tester.pumpWidget(
        _host(const FloeSpinner(label: 'Loading calendar')),
      );
      expect(find.bySemanticsLabel('Loading calendar'), findsOneWidget);
      expect(find.bySemanticsLabel(RegExp(r'^Loading$')), findsNothing);
    } finally {
      semantics.dispose();
    }
  });

  testWidgets('reduced motion preserves loading feedback without rotation', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      await tester.pumpWidget(
        _host(
          const MediaQuery(
            data: MediaQueryData(disableAnimations: true),
            child: FloeSpinner(),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.bySemanticsLabel('Loading'), findsOneWidget);
      expect(tester.binding.hasScheduledFrame, isFalse);
    } finally {
      semantics.dispose();
    }
  });
}
