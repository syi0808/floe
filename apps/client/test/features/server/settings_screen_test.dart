import 'dart:convert';
import 'dart:io';

import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/infrastructure/native/local_context_publication.dart';
import 'package:floe_client/infrastructure/native/native_transport.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../day_canvas/calendar_action_execution_test.dart' show Executor;
import '../../support/server_credentials.dart';
import '../../support/agent_registry.dart';

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
      tester.widget<Text>(find.text('Processing locations')).style?.fontWeight,
      FontWeight.w600,
    );
    expect(
      tester.widget<Text>(find.text('On this device')).style?.fontWeight,
      FontWeight.w500,
    );
    expect(
      find.text('No connected data sources are available yet.'),
      findsOneWidget,
    );
    expect(find.text('Schedule planning'), findsNothing);
  });

  testWidgets('Android Health consent and derived refresh live in settings', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    final androidContext = _AndroidContext();
    addTearDown(controller.dispose);
    await controller.load();

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(
              client: null,
              agentController: controller,
              androidContext: androidContext,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Health Connect'), findsOneWidget);
    expect(find.text('Allow Health Connect'), findsOneWidget);
    expect(
      find.textContaining('Health records stay on this device'),
      findsOneWidget,
    );

    final refresh = find.byKey(const ValueKey('android-health-refresh'));
    await tester.ensureVisible(refresh);
    await tester.tap(refresh);
    await tester.pumpAndSettle();

    expect(androidContext.permissionRequests, 1);
    expect(androidContext.wellbeingReads, 1);
    expect(find.text('Ready'), findsOneWidget);
    expect(find.text('Refresh wellbeing'), findsOneWidget);
  });

  testWidgets('Android Calendar selection is explicit and device-local', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    final androidContext = _AndroidContext(includeCalendar: true);
    addTearDown(controller.dispose);
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(
              client: null,
              agentController: controller,
              androidContext: androidContext,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final allow = find.byKey(const ValueKey('android-calendar-allow'));
    await tester.ensureVisible(allow);
    await tester.tap(allow);
    await tester.pumpAndSettle();
    expect(androidContext.calendarPermissionRequests, 1);

    final calendar = find.byKey(const ValueKey('android-calendar-work'));
    await tester.ensureVisible(calendar);
    await tester.tap(calendar);
    await tester.pumpAndSettle();
    expect(androidContext.selected, {'work'});
    expect(androidContext.calendarReads, 1);
    expect(find.text('Ready'), findsOneWidget);
  });

  testWidgets('Apple feasibility requires an explicit next-event destination', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    final appleContext = _AppleContext();
    Map<String, Object?>? reviewedQuery;
    final personalGateway = NativeAgentVaultGateway((request) async {
      final operation = request['operation']! as Map;
      if (operation['kind'] == 'submit') {
        final action = operation['action']! as Map;
        final change = action['change']! as Map;
        final personalChange = change['change']! as Map;
        if (personalChange['kind'] == 'review') {
          reviewedQuery = Map<String, Object?>.from(
            personalChange['feasibility_query']! as Map,
          );
        }
        final reviewed = personalChange['kind'] == 'review';
        return {
          'request_id': request['request_id'],
          'done': true,
          'events': const <Object?>[],
          'next_sequence': 0,
          'state': 'ready',
          'personal_access': _personalAccessOverview(
            state: reviewed ? 'active' : 'needs_review',
            reviewRequired: !reviewed,
            grantId: reviewed ? 'grant-id' : null,
          ),
        };
      }
      return {
        'request_id': request['request_id'],
        'done': true,
        'events': const <Object?>[],
        'next_sequence': 0,
      };
    }, deviceId: 'apple-test');
    addTearDown(controller.dispose);
    await controller.load();
    final event = EventItem(
      id: '00000000-0000-4000-8000-000000000099',
      title: 'Planning review',
      revision: 1,
      createdAt: DateTime.utc(2026, 9, 11),
      startsAt: DateTime.utc(2026, 9, 11, 3),
      endsAt: DateTime.utc(2026, 9, 11, 4),
    );
    final snapshot = DaySnapshot(
      personId: registryPerson,
      date: DateTime.utc(2026, 9, 11),
      generatedAt: DateTime.utc(2026, 9, 11),
      timezoneOffsetSeconds: 0,
      items: [event],
      nextEventId: event.id,
    );

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(
              client: null,
              agentController: controller,
              appleContext: appleContext,
              daySnapshot: snapshot,
              agentVaultGateway: personalGateway,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Trip feasibility access'), findsOneWidget);
    final review = find.byKey(const ValueKey('personal-feasibility-review'));
    await tester.ensureVisible(review);
    await tester.tap(review);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('feasibility-latitude')),
      '37.5665',
    );
    await tester.enterText(
      find.byKey(const ValueKey('feasibility-longitude')),
      '126.9780',
    );
    await tester.tap(find.byKey(const ValueKey('feasibility-refresh-confirm')));
    await tester.pumpAndSettle();

    expect(appleContext.feasibilityPermissionRequests, 1);
    expect(reviewedQuery?['event_handle'], 'event:${event.id}');
    expect(reviewedQuery?['evidence_handles'], ['calendar.event:${event.id}']);
    expect(reviewedQuery?['destination_latitude'], 37.5665);
    expect(reviewedQuery?['destination_longitude'], 126.978);
    expect(
      reviewedQuery?['event_start_unix_ms'],
      event.startsAt.millisecondsSinceEpoch,
    );
    expect(
      reviewedQuery?['event_end_unix_ms'],
      event.endsAt.millisecondsSinceEpoch,
    );
    expect(reviewedQuery?['travel_mode'], 'transit');
  });

  testWidgets('Apple Wellbeing review requests permission and can pause', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    final appleContext = _AppleContext()..exposeHealthConnection = true;
    var enabled = false;
    var paused = false;
    final personalGateway = NativeAgentVaultGateway((request) async {
      final operation = request['operation']! as Map;
      if (operation['kind'] == 'submit') {
        final action = operation['action']! as Map;
        final change = action['change']! as Map;
        final personalChange = change['change']! as Map;
        final kind = personalChange['kind'];
        if (kind == 'review') {
          enabled = true;
          paused = false;
        }
        if (kind == 'set_enabled') {
          enabled = personalChange['enabled'] == true;
          paused = !enabled;
        }
        return {
          'request_id': request['request_id'],
          'done': true,
          'events': const <Object?>[],
          'next_sequence': 0,
          'state': 'ready',
          'personal_access': _personalAccessOverview(
            connector: 'health.apple',
            connectionId: 'health.apple.local',
            state: paused
                ? 'paused'
                : enabled
                ? 'active'
                : 'needs_review',
            reviewRequired: !enabled && !paused,
            grantId: enabled ? 'wellbeing-grant' : null,
          ),
        };
      }
      return {
        'request_id': request['request_id'],
        'done': true,
        'events': const <Object?>[],
        'next_sequence': 0,
      };
    }, deviceId: 'apple-test');
    final publishing = PublishingAppleContextGateway(
      gateway: appleContext,
      transport: _NoopLocalContextTransport(),
      personId: registryPerson,
      deviceId: 'apple-test',
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
            child: SettingsScreen(
              client: null,
              agentController: controller,
              appleContext: publishing,
              agentVaultGateway: personalGateway,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Wellbeing access'), findsOneWidget);
    expect(appleContext.wellbeingPermissionRequests, 0);

    final review = find.byKey(const ValueKey('personal-wellbeing-review'));
    await tester.ensureVisible(review);
    await tester.tap(review);
    await tester.pumpAndSettle();
    expect(appleContext.wellbeingPermissionRequests, 1);

    final pause = find.byKey(const ValueKey('personal-wellbeing-pause'));
    await tester.ensureVisible(pause);
    await tester.tap(pause);
    await tester.pumpAndSettle();
    expect(find.text('Paused.'), findsOneWidget);
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

  testWidgets('desktop navigation and content scroll independently', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 500));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(body: const SettingsScreen(client: null)),
      ),
    );

    final navigation = tester.widget<SingleChildScrollView>(
      find.byKey(const ValueKey('settings-navigation-scroll')),
    );
    final content = tester.widget<SingleChildScrollView>(
      find.byKey(const ValueKey('settings-content-scroll')),
    );
    expect(navigation.scrollDirection, Axis.vertical);
    expect(content.scrollDirection, Axis.vertical);
    expect(navigation.controller, isNot(same(content.controller)));
    expect(tester.takeException(), isNull);
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
        personId: registryPerson,
        deviceId: 'local-client',
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
    expect(
      tester.widget<Text>(find.text('Task routes')).style?.fontWeight,
      FontWeight.w600,
    );
    expect(
      tester
          .widget<Text>(find.text('Allow external model providers'))
          .style
          ?.fontWeight,
      FontWeight.w600,
    );

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

  testWidgets('server Gmail health joins shared Connections settings', (
    tester,
  ) async {
    final store = MemoryServerCredentials();
    final snapshot = Map<String, dynamic>.from(
      jsonDecode(
        File(
          '../../server/internal/connectors/gmail/testdata/ready_snapshot.json',
        ).readAsStringSync(),
      ) as Map,
    );
    final client = _SettingsServerClient(
      store,
      connectionSnapshots: [snapshot],
    );
    await client.save(
      ServerConnection(
        address: 'http://127.0.0.1:8431',
        token: 'a' * 52,
        clientId: 'paired-client',
        personId: registryPerson,
        deviceId: 'local-client',
      ),
    );
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SettingsScreen(client: client, agentController: controller),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Gmail'), findsOneWidget);
    expect(find.text('Ready'), findsOneWidget);
    expect(find.textContaining('Runs on server'), findsOneWidget);
    expect(
      find.textContaining('Actions require separate approval.'),
      findsOneWidget,
    );
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
    final gateway = _LockableActionExecutor();
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
    gateway.locked = true;
    await tester.runAsync(controller.load);
    await tester.pumpAndSettle();
    expect(
      find.text(
        'Open and unlock the agent vault to review or change action permissions.',
      ),
      findsOneWidget,
    );
    expect(
      tester
          .widget<FloeSelect<ActionAuthorityMode>>(
            find.byType(FloeSelect<ActionAuthorityMode>),
          )
          .enabled,
      isFalse,
    );
  });
}

final class _LockableActionExecutor extends Executor {
  bool locked = false;

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) {
    if (locked) throw const AgentVaultException('vault_unavailable');
    return super.loadActionAuthority(personId);
  }
}

final class _AndroidContext implements AndroidContextApi {
  _AndroidContext({this.includeCalendar = false});

  final bool includeCalendar;
  bool healthReady = false;
  bool calendarGranted = false;
  bool calendarReady = false;
  int permissionRequests = 0;
  int calendarPermissionRequests = 0;
  int wellbeingReads = 0;
  int calendarReads = 0;
  Set<String> selected = {};

  @override
  Future<List<Map<String, dynamic>>> connections() async => [
    _healthConnection(ready: healthReady),
    if (includeCalendar)
      _androidCalendarConnection(
        granted: calendarGranted,
        ready: calendarReady,
      ),
  ];

  @override
  Future<bool> requestPermission(AndroidContextSource source) async {
    if (source == AndroidContextSource.health) {
      permissionRequests += 1;
    } else if (source == AndroidContextSource.calendar) {
      calendarPermissionRequests += 1;
      calendarGranted = true;
    }
    return true;
  }

  @override
  Future<List<AndroidCalendarOption>> listCalendars() async => [
    AndroidCalendarOption.fromJson({
      'calendar_id': 'work',
      'display_name': 'Work',
    }),
  ];

  @override
  Future<List<String>> selectedCalendars() async => selected.toList();

  @override
  Future<List<String>> setSelectedCalendars(List<String> calendarIds) async {
    selected = calendarIds.toSet();
    return calendarIds;
  }

  @override
  Future<Map<String, dynamic>> readCalendar({
    required DateTime rangeStart,
    required DateTime rangeEnd,
    String cursor = '',
    int limit = 128,
  }) async {
    calendarReads += 1;
    calendarReady = true;
    return <String, dynamic>{};
  }

  @override
  Future<Map<String, dynamic>> readAcquisition(
    Map<String, dynamic> request,
  ) async => <String, dynamic>{};

  @override
  Future<Map<String, dynamic>> readContacts({
    int limit = 64,
    List<String>? selectedHandles,
  }) async => {
    'schema_version': 1,
    'view_id': 'people.identity',
    'source_handle': 'people:test',
    'observed_at_unix_ms': 1000,
    'expires_at_unix_ms': 301000,
    'coverage_complete': true,
    'identities': <Object>[],
  };

  @override
  Future<Map<String, dynamic>> readWellbeing() async {
    wellbeingReads += 1;
    healthReady = true;
    return {
      'schema_version': 1,
      'view_id': 'wellbeing.derived',
      'source_handle': 'wellbeing:test',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 301000,
      'capacity': 'typical',
      'recovery': 'recovered',
      'confidence_millis': 700,
      'evidence_handles': <String>['health.sleep.window:test'],
    };
  }
}

final class _AppleContext
    implements
        AppleContextApi,
        AppleFeasibilitySubjectApi,
        AppleHealthSubjectApi {
  AppleFeasibilityQuery? query;
  int feasibilityPermissionRequests = 0;
  bool exposeHealthConnection = false;
  int wellbeingPermissionRequests = 0;

  @override
  Future<List<Map<String, dynamic>>> connections() async => [
    if (exposeHealthConnection) _appleHealthConnection(),
    if (!exposeHealthConnection) _appleFeasibilityConnection(),
  ];

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async {
    this.query = query;
    return <String, dynamic>{};
  }

  @override
  Future<Map<String, dynamic>> inspectFeasibilitySubject() async => {
    'schema_version': 1,
    'subject_fingerprint': 'a' * 64,
    'permission_class': 'location_precise',
  };

  @override
  Future<bool> requestFeasibilityPermission() async {
    feasibilityPermissionRequests += 1;
    return true;
  }

  @override
  Future<Map<String, dynamic>> inspectWellbeingSubject() async => {
    'schema_version': 1,
    'subject_fingerprint': 'b' * 64,
    'permission_class': 'pending',
  };

  @override
  Future<bool> requestWellbeingPermission() async {
    wellbeingPermissionRequests += 1;
    return true;
  }

  @override
  Future<bool> requestPermission(AppleContextSource source) async => true;

  @override
  Future<Map<String, dynamic>> readContacts({
    int limit = 64,
    List<String>? selectedHandles,
  }) async => {};

  @override
  Future<Map<String, dynamic>> readWellbeing() async => {};

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async => {};
}

Map<String, dynamic> _personalAccessOverview({
  required String state,
  required bool reviewRequired,
  required String? grantId,
  String connector = 'feasibility.apple',
  String connectionId = 'feasibility.apple.local',
}) => {
  'schema_version': 1,
  'person_id': registryPerson,
  'connector': connector,
  'device_id': 'apple-test',
  'connection_id': connectionId,
  'source_authority': null,
  'grant_id': grantId,
  'grant_authority': grantId == null
      ? null
      : {'incarnation': 'authority', 'epoch': 1},
  'state': state,
  'review_required': reviewRequired,
  'presence_available': false,
  'consumers': grantId == null ? <String>[] : ['assistant'],
  'native_subject_fingerprint': null,
  'process_incarnation': null,
};

Map<String, dynamic> _healthConnection({required bool ready}) => {
  'descriptor': {
    'schema_version': 1,
    'id': 'health.android',
    'version': '1.0.0',
    'provider': 'health_connect',
    'execution': {'kind': 'device', 'device_id': 'android-test'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'health.derived.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['android.permission.health.READ_SLEEP'],
        'output_view_id': 'wellbeing.derived',
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'wellbeing.derived',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'derived_only',
        'freshness_ttl_ms': 300000,
        'max_items': 1,
        'max_bytes': 32768,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'health.android',
    'state': ready ? 'ready' : 'revoked',
    'granted_scopes': ready
        ? ['android.permission.health.READ_SLEEP']
        : <String>[],
    'observed_at_unix_ms': 2000,
    if (ready) 'last_success_at_unix_ms': 2000,
    if (!ready)
      'last_failure': {
        'kind': 'permission_denied',
        'observed_at_unix_ms': 2000,
      },
  },
  'views': ready
      ? [
          {
            'schema_version': 1,
            'view_id': 'wellbeing.derived',
            'source_handle': 'wellbeing:test',
            'observed_at_unix_ms': 1000,
            'expires_at_unix_ms': 301000,
            'item_count': 1,
            'byte_count': 256,
            'provenance_count': 1,
          },
        ]
      : <Object?>[],
};

Map<String, dynamic> _androidCalendarConnection({
  required bool granted,
  required bool ready,
}) => {
  'descriptor': {
    'schema_version': 1,
    'id': 'calendar.android',
    'version': '1.0.0',
    'provider': 'android_calendar',
    'execution': {'kind': 'device', 'device_id': 'android-test'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'calendar.events.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['android.permission.READ_CALENDAR'],
        'output_view_id': 'calendar.timeline',
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'calendar.timeline',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'ephemeral',
        'freshness_ttl_ms': 300000,
        'max_items': 128,
        'max_bytes': 65536,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'calendar.android',
    'state': ready
        ? 'ready'
        : granted
        ? 'pending'
        : 'revoked',
    'granted_scopes': granted ? ['android.permission.READ_CALENDAR'] : [],
    'observed_at_unix_ms': 2000,
    if (ready) 'last_success_at_unix_ms': 2000,
    if (!granted)
      'last_failure': {
        'kind': 'permission_denied',
        'observed_at_unix_ms': 2000,
      },
  },
  'views': ready
      ? [
          {
            'schema_version': 1,
            'view_id': 'calendar.timeline',
            'source_handle': 'calendar:test',
            'observed_at_unix_ms': 1000,
            'expires_at_unix_ms': 301000,
            'item_count': 1,
            'byte_count': 256,
            'provenance_count': 1,
          },
        ]
      : <Object?>[],
};

Map<String, dynamic> _appleHealthConnection() => {
  'descriptor': {
    'schema_version': 1,
    'id': 'health.apple',
    'version': '1.0.0',
    'provider': 'apple_health',
    'execution': {'kind': 'device', 'device_id': 'apple-test'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'health.derived.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['HKHealthStore.derived.read'],
        'output_view_id': 'wellbeing.derived',
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'wellbeing.derived',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'derived_only',
        'freshness_ttl_ms': 1800000,
        'max_items': 1,
        'max_bytes': 8192,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'health.apple',
    'state': 'pending',
    'granted_scopes': ['HKHealthStore.derived.read'],
    'observed_at_unix_ms': 2000,
  },
  'views': <Object>[],
};

Map<String, dynamic> _appleFeasibilityConnection() => {
  'descriptor': {
    'schema_version': 1,
    'id': 'feasibility.apple',
    'version': '1.0.0',
    'provider': 'apple_feasibility',
    'execution': {'kind': 'device', 'device_id': 'apple-test'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'schedule.feasibility.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['CLLocationManager.whenInUse'],
        'output_view_id': 'schedule.feasibility',
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'schedule.feasibility',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'ephemeral',
        'freshness_ttl_ms': 300000,
        'max_items': 1,
        'max_bytes': 16384,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'feasibility.apple',
    'state': 'pending',
    'granted_scopes': ['CLLocationManager.whenInUse'],
    'observed_at_unix_ms': 2000,
  },
  'views': <Object>[],
};

final class _NoopLocalContextTransport implements LocalContextTransport {
  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Unexpected context transport call: ${invocation.memberName}',
  );
}

final class _SettingsServerClient extends LocalServerClient {
  _SettingsServerClient(
    ServerCredentialStore store, {
    this.connectionSnapshots = const [],
  }) : super(store: store);

  final List<Map<String, dynamic>> connectionSnapshots;

  @override
  Future<List<Map<String, dynamic>>> connections(
    ServerConnection connection,
  ) async => connectionSnapshots;

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
