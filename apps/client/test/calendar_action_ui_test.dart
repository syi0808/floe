import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_controller.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_action_panel.dart';
import 'package:floe_client/l10n/app_localizations.dart';

CalendarAction action({String status = 'pending', String person = 'person'}) {
  final now = DateTime.now().toUtc();
  return CalendarAction.fromJson({
    'id': 'proposal',
    'person_id': person,
    'provider': 'fixture',
    'calendar_id': 'calendar',
    'calendar_name': 'Personal calendar',
    'title': 'Quiet focus',
    'connection_revision': 3,
    'created_at': now.subtract(const Duration(minutes: 1)).toIso8601String(),
    'expires_at': now.add(const Duration(minutes: 14)).toIso8601String(),
    'approved_at': status == 'approved' ? now.toIso8601String() : null,
    'execution_id': 'execution',
    'schedule': {
      'starts_at': now.add(const Duration(hours: 1)).toIso8601String(),
      'ends_at': now.add(const Duration(hours: 2)).toIso8601String(),
      'timezone': 'Asia/Seoul',
    },
    'state': {
      'status': status,
      if (status == 'unknown') 'reason': 'timeout',
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
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: CalendarActionPanel(
              controller: controller,
              connection: connection,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
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
      await controller.load();
      await mount(tester, controller, width);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.text('Destination calendar'), findsOneWidget);
      expect(find.text('Asia/Seoul'), findsOneWidget);
      expect(find.text('Starts (UTC)'), findsOneWidget);
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
      expect(gateway.decisions, 0);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Save approval only'));
      await tester.tap(find.text('Save approval only'));
      await tester.pumpAndSettle();
      expect(gateway.decisions, 1);
      expect(find.text('Save approval only'), findsNothing);
      expect(controller.actions.single.status, CalendarActionStatus.approved);
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
    'closing during a decision preserves its single in-flight request',
    (tester) async {
      final gateway = Gateway()..pending = Completer<CalendarAction>();
      final controller = CalendarActionController(
        gateway: gateway,
        personId: 'person',
      );
      await controller.load();
      await mount(tester, controller, 1200);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Decline'));
      await tester.tap(find.text('Decline'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.byTooltip('Close'));
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(gateway.decisions, 1);
      gateway.pending!.complete(action(status: 'rejected'));
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
      await controller.load();
      await mount(tester, controller, 390);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      gateway.fail = true;
      await tester.ensureVisible(find.text('Save approval only'));
      await tester.tap(find.text('Save approval only'));
      await tester.pumpAndSettle();
      expect(controller.needsReload, isTrue);
      expect(find.byType(CalendarActionDialog), findsOneWidget);
      expect(
        tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
        isNull,
      );
      gateway.fail = false;
      final reload = find.descendant(
        of: find.byType(CalendarActionDialog),
        matching: find.text('Reload proposals'),
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
      await controller.load();
      await mount(tester, controller, 1200);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byType(CalendarActionDialog), findsNothing);
      expect(gateway.decisions, 0);
      for (final status in [
        'rejected',
        'executing',
        'unknown',
        'blocked',
        'succeeded',
      ]) {
        gateway.saved = [action(status: status)];
        await controller.load();
        await tester.pumpAndSettle();
        await tester.tap(find.text('Review proposal'));
        await tester.pumpAndSettle();
        expect(find.text('Save approval only'), findsNothing);
        expect(find.text('Decline'), findsNothing);
        expect(find.text('Execution ID'), findsOneWidget);
        await tester.tap(find.byTooltip('Close'));
        await tester.pumpAndSettle();
      }
      expect(gateway.decisions, 0);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );
}
