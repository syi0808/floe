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

import '../../support/agent_gateway.dart';
import '../../support/agent_vault_gateway.dart';

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
        expect(find.byType(TextField), findsOneWidget);
        expect(controller.canSend, isTrue);
        expect(find.text('Set up secure storage'), findsNothing);
        expect(find.text('Unlock conversation storage'), findsNothing);
        expect(find.byTooltip('Lock conversation storage'), findsNothing);
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
      expect(find.byTooltip('Reload conversation'), findsOneWidget);
      gateway.failLoad = false;
      await gateway.startAgentFixture('test');
      gateway.saved!['active_turn'] = 'interrupted';
      await tester.tap(find.byTooltip('Reload conversation'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Recover conversation'), findsOneWidget);
      await tester.tap(find.byTooltip('Recover conversation'));
      await tester.pumpAndSettle();
      expect(gateway.recoveries, 1);
      expect(gateway.begins, 0);
      expect(find.byTooltip('Ask Floe'), findsOneWidget);
    },
  );

  for (final failure in {
    'server_model_timeout': 'The configured server model timed out. Try again, or choose a faster model route in the server dashboard.',
    'server_model_request_rejected': 'The configured server model rejected this request. Check the latest server trace and model compatibility.',
  }.entries) {
    testWidgets('${failure.key} shows actionable recovery guidance', (
      tester,
    ) async {
      final gateway = TestAgentGateway()..responseFailure = failure.key;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      final run = controller.send(AgentFixturePrompt.today);
      await tester.pump(const Duration(milliseconds: 100));
      await run;
      await tester.pumpWidget(
        app(AgentPanel(controller: controller, onClose: () {})),
      );
      expect(find.text(failure.value), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
    });
  }

  testWidgets('vault-backed model failure is not shown as a storage failure', (
    tester,
  ) async {
    final gateway = TestVaultGateway()
      ..responseFailure = 'server_model_invalid_output'
      ..omitSessionOnFailure = true;
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    await controller.load();
    final run = controller.send(AgentFixturePrompt.today);
    await tester.pump(const Duration(milliseconds: 100));
    await run;

    await tester.pumpWidget(
      app(AgentPanel(controller: controller, onClose: () {})),
    );

    expect(
      find.text(
        'The server model returned a response that Floe could not validate. Check the server trace and model configuration.',
      ),
      findsOneWidget,
    );
    expect(find.textContaining('couldn’t access secure storage'), findsNothing);
    expect(find.byTooltip('Reload conversation'), findsOneWidget);
  });

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

  testWidgets('Today keeps conversation storage open across lifecycle changes', (
    tester,
  ) async {
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
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pumpAndSettle();
    expect(gateway.locks, 0);
    expect(find.text('Reload conversation'), findsNothing);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    await tester.pumpAndSettle();
    expect(gateway.locks, 0);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();
    expect(gateway.locks, 0);
    expect(gateway.unlocks, 0);
    expect(find.text('Reload conversation'), findsNothing);
    await tester.tap(find.byTooltip('Close').last);
    await tester.pumpAndSettle();
    expect(gateway.locks, 0);
  });
}
