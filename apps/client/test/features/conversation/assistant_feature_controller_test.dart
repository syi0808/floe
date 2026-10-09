import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/settings/presentation/assistant_feature_settings.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  test(
    'feature toggle and reviewed source choices use one configure command',
    () async {
      final gateway = _AssistantFeatureGateway();
      final controller = AssistantFeatureController(
        gateway: gateway,
        canOperate: () => true,
        onFatalFailure: (_) {},
      );
      addTearDown(controller.dispose);

      await controller.load();
      final feature = controller.snapshot!.features.single;
      final group = feature.sourceGroups.single;
      final requirement = group.requirements.single;
      controller.setEnabledDraft(feature, false);
      await controller.prepareSourceReview(feature, group, requirement);
      final review = controller.reviewFor(group, requirement)!;
      controller.setSourceCandidates(review, {
        '00000000-0000-0000-0000-00000000000b',
        '00000000-0000-0000-0000-00000000000c',
      });

      expect(controller.canConfigure(feature), isTrue);
      expect(await controller.configure(feature), isTrue);
      expect(gateway.configureCalls, 1);
      expect(
        gateway.configuredFeatureRef,
        '00000000-0000-0000-0000-000000000001',
      );
      expect(gateway.configuredRevision, 8);
      expect(gateway.configuredEnabled, isFalse);
      expect(gateway.configuredSelections, hasLength(1));
      expect(gateway.configuredSelections.single.candidateRefs, [
        '00000000-0000-0000-0000-00000000000b',
        '00000000-0000-0000-0000-00000000000c',
      ]);
      expect(
        gateway.configuredSelections.single.review.reviewRef.id,
        '00000000-0000-0000-0000-0000000000ff',
      );
      expect(controller.snapshot!.revision, 9);
      expect(controller.snapshot!.features.single.enabled, isFalse);
    },
  );

  test(
    'a second tap cannot submit a second save while one is pending',
    () async {
      final gateway = _AssistantFeatureGateway();
      final controller = AssistantFeatureController(
        gateway: gateway,
        canOperate: () => true,
        onFatalFailure: (_) {},
      );
      addTearDown(controller.dispose);

      await controller.load();
      final feature = controller.snapshot!.features.single;
      controller.setEnabledDraft(feature, false);
      gateway.configureResult = Completer<AssistantFeatureSnapshot>();

      final firstSave = controller.configure(feature);
      expect(controller.busy, isTrue);
      expect(await controller.configure(feature), isFalse);
      expect(gateway.configureCalls, 1);

      gateway.configureResult!.complete(
        AssistantFeatureSnapshot.fromJson(
          _snapshotJson(revision: 9, enabled: false),
        ),
      );
      expect(await firstSave, isTrue);
      expect(gateway.configureCalls, 1);
    },
  );

  test('an earlier save preserves a newer focused review', () async {
    final gateway = _AssistantFeatureGateway();
    final controller = AssistantFeatureController(
      gateway: gateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    addTearDown(controller.dispose);

    await controller.load();
    final feature = controller.snapshot!.features.single;
    final group = feature.sourceGroups.single;
    final requirement = group.requirements.single;
    await controller.prepareSourceReview(feature, group, requirement);
    final submittedReview = controller.reviewFor(group, requirement)!;
    controller.setSourceCandidates(submittedReview, {
      '00000000-0000-0000-0000-00000000000b',
    });
    controller.setEnabledDraft(feature, false);
    gateway.configureResult = Completer<AssistantFeatureSnapshot>();

    final save = controller.configure(feature);
    final newerReview = AssistantFeatureSourceReview.fromJson(
      _reviewJson(
        id: '00000000-0000-0000-0000-0000000000fe',
        digest: '2${List.filled(63, '0').join()}',
      ),
    );
    controller.usePreparedReview(newerReview);

    gateway.configureResult!.complete(
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );
    expect(await save, isTrue);
    expect(controller.focusedReview?.reviewRef.id, newerReview.reviewRef.id);
    expect(
      controller.reviewFor(group, requirement)?.reviewRef.id,
      newerReview.reviewRef.id,
    );
    expect(controller.selectedCandidates(newerReview), {
      '00000000-0000-0000-0000-00000000000a',
    });
    expect(controller.didConfigureReview(submittedReview.reviewRef), isTrue);
    expect(controller.didConfigureReview(newerReview.reviewRef), isFalse);
  });

  test('retry applies only the submitted review after focus changes', () async {
    final gateway = _AssistantFeatureGateway();
    final controller = AssistantFeatureController(
      gateway: gateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    addTearDown(controller.dispose);

    await controller.load();
    final feature = controller.snapshot!.features.single;
    final group = feature.sourceGroups.single;
    final requirement = group.requirements.single;
    await controller.prepareSourceReview(feature, group, requirement);
    final submittedReview = controller.reviewFor(group, requirement)!;
    controller.setSourceCandidates(submittedReview, {
      '00000000-0000-0000-0000-00000000000b',
    });
    controller.setEnabledDraft(feature, false);
    gateway.configureFailure = const AppOwnerException('conflict');

    expect(await controller.configure(feature), isFalse);
    expect(controller.canRetryPending, isTrue);
    final newerReview = AssistantFeatureSourceReview.fromJson(_reviewJson());
    controller.usePreparedReview(newerReview);
    gateway.retryResult = AssistantFeatureSnapshotCommandResult(
      AssistantFeatureCommandKind.configure,
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );

    final result = await controller.retryPendingCommand();
    expect(result?.kind, AssistantFeatureCommandKind.configure);
    expect(gateway.retryCalls, 1);
    expect(controller.pendingCommandKind, isNull);
    expect(
      identical(controller.reviewFor(group, requirement), newerReview),
      isTrue,
    );
    expect(identical(controller.focusedReview, newerReview), isTrue);
    expect(controller.selectedCandidates(newerReview), {
      '00000000-0000-0000-0000-00000000000b',
    });
    expect(controller.didConfigureReview(submittedReview.reviewRef), isTrue);
    expect(controller.didConfigureReview(newerReview.reviewRef), isTrue);
    controller.acknowledgeConfiguredReview(newerReview.reviewRef);
    expect(controller.focusedReview, isNull);
    expect(controller.reviewFor(group, requirement), isNull);
  });

  test('clear and dispose ignore a save response that arrives later', () async {
    final clearGateway = _AssistantFeatureGateway();
    final cleared = AssistantFeatureController(
      gateway: clearGateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    await cleared.load();
    final feature = cleared.snapshot!.features.single;
    cleared.setEnabledDraft(feature, false);
    clearGateway.configureResult = Completer<AssistantFeatureSnapshot>();
    final clearSave = cleared.configure(feature);
    cleared.clear();
    clearGateway.configureResult!.complete(
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );
    expect(await clearSave, isFalse);
    expect(cleared.snapshot, isNull);
    expect(cleared.loaded, isFalse);
    cleared.dispose();

    final disposeGateway = _AssistantFeatureGateway();
    final disposed = AssistantFeatureController(
      gateway: disposeGateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    await disposed.load();
    final disposeFeature = disposed.snapshot!.features.single;
    disposed.setEnabledDraft(disposeFeature, false);
    disposeGateway.configureResult = Completer<AssistantFeatureSnapshot>();
    final disposeSave = disposed.configure(disposeFeature);
    disposed.dispose();
    disposeGateway.configureResult!.complete(
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );
    expect(await disposeSave, isFalse);
    expect(disposeGateway.configureCalls, 1);
  });

  testWidgets('a completed save cannot resolve a newer focused interaction', (
    tester,
  ) async {
    final gateway = _AssistantFeatureGateway();
    final controller = AssistantFeatureController(
      gateway: gateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: 'assistant-feature-test-person',
    );
    addTearDown(controller.dispose);
    addTearDown(runtime.dispose);

    var firstResolved = 0;
    var secondResolved = 0;
    Future<void> firstCallback() async {
      firstResolved++;
    }

    Future<void> secondCallback() async {
      secondResolved++;
    }

    final firstFocus = AgentAssistantFeatureSourceTarget(
      review: AssistantFeatureSourceReview.fromJson(_reviewJson()),
    );
    final secondFocus = AgentAssistantFeatureSourceTarget(
      review: AssistantFeatureSourceReview.fromJson(_reviewJson()),
    );

    Widget buildSettings({
      required AgentAssistantFeatureSourceTarget focus,
      required Future<void> Function() onConfigured,
    }) => MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(
        body: SingleChildScrollView(
          child: AssistantFeatureSettings(
            controller: controller,
            runtime: runtime,
            focus: focus,
            onConfigured: onConfigured,
          ),
        ),
      ),
    );

    await tester.pumpWidget(
      buildSettings(focus: firstFocus, onConfigured: firstCallback),
    );
    await runtime.open();
    await tester.pumpAndSettle();
    final feature = controller.snapshot!.features.single;
    controller.setEnabledDraft(feature, false);
    await tester.pump();
    final saveButton = find.byKey(
      const ValueKey(
        'assistant-feature-save-00000000-0000-0000-0000-000000000001',
      ),
    );
    await tester.ensureVisible(saveButton);
    gateway.configureResult = Completer<AssistantFeatureSnapshot>();
    await tester.tap(saveButton);
    await tester.pump();
    expect(gateway.configureCalls, 1);

    await tester.pumpWidget(
      buildSettings(focus: secondFocus, onConfigured: secondCallback),
    );
    await tester.pump();
    gateway.configureResult!.complete(
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );
    await tester.pumpAndSettle();

    expect(firstResolved, 0);
    expect(secondResolved, 0);
    expect(identical(controller.focusedReview, secondFocus.review), isTrue);
  });

  testWidgets('leaving settings while a save is pending does not resolve it', (
    tester,
  ) async {
    final gateway = _AssistantFeatureGateway();
    final controller = AssistantFeatureController(
      gateway: gateway,
      canOperate: () => true,
      onFatalFailure: (_) {},
    );
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: 'assistant-feature-dispose-person',
    );
    addTearDown(controller.dispose);
    addTearDown(runtime.dispose);

    var resolved = 0;
    Future<void> callback() async {
      resolved++;
    }

    final focus = AgentAssistantFeatureSourceTarget(
      review: AssistantFeatureSourceReview.fromJson(_reviewJson()),
    );
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: AssistantFeatureSettings(
              controller: controller,
              runtime: runtime,
              focus: focus,
              onConfigured: callback,
            ),
          ),
        ),
      ),
    );
    await runtime.open();
    await tester.pumpAndSettle();
    controller.setEnabledDraft(controller.snapshot!.features.single, false);
    await tester.pump();
    final saveButton = find.byKey(
      const ValueKey(
        'assistant-feature-save-00000000-0000-0000-0000-000000000001',
      ),
    );
    await tester.ensureVisible(saveButton);
    gateway.configureResult = Completer<AssistantFeatureSnapshot>();
    await tester.tap(saveButton);
    await tester.pump();
    expect(gateway.configureCalls, 1);

    await tester.pumpWidget(const SizedBox());
    gateway.configureResult!.complete(
      AssistantFeatureSnapshot.fromJson(
        _snapshotJson(revision: 9, enabled: false),
      ),
    );
    await tester.pumpAndSettle();

    expect(resolved, 0);
  });
}

final class _AssistantFeatureGateway implements AssistantFeatureGateway {
  int configureCalls = 0;
  String? configuredFeatureRef;
  int? configuredRevision;
  bool? configuredEnabled;
  List<AssistantFeatureSourceSelection> configuredSelections = const [];
  Completer<AssistantFeatureSnapshot>? configureResult;
  Object? configureFailure;
  AssistantFeatureCommandResult? retryResult;
  AssistantFeatureCommandKind? _pendingKind;
  int retryCalls = 0;

  @override
  AssistantFeatureCommandKind? get pendingCommandKind => _pendingKind;

  @override
  Future<AssistantFeatureSnapshot> readSnapshot() async =>
      AssistantFeatureSnapshot.fromJson(_snapshotJson());

  @override
  Future<AssistantFeatureSourceReview> prepareSourceReview({
    required String featureRef,
    required String sourceScopeRef,
    required String sourceRequirementRef,
    required int expectedBindingRevision,
  }) async => AssistantFeatureSourceReview.fromJson(_reviewJson());

  @override
  Future<AssistantFeatureSourceReview> inspectSourceReview(
    AssistantFeatureSourceReviewRef reviewRef,
  ) async => AssistantFeatureSourceReview.fromJson(_reviewJson());

  @override
  Future<AssistantFeatureSnapshot> configure({
    required String featureRef,
    required int expectedRevision,
    required bool enabled,
    required List<AssistantFeatureSourceSelection> sourceSelections,
  }) async {
    configureCalls++;
    configuredFeatureRef = featureRef;
    configuredRevision = expectedRevision;
    configuredEnabled = enabled;
    configuredSelections = sourceSelections;
    final failure = configureFailure;
    if (failure != null) {
      _pendingKind = AssistantFeatureCommandKind.configure;
      throw failure;
    }
    final result = configureResult;
    if (result != null) return result.future;
    return AssistantFeatureSnapshot.fromJson(
      _snapshotJson(
        revision: expectedRevision + 1,
        enabled: enabled,
        selectedCount: sourceSelections.single.candidateRefs.length,
      ),
    );
  }

  @override
  Future<AssistantFeatureCommandResult> retryPendingCommand() async {
    retryCalls++;
    final result = retryResult;
    if (result == null) throw UnimplementedError();
    _pendingKind = null;
    return result;
  }
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);
}

Map<String, Object?> _snapshotJson({
  int revision = 8,
  bool enabled = true,
  int selectedCount = 1,
}) => {
  'revision': revision,
  'features': [
    {
      'feature_ref': '00000000-0000-0000-0000-000000000001',
      'display_name': 'Schedule Expert',
      'description': 'Schedule assistance.',
      'enabled': enabled,
      'source_groups': [
        {
          'source_scope_ref': '00000000-0000-0000-0000-000000000002',
          'display_name': 'Schedule Expert',
          'enabled': true,
          'binding_revision': 12,
          'requirements': [
            {
              'requirement_ref': 'floe.source.calendar',
              'label': 'floe.source.calendar',
              'selected_count': selectedCount,
              'minimum_sources': 1,
            },
          ],
        },
      ],
    },
  ],
};

Map<String, Object?> _reviewJson({
  String id = '00000000-0000-0000-0000-0000000000ff',
  String? digest,
}) => {
  'review_ref': {
    'id': id,
    'digest': digest ?? '1${List.filled(63, '0').join()}',
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
    {
      'candidate_ref': '00000000-0000-0000-0000-00000000000c',
      'label': 'Family calendar',
      'availability': 'available',
      'selected': false,
    },
  ],
  'expires_at_unix_ms': DateTime.now().millisecondsSinceEpoch + 60000,
  'allowed_actions': ['replace'],
};
