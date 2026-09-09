import 'dart:async';

import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_calendar_expert_dialog.dart';
import 'package:floe_client/features/agent/agent_calendar_sources.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/server/settings_screen.dart';
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
) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
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
open(WidgetTester tester) async {
  final gateway = TestCalendarExpertGateway();
  final controller = AgentController(
    gateway: gateway,
    personId: registryPerson,
  );
  final source = ValueNotifier<AgentCalendarSources?>(sources());
  addTearDown(controller.dispose);
  addTearDown(source.dispose);
  await controller.load();
  await tester.pumpWidget(app(controller, source));
  await tester.pumpAndSettle();
  expect(find.byType(AgentCalendarSettings), findsOneWidget);
  return (controller, gateway, source);
}

Future<void> installHome(WidgetTester tester) async {
  await tapKey(tester, 'calendar-access-setup');
  await tapKey(tester, 'calendar-choice-home');
  await tapKey(tester, 'calendar-access-save');
}

void main() {
  test('source projection remains exact and never widens legacy metadata', () {
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
    expect(projected.containsScope('event_kit', ['new']), false);
    expect(projected.containsScope('fixture', ['home']), false);
    expect(() => projected.calendars.clear(), throwsUnsupportedError);
    expect(sources(ids: ['home', 'home']).usable, false);
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
    testWidgets('setup grants one active, readable Calendar scope at $width', (
      tester,
    ) async {
      tester.view.physicalSize = Size(width, 1400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final (controller, gateway, _) = await open(tester);

      expect(find.text('Data & privacy'), findsNWidgets(2));
      expect(
        find.textContaining('Apple Calendar is connected'),
        findsOneWidget,
      );
      await tapKey(tester, 'calendar-access-setup');
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(const ValueKey('calendar-access-save')),
            )
            .onPressed,
        isNull,
      );
      await tapKey(tester, 'calendar-choice-home');
      expect(find.textContaining('Action permissions'), findsWidgets);
      await tapKey(tester, 'calendar-access-save');

      expect(gateway.transport.installations, 1);
      expect(gateway.requests.single.calendarIds, ['home']);
      expect(
        controller.calendarExperts!.accessEnabled(
          controller.calendarExperts!.setups.single,
        ),
        true,
      );
      expect(find.text('Active'), findsOneWidget);
      expect(
        tester.widget<Text>(find.text('Calendars')).style?.fontWeight,
        FontWeight.w500,
      );
      expect(find.text('Personal · home'), findsOneWidget);
      expect(find.text('1 calendar'), findsOneWidget);
      expect(find.textContaining(' — '), findsNothing);
      expect(
        find.ancestor(
          of: find.text('1 calendar'),
          matching: find.byType(FloeBadge),
        ),
        findsOneWidget,
      );
      expect(
        find.ancestor(
          of: find.text('Active'),
          matching: find.byType(FloeBadge),
        ),
        findsOneWidget,
      );
      expect(find.text('Pause'), findsOneWidget);
      await tapKey(tester, 'calendar-access-add');
      expect(find.text('Add Calendar scope'), findsOneWidget);
      expect(find.byType(Dialog), findsOneWidget);
      await tapKey(tester, 'calendar-choice-work');
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(const ValueKey('calendar-access-save')),
            )
            .onPressed,
        isNotNull,
      );
      await tester.tap(find.text('Cancel').last);
      await tester.pumpAndSettle();
      expect(find.text('Add Calendar scope'), findsNothing);
      expect(gateway.transport.installations, 1);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
    'scope changes, pause, resume and removal are aggregate changes',
    (tester) async {
      final (controller, _, _) = await open(tester);
      await installHome(tester);
      var setupId = controller.calendarExperts!.setups.single.setupId;

      await tapKey(tester, 'calendar-access-change-$setupId');
      await tapKey(tester, 'calendar-choice-work');
      await tapKey(tester, 'calendar-access-save');
      expect(controller.calendarExperts!.views.single.calendarIds, [
        'home',
        'work',
      ]);
      setupId = controller.calendarExperts!.setups.single.setupId;

      await tapKey(tester, 'calendar-access-toggle-$setupId');
      expect(find.text('Paused'), findsOneWidget);
      expect(
        controller.calendarExperts!.registry.installations.every(
          (entry) => !entry.enabled,
        ),
        true,
      );
      await tapKey(tester, 'calendar-access-toggle-$setupId');
      expect(find.text('Active'), findsOneWidget);

      await tapKey(tester, 'calendar-access-remove-$setupId');
      await tapKey(tester, 'calendar-access-remove-confirm');
      expect(controller.calendarExperts!.setups, isEmpty);
      expect(controller.calendarExperts!.views, isEmpty);
      expect(controller.calendarExperts!.registry.installations, isEmpty);
      expect(find.text('Set up Calendar access'), findsOneWidget);
    },
  );

  testWidgets('selection is bounded and a source change cancels editing', (
    tester,
  ) async {
    final (_, gateway, source) = await open(tester);
    source.value = sources(ids: ['home', 'work', 'third', 'fourth', 'fifth']);
    await tester.pumpAndSettle();
    await tapKey(tester, 'calendar-access-setup');
    for (final identifier in ['home', 'work', 'third', 'fourth']) {
      await tapKey(tester, 'calendar-choice-$identifier');
    }
    expect(
      tester
          .widget<FloeCheckboxTile>(
            find.byKey(const ValueKey('calendar-choice-fifth')),
          )
          .onChanged,
      isNull,
    );
    source.value = sources(ids: ['home'], revision: 3);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('calendar-choice-home')), findsNothing);
    expect(gateway.transport.installations, 0);
    expect(find.textContaining('connection changed'), findsOneWidget);
  });

  testWidgets(
    'missing source blocks new grants but existing access is removable',
    (tester) async {
      final (controller, _, source) = await open(tester);
      await installHome(tester);
      final setupId = controller.calendarExperts!.setups.single.setupId;
      source.value = null;
      await tester.pumpAndSettle();
      expect(find.text('Needs attention'), findsOneWidget);
      expect(
        tester
            .widget<FloeButton>(
              find.byKey(ValueKey('calendar-access-toggle-$setupId')),
            )
            .onPressed,
        isNotNull,
      );
      await tapKey(tester, 'calendar-access-remove-$setupId');
      await tapKey(tester, 'calendar-access-remove-confirm');
      expect(controller.calendarExperts!.setups, isEmpty);
    },
  );

  testWidgets('uncertain setup exposes only exact retry and reconciliation', (
    tester,
  ) async {
    final (controller, gateway, _) = await open(tester);
    await tapKey(tester, 'calendar-access-setup');
    await tapKey(tester, 'calendar-choice-home');
    gateway.transport.loss = 'submit';
    await tapKey(tester, 'calendar-access-save');
    expect(controller.pendingCalendarSetup, isNotNull);
    expect(find.text('Calendar access needs attention'), findsOneWidget);
    expect(find.byKey(const ValueKey('calendar-access-retry')), findsOneWidget);
    expect(find.byKey(const ValueKey('calendar-access-save')), findsNothing);

    await tapKey(tester, 'calendar-access-retry');
    expect(controller.pendingCalendarSetup, isNull);
    expect(find.text('Active'), findsOneWidget);
    expect(gateway.transport.installations, 1);
  });

  testWidgets(
    'locking during setup removes scope and ignores late completion',
    (tester) async {
      final (controller, gateway, _) = await open(tester);
      await tapKey(tester, 'calendar-access-setup');
      await tapKey(tester, 'calendar-choice-home');
      gateway.gate = Completer<void>();
      final button = find.byKey(const ValueKey('calendar-access-save'));
      await tester.ensureVisible(button);
      await tester.tap(button);
      await tester.pump();
      final closing = controller.closeView();
      await tester.pump();
      expect(find.text('Personal · home'), findsNothing);
      gateway.gate!.complete();
      await closing;
      await tester.pumpAndSettle();
      expect(controller.calendarExperts, isNull);
      expect(tester.takeException(), isNull);
    },
  );
}
