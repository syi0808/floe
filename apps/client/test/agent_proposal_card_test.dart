import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_panel.dart';
import 'package:floe_client/features/agent/agent_proposal_card.dart';
import 'package:floe_client/features/day_canvas/application/calendar_action_gateway.dart';
import 'package:floe_client/features/day_canvas/application/day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_action_panel.dart';
import 'package:floe_client/features/day_canvas/presentation/personal_day_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_proposal.dart';

Widget app(Widget child, {double scale = 1}) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(
      context,
    ).copyWith(textScaler: TextScaler.linear(scale), disableAnimations: true),
    child: child!,
  ),
  home: Scaffold(body: child),
);

Future<void> tap(WidgetTester tester, String text) async {
  await tester.ensureVisible(find.text(text));
  await tester.tap(find.text(text));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() async {
    await (FontLoader('Pretendard')
          ..addFont(rootBundle.load('assets/fonts/Pretendard-Regular.otf'))
          ..addFont(rootBundle.load('assets/fonts/Pretendard-SemiBold.otf')))
        .load();
    await (FontLoader(
      'MaterialIcons',
    )..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'))).load();
    await (FontLoader('packages/lucide_icons_flutter/Lucide')..addFont(
          rootBundle.load('packages/lucide_icons_flutter/assets/lucide.ttf'),
        ))
        .load();
  });

  for (final width in [520.0, 320.0]) {
    testWidgets(
      'proposal card is explicit, read-only and usable at width $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 1100);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final gateway = TestProposalGateway();
        final controller = AgentController(
          gateway: gateway,
          personId: proposalPerson,
        );
        addTearDown(controller.dispose);
        await controller.load();
        final opened = <String>[];
        await tester.pumpWidget(
          app(
            AgentPanel(
              controller: controller,
              onClose: () {},
              onOpenAction: (id) async => opened.add(id),
            ),
            scale: width == 320 ? 2 : 1,
          ),
        );
        await tap(tester, 'View source');
        expect(find.text('Suggested focus time'), findsOneWidget);
        expect(gateway.requests, isEmpty);
        expect(find.text('Open Calendar action'), findsNothing);
        await tap(tester, 'Check saved action');
        expect(find.textContaining('Recorded action status:'), findsOneWidget);
        expect(opened, isEmpty);
        expect(gateway.begins, 0);
        if (width == 520) {
          await expectLater(
            find.byType(AgentPanel),
            matchesGoldenFile('goldens/agent_proposal_card.png'),
          );
        }
        await tap(tester, 'Open Calendar action');
        expect(opened, [proposalCall]);
        gateway.response = inspectionJson(status: null);
        await tap(tester, 'Check saved action');
        expect(find.textContaining('Nothing was created.'), findsOneWidget);
        expect(find.text('Open Calendar action'), findsNothing);
        gateway.inspectionFailure = 'storage_unavailable';
        await tap(tester, 'Check saved action');
        expect(
          find.textContaining('This does not mean it is absent.'),
          findsOneWidget,
        );
        expect(find.textContaining('Nothing was created.'), findsNothing);
        expect(tester.takeException(), isNull);
        await controller.closeView();
        await tester.pumpAndSettle();
        expect(find.byType(AgentProposalCard), findsNothing);
      },
    );
  }

  for (final status in [
    'pending',
    'approved',
    'rejected',
    'executing',
    'blocked',
    'unknown',
    'succeeded',
  ]) {
    testWidgets(
      'recorded $status is displayed without approval or retry controls',
      (tester) async {
        final gateway = TestProposalGateway()
          ..response = inspectionJson(status: status);
        final controller = AgentController(
          gateway: gateway,
          personId: proposalPerson,
        );
        addTearDown(controller.dispose);
        await controller.load();
        final message = controller.messages.single as AgentCapabilityMessage;
        await controller.inspectProposal(message);
        await tester.pumpWidget(
          app(AgentProposalCard(controller: controller, message: message)),
        );
        expect(find.textContaining('Recorded action status:'), findsOneWidget);
        expect(find.text('Approve'), findsNothing);
        expect(find.text('Retry'), findsNothing);
        expect(find.text('Open Calendar action'), findsNothing);
        expect(gateway.requests.length, 1);
        expect(gateway.begins, 0);
      },
    );
  }

  for (final width in [1280.0, 390.0]) {
    testWidgets(
      'Today opens the existing S3 action without deciding at width $width',
      (tester) async {
        tester.view.physicalSize = Size(width, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final agent = TestProposalGateway();
        final day = ProposalDayGateway();
        await tester.pumpWidget(
          app(
            PersonalDayScreen(
              gateway: day,
              agentGateway: agent,
              query: DayQuery(
                personId: proposalPerson,
                date: DateTime(2050, 1, 15),
                timezoneOffsetSeconds: 0,
                now: DateTime(2050, 1, 15, 9),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        await tap(tester, 'Floe is here to help');
        await tap(tester, 'View source');
        await tap(tester, 'Check saved action');
        await tap(tester, 'Open Calendar action');
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();
        final dialog = tester.widget<ActionReviewDialog>(
          find.byType(ActionReviewDialog),
        );
        expect(dialog.actionId, proposalCall);
        expect(
          find.descendant(
            of: find.byType(ActionReviewDialog),
            matching: find.text('Focus time'),
          ),
          findsOneWidget,
        );
        expect(day.loads, greaterThanOrEqualTo(2));
        expect(day.decisions, 0);
        expect(agent.requests.length, 1);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpAndSettle();
      },
    );
  }
}

class ProposalDayGateway implements DayGateway, CalendarActionGateway {
  int decisions = 0;
  int loads = 0;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) =>
      FakeDayGateway().loadDay(query);

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) async {
    loads++;
    return [
      CalendarAction.fromJson({
        'id': proposalCall,
        'person_id': personId,
        'provider': 'fixture',
        'calendar_id': 'synthetic-calendar',
        'calendar_name': 'Synthetic calendar',
        'title': 'Focus time',
        'connection_revision': 1,
        'created_at': '2050-01-15T09:00:00Z',
        'expires_at': '2050-01-15T09:02:00Z',
        'approved_at': null,
        'execution_id': proposalExecution,
        'schedule': {
          'starts_at': '2050-01-15T10:00:00Z',
          'ends_at': '2050-01-15T11:00:00Z',
          'timezone': 'UTC',
        },
        'state': {'status': 'pending'},
        'agent_origin': {
          'schema_version': 1,
          'session_id': proposalSession,
          'invocation_id': proposalCall,
          'package': {'kind': 'expert', 'id': 'schedule', 'version': '1.0.0'},
          'data_class': 'synthetic',
        },
      }),
    ];
  }

  @override
  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  }) async {
    decisions++;
    throw StateError('Inspection must not decide');
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnsupportedError('Unexpected day mutation');
}
