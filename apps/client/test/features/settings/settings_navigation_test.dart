import 'dart:convert';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';

const _personId = 'synthetic-settings-navigation-person';
const _contentScrollKey = ValueKey('settings-content-scroll');

void main() {
  setUpAll(_loadBundledFonts);

  testWidgets(
    'new Settings sections open at their heading; reselect and resize stay stable',
    (tester) async {
      _setViewport(tester, const Size(390, 560));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);

      await _mountSettings(tester, fixture, width: 390, height: 560);
      await _selectSection(tester, 'dataPrivacy');
      final scroll = _contentScroll(tester);
      expect(scroll.position.maxScrollExtent, greaterThan(80));
      scroll.jumpTo(80);
      await tester.pump();
      expect(scroll.offset, 80);

      await tester.tap(_section('assistantFeatures'));
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectHeadingVisible(tester, 'Assistant features');
      await expectLater(
        find.byKey(const ValueKey('settings-screen')),
        matchesGoldenFile('goldens/v022-settings-390x560.png'),
      );
      expect(
        find.text(
          AppLocalizations.of(
            tester.element(find.text('Assistant features').last),
          ).assistantFeatureBoundary,
        ),
        findsOneWidget,
      );
      _expectSectionFocus(tester, 'assistantFeatures');
      expect(_selected(tester, 'assistantFeatures'), isTrue);
      expect(_textScale(tester), 1);

      // A current-page reselect is intentionally a no-op for scroll position.
      scroll.jumpTo(180);
      await tester.pump();
      await tester.tap(_section('assistantFeatures'));
      await tester.pump();
      expect(scroll.offset, 180);
      _expectSectionFocus(tester, 'assistantFeatures');

      // Rapid switches cannot leave a queued reset associated with an older page.
      await tester.tap(_section('dataPrivacy'));
      expect(scroll.offset, 0);
      await tester.pump(const Duration(milliseconds: 40));
      await tester.tap(_section('assistantFeatures'));
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectHeadingVisible(tester, 'Assistant features');

      tester.view.physicalSize = const Size(900, 760);
      await _pumpFrames(tester);
      expect(_selected(tester, 'assistantFeatures'), isTrue);
      _expectHeadingVisible(tester, 'Assistant features');
      tester.view.physicalSize = const Size(390, 560);
      await _pumpFrames(tester);
      expect(_selected(tester, 'assistantFeatures'), isTrue);
      _expectHeadingVisible(tester, 'Assistant features');

      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    '200% text: Memory entry and Back expose their own page context',
    (tester) async {
      _setViewport(tester, const Size(390, 900));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);

      await _mountSettings(
        tester,
        fixture,
        width: 390,
        height: 900,
        textScale: 2,
      );
      await _selectSection(tester, 'dataPrivacy');
      final scroll = _contentScroll(tester);
      expect(_textScale(tester), 2);
      expect(scroll.position.maxScrollExtent, greaterThan(320));
      scroll.jumpTo(320);
      await tester.pump();

      await tester.ensureVisible(find.byKey(const ValueKey('manage-memory')));
      await _pumpFrames(tester);
      final manage = tester.widget<FloeButton>(
        find.byKey(const ValueKey('manage-memory')),
      );
      expect(manage.onPressed, isNotNull);
      await tester.tap(find.byKey(const ValueKey('manage-memory')));
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectSectionFocus(tester, 'dataPrivacy');
      _expectHeadingVisible(tester, 'Memory');
      expect(find.byKey(const ValueKey('memory-back')), findsOneWidget);
      expect(find.text('Saved memories'), findsOneWidget);
      expect(_textScale(tester), 2);
      await expectLater(
        find.byKey(const ValueKey('settings-screen')),
        matchesGoldenFile('goldens/v022-memory-390x900-200.png'),
      );

      expect(scroll.position.maxScrollExtent, greaterThan(200));
      scroll.jumpTo(200);
      await tester.pump();
      // Exercise the same Back callback while Memory is scrolled. A real user
      // reaches the control by scrolling it into view first.
      final back = tester.widget<FloeButton>(
        find.byKey(const ValueKey('memory-back')),
      );
      back.onPressed!();
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectHeadingVisible(tester, 'Data & privacy');
      expect(
        find.text(
          'Open Connections to connect sources, choose resources, or change Use with Floe.',
        ),
        findsOneWidget,
      );
      expect(_textScale(tester), 2);
      _expectSectionFocus(tester, 'dataPrivacy');
      await expectLater(
        find.byKey(const ValueKey('settings-screen')),
        matchesGoldenFile('goldens/v022-settings-390x900-200.png'),
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'external assistant review target and controller replacement start at context',
    (tester) async {
      _setViewport(tester, const Size(390, 560));
      final fixture = await _fixture();
      addTearDown(fixture.dispose);
      await _mountSettings(tester, fixture, width: 390, height: 560);
      await _selectSection(tester, 'dataPrivacy');
      final scroll = _contentScroll(tester);
      scroll.jumpTo(math.min(100, scroll.position.maxScrollExtent));
      await tester.pump();
      expect(scroll.offset, greaterThan(0));

      final firstTarget = _assistantTarget(
        '00000000-0000-0000-0000-0000000000ff',
      );
      await _mountSettings(
        tester,
        fixture,
        width: 390,
        height: 560,
        target: firstTarget,
      );
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectHeadingVisible(tester, 'Assistant features');
      _expectSectionFocus(tester, 'assistantFeatures');
      expect(find.text('Schedule Expert'), findsOneWidget);
      expect(find.text('floe.source.calendar'), findsOneWidget);
      expect(find.text('Existing calendar'), findsOneWidget);

      scroll.jumpTo(120);
      await tester.pump();
      final replacementAssistant = AssistantFeatureController(
        gateway: _ReadOnlyAssistantGateway(),
        canOperate: () => fixture.runtime.ready,
        onFatalFailure: (_) {},
      );
      fixture.extraAssistants.add(replacementAssistant);
      final secondTarget = _assistantTarget(
        '00000000-0000-0000-0000-0000000000fe',
      );
      await _mountSettings(
        tester,
        fixture,
        width: 390,
        height: 560,
        target: secondTarget,
        assistantController: replacementAssistant,
      );
      expect(scroll.offset, 0);
      await _pumpFrames(tester);
      _expectHeadingVisible(tester, 'Assistant features');
      _expectSectionFocus(tester, 'assistantFeatures');
      expect(find.text('Existing calendar'), findsOneWidget);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump(const Duration(milliseconds: 300));
      expect(tester.takeException(), isNull);
    },
  );
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

Future<void> _mountSettings(
  WidgetTester tester,
  _SettingsFixture fixture, {
  required double width,
  required double height,
  double textScale = 1,
  AgentAssistantFeatureSourceTarget? target,
  AssistantFeatureController? assistantController,
}) async {
  final narrow = width < 720;
  await tester.pumpWidget(
    MaterialApp(
      theme: FloeTheme.light,
      locale: const Locale('en'),
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: MediaQuery(
        data: MediaQueryData(textScaler: TextScaler.linear(textScale)),
        child: Scaffold(
          body: Padding(
            padding: EdgeInsets.fromLTRB(
              narrow ? 16 : 120,
              narrow ? 64 : 24,
              narrow ? 16 : 36,
              narrow ? (height == 560 ? 32 : 96) : 24,
            ),
            child: SettingsScreen(
              key: const ValueKey('settings-screen'),
              connectionsController: null,
              operationPolicyController: fixture.policy,
              runtime: fixture.runtime,
              assistantFeatureController:
                  assistantController ?? fixture.assistant,
              memoryController: fixture.memory,
              assistantFeatureSourceTarget: target,
              platform: TargetPlatform.linux,
            ),
          ),
        ),
      ),
    ),
  );
}

Future<void> _selectSection(WidgetTester tester, String pageName) async {
  final section = _section(pageName);
  await tester.ensureVisible(section);
  await tester.pump();
  await tester.tap(section);
  await _pumpFrames(tester);
}

Finder _section(String pageName) =>
    find.byKey(ValueKey<String>('settings-$pageName'));

ScrollController _contentScroll(WidgetTester tester) => tester
    .widget<SingleChildScrollView>(find.byKey(_contentScrollKey))
    .controller!;

Future<void> _pumpFrames(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 240));
  await tester.pump(const Duration(milliseconds: 240));
}

void _expectSectionFocus(WidgetTester tester, String pageName) {
  final button = find.descendant(
    of: _section(pageName),
    matching: find.byType(InkWell),
  );
  expect(button, findsOneWidget);
  final node = tester.widget<InkWell>(button).focusNode;
  expect(node, isNotNull);
  expect(FocusManager.instance.primaryFocus, same(node));
}

double _textScale(WidgetTester tester) =>
    MediaQuery.textScalerOf(tester.element(find.byKey(_contentScrollKey)))
        .scale(1);

bool _selected(WidgetTester tester, String pageName) =>
    tester
        .getSemantics(_section(pageName))
        .getSemanticsData()
        .flagsCollection
        .isSelected ==
    ui.Tristate.isTrue;

void _expectHeadingVisible(WidgetTester tester, String label) {
  final viewport = tester.getRect(find.byKey(_contentScrollKey));
  final all = find.text(label).evaluate().toList();
  var bestIndex = -1;
  var bestSize = 0.0;
  for (var index = 0; index < all.length; index++) {
    final text = all[index].widget as Text;
    final size = text.style?.fontSize ?? 0;
    if (size > bestSize) {
      bestIndex = index;
      bestSize = size;
    }
  }
  expect(bestIndex, greaterThanOrEqualTo(0), reason: 'Find heading $label');
  final heading = tester.getRect(find.text(label).at(bestIndex));
  expect(heading.top, greaterThanOrEqualTo(viewport.top));
  expect(heading.bottom, lessThanOrEqualTo(viewport.bottom));
}

Future<_SettingsFixture> _fixture() async {
  final runtime = RuntimeController(
    gateway: _ReadyRuntimeGateway(),
    personId: _personId,
  );
  await runtime.open();
  final policy = OperationPolicyController(
    gateway: _ReadOnlyPolicyGateway(),
    runtime: runtime,
  );
  final assistant = AssistantFeatureController(
    gateway: _ReadOnlyAssistantGateway(),
    canOperate: () => runtime.ready,
    onFatalFailure: (_) {},
  );
  final memory = AgentMemoryController(
    memoryGateway: _ReadOnlyMemoryGateway(),
    reviewGateway: null,
    personId: _personId,
    canOperate: () => runtime.ready,
    onFatalFailure: (_) {},
  );
  return _SettingsFixture(runtime, policy, assistant, memory);
}

AgentAssistantFeatureSourceTarget _assistantTarget(String reviewId) =>
    AgentAssistantFeatureSourceTarget(
      review: AssistantFeatureSourceReview.fromJson({
        'review_ref': {
          'id': reviewId,
          'digest': '1${List.filled(63, '0').join()}',
        },
        'source_scope_ref': '00000000-0000-0000-0000-000000000002',
        'source_requirement_ref': 'floe.source.calendar',
        'binding_revision': 12,
        'candidates': [
          {
            'candidate_ref': '00000000-0000-0000-0000-00000000000a',
            'label': 'Existing calendar',
            'availability': 'available',
            'selected': true,
          },
          {
            'candidate_ref': '00000000-0000-0000-0000-00000000000b',
            'label': 'Work calendar',
            'availability': 'available',
            'selected': false,
          },
        ],
        'expires_at_unix_ms': DateTime.now().millisecondsSinceEpoch + 60000,
        'allowed_actions': ['replace'],
      }),
    );

final class _SettingsFixture {
  _SettingsFixture(this.runtime, this.policy, this.assistant, this.memory);

  final RuntimeController runtime;
  final OperationPolicyController policy;
  final AssistantFeatureController assistant;
  final AgentMemoryController memory;
  final extraAssistants = <AssistantFeatureController>[];

  void dispose() {
    memory.dispose();
    assistant.dispose();
    for (final controller in extraAssistants) {
      controller.dispose();
    }
    policy.dispose();
    runtime.dispose();
  }
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected Runtime command in Settings test.');
}

final class _ReadOnlyPolicyGateway implements OperationAuthorizationGateway {
  @override
  Future<ActionAuthority> loadAuthority() async => const ActionAuthority(
    revision: 1,
    calendarCreate: ActionAuthorityMode.ask,
  );

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) async =>
      throw StateError('Policy mutation is forbidden in Settings test.');
}

final class _ReadOnlyAssistantGateway implements AssistantFeatureGateway {
  @override
  AssistantFeatureCommandKind? get pendingCommandKind => null;

  @override
  Future<AssistantFeatureSnapshot> readSnapshot() async =>
      AssistantFeatureSnapshot.fromJson({
        'revision': 1,
        'features': [
          for (var index = 1; index <= 12; index++)
            {
              'feature_ref': _uuid(1000 + index),
              'display_name': 'Synthetic assistant feature $index',
              'description':
                  'Synthetic feature content for Settings navigation testing.',
              'enabled': false,
              'source_groups': index == 1
                  ? [
                      {
                        'source_scope_ref':
                            '00000000-0000-0000-0000-000000000002',
                        'display_name': 'Schedule Expert',
                        'enabled': true,
                        'binding_revision': 12,
                        'requirements': [
                          {
                            'requirement_ref': 'floe.source.calendar',
                            'label': 'floe.source.calendar',
                            'selected_count': 1,
                            'minimum_sources': 1,
                          },
                        ],
                      },
                    ]
                  : <Object>[],
            },
        ],
      });

  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Assistant feature mutation is forbidden in Settings test.',
  );
}

final class _ReadOnlyMemoryGateway implements AgentMemoryGateway {
  @override
  Future<AgentMemoryOverview> readMemory() async => AgentMemoryOverview(
    personId: _personId,
    savedCount: 16,
    pendingCount: 0,
    memories: [
      for (var index = 1; index <= 16; index++)
        AgentMemory(
          targetId: _uuid(2000 + index),
          revision: 1,
          statement: 'Synthetic saved memory $index.',
          memoryKind: 'preference',
          epistemicStatus: 'fact',
          confidenceMillis: 800,
          sourceCount: 1,
          origin: AgentMemoryOrigin.userProvided,
          createdAt: DateTime.utc(2026, 10, 1),
        ),
    ],
  );
}

String _uuid(int number) =>
    '00000000-0000-4000-8000-${number.toString().padLeft(12, '0')}';
