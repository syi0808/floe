import 'dart:convert';

import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_panel.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/personal_day_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_gateway.dart';
import 'support/agent_vault_gateway.dart';
import 'support/expert_result.dart';

Widget app(Widget child, {double scale = 1}) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(
      context,
    ).copyWith(textScaler: TextScaler.linear(scale), disableAnimations: true),
    child: child!,
  ),
  home: Scaffold(body: child),
);

void main() {
  testWidgets(
    'structured Expert source is readable at 320 pixels and 200 percent text',
    (tester) async {
      tester.view.physicalSize = const Size(320, 780);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = TestAgentGateway()
        ..capabilityOutput = jsonEncode(expertResultFixture());
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      await tester.pumpWidget(
        app(AgentPanel(controller: controller, onClose: () {}), scale: 2),
      );
      await tester.ensureVisible(find.text('Send sample'));
      await tester.tap(find.text('Send sample'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 200));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('View sample source'));
      await tester.tap(find.text('View sample source'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Expert: floe.schedule 1.0.0'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Possible focus time: 11:00–12:00'),
        findsOneWidget,
      );
      expect(find.textContaining('"schema_version"'), findsNothing);
      expect(tester.takeException(), isNull);
      await expectLater(
        find.byType(AgentPanel),
        matchesGoldenFile('goldens/agent_expert_source.png'),
      );
    },
  );

  setUpAll(() async {
    final font = FontLoader('Pretendard')
      ..addFont(rootBundle.load('assets/fonts/Pretendard-Regular.otf'))
      ..addFont(rootBundle.load('assets/fonts/Pretendard-SemiBold.otf'));
    await font.load();
    final icons = FontLoader('packages/lucide_icons_flutter/Lucide')
      ..addFont(
        rootBundle.load('packages/lucide_icons_flutter/assets/lucide.ttf'),
      );
    await icons.load();
    final material = FontLoader('MaterialIcons')
      ..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'));
    await material.load();
  });
  for (final width in [320.0, 390.0]) {
    testWidgets(
      'secure storage opens automatically at width $width and 200 percent text',
      (tester) async {
        tester.view.physicalSize = Size(width, 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final gateway = TestVaultGateway();
        final controller = AgentController(gateway: gateway, personId: 'test');
        addTearDown(controller.dispose);
        await controller.load();
        await tester.pumpWidget(
          app(AgentPanel(controller: controller, onClose: () {}), scale: 2),
        );
        expect(gateway.creates, 1);
        expect(find.byType(TextField), findsNothing);
        expect(controller.canSend, isTrue);
        expect(find.text('Set up secure storage'), findsNothing);
        expect(find.text('Unlock conversation storage'), findsNothing);
        expect(find.byTooltip('Lock conversation storage'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'sample panel shows live progress, stops with keyboard and restores composer focus',
    (tester) async {
      final gateway = TestAgentGateway()..hold = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      var closed = false;
      await tester.pumpWidget(
        app(AgentPanel(controller: controller, onClose: () => closed = true)),
      );
      expect(find.byType(TextField), findsNothing);
      expect(find.textContaining('Personal chat stays locked'), findsOneWidget);
      await tester.tap(find.text('Send sample'));
      await tester.pump();
      expect(find.text('Preparing sample reply…'), findsOneWidget);
      expect(find.text('Stop response'), findsOneWidget);
      expect(find.text(AgentFixturePrompt.today.sampleText), findsOneWidget);
      await tester.tap(find.text('Stop response'));
      await tester.pump(const Duration(milliseconds: 200));
      await tester.pumpAndSettle();
      expect(
        find.text('Response stopped. Saved messages are kept.'),
        findsOneWidget,
      );
      expect(find.text('Try again'), findsOneWidget);
      gateway.hold = false;
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 200));
      await tester.pumpAndSettle();
      expect(gateway.begins, 2);
      expect(
        find.textContaining('Sample briefing: Design review'),
        findsOneWidget,
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(closed, isTrue);
    },
  );

  for (final width in [320.0, 390.0]) {
    testWidgets(
      'sample panel stays usable at width $width and 200 percent text',
      (tester) async {
        tester.view.physicalSize = Size(width, 780);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final gateway = TestAgentGateway();
        final controller = AgentController(gateway: gateway, personId: 'test');
        addTearDown(controller.dispose);
        await controller.load();
        await tester.pumpWidget(
          app(AgentPanel(controller: controller, onClose: () {}), scale: 2),
        );
        await tester.ensureVisible(find.text('Send sample'));
        await tester.tap(find.text('Send sample'));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 200));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        await tester.ensureVisible(find.text('View sample source'));
        await tester.tap(find.text('View sample source'));
        await tester.pumpAndSettle();
        expect(find.textContaining('Synthetic timeline:'), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'interrupted and failed sessions show explicit recovery and read-only reload',
    (tester) async {
      final gateway = TestAgentGateway()..failLoad = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      await tester.pumpWidget(
        app(AgentPanel(controller: controller, onClose: () {})),
      );
      expect(find.text('Reload conversation'), findsOneWidget);
      gateway.failLoad = false;
      await gateway.startAgentFixture('test');
      gateway.saved!['active_turn'] = 'interrupted';
      await tester.tap(find.text('Reload conversation'));
      await tester.pumpAndSettle();
      expect(find.text('Recover conversation'), findsOneWidget);
      await tester.tap(find.text('Recover conversation'));
      await tester.pumpAndSettle();
      expect(gateway.recoveries, 1);
      expect(gateway.begins, 0);
      expect(find.text('Send sample'), findsOneWidget);
    },
  );

  for (final width in [1280.0, 390.0]) {
    testWidgets(
      'Today opens one user-invoked assistant surface at width $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final date = DateTime(2026, 9, 7, 9);
        await tester.pumpWidget(
          app(
            PersonalDayScreen(
              gateway: FakeDayGateway(),
              agentGateway: TestAgentGateway(),
              query: DayQuery(
                personId: 'test',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(find.byType(AgentPanel), findsNothing);
        await tester.ensureVisible(find.text('Floe is here to help'));
        await tester.tap(find.text('Floe is here to help'));
        await tester.pumpAndSettle();
        expect(find.byType(AgentPanel), findsOneWidget);
        expect(
          find.byType(BottomSheet),
          width < 960 ? findsOneWidget : findsNothing,
        );
        await tester.tap(find.byTooltip('Close').last);
        await tester.pumpAndSettle();
        expect(find.byType(AgentPanel), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'Today seals secure conversations when the app becomes inactive',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = TestVaultGateway();
      await gateway.createVault('test');
      final date = DateTime(2026, 9, 7, 9);
      await tester.pumpWidget(
        app(
          PersonalDayScreen(
            gateway: FakeDayGateway(),
            agentGateway: gateway,
            query: DayQuery(
              personId: 'test',
              date: date,
              now: date,
              timezoneOffsetSeconds: 0,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Floe is here to help'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Send sample'));
      await tester.pumpAndSettle();
      expect(find.text(AgentFixturePrompt.today.sampleText), findsOneWidget);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pumpAndSettle();
      expect(find.text(AgentFixturePrompt.today.sampleText), findsNothing);
      expect(find.text('Unlock conversation storage'), findsNothing);
      expect(find.text('Reload conversation'), findsOneWidget);
      expect(gateway.locks, 1);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pumpAndSettle();
      expect(gateway.unlocks, 1);
      expect(find.text(AgentFixturePrompt.today.sampleText), findsOneWidget);
    },
  );

  testWidgets('sample assistant panel visual reference', (tester) async {
    tester.view.physicalSize = const Size(420, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final controller = AgentController(
      gateway: TestAgentGateway(),
      personId: 'test',
    );
    addTearDown(controller.dispose);
    await controller.load();
    await tester.runAsync(() => controller.send(AgentFixturePrompt.today));
    await tester.pumpWidget(
      app(
        RepaintBoundary(
          key: const ValueKey('panel-golden'),
          child: AgentPanel(controller: controller, onClose: () {}),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await expectLater(
      find.byKey(const ValueKey('panel-golden')),
      matchesGoldenFile('goldens/agent_panel.png'),
    );
  });
}
