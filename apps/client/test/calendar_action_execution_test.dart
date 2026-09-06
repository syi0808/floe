import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_action_panel.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_action_proposal.dart';

import 'calendar_action_ui_test.dart' show Gateway, action, connection;

class Executor extends Gateway
    implements CalendarActionExecutionGateway, CalendarDirectActionGateway {
  bool direct = false;
  int directSubmissions = 0;

  @override
  Future<CalendarAction> submitDirectCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
    String? eventId,
    int? eventRevision,
    bool delete = false,
  }) async {
    direct = true;
    directSubmissions++;
    proposedStart = startsAt;
    proposedTimezone = timezone;
    saved = [action(status: 'approved', direct: true)];
    return saved.single;
  }

  bool enabled = true;
  int creates = 0;
  int lookups = 0;
  int proposals = 0;
  ActionAuthority authority = const ActionAuthority(
    calendarCreate: ActionAuthorityMode.ask,
  );

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) async =>
      authority;

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) async => authority = ActionAuthority(calendarCreate: mode);
  DateTime? proposedStart;
  String? proposedTimezone;
  String outcome = 'succeeded';

  @override
  Future<bool> calendarWritesEnabled(String personId) async => enabled;
  @override
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  ) async {
    creates++;
    saved = [action(status: outcome, direct: direct)];
    return saved.single;
  }

  @override
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  ) async {
    lookups++;
    saved = [action(status: 'succeeded')];
    return saved.single;
  }

  @override
  Future<CalendarAction> proposeCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  }) async {
    proposals++;
    proposedStart = startsAt;
    proposedTimezone = timezone;
    saved = [action()];
    return saved.single;
  }
}

void main() {
  test('manual actions stay out of Review after reload regardless of automation policy', () async {
    for (final mode in ActionAuthorityMode.values) {
      final gateway = Executor()
        ..saved = []
        ..authority = ActionAuthority(calendarCreate: mode);
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
        collect: (_) async {},
      );
      await controller.load();
      expect(controller.canDirect, isTrue);
      final result = await controller.direct(
        calendarId: 'calendar',
        title: 'Manual',
        startsAt: DateTime(2026, 1, 1, 9),
        endsAt: DateTime(2026, 1, 1, 10),
        timezone: 'UTC',
      );
      expect(result?.needsReview, isFalse);
      expect(gateway.decisions, 0);
      expect(gateway.proposals, 0);
      expect(gateway.creates, 1);
      await controller.load();
      expect(controller.actions.single.direct, isTrue);
      expect(controller.actions.where((action) => action.needsReview), isEmpty);
      controller.dispose();
    }
  });

  test(
    'uncertain manual work remains Activity-only and prevents another write',
    () async {
      final gateway = Executor()
        ..saved = []
        ..outcome = 'unknown';
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await controller.load();
      await controller.direct(
        calendarId: 'calendar',
        title: 'Manual',
        startsAt: DateTime(2026, 9, 9, 9),
        endsAt: DateTime(2026, 9, 9, 10),
        timezone: 'UTC',
      );
      await controller.load();
      expect(controller.actions.single.needsReview, isFalse);
      expect(controller.actions.single.status, CalendarActionStatus.unknown);
      expect(controller.canDirect, isFalse);
      expect(gateway.creates, 1);
      controller.dispose();
    },
  );

  test(
    'calendar create authority allows automatic execution or denies intent',
    () async {
      final gateway = Executor()
        ..saved = []
        ..authority = const ActionAuthority(
          calendarCreate: ActionAuthorityMode.allow,
        );
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
        collect: (_) async {},
      );
      addTearDown(controller.dispose);
      await controller.load();

      final result = await controller.propose(
        calendarId: 'calendar',
        title: 'Quiet focus',
        startsAt: DateTime.now().add(const Duration(hours: 1)),
        endsAt: DateTime.now().add(const Duration(hours: 2)),
        timezone: 'UTC+09:00',
      );

      expect(result?.status, CalendarActionStatus.succeeded);
      expect(gateway.decisions, 1);
      expect(gateway.creates, 1);
      expect(controller.actions.single.status, CalendarActionStatus.succeeded);

      await controller.setCalendarCreateAuthority(ActionAuthorityMode.deny);
      expect(controller.canPropose, isFalse);
    },
  );

  test(
    'explicit approval executes once; failed read retries only collection',
    () async {
      final gateway = Executor();
      var reads = 0;
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
        collect: (_) async {
          reads++;
          if (reads == 1) throw StateError('Read unavailable');
        },
      );
      addTearDown(controller.dispose);
      await controller.load();
      await controller.decide(
        'proposal',
        CalendarActionDecision.approve,
        connection(),
        DateTime.now(),
      );
      expect(gateway.creates, 1);
      expect(controller.actions.single.status, CalendarActionStatus.succeeded);
      expect(controller.collection['proposal'], 'failed');
      expect(controller.needsReload, isFalse);
      await controller.retryRead('proposal');
      expect(reads, 2);
      expect(gateway.creates, 1);
      expect(controller.collection['proposal'], 'collected');
    },
  );

  test('relaunch and lookup never create; unresolved actions prevent replacement proposals', () async {
    final gateway = Executor()..outcome = 'unknown';
    var controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    await controller.load();
    await controller.decide(
      'proposal',
      CalendarActionDecision.approve,
      connection(),
      DateTime.now(),
    );
    controller.dispose();
    controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
      collect: (_) async {},
    );
    addTearDown(controller.dispose);
    await controller.load();
    expect(controller.canPropose, isFalse);
    await controller.run('proposal', connection: connection());
    expect(gateway.creates, 1);
    await controller.run('proposal', recover: true);
    expect(gateway.lookups, 1);
    expect(gateway.creates, 1);
    expect(controller.actions.single.status, CalendarActionStatus.succeeded);
  });

  test('old approval and disabled builds never auto-execute', () async {
    final gateway = Executor()..saved = [action(status: 'approved')];
    final controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    addTearDown(controller.dispose);
    await controller.load();
    expect(gateway.creates, 0);
    gateway.enabled = false;
    gateway.saved = [action()];
    await controller.load();
    await controller.decide(
      'proposal',
      CalendarActionDecision.approve,
      connection(),
      DateTime.now(),
    );
    expect(gateway.creates, 0);
    expect(controller.actions.single.status, CalendarActionStatus.approved);
  });

  testWidgets(
    'segmented event editor validates and saves without creating a review',
    (tester) async {
      tester.view.physicalSize = const Size(390, 950);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = Executor()..saved = [];
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
        collect: (_) async {},
      );
      final initialLoad = controller.load();
      await tester.pump(FloeLoading.minimumDuration);
      await initialLoad;
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: CalendarEventComposer(
                controller: controller,
                connection: connection,
              ),
            ),
          ),
        ),
      );
      expect(find.text('Starts'), findsOneWidget);
      expect(find.text('Ends'), findsOneWidget);
      expect(find.textContaining('time zone'), findsNothing);
      expect(find.textContaining('UTC'), findsNothing);
      final inputs = tester
          .widgetList<TextFormField>(find.byType(TextFormField))
          .toList();
      expect(inputs, hasLength(5));
      expect(inputs[1].controller!.text, matches(r'^\d{2}$'));
      expect(inputs[2].controller!.text, matches(r'^\d{2}$'));
      await tester.ensureVisible(find.text('Create event'));
      await tester.tap(find.text('Create event'));
      await tester.pumpAndSettle();
      expect(gateway.proposals, 0);
      await tester.enterText(
        find.widgetWithText(TextFormField, 'Event title'),
        'Quiet focus',
      );
      await tester.ensureVisible(find.text('Create event'));
      await tester.tap(find.text('Create event'));
      await tester.pumpAndSettle();
      expect(gateway.proposals, 0);
      expect(gateway.directSubmissions, 1);
      expect(gateway.proposedStart!.isUtc, isFalse);
      expect(gateway.proposedTimezone, matches(r'^UTC[+-]\d{2}:\d{2}$'));
      expect(gateway.decisions, 0);
      expect(gateway.creates, 1);
      expect(controller.actions.single.needsReview, isFalse);
      expect(find.byType(ActionReviewDialog), findsNothing);
      expect(find.text('Approve & create'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );
}
