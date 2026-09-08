import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'calendar_action_execution_test.dart' show Executor;
import 'support/server_credentials.dart';
import 'support/agent_registry.dart';

void main() {
  testWidgets('assistant permission management lives in Settings', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(client: null, agentController: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Data & privacy'), findsNWidgets(2));
    expect(find.text('AI processing'), findsOneWidget);
    expect(
      find.text('No connected data sources are available yet.'),
      findsOneWidget,
    );
    expect(find.text('Schedule planning'), findsNothing);
  });

  testWidgets('settings navigation switches between separate pages', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final actionController = CalendarActionController(
      gateway: Executor(),
      personId: 'person',
    );
    final agentController = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    addTearDown(actionController.dispose);
    addTearDown(agentController.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SettingsScreen(
            client: LocalServerClient(store: MemoryServerCredentials()),
            actionController: actionController,
            agentController: agentController,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Allow all supported actions'), findsOneWidget);
    expect(find.text('Remote server connection'), findsNothing);

    await tester.tap(find.byKey(const Key('settings-dataPrivacy')));
    await tester.pumpAndSettle();
    expect(find.text('AI processing'), findsOneWidget);
    expect(find.text('Allow all supported actions'), findsNothing);

    await tester.tap(find.byKey(const Key('settings-remoteServer')));
    await tester.pumpAndSettle();
    expect(find.text('Remote server connection'), findsOneWidget);
  });

  testWidgets('external processing consent lives under Data & privacy', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final store = MemoryServerCredentials();
    final client = _SettingsServerClient(store);
    await client.save(
      ServerConnection(
        address: 'http://127.0.0.1:8431',
        token: 'a' * 52,
        clientId: 'paired-client',
      ),
    );
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(client: client, agentController: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Paired'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('external-model-consent')),
      findsOneWidget,
    );
    expect(find.text('Allow external model transfer'), findsNothing);
    expect(find.text('Needs consent'), findsOneWidget);
    expect(find.text('Unavailable'), findsOneWidget);
    expect(find.text('View recent data use (1)'), findsOneWidget);
    expect(find.text('Completed'), findsNothing);

    final activityButton = find.byKey(
      const ValueKey('processing-activity-open'),
    );
    await tester.ensureVisible(activityButton);
    await tester.tap(activityButton);
    await tester.pumpAndSettle();
    expect(find.text('Recent data use'), findsOneWidget);
    expect(find.text('Completed'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('processing-activity-close')));
    await tester.pumpAndSettle();

    final consentSwitch = find.byKey(const ValueKey('external-model-consent'));
    await tester.ensureVisible(consentSwitch);
    await tester.tap(consentSwitch);
    await tester.pumpAndSettle();
    expect((await client.connection())!.allowExternal, true);
    expect(find.text('Needs consent'), findsNothing);
  });

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

final class _SettingsServerClient extends LocalServerClient {
  _SettingsServerClient(ServerCredentialStore store) : super(store: store);

  @override
  Future<Map<InferencePurpose, InferencePurposeAvailability>> purposes(
    ServerConnection connection,
  ) async => {
    InferencePurpose.quickResponse: const InferencePurposeAvailability(
      available: true,
      requiresExternalConsent: false,
    ),
    InferencePurpose.everydayAssistance: const InferencePurposeAvailability(
      available: true,
      requiresExternalConsent: true,
      placement: 'external',
      recipient: 'Example AI',
    ),
    InferencePurpose.deepWork: const InferencePurposeAvailability(
      available: false,
      requiresExternalConsent: false,
    ),
  };

  @override
  Future<List<InferenceAuditRecord>> privacyActivity(
    ServerConnection connection,
  ) async => [
    InferenceAuditRecord(
      traceId: '0123456789abcdef0123456789abcdef',
      createdAt: DateTime(2026, 9, 8, 12),
      purpose: 'everyday_assistance',
      dataClasses: const ['personal'],
      placement: 'remote',
      externalTransfer: true,
      outcome: 'completed',
    ),
  ];
}
