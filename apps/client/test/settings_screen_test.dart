import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'calendar_action_execution_test.dart' show Executor;
import 'support/server_credentials.dart';

void main() {
  for (final width in [390.0, 1200.0]) {
    testWidgets('remote server lives under Settings at $width', (tester) async {
      await tester.binding.setSurfaceSize(Size(width, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          home: Scaffold(
            body: SingleChildScrollView(
              child: SettingsScreen(
                client: LocalServerClient(store: MemoryServerCredentials()),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Settings'), findsOneWidget);
      expect(find.text('Remote server'), findsOneWidget);
      expect(find.text('Remote server connection'), findsOneWidget);
      expect(find.byKey(const Key('server-address')), findsOneWidget);
      expect(find.text('Pair this device'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('action permissions use presets and the Floe select', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = Executor();
    final controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(client: null, actionController: controller),
          ),
        ),
      ),
    );

    expect(
      find.byType(DropdownButtonFormField<ActionAuthorityMode>),
      findsNothing,
    );
    expect(find.byType(FloeSelect<ActionAuthorityMode>), findsOneWidget);
    expect(find.text('Allow all supported actions'), findsOneWidget);
    expect(find.text('Customize permissions'), findsOneWidget);
    expect(
      tester
          .widget<FloeSelect<ActionAuthorityMode>>(
            find.byType(FloeSelect<ActionAuthorityMode>),
          )
          .enabled,
      isTrue,
    );

    await tester.tap(find.text('Allow all supported actions'));
    await tester.pumpAndSettle();
    expect(controller.authority.calendarCreate, ActionAuthorityMode.allow);
    expect(
      tester
          .widget<FloeSelect<ActionAuthorityMode>>(
            find.byType(FloeSelect<ActionAuthorityMode>),
          )
          .enabled,
      isFalse,
    );

    await tester.tap(find.text('Customize permissions'));
    await tester.pump();
    expect(
      tester
          .widget<FloeSelect<ActionAuthorityMode>>(
            find.byType(FloeSelect<ActionAuthorityMode>),
          )
          .enabled,
      isTrue,
    );

    await tester.tap(find.text('Allow automatically'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Do not allow'));
    await tester.pumpAndSettle();
    expect(controller.authority.calendarCreate, ActionAuthorityMode.deny);
  });
}
