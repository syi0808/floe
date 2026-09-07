import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_registry_dialog.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_registry.dart';

Widget app(AgentController controller, double scale) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(
      context,
    ).copyWith(textScaler: TextScaler.linear(scale), disableAnimations: true),
    child: child!,
  ),
  home: Scaffold(
    body: SingleChildScrollView(
      child: SettingsScreen(client: null, agentController: controller),
    ),
  ),
);

void main() {
  setUpAll(() async {
    final font = FontLoader('Pretendard')
      ..addFont(rootBundle.load('assets/fonts/Pretendard-Regular.otf'))
      ..addFont(rootBundle.load('assets/fonts/Pretendard-SemiBold.otf'));
    await font.load();
    await (FontLoader(
      'MaterialIcons',
    )..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'))).load();
    await (FontLoader('packages/lucide_icons_flutter/Lucide')..addFont(
          rootBundle.load('packages/lucide_icons_flutter/assets/lucide.ttf'),
        ))
        .load();
  });

  for (final width in [320.0, 520.0]) {
    testWidgets(
      'registry management remains readable and operates real controller at width $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final gateway = TestRegistryGateway();
        final controller = AgentController(
          gateway: gateway,
          personId: registryPerson,
        );
        addTearDown(controller.dispose);
        await controller.load();
        await tester.pumpWidget(app(controller, width == 320 ? 2 : 1));
        await tester.ensureVisible(find.text('Manage assistant access'));
        await tester.tap(find.text('Manage assistant access'));
        await tester.pumpAndSettle();
        expect(find.byType(AgentRegistryDialog), findsOneWidget);
        final toggle = find.byKey(
          const ValueKey('assignment-$registryAssignment'),
        );
        await tester.ensureVisible(toggle);
        await tester.tap(toggle);
        await tester.pumpAndSettle();
        expect(controller.registry!.assignments.single.enabled, false);
        expect(gateway.changes, 1);
        expect(
          find.text('State revision 2 · 2 completed invocations'),
          findsOneWidget,
        );
        expect(find.text(registryAssignment), findsNothing);
        expect(tester.takeException(), isNull);
        if (width == 520) {
          await expectLater(
            find.byType(AgentRegistryDialog),
            matchesGoldenFile('goldens/agent_registry.png'),
          );
        }
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        await tester.pumpAndSettle();
        expect(find.byType(AgentRegistryDialog), findsNothing);
      },
    );
  }

  testWidgets(
    'empty registry is inspectable without installing packages and failure offers refresh',
    (tester) async {
      final gateway = TestRegistryGateway()..snapshot = null;
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await tester.pumpWidget(app(controller, 1));
      await tester.tap(find.text('Manage assistant access'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('No packages are installed yet.'),
        findsOneWidget,
      );
      expect(gateway.changes, 0);
      gateway.registryError = 'conflict';
      await tester.tap(find.text('Refresh settings'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('The current settings could not be confirmed.'),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
