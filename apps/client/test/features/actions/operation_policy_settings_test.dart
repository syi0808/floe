import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';

const _explanation =
    'Choose whether Floe can create events suggested by Experts. Calendar permissions still apply; direct changes use your current permissions.';

void main() {
  setUpAll(_loadBundledFonts);

  for (final (initial, next, label) in [
    (ActionAuthorityMode.ask, ActionAuthorityMode.allow, 'Allow automatically'),
    (ActionAuthorityMode.allow, ActionAuthorityMode.ask, 'Ask every time'),
    (ActionAuthorityMode.ask, ActionAuthorityMode.deny, 'Do not allow'),
    (ActionAuthorityMode.ask, ActionAuthorityMode.ask, 'Ask every time'),
  ]) {
    testWidgets(
      'selecting $label from ${initial.name} submits and reconciles confirmed authority',
      (tester) async {
        _setViewport(tester, const Size(1280, 720));
        final fixture = await _fixture(initial: initial);
        addTearDown(fixture.dispose);
        await _mount(tester, fixture);

        _expectSingleSelect(tester, initial, enabled: true);
        if (next == ActionAuthorityMode.allow) {
          await expectLater(
            find.byType(SettingsScreen),
            matchesGoldenFile('goldens/v026-policy-desktop.png'),
          );
        }
        await _choose(tester, label);
        await _pumpFrames(tester);

        expect(fixture.gateway.calls, hasLength(1));
        expect(fixture.gateway.calls.single.mode, next);
        expect(fixture.gateway.calls.single.expectedRevision, 1);
        _expectSingleSelect(tester, next, enabled: true);
        expect(find.text('Allow Calendar event creation'), findsNothing);
        expect(find.text('Customize permissions'), findsNothing);
        expect(find.text(_explanation), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets('unknown authority stays unavailable until the owner loads it', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 900));
    final load = Completer<ActionAuthority>();
    final fixture = await _fixture(loadResult: load.future);
    addTearDown(fixture.dispose);
    await _mount(tester, fixture, textScale: 2);

    expect(
      find.text('Load Access to view the Expert Calendar policy.'),
      findsOneWidget,
    );
    expect(find.byType(FloeSelect<ActionAuthorityMode>), findsNothing);
    expect(find.text('Allow automatically'), findsNothing);
    expect(find.text(_explanation), findsOneWidget);

    load.complete(
      const ActionAuthority(
        revision: 1,
        calendarCreate: ActionAuthorityMode.ask,
      ),
    );
    await _pumpFrames(tester);
    _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: true);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'selection stays at confirmed value while busy and after failure',
    (tester) async {
      _setViewport(tester, const Size(390, 900));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);
      final result = Completer<ActionAuthority>();
      fixture.gateway.onSet = (_, _, _) => result.future;
      await _mount(tester, fixture, textScale: 2);

      await _openAndChoose(tester, 'Do not allow');
      await tester.pump();
      expect(fixture.gateway.calls, hasLength(1));
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: false);

      result.completeError(
        const AppRuntimeException(
          'conflict',
          'Synthetic settled failure',
          commandOutcome: CommandOutcome.notApplied,
        ),
      );
      await _pumpFrames(tester);
      expect(fixture.controller.hasPendingChange, isFalse);
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: false);
      expect(find.text('Reload Access policy'), findsOneWidget);
      expect(fixture.gateway.calls, hasLength(1));

      await tester.tap(find.text('Reload Access policy'));
      await _pumpFrames(tester);
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: true);
      expect(fixture.gateway.calls, hasLength(1));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'owner response with the original mode keeps the rendered value confirmed',
    (tester) async {
      _setViewport(tester, const Size(390, 900));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);
      fixture.gateway.onSet = (_, _, _) =>
          Future.value(fixture.gateway.authority);
      await _mount(tester, fixture, textScale: 2);

      await _openAndChoose(tester, 'Do not allow');
      await _pumpFrames(tester);

      expect(
        fixture.controller.authority?.calendarCreate,
        ActionAuthorityMode.ask,
      );
      expect(fixture.controller.hasPendingChange, isTrue);
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: false);
      expect(find.text('Reload Access policy'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'narrow 200% layout keeps the full scope explanation before the select',
    (tester) async {
      _setViewport(tester, const Size(390, 900));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);
      await _mount(tester, fixture, textScale: 2);

      final explanation = tester.getRect(find.text(_explanation));
      final select = tester.getRect(
        find.byKey(const ValueKey('floe-selection-trigger')),
      );
      expect(explanation.bottom, lessThan(select.top));
      expect(_textScale(tester), 2);
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: true);
      _expectNoPresetRadios(tester);
      expect(tester.takeException(), isNull);
      await expectLater(
        find.byType(SettingsScreen),
        matchesGoldenFile('goldens/v026-policy-narrow-200.png'),
      );
    },
  );

  testWidgets('narrow 200% selector keeps every choice reachable', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 560));
    final fixture = await _fixture();
    addTearDown(fixture.dispose);
    await _mount(tester, fixture, textScale: 2);

    final trigger = find.byKey(const ValueKey('floe-selection-trigger'));
    await tester.ensureVisible(trigger);
    await tester.tap(trigger);
    await tester.pumpAndSettle();

    final popup = find.byKey(const ValueKey('floe-selection-popup'));
    final viewport = tester.getRect(popup);
    expect(viewport.top, greaterThanOrEqualTo(0));
    expect(viewport.bottom, lessThanOrEqualTo(560));
    final labels = ['Allow automatically', 'Ask every time', 'Do not allow'];
    for (final label in labels) {
      expect(
        find.descendant(of: popup, matching: find.text(label)),
        findsOneWidget,
      );
    }
    await expectLater(
      find.byKey(const ValueKey('policy-preview-boundary')),
      matchesGoldenFile('goldens/v026-policy-menu-narrow-200.png'),
    );

    final lastChoice = find.descendant(
      of: popup,
      matching: find.text('Do not allow'),
    );
    await tester.ensureVisible(lastChoice);
    await tester.pumpAndSettle();
    final lastBounds = tester.getRect(lastChoice);
    final popupBounds = tester.getRect(popup);
    expect(lastBounds.top, greaterThanOrEqualTo(popupBounds.top));
    expect(lastBounds.bottom, lessThanOrEqualTo(popupBounds.bottom));
    await tester.tap(lastChoice);
    await _pumpFrames(tester);
    expect(fixture.gateway.calls.single.mode, ActionAuthorityMode.deny);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'indeterminate change reloads without settling, exact retry succeeds',
    (tester) async {
      _setViewport(tester, const Size(390, 900));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);
      var submissions = 0;
      fixture.gateway.onSet = (id, mode, revision) async {
        submissions++;
        if (submissions == 1) {
          throw const AppRuntimeException(
            'timeout',
            'Synthetic lost acknowledgement',
            commandOutcome: CommandOutcome.indeterminate,
          );
        }
        return ActionAuthority(revision: revision + 1, calendarCreate: mode);
      };
      await _mount(tester, fixture, textScale: 2);

      await _openAndChoose(tester, 'Do not allow');
      await _pumpFrames(tester);
      expect(fixture.controller.hasPendingChange, isTrue);
      _expectSingleSelect(tester, ActionAuthorityMode.ask, enabled: false);
      expect(find.text('Retry pending policy change'), findsOneWidget);
      expect(
        find.text(
          'A previous policy change has no confirmed result. Reloading reads the current value; it does not resolve that change.',
        ),
        findsOneWidget,
      );
      final first = fixture.gateway.calls.single;
      await tester.ensureVisible(find.text('Retry pending policy change'));
      await _pumpFrames(tester);
      await expectLater(
        find.byType(SettingsScreen),
        matchesGoldenFile('goldens/v026-policy-recovery-narrow-200.png'),
      );

      await tester.tap(find.text('Reload Access policy'));
      await _pumpFrames(tester);
      expect(fixture.gateway.calls, hasLength(1));
      expect(fixture.controller.hasPendingChange, isTrue);

      await tester.ensureVisible(find.text('Retry pending policy change'));
      await tester.tap(find.text('Retry pending policy change'));
      await _pumpFrames(tester);
      expect(fixture.gateway.calls, hasLength(2));
      expect(fixture.gateway.calls[1], first);
      expect(fixture.controller.hasPendingChange, isFalse);
      _expectSingleSelect(tester, ActionAuthorityMode.deny, enabled: true);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'selection semantics expose label, confirmed value and selected option',
    (tester) async {
      _setViewport(tester, const Size(900, 760));
      final fixture = await _fixture(initial: ActionAuthorityMode.allow);
      addTearDown(fixture.dispose);
      final handle = tester.ensureSemantics();
      await _mount(tester, fixture);

      final trigger = tester.getSemantics(_selectionSemantics(tester));
      final data = trigger.getSemanticsData();
      expect(data.label, contains('Create Calendar events'));
      expect(data.value, contains('Allow automatically'));
      expect(data.flagsCollection.isEnabled, isNot(ui.Tristate.isFalse));
      expect(find.byType(FloeRadioGroup<ActionAuthorityMode>), findsNothing);

      await tester.tap(find.byKey(const ValueKey('floe-selection-trigger')));
      await tester.pumpAndSettle();
      final selectedOption = find.ancestor(
        of: find.text('Allow automatically').last,
        matching: find.byWidgetPredicate(
          (widget) => widget is Semantics && widget.properties.selected == true,
        ),
      );
      expect(selectedOption, findsOneWidget);
      expect(
        tester
            .getSemantics(selectedOption)
            .getSemanticsData()
            .flagsCollection
            .isSelected,
        ui.Tristate.isTrue,
      );
      expect(tester.takeException(), isNull);
      handle.dispose();
    },
  );

  testWidgets('ordinary FloeSelect still stages its form value locally', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 700));
    final selected = <String?>[];
    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: FloeTheme.light,
        home: Scaffold(
          body: FloeSelect<String>(
            label: 'Example selection',
            value: 'first',
            options: const [
              FloeSelectOption(value: 'first', label: 'First choice'),
              FloeSelectOption(value: 'second', label: 'Second choice'),
            ],
            onChanged: selected.add,
          ),
        ),
      ),
    );

    await tester.tap(find.byKey(const ValueKey('floe-selection-trigger')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Second choice').last);
    await tester.pumpAndSettle();

    expect(selected, ['second']);
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('floe-selection-trigger')),
        matching: find.text('Second choice'),
      ),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}

Future<void> _loadBundledFonts() async {
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
}

void _setViewport(WidgetTester tester, Size size) {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _mount(
  WidgetTester tester,
  _Fixture fixture, {
  double textScale = 1,
}) async {
  await tester.pumpWidget(
    RepaintBoundary(
      key: const ValueKey('policy-preview-boundary'),
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context)
              .copyWith(textScaler: TextScaler.linear(textScale)),
          child: child!,
        ),
        home: Scaffold(
          body: SettingsScreen(
            connectionsController: null,
            operationPolicyController: fixture.controller,
          ),
        ),
      ),
    ),
  );
  await tester.pump();
}

Future<void> _openAndChoose(WidgetTester tester, String label) async {
  await tester.tap(find.byKey(const ValueKey('floe-selection-trigger')));
  await tester.pumpAndSettle();
  await tester.tap(find.text(label).last);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 240));
}

Future<void> _choose(WidgetTester tester, String label) async {
  await _openAndChoose(tester, label);
}

Future<void> _pumpFrames(WidgetTester tester) async {
  for (var frame = 0; frame < 6; frame++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

double _textScale(WidgetTester tester) =>
    tester
        .element(find.byType(SettingsScreen))
        .findAncestorWidgetOfExactType<MediaQuery>()!
        .data
        .textScaler
        .scale(10) /
    10;

void _expectSingleSelect(
  WidgetTester tester,
  ActionAuthorityMode mode, {
  required bool enabled,
}) {
  expect(find.byType(FloeSelect<ActionAuthorityMode>), findsOneWidget);
  _expectNoPresetRadios(tester);
  final triggerSemantics = tester.getSemantics(_selectionSemantics(tester));
  final data = triggerSemantics.getSemanticsData();
  expect(data.label, contains('Create Calendar events'));
  expect(data.value, contains(_labelFor(mode)));
  final trigger = find.byKey(const ValueKey('floe-selection-trigger'));
  expect(
    find.descendant(of: trigger, matching: find.text(_labelFor(mode))),
    findsOneWidget,
  );
  for (final other in ActionAuthorityMode.values.where(
    (value) => value != mode,
  )) {
    expect(
      find.descendant(of: trigger, matching: find.text(_labelFor(other))),
      findsNothing,
    );
  }
  expect(
    data.flagsCollection.isEnabled,
    enabled ? isNot(ui.Tristate.isFalse) : ui.Tristate.isFalse,
  );
}

void _expectNoPresetRadios(WidgetTester tester) {
  expect(
    find.byWidgetPredicate(
      (widget) => widget is FloeRadioGroup || widget is FloeRadioTile,
    ),
    findsNothing,
  );
}

Finder _selectionSemantics(WidgetTester tester) => find.ancestor(
  of: find.byKey(const ValueKey('floe-selection-trigger')),
  matching: find.byWidgetPredicate(
    (widget) =>
        widget is Semantics &&
        widget.properties.label == 'Create Calendar events',
  ),
);

String _labelFor(ActionAuthorityMode mode) => switch (mode) {
  ActionAuthorityMode.allow => 'Allow automatically',
  ActionAuthorityMode.ask => 'Ask every time',
  ActionAuthorityMode.deny => 'Do not allow',
};

Future<_Fixture> _fixture({
  ActionAuthorityMode initial = ActionAuthorityMode.ask,
  Future<ActionAuthority>? loadResult,
}) async {
  final gateway = _Gateway(initial: initial, loadResult: loadResult);
  final runtime = RuntimeController(gateway: _Runtime(), personId: 'synthetic');
  await runtime.open();
  final controller = OperationPolicyController(
    gateway: gateway,
    runtime: runtime,
  );
  if (loadResult == null) {
    await Future<void>.value();
    expect(controller.loaded, isTrue);
  }
  return _Fixture(gateway: gateway, runtime: runtime, controller: controller);
}

final class _Fixture {
  const _Fixture({
    required this.gateway,
    required this.runtime,
    required this.controller,
  });

  final _Gateway gateway;
  final RuntimeController runtime;
  final OperationPolicyController controller;

  void dispose() {
    controller.dispose();
    runtime.dispose();
  }
}

final class _Gateway implements OperationAuthorizationGateway {
  _Gateway({required ActionAuthorityMode initial, this.loadResult})
    : authority = ActionAuthority(revision: 1, calendarCreate: initial);

  ActionAuthority authority;
  final Future<ActionAuthority>? loadResult;
  int loads = 0;
  final calls =
      <({String id, ActionAuthorityMode mode, int expectedRevision})>[];
  Future<ActionAuthority> Function(String, ActionAuthorityMode, int)? onSet;

  @override
  Future<ActionAuthority> loadAuthority() {
    loads++;
    return loadResult ?? Future.value(authority);
  }

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) {
    calls.add((id: commandId, mode: mode, expectedRevision: expectedRevision));
    if (onSet != null) return onSet!(commandId, mode, expectedRevision);
    if (mode != authority.calendarCreate) {
      authority = ActionAuthority(
        revision: expectedRevision + 1,
        calendarCreate: mode,
      );
    }
    return Future.value(authority);
  }
}

final class _Runtime implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected Runtime operation in policy test.');
}
