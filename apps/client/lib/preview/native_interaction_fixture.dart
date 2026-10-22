import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/preview/product_fixture.dart';
import 'package:flutter/widgets.dart';

const nativePreviewPersonId = 'synthetic-native-interaction-preview';
const nativePreviewLongNoteId = 'native-preview-long-reader';

DayQuery get nativePreviewQuery => DayQuery(
  personId: nativePreviewPersonId,
  date: previewNow,
  now: previewNow,
  timezoneOffsetSeconds: 0,
);

/// Synthetic-only app composition for native Notes and Settings interaction QA.
///
/// This fixture deliberately has no live ConnectionsController, conversation
/// gateway, voice gateway, provider, calendar service, or persisted profile.
final class NativeInteractionPreviewFixture {
  NativeInteractionPreviewFixture._({
    required this.dayGateway,
    required this.runtimeGateway,
    required this.operationAuthorizationGateway,
    required this.assistantFeatureGateway,
    required this.memoryGateway,
    required this.runtime,
    required this.assistantFeatures,
    required this.memory,
  });

  final NativePreviewDayGateway dayGateway;
  final RuntimeGateway runtimeGateway;
  final OperationAuthorizationGateway operationAuthorizationGateway;
  final AssistantFeatureGateway assistantFeatureGateway;
  final AgentMemoryGateway memoryGateway;
  final RuntimeController runtime;
  final AssistantFeatureController assistantFeatures;
  final AgentMemoryController memory;
  bool _disposed = false;

  static Future<NativeInteractionPreviewFixture> create() async {
    final runtimeGateway = _PreviewRuntimeGateway();
    final runtime = RuntimeController(
      gateway: runtimeGateway,
      personId: nativePreviewPersonId,
    );
    await runtime.open();

    final assistantFeatureGateway = _PreviewAssistantFeatureGateway();
    final assistantFeatures = AssistantFeatureController(
      gateway: assistantFeatureGateway,
      canOperate: () => runtime.ready,
      onFatalFailure: (_) {},
    );
    final memoryGateway = _PreviewMemoryGateway();
    final memory = AgentMemoryController(
      memoryGateway: memoryGateway,
      reviewGateway: null,
      forgetGateway: null,
      personId: nativePreviewPersonId,
      canOperate: () => runtime.ready,
      onFatalFailure: (_) {},
    );

    return NativeInteractionPreviewFixture._(
      dayGateway: NativePreviewDayGateway(previewGateway()),
      runtimeGateway: runtimeGateway,
      operationAuthorizationGateway: _PreviewOperationAuthorizationGateway(),
      assistantFeatureGateway: assistantFeatureGateway,
      memoryGateway: memoryGateway,
      runtime: runtime,
      assistantFeatures: assistantFeatures,
      memory: memory,
    );
  }

  FloeApp buildFloeApp() => FloeApp(
    gateway: dayGateway,
    personId: nativePreviewPersonId,
    query: nativePreviewQuery,
    ownerGateways: LocalOwnerGateways(
      runtime: runtime,
      assistantFeatures: assistantFeatures,
      memory: memory,
      operationAuthorization: operationAuthorizationGateway,
    ),
    // Deliberately omit live conversation and Connections integrations.
    agentGateway: null,
    connectionsController: null,
    onDisposeGateway: dispose,
  );

  Widget buildWidget() => previewAppearance(buildFloeApp());

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    memory.dispose();
    assistantFeatures.dispose();
    runtime.dispose();
  }
}

/// Read-only Day projection extending the existing nine-note product fixture.
final class NativePreviewDayGateway implements DayGateway {
  NativePreviewDayGateway(this._fixtureGateway);

  final FakeDayGateway _fixtureGateway;

  static const extraNoteCount = 36;

  static String longReaderText() => [
    'Launch brief focus block',
    for (var paragraph = 1; paragraph <= 32; paragraph++)
      'Paragraph $paragraph — Review the launch brief, capture the decision, '
          'and keep a clear record of the reasoning for the next planning pass. '
          'This is synthetic preview text written only to make the Notes reader '
          'long enough for native scrolling checks.',
  ].join('\n\n');

  static List<NoteItem> additionalNotes() => [
    for (var index = 1; index <= extraNoteCount; index++)
      NoteItem(
        id: 'native-preview-note-${index.toString().padLeft(2, '0')}',
        title:
            'Synthetic native preview note $index\n'
            'A short, deterministic entry used to make the Notes collection '
            'scroll below the fold.',
        revision: 0,
        createdAt: previewNow.subtract(Duration(minutes: index)),
      ),
    NoteItem(
      id: nativePreviewLongNoteId,
      title: longReaderText(),
      revision: 0,
      createdAt: DateTime.utc(2026, 9, 2, 8),
    ),
  ];

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    if (query.personId != nativePreviewPersonId) {
      throw StateError('Preview-only: unexpected Day person identity.');
    }
    final base = await _fixtureGateway.loadDay(query);
    return DaySnapshot(
      personId: base.personId,
      date: base.date,
      generatedAt: base.generatedAt,
      timezoneOffsetSeconds: base.timezoneOffsetSeconds,
      items: List.unmodifiable([...base.items, ...additionalNotes()]),
      nowEventId: base.nowEventId,
      nextEventId: base.nextEventId,
      overdueTaskCount: base.overdueTaskCount,
      calendarCoverage: base.calendarCoverage,
    );
  }

  @override
  Future<List<ManualCalendarDestination>>
  loadExternalCalendarDestinations() async => const [];

  @override
  Future<ManualCalendarOperationPage> loadExternalCalendarOperations({
    String? cursor,
    int limit = 100,
  }) async =>
      const ManualCalendarOperationPage(operations: [], nextCursor: null);

  @override
  Future<ManualCalendarOperationReceipt> inspectExternalCalendarOperation(
    String operationRef,
  ) => _disabled('Calendar operation inspection');

  @override
  Future<ManualCalendarOperationReceipt> reconcileExternalCalendarOperation(
    String operationRef,
    int expectedRevision,
  ) => _disabled('Calendar operation reconciliation');

  @override
  Future<ManualCalendarOperationReceipt> executeExternalCalendarOperation(
    ManualCalendarOperationIntent operation,
  ) => _disabled('Calendar operation execution');

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) =>
      _disabled('Capture submission');

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) => _disabled('Capture classification');

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) => _disabled('Task mutation');

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) =>
      _disabled('Day item deletion');
}

Future<T> _disabled<T>(String action) => Future<T>.error(
  StateError('Preview-only: $action is disabled; no command was sent.'),
);

final class _PreviewRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) =>
      _disabled('Runtime preparation');

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) =>
      _disabled('Runtime preparation observation');

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) =>
      _disabled('Runtime preparation acknowledgement');
}

final class _PreviewOperationAuthorizationGateway
    implements OperationAuthorizationGateway {
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
  }) => _disabled('Operation policy changes');
}

final class _PreviewAssistantFeatureGateway implements AssistantFeatureGateway {
  @override
  AssistantFeatureCommandKind? get pendingCommandKind => null;

  @override
  Future<AssistantFeatureSnapshot>
  readSnapshot() async => AssistantFeatureSnapshot.fromJson({
    'revision': 1,
    'features': [
      for (var index = 1; index <= 12; index++)
        {
          'feature_ref': _previewUuid(1000 + index),
          'display_name': 'Synthetic assistant feature $index',
          'description':
              'Synthetic Settings preview content. No provider is connected.',
          'enabled': false,
          'source_groups': <Object>[],
        },
    ],
  });

  @override
  Future<AssistantFeatureSourceReview> prepareSourceReview({
    required String featureRef,
    required String sourceScopeRef,
    required String sourceRequirementRef,
    required int expectedBindingRevision,
  }) => _disabled('Assistant source review preparation');

  @override
  Future<AssistantFeatureSourceReview> inspectSourceReview(
    AssistantFeatureSourceReviewRef reviewRef,
  ) => _disabled('Assistant source review inspection');

  @override
  Future<AssistantFeatureSnapshot> configure({
    required String featureRef,
    required int expectedRevision,
    required bool enabled,
    required List<AssistantFeatureSourceSelection> sourceSelections,
  }) => _disabled('Assistant feature configuration');

  @override
  Future<AssistantFeatureCommandResult> retryPendingCommand() =>
      _disabled('Assistant command retry');
}

final class _PreviewMemoryGateway implements AgentMemoryGateway {
  @override
  Future<AgentMemoryOverview> readMemory() async => AgentMemoryOverview(
    personId: nativePreviewPersonId,
    savedCount: 16,
    pendingCount: 0,
    memories: [
      for (var index = 1; index <= 16; index++)
        AgentMemory(
          targetId: _previewUuid(2000 + index),
          revision: 1,
          statement: 'Synthetic saved memory $index for Settings preview.',
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

String _previewUuid(int value) =>
    '00000000-0000-4000-8000-${value.toString().padLeft(12, '0')}';
