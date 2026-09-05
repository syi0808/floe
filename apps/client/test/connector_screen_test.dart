import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/connector_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('service has only an icon surface and still opens details', (
    tester,
  ) async {
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ConnectorScreen(
              gateway: null,
              query: DayQuery(
                personId: 'test',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
              connection: null,
              onChanged: () async {},
            ),
          ),
        ),
      ),
    );
    final strings = AppLocalizations.of(
      tester.element(find.byType(ConnectorScreen)),
    );
    expect(find.byType(FloeSquircle), findsOneWidget);
    final iconSurface = tester.widget<FloeSquircle>(find.byType(FloeSquircle));
    expect(iconSurface.child, isA<Icon>());
    expect(iconSurface.borderWidth, 0);
    final serviceMaterial = tester.widget<Material>(
      find
          .ancestor(of: find.byType(InkWell), matching: find.byType(Material))
          .first,
    );
    expect(serviceMaterial.type, MaterialType.transparency);
    await tester.tap(find.text(strings.macosCalendar));
    await tester.pumpAndSettle();
    expect(find.text(strings.backToConnections), findsOneWidget);
    await tester.tap(find.text(strings.backToConnections));
    await tester.pumpAndSettle();
    expect(find.text(strings.availableServices), findsOneWidget);
    expect(find.text('Remote server connection'), findsNothing);
    expect(tester.takeException(), isNull);
  });
}
