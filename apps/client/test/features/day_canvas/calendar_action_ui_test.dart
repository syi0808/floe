import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_action_panel.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:intl/intl.dart';

CalendarAction action({
  String status = 'pending',
  String person = 'person',
  DateTime? startsAt,
  DateTime? endsAt,
  DateTime? expiresAt,
  String? reason,
  bool direct = false,
  String? targetEventId,
  Map<String, dynamic>? agentOrigin,
}) {
  final now = DateTime.now().toUtc();
  return CalendarAction.fromJson({
    'direct': direct,
    'agent_origin': ?agentOrigin,
    if (targetEventId != null)
      'mutation': {
        'original': {'id': targetEventId},
        'delete': false,
      },
    'id': 'proposal',
    'person_id': person,
    'provider': 'fixture',
    'calendar_id': 'calendar',
    'calendar_name': 'Personal calendar',
    'title': 'Quiet focus',
    'connection_revision': 3,
    'created_at': now.subtract(const Duration(minutes: 1)).toIso8601String(),
    'expires_at': (expiresAt ?? now.add(const Duration(minutes: 14)))
        .toIso8601String(),
    'approved_at': status == 'approved' ? now.toIso8601String() : null,
    'execution_id': 'execution',
    'schedule': {
      'starts_at': (startsAt ?? now.add(const Duration(hours: 1)))
          .toIso8601String(),
      'ends_at': (endsAt ?? now.add(const Duration(hours: 2)))
          .toIso8601String(),
      'timezone': 'Asia/Seoul',
    },
    'state': {
      'status': status,
      'reason': reason ?? (status == 'unknown' ? 'timeout' : null),
      if (status == 'succeeded') 'external_id': 'external-event',
    },
  });
}

CalendarConnection connection({int revision = 3, String? error}) =>
    CalendarConnection(
      id: 'calendar',
      name: 'Personal calendar',
      provider: 'fixture',
      revision: revision,
      lastSuccessAt: DateTime.now(),
      error: error,
    );

class Gateway implements CalendarActionGateway {
  List<CalendarAction> saved = [action()];
  int decisions = 0;
  Completer<CalendarAction>? pending;
  bool fail = false;

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) async {
    if (fail) throw StateError('Unavailable');
    return saved;
  }

  @override
  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  }) async {
    decisions++;
    final result = action(
      status: decision == CalendarActionDecision.approve
          ? 'approved'
          : 'rejected',
    );
    saved = [result];
    if (fail) throw StateError('Response lost after save');
    return pending == null ? result : await pending!.future;
  }
}

void main() {
  setUpAll(() async {
    final font = FontLoader('Pretendard')
      ..addFont(rootBundle.load('assets/fonts/Pretendard-Regular.otf'))
      ..addFont(rootBundle.load('assets/fonts/Pretendard-SemiBold.otf'));
    await font.load();
    final icons = FontLoader('packages/lucide_icons_flutter/Lucide')
      ..addFont(
        rootBundle.load('packages/lucide_icons_flutter/assets/lucide.ttf'),
      );
    await icons.load();
    final material = FontLoader('MaterialIcons')
      ..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'));
    await material.load();
  });

  Map<String, dynamic> origin() => {
    'schema_version': 1,
    'session_id': 'saved-conversation',
    'invocation_id': 'proposal',
    'package': {'kind': 'expert', 'id': 'floe.schedule', 'version': '1.0.0'},
    'data_class': 'synthetic',
  };

  test('Agent action attribution is optional, scoped and versioned', () {
    expect(action().agentOrigin, isNull);
    final attributed = action(agentOrigin: origin());
    expect(attributed.agentOrigin!.expertId, 'floe.schedule');
    expect(attributed.agentOrigin!.sessionId, 'saved-conversation');
    expect(attributed.status, CalendarActionStatus.pending);
    for (final change in [
      {'schema_version': 2},
      {'invocation_id': 'another-action'},
      {'data_class': 'highly_sensitive'},
    ]) {
      expect(
        () => action(agentOrigin: {...origin(), ...change}),
        throwsFormatException,
      );
    }
  });
  test(
    'approval guards stale, expired, disconnected and foreign proposals',
    () async {
      final gateway = Gateway();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      addTearDown(controller.dispose);
      await controller.load();
      final proposal = controller.actions.single;
      expect(
        controller.canApprove(proposal, connection(), DateTime.now()),
        isTrue,
      );
      expect(controller.canApprove(proposal, null, DateTime.now()), isFalse);
      expect(
        controller.canApprove(
          proposal,
          connection(revision: 4),
          DateTime.now(),
        ),
        isFalse,
      );
      expect(
        controller.canApprove(
          proposal,
          connection(error: 'denied'),
          DateTime.now(),
        ),
        isFalse,
      );
      expect(
        controller.canApprove(proposal, connection(), proposal.expiresAt),
        isFalse,
      );
      expect(
        controller.canApprove(
          proposal,
          connection(),
          proposal.createdAt.subtract(const Duration(seconds: 1)),
        ),
        isFalse,
      );
      gateway.saved = [action(person: 'other')];
      await controller.load();
      expect(controller.needsReload, isTrue);
      expect(controller.failed, isTrue);
    },
  );

  test(
    'duplicate decisions are suppressed and response loss requires lookup',
    () async {
      final gateway = Gateway()..pending = Completer<CalendarAction>();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      addTearDown(controller.dispose);
      await controller.load();
      final first = controller.decide(
        'proposal',
        CalendarActionDecision.approve,
        connection(),
        DateTime.now(),
      );
      await controller.decide(
        'proposal',
        CalendarActionDecision.approve,
        connection(),
        DateTime.now(),
      );
      expect(gateway.decisions, 1);
      gateway.pending!.completeError(StateError('Lost response'));
      await first;
      expect(controller.needsReload, isTrue);
      await controller.decide(
        'proposal',
        CalendarActionDecision.reject,
        connection(),
        DateTime.now(),
      );
      expect(gateway.decisions, 1);
      await controller.load();
      expect(controller.actions.single.status, CalendarActionStatus.approved);
      expect(controller.needsReload, isFalse);
    },
  );

  test('a decision can finish after its owner is disposed', () async {
    final gateway = Gateway()..pending = Completer<CalendarAction>();
    final controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    await controller.load();
    final result = controller.decide(
      'proposal',
      CalendarActionDecision.reject,
      null,
      DateTime.now(),
    );
    controller.dispose();
    gateway.pending!.complete(action(status: 'rejected'));
    await result;
  });

  Future<void> mount(
    WidgetTester tester,
    CalendarActionController controller,
    double width,
  ) async {
    tester.view.physicalSize = Size(width, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ReviewRequestPanel(
              controller: controller,
              connection: connection,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Future<void> loadController(
    WidgetTester tester,
    CalendarActionController controller,
  ) async {
    final loading = controller.load();
    await tester.pump(
      FloeLoading.minimumDuration + const Duration(milliseconds: 1),
    );
    await loading;
  }

  for (final width in [390.0, 1200.0]) {
    testWidgets('review, close and approve without creating at $width', (
      tester,
    ) async {
      final gateway = Gateway();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await mount(tester, controller, width);
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      expect(find.text('Destination calendar'), findsOneWidget);
      expect(find.textContaining('time zone'), findsNothing);
      expect(find.textContaining('UTC'), findsNothing);
      expect(find.text('When'), findsOneWidget);
      expect(find.text('Execution ID'), findsNothing);
      expect(find.text('proposal'), findsNothing);
      final proposal = controller.actions.single;
      final start = proposal.startsAt.toLocal();
      final end = proposal.endsAt.toLocal();
      if (start.year == end.year &&
          start.month == end.month &&
          start.day == end.day) {
        expect(
          find.text(
            '${DateFormat.yMMMd('en').add_jm().format(start)} – ${DateFormat.jm('en').format(end)}',
          ),
          findsOneWidget,
        );
      }
      await tester.ensureVisible(find.text('Technical details'));
      await tester.tap(find.text('Technical details'));
      await tester.pumpAndSettle();
      expect(find.text('Starts'), findsOneWidget);
      expect(find.textContaining('UTC'), findsNothing);
      expect(find.textContaining('Asia/Seoul'), findsNothing);
      expect(find.text('Execution ID'), findsOneWidget);
      expect(find.text('proposal'), findsOneWidget);
      await tester.ensureVisible(find.byTooltip('Close'));
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
      expect(gateway.decisions, 0);
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Save approval only'));
      await tester.tap(find.text('Save approval only'));
      await tester.pumpAndSettle();
      expect(gateway.decisions, 1);
      expect(find.text('Save approval only'), findsNothing);
      expect(controller.actions.single.status, CalendarActionStatus.approved);
      expect(
        find.descendant(
          of: find.byType(ReviewRequestPanel),
          matching: find.text('Review request'),
        ),
        findsNothing,
      );
      expect(
        find.text('Approval saved. No event has been created by this app.'),
        findsWidgets,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    });
  }

  testWidgets(
    'Expert action attribution stays readable at 320 pixels and large text',
    (tester) async {
      tester.platformDispatcher.textScaleFactorTestValue = 2;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      final gateway = Gateway()
        ..saved = [
          action(
            agentOrigin: origin(),
            startsAt: DateTime(2099, 1, 1, 10).toUtc(),
            endsAt: DateTime(2099, 1, 1, 11).toUtc(),
          ),
        ];
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await mount(tester, controller, 320);
      await tester.ensureVisible(find.text('Review request'));
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      expect(find.text('Suggested by Floe'), findsOneWidget);
      expect(find.text('saved-conversation'), findsNothing);
      expect(tester.takeException(), isNull);
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('../../goldens/agent_action_review.png'),
      );
      await tester.ensureVisible(find.text('Technical details'));
      await tester.tap(find.text('Technical details'));
      await tester.pumpAndSettle();
      expect(find.text('Schedule planning'), findsOneWidget);
      expect(find.text('saved-conversation'), findsOneWidget);
      expect(find.text('Expert call ID'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(gateway.decisions, 0);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );

  testWidgets('activity reload is positioned in the page header', (
    tester,
  ) async {
    final gateway = Gateway()..saved = [action(status: 'succeeded')];
    final controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    await loadController(tester, controller);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: ActivityPanel(controller: controller, connection: connection),
        ),
      ),
    );

    final titleBounds = tester.getRect(find.text('Activity'));
    final reloadBounds = tester.getRect(find.byTooltip('Reload activity'));
    final activityBounds = tester.getRect(find.text('Quiet focus'));
    expect(reloadBounds.center.dy, closeTo(titleBounds.center.dy, 8));
    expect(reloadBounds.right, greaterThan(activityBounds.center.dx));
    expect(reloadBounds.bottom, lessThan(activityBounds.top));

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets(
    'overnight review shows both local dates and a plain-language block reason',
    (tester) async {
      final startsAt = DateTime(2026, 9, 6, 23, 45);
      final endsAt = DateTime(2026, 9, 7, 0, 30);
      final gateway = Gateway()
        ..saved = [
          action(
            status: 'blocked',
            startsAt: startsAt,
            endsAt: endsAt,
            reason: 'schedule_conflict',
          ),
        ];
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: ActivityPanel(controller: controller, connection: connection),
          ),
        ),
      );
      await tester.tap(find.text('View details'));
      await tester.pumpAndSettle();
      final format = DateFormat.yMMMd('en').add_jm();
      expect(
        find.text('${format.format(startsAt)} – ${format.format(endsAt)}'),
        findsOneWidget,
      );
      expect(
        find.text('Another event overlaps this time. Nothing was created.'),
        findsOneWidget,
      );
      expect(find.text('schedule_conflict'), findsNothing);
      expect(find.text('Save approval only'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );

  testWidgets('expired review explains why approval is unavailable', (
    tester,
  ) async {
    final gateway = Gateway()
      ..saved = [
        action(expiresAt: DateTime.now().subtract(const Duration(seconds: 1))),
      ];
    final controller = CalendarActionController(
      gateway: gateway,
      personId: 'person',
    );
    await loadController(tester, controller);
    await mount(tester, controller, 390);
    await tester.tap(find.text('Review request'));
    await tester.pumpAndSettle();
    expect(
      find.text('This review request has expired. Nothing was created.'),
      findsOneWidget,
    );
    expect(find.text('Technical details'), findsOneWidget);
    expect(find.text('execution'), findsNothing);
    expect(gateway.decisions, 0);
    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets(
    'closing during a decision preserves its single in-flight request',
    (tester) async {
      final gateway = Gateway()..pending = Completer<CalendarAction>();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await mount(tester, controller, 1200);
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Decline'));
      await tester.tap(find.text('Decline'));
      await tester.pump();
      await tester.ensureVisible(find.byTooltip('Close'));
      await tester.tap(find.byTooltip('Close'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      await tester.tap(find.text('Review request'));
      await tester.pump(const Duration(milliseconds: 300));
      expect(gateway.decisions, 1);
      gateway.pending!.complete(action(status: 'rejected'));
      await tester.pump(FloeLoading.minimumDuration);
      await tester.pumpAndSettle();
      expect(find.text('Declined. No event was created.'), findsWidgets);
      expect(find.text('Save approval only'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );

  testWidgets(
    'failed decision keeps review visible and disables decisions until reload',
    (tester) async {
      final gateway = Gateway();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await mount(tester, controller, 390);
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      gateway.fail = true;
      await tester.ensureVisible(find.text('Save approval only'));
      await tester.tap(find.text('Save approval only'));
      await tester.pumpAndSettle();
      expect(controller.needsReload, isTrue);
      expect(find.byType(ActionReviewDialog), findsOneWidget);
      expect(
        tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
        isNull,
      );
      gateway.fail = false;
      final reload = find.descendant(
        of: find.byType(ActionReviewDialog),
        matching: find.text('Reload reviews'),
      );
      await tester.ensureVisible(reload);
      await tester.tap(reload);
      await tester.pumpAndSettle();
      expect(controller.actions.single.status, CalendarActionStatus.approved);
      expect(gateway.decisions, 1);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );

  testWidgets(
    'Escape closes without a decision; terminal states never offer create',
    (tester) async {
      final gateway = Gateway();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await loadController(tester, controller);
      await mount(tester, controller, 1200);
      await tester.tap(find.text('Review request'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byType(ActionReviewDialog), findsNothing);
      expect(gateway.decisions, 0);
      for (final status in ['rejected', 'blocked', 'succeeded']) {
        gateway.saved = [action(status: status)];
        await loadController(tester, controller);
        await tester.pumpAndSettle();
        expect(find.text('Review request'), findsNothing);
        await tester.pumpWidget(
          MaterialApp(
            theme: FloeTheme.light,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: ActivityPanel(
                controller: controller,
                connection: connection,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        await tester.tap(find.text('View details'));
        await tester.pumpAndSettle();
        expect(find.text('Save approval only'), findsNothing);
        expect(find.text('Decline'), findsNothing);
        expect(find.text('Execution ID'), findsNothing);
        await tester.tap(find.byTooltip('Close'));
        await tester.pumpAndSettle();
      }
      expect(gateway.decisions, 0);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );
}
