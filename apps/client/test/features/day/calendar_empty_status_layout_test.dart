import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  for (final width in [390.0, 1200.0]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets('unknown coverage fits width $width at text scale $scale', (
        tester,
      ) async {
        tester.view.physicalSize = Size(width, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        var refreshes = 0;
        var connections = 0;
        final now = DateTime(2026, 10, 14, 12);
        final query = DayQuery.local(personId: 'person', date: now, now: now);
        await tester.pumpWidget(
          MaterialApp(
            locale: const Locale('en'),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            builder: (context, child) => MediaQuery(
              data: MediaQuery.of(context)
                  .copyWith(textScaler: TextScaler.linear(scale)),
              child: child!,
            ),
            home: Scaffold(
              body: CalendarAgenda(
                query: query,
                snapshot: DaySnapshot(
                  personId: query.personId,
                  date: query.date,
                  generatedAt: now,
                  timezoneOffsetSeconds: query.timezoneOffsetSeconds,
                  items: const [],
                ),
                onConnections: () => connections++,
                onRefresh: () => refreshes++,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        await tester.ensureVisible(find.byKey(const Key('empty-day-status')));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        await tester.tap(find.text('Refresh date'));
        await tester.tap(find.text('View connected calendars'));
        expect(refreshes, 1);
        expect(connections, 1);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
      });
    }
  }
}
