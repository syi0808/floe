import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/day_appearance.dart';

void main() {
  for (final width in [320.0, 1200.0]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets('original note stays readable at $width and ${scale}x', (
        tester,
      ) async {
        tester.view.physicalSize = Size(width, 740);
        tester.view.devicePixelRatio = 1;
        tester.platformDispatcher.textScaleFactorTestValue = scale;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
        final body =
            'Reading draft\n\n  Keep this indentation.\n\n${List.generate(28, (i) => 'Paragraph ${i + 1}: Keep the original wording and enough room to read it.').join('\n\n')}\n\nThe final sentence stays reachable.';
        final created = DateTime.utc(2026, 10, 22, 12);
        const source = CaptureDayItemSource(captureId: 'synthetic-capture');
        final note = NoteItem(
          id: 'reading-note',
          title: body,
          revision: 7,
          createdAt: created,
          source: source,
        );
        final gateway = FakeDayGateway(initialItems: [note]);
        final query = DayQuery.local(
          personId: 'fixture',
          date: created,
          now: created,
        );
        await tester.pumpWidget(
          DayAppearance(
            notes: const {
              'reading-note': NoteAppearance(
                excerpt: 'This is not the saved note.',
                timestamp: 'Preview only',
              ),
            },
            child: FloeApp(
              personId: 'fixture',
              gateway: gateway,
              query: query,
              ownerGateways: const LocalOwnerGateways(),
              builder: (context, child) => MediaQuery(
                data: MediaQuery.of(context)
                    .copyWith(disableAnimations: width == 320 && scale == 2),
                child: child!,
              ),
            ),
          ),
        );
        // Reduced motion has no spinner frames to advance the loading timer.
        await tester.pump(FloeLoading.minimumDuration);
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull, reason: 'initial Today surface');
        await tester.tap(find.byTooltip('Notes'));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull, reason: 'Notes collection');
        final heading = find.text('Reading draft');
        await tester.ensureVisible(heading);
        await tester.pumpAndSettle();
        expect(tester.widget<Text>(heading).maxLines, 2);
        final handle = tester.ensureSemantics();
        try {
          await tester.pump();
          final card = find.byKey(const ValueKey('note-preview-reading-note'));
          final node = tester.getSemantics(card);
          final data = node.getSemanticsData();
          expect(data.label, contains('Reading draft'));
          expect(
            data.label,
            isNot(contains('The final sentence stays reachable.')),
          );
          expect(data.label.length, lessThan(300));
          expect(data.flagsCollection.isButton, isTrue);
          expect(data.hasAction(ui.SemanticsAction.tap), isTrue);
          tester
              .renderObject(card)
              .owner!
              .semanticsOwner!
              .performAction(node.id, ui.SemanticsAction.tap);
          await tester.pumpAndSettle();
          expect(
            tester
                .getSemantics(
                  find.descendant(
                    of: find.byKey(const Key('note-original-content')),
                    matching: find.byType(EditableText),
                  ),
                )
                .value,
            body,
          );
        } finally {
          handle.dispose();
        }
        expect(tester.takeException(), isNull, reason: 'Note reader');
        final original = find.byKey(const Key('note-original-content'));
        expect(tester.widget<SelectableText>(original).data, body);
        if (width == 320 && scale == 2) {
          expect(
            ModalRoute.of(tester.element(original))!.transitionDuration,
            Duration.zero,
          );
          expect(
            ModalRoute.of(tester.element(original))!.reverseTransitionDuration,
            Duration.zero,
          );
        }
        expect(
          MediaQuery.textScalerOf(tester.element(original)).scale(10),
          10 * scale,
        );
        expect(find.text('This is not the saved note.'), findsNothing);
        expect(find.textContaining('Editing is not available'), findsNothing);
        expect(
          find.descendant(
            of: find.byKey(const Key('note-reader')),
            matching: find.byType(TextField),
          ),
          findsNothing,
        );
        expect(
          find.descendant(
            of: find.byKey(const Key('note-reader')),
            matching: find.text('Oct 22, 2026'),
          ),
          findsOneWidget,
        );
        final viewport = find.byKey(const Key('note-reader-scroll'));
        final scroll = tester.state<ScrollableState>(
          find
              .descendant(of: viewport, matching: find.byType(Scrollable))
              .first,
        );
        expect(scroll.position.maxScrollExtent, greaterThan(0));
        scroll.position.jumpTo(scroll.position.maxScrollExtent);
        await tester.pumpAndSettle();
        final editable = tester.state<EditableTextState>(
          find.descendant(of: original, matching: find.byType(EditableText)),
        );
        final boxes = editable.renderEditable.getBoxesForSelection(
          TextSelection(
            baseOffset: body.indexOf('The final sentence'),
            extentOffset: body.length,
          ),
        );
        expect(boxes, isNotEmpty);
        final finalPoint = editable.renderEditable.localToGlobal(
          boxes.last.toRect().center,
        );
        expect(tester.getRect(viewport).contains(finalPoint), isTrue);
        final close = find.byTooltip('Close');
        expect(tester.getRect(close).top, greaterThanOrEqualTo(0));
        expect(tester.getRect(close).bottom, lessThanOrEqualTo(740));
        await tester.tap(close);
        await tester.pumpAndSettle();
        expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
        final retained =
            (await gateway.loadDay(query)).items.single as NoteItem;
        expect(identical(retained, note), isTrue);
        expect(retained.title, body);
        expect(retained.revision, 7);
        expect(retained.source, same(source));
        await tester.ensureVisible(find.text('Reading draft'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Reading draft'));
        await tester.pumpAndSettle();
        expect(
          tester
              .widget<SelectableText>(
                find.byKey(const Key('note-original-content')),
              )
              .data,
          body,
        );
        await tester.tap(find.byTooltip('Close'));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
      });
    }
  }
}
