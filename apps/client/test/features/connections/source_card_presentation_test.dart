import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/connections/application/calendar_system_access_gateway.dart';
import 'package:floe_client/features/connections/application/connections_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';
import 'package:floe_client/features/connections/presentation/service_presentation.dart';
import 'package:floe_client/l10n/app_localizations.dart';

const _sourceRef = '2f187d11-3da3-4316-b28a-c99fcdd92d1f';
const _sourceName = 'Synthetic Work account';
const _genericSourceDescription =
    'Manage the resources and access reviewed for this source.';

const _availabilityLabels = <String, String>{
  'available': 'Connected',
  'permission_required': 'Finish setup',
  'identity_changed': 'Reconnect needed',
  'disconnected': 'Disconnected',
  'unavailable': 'Unavailable',
  'future_owner_state': 'Unavailable',
};

void main() {
  setUpAll(() async {
    final manifest = jsonDecode(
      await rootBundle.loadString('FontManifest.json'),
    ) as List<dynamic>;
    for (final value in manifest) {
      final entry = value as Map<String, dynamic>;
      final loader = FontLoader(entry['family'] as String);
      for (final font in entry['fonts'] as List<dynamic>) {
        loader.addFont(
          rootBundle.load((font as Map<String, dynamic>)['asset'] as String),
        );
      }
      await loader.load();
    }
  });

  test('source availability labels cover known states and fallback', () {
    for (final entry in _availabilityLabels.entries) {
      expect(
        ServicePresentation.sourceAvailabilityLabel(entry.key),
        entry.value,
      );
    }
  });

  for (final entry in _availabilityLabels.entries) {
    testWidgets(
      'source list and detail both show ${entry.value} for ${entry.key}',
      (tester) async {
        final fixture = await _fixture(availability: entry.key);
        addTearDown(() {
          fixture.controller.dispose();
          fixture.runtime.dispose();
        });
        _setViewport(tester, width: 388, height: 760);

        await tester.pumpWidget(_app(fixture.controller));
        await _pumpCurrent(tester);
        expect(find.text(entry.value), findsOneWidget);
        expect(find.text(_sourceName), findsOneWidget);

        await tester.tap(find.text(_sourceName));
        await _pumpCurrent(tester);
        expect(find.text(entry.value), findsOneWidget);
        expect(find.text(_sourceName), findsOneWidget);
        expect(find.text('Launch planning calendar'), findsOneWidget);
        expect(find.text('Use with Floe'), findsOneWidget);
        expect(fixture.gateway.unexpectedCalls, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'source list omits boilerplate but keeps source and service descriptions',
    (tester) async {
      final fixture = await _fixture(availability: 'identity_changed');
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
      });
      _setViewport(tester, width: 388, height: 760);

      await tester.pumpWidget(_app(fixture.controller));
      await _pumpCurrent(tester);

      expect(find.text(_sourceName), findsOneWidget);
      expect(find.text(_genericSourceDescription), findsNothing);
      expect(
        find.text(
          'Use deterministic synthetic Calendar data for Linux QA. '
          'No real calendar is accessed or changed.',
        ),
        findsOneWidget,
      );

      await tester.tap(find.text(_sourceName));
      await _pumpCurrent(tester);
      expect(find.text(_sourceName), findsOneWidget);
      expect(find.text(_genericSourceDescription), findsOneWidget);
      expect(find.text('Synthetic Work · 1'), findsOneWidget);
      expect(find.text('Launch planning calendar'), findsOneWidget);
      expect(find.text('Choose resources'), findsOneWidget);
      expect(find.text('Disconnect'), findsOneWidget);
      expect(fixture.gateway.unexpectedCalls, isEmpty);
    },
  );

  for (final width in [360.0, 900.0]) {
    for (final textScale in [1.0, 2.0]) {
      testWidgets(
        'source list/detail lays out at ${width.toInt()}px and ${textScale}x text',
        (tester) async {
          final fixture = await _fixture(availability: 'permission_required');
          addTearDown(() {
            fixture.controller.dispose();
            fixture.runtime.dispose();
          });
          _setViewport(tester, width: width, height: 820);

          await tester.pumpWidget(
            _app(fixture.controller, textScale: textScale),
          );
          await _pumpCurrent(tester);
          await tester.ensureVisible(find.text(_sourceName));
          await tester.tap(find.text(_sourceName));
          await _pumpCurrent(tester);

          expect(find.text('Finish setup'), findsOneWidget);
          expect(find.text(_sourceName), findsOneWidget);
          expect(find.text('Launch planning calendar'), findsOneWidget);
          expect(find.text('Choose resources'), findsOneWidget);
          expect(find.text('Disconnect'), findsOneWidget);
          expect(fixture.gateway.unexpectedCalls, isEmpty);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }
}

Widget _app(ConnectionsController controller, {double textScale = 1}) =>
    MaterialApp(
      theme: FloeTheme.light,
      locale: const Locale('en'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: Scaffold(
        body: SingleChildScrollView(
          child: ConnectorScreen(controller: controller),
        ),
      ),
    );

void _setViewport(
  WidgetTester tester, {
  required double width,
  required double height,
}) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _pumpCurrent(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 250));
}

Future<_Fixture> _fixture({required String availability}) async {
  final group = ResourceGroup(
    groupRef: ResourceGroupRef('bf757f49-49a0-4ed9-8daa-2e886f86eb7b'),
    label: 'Synthetic Work',
  );
  final source = SourceSummary(
    sourceRef: SourceRef(_sourceRef),
    revision: 1,
    displayLabels: const [_sourceName],
    availability: availability,
    selectedResources: [
      ResourceSummary(
        resourceRef: ResourceRef('be8ff17d-4f50-4a1f-bbe2-a5a2ee9d081a'),
        label: 'Launch planning calendar',
        group: group,
      ),
    ],
    observeState: 'disabled',
    allowedActions: const {'configure', 'disconnect'},
  );
  final integration = IntegrationSummary(
    serviceKind: 'synthetic_qa_calendar',
    integrationRef: IntegrationRef('1ddbd2e1-42e2-4a5c-9201-58dc8f094f87'),
    revision: 1,
    displayName: 'Synthetic QA Calendar',
    category: 'calendar',
    state: 'available',
    capabilities: const {},
  );
  final runtime = RuntimeController(
    gateway: _ReadyRuntimeGateway(),
    personId: 'synthetic-preview-person',
  );
  await runtime.open();
  final gateway = _PreviewConnectionsGateway(
    ConnectionsOverview(
      gateways: const [],
      integrations: [integration],
      sources: [source],
    ),
  );
  final controller = ConnectionsController(
    gateway,
    runtime: runtime,
    calendarSystemAccess: _UnusedCalendarSystemAccess(),
  );
  return _Fixture(controller, runtime, gateway);
}

final class _Fixture {
  const _Fixture(this.controller, this.runtime, this.gateway);
  final ConnectionsController controller;
  final RuntimeController runtime;
  final _PreviewConnectionsGateway gateway;
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected synthetic Runtime call.');
}

final class _PreviewConnectionsGateway implements ConnectionsGateway {
  _PreviewConnectionsGateway(this.value);
  final ConnectionsOverview value;
  final unexpectedCalls = <String>[];

  @override
  Future<ConnectionsOverview> overview() async => value;

  @override
  dynamic noSuchMethod(Invocation invocation) {
    unexpectedCalls.add(invocation.memberName.toString());
    throw StateError('Unexpected synthetic connection operation.');
  }
}

final class _UnusedCalendarSystemAccess implements CalendarSystemAccessGateway {
  @override
  Future<CalendarSystemAccess> inspect() async =>
      CalendarSystemAccess.notRequested;

  @override
  Future<void> openSettings() async {
    throw StateError('This synthetic test must not open system settings.');
  }
}
