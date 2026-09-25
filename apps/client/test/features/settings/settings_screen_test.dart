import 'dart:convert';
import 'dart:io';

import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../actions/calendar_action_execution_test.dart' show Executor;
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
    expect(find.text('AI processing'), findsNothing);
    expect(
      find.byKey(const ValueKey('connections-privacy-navigation')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('android-data-sources')), findsNothing);
    expect(find.text('Schedule planning'), findsNothing);
  });

  testWidgets('Calendar consumer grants are absent from Data & privacy', (
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
    expect(find.text('Data Floe can use'), findsNothing);
    expect(find.text('AI processing'), findsNothing);
  });

  testWidgets('Android source editors move out of Data & privacy', (
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
        home: Scaffold(
          body: SettingsScreen(
            client: null,
            agentController: controller,
            androidContext: androidContext,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('android-data-sources')), findsNothing);
    expect(find.byKey(const ValueKey('android-calendar-allow')), findsNothing);
    expect(find.byKey(const ValueKey('android-health-refresh')), findsNothing);
    expect(androidContext.permissionRequests, 0);
    expect(androidContext.calendarPermissionRequests, 0);
  });

  testWidgets('Apple access controls move out of Data & privacy', (
    tester,
  ) async {
    final appleContext = _AppleContext();
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
            child: SettingsScreen(
              client: null,
              agentController: controller,
              appleContext: appleContext,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Wellbeing access'), findsNothing);
    expect(find.text('Apple Contacts'), findsNothing);
    expect(find.text('Open Connections'), findsNothing);
    expect(appleContext.connectionsCalls, 0);
    expect(appleContext.feasibilityReads, 0);
    expect(appleContext.wellbeingReads, 0);
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
    expect(find.byKey(const ValueKey('connections-privacy-navigation')), findsOneWidget);
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

  testWidgets('external model approval has no blanket Settings editor', (
    tester,
  ) async {
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
        home: Scaffold(
          body: SettingsScreen(client: client, agentController: controller),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Allow external model providers'), findsNothing);
    expect(find.byKey(const ValueKey('external-model-consent')), findsNothing);
    expect(
      find.byKey(const ValueKey('connections-privacy-navigation')),
      findsOneWidget,
    );
    expect((await client.connection())!.toJson(), isNot(contains('allow_external')));
  });

  testWidgets('server connection inventory stays out of Data & privacy', (
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

    expect(find.text('Gmail'), findsNothing);
    expect(find.text('Ready'), findsNothing);
    expect(find.text('Open Connections'), findsNothing);
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
  int connectionsCalls = 0;
  int feasibilityReads = 0;
  int wellbeingReads = 0;
  int feasibilityPermissionRequests = 0;
  bool exposeHealthConnection = false;
  int wellbeingPermissionRequests = 0;

  @override
  Future<List<Map<String, dynamic>>> connections() async {
    connectionsCalls += 1;
    return [
      if (exposeHealthConnection) _appleHealthConnection(),
      if (!exposeHealthConnection) _appleFeasibilityConnection(),
    ];
  }

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async {
    this.query = query;
    feasibilityReads += 1;
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
  Future<Map<String, dynamic>> readWellbeing() async {
    wellbeingReads += 1;
    return {};
  }

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async => {};
}

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
