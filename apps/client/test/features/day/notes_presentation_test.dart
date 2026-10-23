import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/preview/calendar_fixture.dart';
import 'package:floe_client/preview/product_fixture.dart';

Future<void> openNotes(
  WidgetTester tester, {
  bool empty = false,
  double width = 1200,
  List<NoteItem>? notes,
}) async {
  tester.view.physicalSize = Size(width, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    previewAppearance(
      FloeApp(
        personId: calendarPreviewQuery.personId,
        gateway: notes != null
            ? FakeDayGateway(initialItems: notes)
            : empty
            ? FakeDayGateway()
            : calendarPreviewGateway(),
        query: calendarPreviewQuery,
        ownerGateways: const LocalOwnerGateways(),
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.byTooltip('Notes'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'saving from another day explains where the note went without replay',
    (tester) async {
      tester.view.physicalSize = const Size(1200, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = _DateScopedNotesGateway();
      final now = DateTime.now();
      final past = DateTime(2000, 1, 2);
      await tester.pumpWidget(
        FloeApp(
          personId: 'notes-fixture',
          gateway: gateway,
          query: DayQuery.local(
            personId: 'notes-fixture',
            date: past,
            now: now,
          ),
          ownerGateways: const LocalOwnerGateways(),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('New note'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, 'A note saved today');
      await tester.pump();
      await tester.tap(find.text('Save note'));
      await tester.pumpAndSettle();
      expect(gateway.classifications, 1);
      expect(find.text('Saved to today’s notes'), findsOneWidget);
      expect(find.text('A note saved today'), findsNothing);
      expect(
        tester.widget<Text>(find.byKey(const Key('notes-selected-date'))).data,
        'Jan 2, 2000',
      );
      await tester.tap(find.text('View notes'));
      await tester.pump(FloeLoading.minimumDuration);
      await tester.pumpAndSettle();
      expect(find.text('A note saved today'), findsOneWidget);
      expect(gateway.classifications, 1);
      expect(
        tester.widget<Text>(find.byKey(const Key('notes-selected-date'))).data,
        DateFormat.yMMMd('en').format(DateTime.now()),
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'empty collection does not ask users to clear nonexistent filters',
    (tester) async {
      await openNotes(tester, empty: true);
      expect(find.text('A place for your thoughts'), findsOneWidget);
      expect(find.text('Clear search'), findsNothing);
      expect(find.text('New note'), findsOneWidget);
      await tester.tap(find.text('New note'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(find.text('A place for your thoughts'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets(
    'note details close cleanly without a placeholder review action',
    (tester) async {
      await openNotes(tester);
      for (var repeat = 0; repeat < 2; repeat++) {
        await tester.tap(find.text('Launch plan'));
        await tester.pumpAndSettle();
        expect(find.text('Review with Floe'), findsNothing);
        expect(find.textContaining('Editing is not available'), findsNothing);
        expect(
          find.widgetWithText(SelectableText, 'Launch plan'),
          findsOneWidget,
        );
        await tester.tap(find.byTooltip('Close'));
        await tester.pumpAndSettle();
        expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
      }
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets(
    'task collection and details expose no pretend creation actions',
    (tester) async {
      await openNotes(tester);
      await tester.tap(find.byTooltip('Tasks'));
      await tester.pumpAndSettle();
      expect(find.text('Tasks'), findsNWidgets(2));
      expect(find.text('New task'), findsNothing);
      expect(find.byType(FloeCheckbox), findsOneWidget);
      await tester.tap(find.text('Prepare launch brief'));
      await tester.pumpAndSettle();
      expect(find.text('Add a subtask'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  for (final width in [390.0, 1200.0]) {
    testWidgets(
      'notes show their selected date and newest-first order at $width',
      (tester) async {
        final day = calendarPreviewQuery.date;
        NoteItem note(String id, int hour) => NoteItem(
          id: id,
          title: 'Note $id',
          revision: 1,
          createdAt: DateTime(day.year, day.month, day.day, hour),
        );
        // All fixtures belong to the selected day. This verifies presentation,
        // not the owner's cross-day query filtering.
        final notes = [
          note('older', 6),
          note('z-tie', 8),
          note('a-tie', 8),
          note('newest', 9),
        ];
        await openNotes(tester, width: width, notes: notes);
        final date = find.byKey(const Key('notes-selected-date'));
        expect(
          tester.widget<Text>(date).data,
          DateFormat.yMMMd('en').format(day),
        );
        expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
        expect(
          tester.widget<Text>(find.byKey(const Key('notes-count'))).data,
          '4',
        );
        final cards = find.byWidgetPredicate(
          (widget) =>
              widget is Semantics &&
              widget.key is ValueKey<String> &&
              (widget.key! as ValueKey<String>).value.startsWith(
                'note-preview-',
              ),
        );
        expect(
          tester
              .widgetList<Semantics>(cards)
              .map((widget) => (widget.key! as ValueKey<String>).value),
          [
            'note-preview-newest',
            'note-preview-a-tie',
            'note-preview-z-tie',
            'note-preview-older',
          ],
        );
        expect(notes.map((note) => note.id), [
          'older',
          'z-tie',
          'a-tie',
          'newest',
        ]);
        await tester.enterText(find.byType(TextField).first, 'missing');
        await tester.pumpAndSettle();
        expect(
          tester.widget<Text>(date).data,
          DateFormat.yMMMd('en').format(day),
        );
        expect(find.text('Matching notes · 0'), findsOneWidget);
        final semantics = tester.ensureSemantics();
        try {
          expect(find.bySemanticsLabel('Matching notes · 0'), findsOneWidget);
        } finally {
          semantics.dispose();
        }
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('search is preserved and reversible at $width', (tester) async {
      await openNotes(tester, width: width);
      expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
      expect(
        tester.widget<Text>(find.byKey(const Key('notes-count'))).data,
        '1',
      );
      expect(find.text('Personal only'), findsNothing);
      expect(find.text('Work'), findsNothing);
      await tester.enterText(find.byType(TextField).first, 'launch');
      await tester.pumpAndSettle();
      expect(find.text('Matching notes · 1'), findsOneWidget);
      await tester.tap(find.byTooltip('Tasks'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(find.byType(TextField).first).controller!.text,
        'launch',
      );
      await tester.enterText(find.byType(TextField).first, 'missing note');
      await tester.pumpAndSettle();
      expect(find.text('Matching notes · 0'), findsOneWidget);
      expect(find.text('No notes found'), findsOneWidget);
      expect(find.text('Clear search'), findsOneWidget);
      await tester.tap(find.text('Clear search'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
      expect(
        tester.widget<Text>(find.byKey(const Key('notes-count'))).data,
        '1',
      );
      expect(find.text('Launch plan'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }
}

// Production notes are day-scoped. Keep this test fake scoped too so a saved
// current-day note cannot incorrectly appear in a historical collection.
class _DateScopedNotesGateway implements DayGateway {
  final delegate = FakeDayGateway();
  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) =>
      delegate.submitCapture(input, query);
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected notes fixture operation: $invocation');
  int classifications = 0;
  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) {
    classifications++;
    return delegate.classifyCapture(capture, classification, query);
  }

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    final snapshot = await delegate.loadDay(query);
    return DaySnapshot(
      personId: snapshot.personId,
      date: snapshot.date,
      generatedAt: snapshot.generatedAt,
      timezoneOffsetSeconds: snapshot.timezoneOffsetSeconds,
      items: snapshot.items
          .where(
            (item) =>
                item is! NoteItem ||
                (!item.createdAt.isBefore(query.startsAt) &&
                    item.createdAt.isBefore(query.endsAt)),
          )
          .toList(),
    );
  }
}
