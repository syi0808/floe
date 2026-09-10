import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_event_details.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('event details close with X without a duplicate footer', (
    tester,
  ) async {
    final date = DateTime.utc(2026, 9, 5);
    final event = EventItem(
      id: 'event',
      title: 'Team meeting',
      revision: 1,
      createdAt: date,
      startsAt: date,
      endsAt: date.add(const Duration(hours: 1)),
      externalId: 'external-event',
    );
    final snapshot = DaySnapshot(
      personId: 'test',
      date: date,
      generatedAt: date,
      timezoneOffsetSeconds: 0,
      items: [event],
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => openCalendarEvent(context, event, snapshot),
              child: const Text('Open event'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open event'));
    await tester.pumpAndSettle();
    expect(find.text('Team meeting'), findsOneWidget);
    expect(find.text('Back to my day'), findsNothing);
    expect(find.byType(FilledButton), findsNothing);
    expect(find.text('Source details'), findsOneWidget);
    expect(find.textContaining('time zone'), findsNothing);
    expect(find.textContaining('UTC'), findsNothing);
    await tester.tap(find.byTooltip('Close'));
    await tester.pumpAndSettle();
    expect(find.byType(CalendarEventDetails), findsNothing);
    expect(find.text('Open event'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
