import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day_canvas/application/day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/preview/prototype_fixture.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('failed note save retains text for retry', (tester) async {
    final gateway = _FailOnceGateway(prototypeGateway());
    await tester.pumpWidget(
      prototypeAppearance(FloeApp(gateway: gateway, query: prototypeQuery)),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('New note'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const Key('new-note-content')),
      'Keep this draft',
    );
    await tester.pump();
    await tester.tap(find.text('Save note'));
    await tester.pumpAndSettle();
    expect(find.text('Could not save. Please try again.'), findsOneWidget);
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('new-note-content')))
          .controller!
          .text,
      'Keep this draft',
    );
    await tester.tap(find.text('Save note'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(
      (await gateway.loadDay(prototypeQuery)).items
          .where((item) => item.title == 'Keep this draft'),
      hasLength(1),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('navigation and text links show hover feedback', (tester) async {
    tester.view.physicalSize = const Size(1440, 1100);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      prototypeAppearance(
        FloeApp(gateway: prototypeGateway(), query: prototypeQuery),
      ),
    );
    await tester.pumpAndSettle();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    final notesSurface = find
        .ancestor(
          of: find.byTooltip('Notes'),
          matching: find.byType(FloeSquircle),
        )
        .first;
    await mouse.moveTo(tester.getCenter(find.byTooltip('Notes')));
    await tester.pumpAndSettle();
    expect(
      tester.widget<FloeSquircle>(notesSurface).fill,
      FloePalette.neutral100,
    );
    final link = find.widgetWithText(TextButton, 'See your tasks');
    await mouse.moveTo(tester.getCenter(link));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextButton>(link).style!.textStyle!.resolve({
        WidgetState.hovered,
      })!.decoration,
      TextDecoration.underline,
    );
    await mouse.removePointer();
  });

  for (final width in [390.0, 1440.0]) {
    testWidgets('new note saves and clears hidden results at $width', (
      tester,
    ) async {
      tester.view.physicalSize = Size(width, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = prototypeGateway();
      await tester.pumpWidget(
        prototypeAppearance(FloeApp(gateway: gateway, query: prototypeQuery)),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'no match');
      await tester.tap(find.text('Filter'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Clear filters'));
      await tester.pumpAndSettle();
      expect(find.text('Launch plan'), findsOneWidget);
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        isEmpty,
      );
      await tester.enterText(find.byType(TextField), 'no match');
      await tester.tap(find.text('New note'));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<FilledButton>(
              find.widgetWithText(FilledButton, 'Save note'),
            )
            .onPressed,
        isNull,
      );
      await tester.enterText(
        find.byKey(const Key('new-note-content')),
        'Remember the launch decision',
      );
      await tester.pump();
      await tester.tap(find.text('Save note'));
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsNothing);
      expect(find.text('Remember the launch decision'), findsOneWidget);
      final snapshot = await gateway.loadDay(prototypeQuery);
      expect(
        snapshot.items.whereType<NoteItem>().where(
          (note) => note.title == 'Remember the launch decision',
        ),
        hasLength(1),
      );
      await tester.tap(find.text('New note'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const Key('new-note-content')),
        'Discard this',
      );
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(
        (await gateway.loadDay(prototypeQuery)).items.length,
        snapshot.items.length,
      );
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('connector card opens detail and returns to list', (
    tester,
  ) async {
    await tester.pumpWidget(
      prototypeAppearance(
        FloeApp(gateway: prototypeGateway(), query: prototypeQuery),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Connect'));
    await tester.pumpAndSettle();
    expect(find.text('Connections'), findsOneWidget);
    expect(find.text('Read-only'), findsNothing);
    await tester.tap(find.text('macOS Calendar'));
    await tester.pumpAndSettle();
    expect(find.text('A clear boundary.'), findsOneWidget);
    await tester.tap(find.text('Back to connections'));
    await tester.pumpAndSettle();
    expect(find.text('Connections'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('notes filter and search use the displayed content', (
    tester,
  ) async {
    await tester.pumpWidget(
      prototypeAppearance(
        FloeApp(gateway: prototypeGateway(), query: prototypeQuery),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Filter'));
    await tester.pumpAndSettle();
    expect(find.text('Gratitude'), findsOneWidget);
    expect(find.text('Launch plan'), findsNothing);
    await tester.enterText(find.byType(TextField), 'Slow down');
    await tester.pumpAndSettle();
    expect(find.text('Evening reset'), findsOneWidget);
    expect(find.text('Gratitude'), findsNothing);
    expect(tester.takeException(), isNull);
  });
}

class _FailOnceGateway implements DayGateway {
  _FailOnceGateway(this.delegate);
  final DayGateway delegate;
  bool failNext = true;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) => delegate.loadDay(query);

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) =>
      delegate.submitCapture(input, query);

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) {
    if (failNext) {
      failNext = false;
      return Future.error(StateError('Save unavailable'));
    }
    return delegate.classifyCapture(capture, classification, query);
  }

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) =>
      delegate.deleteItem(item, query);

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) => delegate.setTaskCompleted(task, completed, query);
}
