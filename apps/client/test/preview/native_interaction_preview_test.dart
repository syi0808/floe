import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/preview/native_interaction_fixture.dart';

void main() {
  test(
    'native preview extends the product fixture with scrollable Notes data',
    () async {
      final fixture = await NativeInteractionPreviewFixture.create();
      addTearDown(fixture.dispose);

      final snapshot = await fixture.dayGateway.loadDay(nativePreviewQuery);
      final notes = snapshot.items.whereType<NoteItem>().toList();
      final longNote = notes.singleWhere(
        (note) => note.id == nativePreviewLongNoteId,
      );

      expect(snapshot.personId, nativePreviewPersonId);
      expect(
        notes,
        hasLength(46),
      ); // Existing nine plus 36 and the reader note.
      expect(longNote.title, contains('\n\n'));
      expect(longNote.title.length, greaterThan(3000));
      expect(notes.map((note) => note.id).toSet(), hasLength(notes.length));
      expect(NativePreviewDayGateway.extraNoteCount, 36);
    },
  );

  test(
    'native preview composition excludes live paths and fails closed',
    () async {
      final fixture = await NativeInteractionPreviewFixture.create();
      addTearDown(fixture.dispose);

      final app = fixture.buildFloeApp();
      expect(app.gateway, same(fixture.dayGateway));
      expect(app.personId, nativePreviewPersonId);
      expect(app.connectionsController, isNull);
      expect(app.agentGateway, isNull);
      expect(app.ownerGateways.voice, isNull);
      expect(app.ownerGateways.runtime, same(fixture.runtime));
      expect(
        app.ownerGateways.assistantFeatures,
        same(fixture.assistantFeatures),
      );
      expect(app.ownerGateways.memory, same(fixture.memory));
      expect(
        app.ownerGateways.operationAuthorization,
        same(fixture.operationAuthorizationGateway),
      );
      expect(fixture.memory.reviewGateway, isNull);
      expect(fixture.memory.forgetGateway, isNull);

      final query = nativePreviewQuery;
      final capture = CaptureReceipt(
        id: 'synthetic-capture',
        originalInput: 'synthetic only',
        capturedAt: query.now,
        revision: 1,
      );
      final note = NoteItem(
        id: 'synthetic-delete-target',
        title: 'Synthetic note',
        revision: 1,
        createdAt: query.now,
      );
      final task = TaskItem(
        id: 'synthetic-task-target',
        title: 'Synthetic task',
        revision: 1,
        createdAt: query.now,
      );
      const policy = ActionAuthorityMode.ask;
      const reviewRef = AssistantFeatureSourceReviewRef(
        id: '00000000-0000-4000-8000-000000000123',
        digest:
            '1000000000000000000000000000000000000000000000000000000000000000',
      );

      await expectLater(
        fixture.dayGateway.submitCapture('synthetic', query),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            contains('Preview-only'),
          ),
        ),
      );
      await expectLater(
        fixture.dayGateway.classifyCapture(
          capture,
          const NoteDraft(content: 'synthetic'),
          query,
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.dayGateway.setTaskCompleted(task, true, query),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.dayGateway.deleteItem(note, query),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.dayGateway.inspectExternalCalendarOperation(
          'synthetic-operation',
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.dayGateway.reconcileExternalCalendarOperation(
          'synthetic-operation',
          1,
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.dayGateway.executeExternalCalendarOperation(
          CreateManualCalendarEvent(
            destinationRef: 'synthetic-destination',
            title: 'Synthetic event',
            startsAt: query.now,
            endsAt: query.now.add(const Duration(minutes: 30)),
            timezone: 'UTC',
          ),
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.operationAuthorizationGateway.setAuthority(
          commandId: 'synthetic-command',
          mode: policy,
          expectedRevision: 1,
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.runtimeGateway.prepare('synthetic-operation'),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.runtimeGateway.getPreparation('synthetic-operation'),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.runtimeGateway.acknowledge('synthetic-operation'),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.assistantFeatureGateway.prepareSourceReview(
          featureRef: '00000000-0000-4000-8000-000000001001',
          sourceScopeRef: '00000000-0000-4000-8000-000000000001',
          sourceRequirementRef: 'floe.source.calendar',
          expectedBindingRevision: 1,
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.assistantFeatureGateway.inspectSourceReview(reviewRef),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.assistantFeatureGateway.configure(
          featureRef: '00000000-0000-4000-8000-000000001001',
          expectedRevision: 1,
          enabled: true,
          sourceSelections: const [],
        ),
        throwsA(isA<StateError>()),
      );
      await expectLater(
        fixture.assistantFeatureGateway.retryPendingCommand(),
        throwsA(isA<StateError>()),
      );
      expect(
        await fixture.memoryGateway.readMemory(),
        isA<AgentMemoryOverview>(),
      );
    },
  );
}
