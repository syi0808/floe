import 'dart:async';

import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_switch.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_calendar_expert_dialog.dart';
import 'package:floe_client/features/agent/agent_calendar_sources.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/application/day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/presentation/personal_day_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_calendar_experts.dart';
import 'support/agent_registry.dart';

AgentCalendarSources sources({
  List<String> ids = const ['home', 'work'],
  int revision = 2,
  String person = registryPerson,
}) => AgentCalendarSources(
  personId: person,
  connection: CalendarConnection(
    id: 'connection',
    name: 'Apple Calendar',
    provider: 'event_kit',
    revision: revision,
    includeAll: true,
    calendars: ids
        .map(
          (identifier) =>
              ConnectedCalendar(id: identifier, name: 'Personal · $identifier'),
        )
        .toList(),
  ),
);

Widget app(
  AgentController controller,
  ValueNotifier<AgentCalendarSources?> source,
  double scale,
) => MaterialApp(
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
      child: SettingsScreen(
        client: null,
        agentController: controller,
        calendarSources: () => source.value,
        calendarSourceChanges: source,
      ),
    ),
  ),
);

Future<void> tapKey(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.tap(target);
  await tester.pumpAndSettle();
}

Future<
  (
    AgentController,
    TestCalendarExpertGateway,
    ValueNotifier<AgentCalendarSources?>,
  )
>
open(WidgetTester tester, {double scale = 1}) async {
  final gateway = TestCalendarExpertGateway();
  final controller = AgentController(
    gateway: gateway,
    personId: registryPerson,
  );
  final source = ValueNotifier<AgentCalendarSources?>(sources());
  addTearDown(controller.dispose);
  addTearDown(source.dispose);
  await controller.load();
  await tester.pumpWidget(app(controller, source, scale));
  await tester.ensureVisible(find.text('Manage assistant access'));
  await tester.tap(find.text('Manage assistant access'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Calendar access'));
  await tester.pumpAndSettle();
  expect(find.byType(AgentCalendarExpertDialog), findsOneWidget);
  expect(gateway.transport.installations, 0);
  return (controller, gateway, source);
}

void main() {
  test('source projection uses exact selected IDs and handles legacy metadata without widening', () {
    final projected = AgentCalendarSources(
      personId: registryPerson,
      connection: const CalendarConnection(
        id: 'home',
        name: 'Personal home',
        provider: 'event_kit',
        revision: 2,
        calendarIds: ['home', 'work'],
        includeAll: true,
      ),
    );
    expect(projected.calendars.map((entry) => entry.id), ['home', 'work']);
    expect(projected.calendars.map((entry) => entry.name), [
      'Personal home',
      'work',
    ]);
    expect(projected.containsScope('event_kit', ['new']), false);
    expect(projected.containsScope('fixture', ['home']), false);
    expect(() => projected.calendars.clear(), throwsUnsupportedError);
    expect(sources(ids: ['home', 'home']).usable, false);
    final partial = AgentCalendarSources(
      personId: registryPerson,
      connection: const CalendarConnection(
        id: 'connection',
        name: 'Calendar',
        provider: 'event_kit',
        revision: 3,
        error: 'partial_import',
        calendars: [
          ConnectedCalendar(id: 'home', name: 'Home'),
          ConnectedCalendar(id: 'work', name: 'Work', error: 'unavailable'),
        ],
      ),
    );
    expect(partial.calendars.first.error, isNull);
    expect(partial.calendars.last.error, 'unavailable');
  });
  setUpAll(() async {
    await (FontLoader('Pretendard')
          ..addFont(rootBundle.load('assets/fonts/Pretendard-Regular.otf'))
          ..addFont(rootBundle.load('assets/fonts/Pretendard-SemiBold.otf')))
        .load();
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
      'exact consent installs disabled scope and remains readable at $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 1400);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final (controller, gateway, _) = await open(
          tester,
          scale: width == 320 ? 2 : 1,
        );
        expect(
          tester
              .widget<FloeButton>(
                find.byKey(const ValueKey('calendar-setup-install')),
              )
              .onPressed,
          isNull,
        );
        await tapKey(tester, 'calendar-choice-home');
        expect(
          tester
              .widget<FloeButton>(
                find.byKey(const ValueKey('calendar-setup-install')),
              )
              .onPressed,
          isNull,
        );
        await tapKey(tester, 'calendar-setup-consent');
        if (width == 520) {
          await expectLater(
            find.byType(AgentCalendarExpertDialog),
            matchesGoldenFile('goldens/agent_calendar_consent.png'),
          );
        }
        await tapKey(tester, 'calendar-setup-install');
        expect(gateway.transport.installations, 1);
        expect(gateway.requests.single.calendarIds, ['home']);
        expect(controller.calendarExperts!.views.single.enabled, false);
        expect(
          controller.calendarExperts!.registry.installations.every(
            (entry) => !entry.enabled,
          ),
          true,
        );
        expect(
          controller.calendarExperts!.registry.assignments.every(
            (entry) => !entry.enabled,
          ),
          true,
        );
        await tapKey(tester, 'calendar-scope-$calendarViewId');
        expect(controller.calendarExperts!.views.single.enabled, true);
        expect(tester.takeException(), isNull);
      },
    );
  }

  for (final width in [390.0, 1280.0]) {
    testWidgets(
      'Today forwards connected selection to the consent surface at $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 1100);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final gateway = TestCalendarExpertGateway();
        final date = DateTime(2026, 9, 7, 9);
        await tester.pumpWidget(
          MaterialApp(
            theme: FloeTheme.light,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: PersonalDayScreen(
              gateway: _ConnectedDayGateway(),
              agentGateway: gateway,
              query: DayQuery(
                personId: registryPerson,
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        await tester.ensureVisible(find.byTooltip('Settings'));
        await tester.tap(find.byTooltip('Settings'));
        await tester.pumpAndSettle();
        await tester.ensureVisible(find.text('Manage assistant access'));
        await tester.tap(find.text('Manage assistant access'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Calendar access'));
        await tester.pumpAndSettle();
        expect(find.text('Connected test Calendar'), findsOneWidget);
        expect(
          find.byKey(const ValueKey('calendar-choice-home')),
          findsOneWidget,
        );
        expect(gateway.transport.installations, 0);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'selection is bounded and source change invalidates consent without expanding saved scopes',
    (tester) async {
      final (controller, gateway, source) = await open(tester);
      source.value = sources(ids: ['home', 'work', 'third', 'fourth', 'fifth']);
      await tester.pumpAndSettle();
      for (final identifier in ['home', 'work', 'third', 'fourth']) {
        await tapKey(tester, 'calendar-choice-$identifier');
      }
      expect(
        tester
            .widget<CheckboxListTile>(
              find.byKey(const ValueKey('calendar-choice-fifth')),
            )
            .onChanged,
        isNull,
      );
      await tapKey(tester, 'calendar-setup-consent');
      source.value = sources(
        ids: ['home', 'work', 'third', 'fourth', 'fifth', 'new'],
        revision: 3,
      );
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<CheckboxListTile>(
              find.byKey(const ValueKey('calendar-setup-consent')),
            )
            .value,
        false,
      );
      expect(find.text('0 of 4 selected'), findsOneWidget);
      expect(gateway.transport.installations, 0);
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-setup-consent');
      await tapKey(tester, 'calendar-setup-install');
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-setup-consent');
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(const ValueKey('calendar-setup-install')),
            )
            .onPressed,
        isNull,
      );
      expect(controller.calendarExperts!.views.single.calendarIds, ['home']);
      expect(gateway.transport.installations, 1);
    },
  );

  testWidgets(
    'missing or foreign connection blocks new grants but existing access can be revoked',
    (tester) async {
      final (controller, _, source) = await open(tester);
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-setup-consent');
      await tapKey(tester, 'calendar-setup-install');
      await tapKey(tester, 'calendar-scope-$calendarViewId');
      source.value = sources(person: registryInstance);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('calendar-choice-home')), findsNothing);
      await tapKey(tester, 'calendar-scope-$calendarViewId');
      expect(controller.calendarExperts!.views.single.enabled, false);
      expect(
        tester
            .widget<FloeSwitch>(
              find.byKey(const ValueKey('calendar-scope-$calendarViewId')),
            )
            .onChanged,
        isNull,
      );
      source.value = null;
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('calendar-setup-install')),
        findsNothing,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a legacy binding without a setup receipt does not masquerade as an installed Expert',
    (tester) async {
      final (controller, gateway, _) = await open(tester);
      gateway.transport.snapshot = calendarExpertsFixture();
      (gateway.transport.snapshot['setups'] as List).clear();
      await controller.loadCalendarExperts();
      await tester.pumpAndSettle();
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-choice-work');
      await tapKey(tester, 'calendar-setup-consent');
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(const ValueKey('calendar-setup-install')),
            )
            .onPressed,
        isNotNull,
      );
      expect(gateway.transport.installations, 0);
    },
  );

  testWidgets(
    'uncertain installation offers refresh without new setup or implicit retry',
    (tester) async {
      final (controller, gateway, source) = await open(tester);
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-setup-consent');
      gateway.transport.loss = 'submit';
      await tapKey(tester, 'calendar-setup-install');
      expect(controller.pendingCalendarSetup, isNotNull);
      expect(
        find.byKey(const ValueKey('calendar-setup-install')),
        findsNothing,
      );
      expect(
        find.byKey(const ValueKey('calendar-setup-retry')),
        findsOneWidget,
      );
      source.value = sources(ids: ['work'], revision: 3);
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(const ValueKey('calendar-setup-retry')),
            )
            .onPressed,
        isNull,
      );
      await tapKey(tester, 'calendar-setup-refresh');
      expect(controller.pendingCalendarSetup, isNull);
      expect(gateway.transport.installations, 1);
      expect(gateway.transport.submissions, 1);
      expect(controller.calendarExperts!.views.single.calendarIds, ['home']);
    },
  );

  testWidgets(
    'locking during installation immediately removes scopes and ignores late UI completion',
    (tester) async {
      final (controller, gateway, _) = await open(tester);
      await tapKey(tester, 'calendar-choice-home');
      await tapKey(tester, 'calendar-setup-consent');
      gateway.gate = Completer<void>();
      final button = find.byKey(const ValueKey('calendar-setup-install'));
      await tester.ensureVisible(button);
      await tester.tap(button);
      await tester.pump();
      final closing = controller.closeView();
      await tester.pump();
      expect(find.text('home'), findsNothing);
      expect(find.text('Personal · home'), findsNothing);
      expect(find.byKey(const ValueKey('calendar-setup-retry')), findsNothing);
      gateway.gate!.complete();
      await closing;
      await tester.pumpAndSettle();
      expect(controller.calendarExperts, isNull);
      expect(
        find.byKey(const ValueKey('calendar-scope-$calendarViewId')),
        findsNothing,
      );
      expect(tester.takeException(), isNull);
    },
  );
}

final class _ConnectedDayGateway implements DayGateway {
  final _base = FakeDayGateway();

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async => DaySnapshot(
    personId: query.personId,
    date: query.date,
    generatedAt: query.now,
    timezoneOffsetSeconds: query.timezoneOffsetSeconds,
    items: const [],
    calendar: const CalendarConnection(
      id: 'home',
      name: 'Connected test Calendar',
      provider: 'event_kit',
      revision: 2,
    ),
  );

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) =>
      _base.submitCapture(input, query);
  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) => _base.classifyCapture(capture, classification, query);
  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) => _base.setTaskCompleted(task, completed, query);
  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) =>
      _base.deleteItem(item, query);
}
