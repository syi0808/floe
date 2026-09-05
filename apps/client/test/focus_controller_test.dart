import 'dart:async';

import 'package:floe_client/features/day_canvas/application/focus_controller.dart';
import 'package:floe_client/features/day_canvas/application/focus_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/domain/focus_models.dart';
import 'package:flutter_test/flutter_test.dart';

final now = DateTime.utc(2026, 9, 5);
final query = DayQuery(
  personId: 'person',
  date: now,
  now: now,
  timezoneOffsetSeconds: 0,
);
const preference = FocusPreferenceValue(
  startMinute: 600,
  endMinute: 900,
  durationMinutes: 45,
);

class FixtureGateway implements FocusGateway {
  FocusPreference? saved;
  String? failure;
  Completer<FocusProposal>? pending;
  int calls = 0;
  bool lastAllowExternal = false;

  @override
  Future<FocusPreference?> loadFocusPreference(DayQuery query) async {
    if (failure case final code?) throw FocusGatewayException(code);
    return saved;
  }

  @override
  Future<FocusPreference> saveFocusPreference(
    DayQuery query,
    int expectedRevision,
    FocusPreferenceValue? value,
  ) async {
    if (failure case final code?) throw FocusGatewayException(code);
    expect(expectedRevision, saved?.revision ?? 0);
    return saved = FocusPreference(
      personId: query.personId,
      revision: expectedRevision + 1,
      source: 'user_entered',
      updatedAt: now,
      value: value,
    );
  }

  @override
  Future<FocusProposal> suggestFocus(
    DayQuery query, {
    bool allowExternal = false,
  }) async {
    calls++;
    lastAllowExternal = allowExternal;
    if (failure case final code?) throw FocusGatewayException(code);
    return pending == null ? proposal() : pending!.future;
  }

  FocusProposal proposal() => FocusProposal(
    id: 'proposal',
    personId: 'person',
    startsAt: now.add(const Duration(hours: 10)),
    endsAt: now.add(const Duration(hours: 11)),
    timezoneOffsetSeconds: 0,
    reason: 'An unoccupied interval.',
    evidence: const [FocusEvidence(id: 'schedule', label: 'Loaded schedule')],
    inferenceClass: 'fixture',
    calendarWarning: true,
  );
}

void main() {
  test(
    'preference load, save, edit and delete clear previous suggestions',
    () async {
      final gateway = FixtureGateway();
      final controller = FocusController(gateway: gateway, query: query);
      addTearDown(controller.dispose);
      expect(await controller.load(), isTrue);
      expect(controller.preference, isNull);
      expect(await controller.save(preference), isTrue);
      expect(controller.preference?.revision, 1);
      expect(await controller.suggest(), isTrue);
      expect(gateway.lastAllowExternal, isFalse);
      expect(await controller.suggest(allowExternal: true), isTrue);
      expect(gateway.lastAllowExternal, isTrue);
      expect(controller.proposal, isNotNull);
      expect(await controller.save(preference), isTrue);
      expect(controller.preference?.revision, 2);
      expect(controller.proposal, isNull);
      expect(await controller.save(null), isTrue);
      expect(controller.preference?.value, isNull);
      expect(controller.preference?.revision, 3);
    },
  );

  test('each typed model error clears proposal and permits retry', () async {
    for (final code in [
      'model_timeout',
      'model_unavailable',
      'invalid_proposal',
      'no_focus_slot',
      'external_transfer_denied',
    ]) {
      final gateway = FixtureGateway();
      final controller = FocusController(gateway: gateway, query: query);
      await controller.load();
      await controller.suggest();
      gateway.failure = code;
      expect(await controller.suggest(), isFalse);
      expect(controller.proposal, isNull);
      expect(controller.errorCode, code);
      expect(controller.pending, isFalse);
      gateway.failure = null;
      expect(await controller.suggest(), isTrue);
      expect(controller.errorCode, isNull);
      controller.dispose();
    }
  });

  test('load failure and revision conflicts require a reload', () async {
    final gateway = FixtureGateway()..failure = 'storage';
    final controller = FocusController(gateway: gateway, query: query);
    addTearDown(controller.dispose);
    expect(await controller.load(), isFalse);
    expect(await controller.save(preference), isFalse);
    expect(await controller.suggest(), isFalse);
    gateway.failure = null;
    await controller.load();
    gateway.failure = 'conflict';
    expect(await controller.save(preference), isFalse);
    expect(controller.loaded, isFalse);
    gateway.failure = null;
    expect(await controller.load(), isTrue);
    expect(await controller.save(preference), isTrue);
  });

  test(
    'duplicate requests and late responses after disposal are ignored',
    () async {
      final gateway = FixtureGateway()..pending = Completer<FocusProposal>();
      final controller = FocusController(gateway: gateway, query: query);
      await controller.load();
      final request = controller.suggest();
      expect(controller.pending, isTrue);
      expect(await controller.suggest(), isFalse);
      expect(await controller.save(preference), isFalse);
      expect(gateway.calls, 1);
      controller.dispose();
      gateway.pending!.complete(gateway.proposal());
      expect(await request, isFalse);
      expect(controller.proposal, isNull);
    },
  );

  test('proposal decoder preserves offset, Person and source references', () {
    final proposal = FocusProposal.fromJson({
      'id': 'proposal',
      'person_id': 'person',
      'timezone_offset_seconds': 32400,
      'slot': {
        'starts_at': '2026-09-05T01:00:00Z',
        'ends_at': '2026-09-05T02:00:00Z',
      },
      'reason': 'Unoccupied',
      'evidence': [
        {'id': 'preference', 'label': 'User-entered preference'},
      ],
      'inference_class': 'fixture',
      'calendar_warning': true,
    });
    expect(proposal.personId, 'person');
    expect(proposal.startsAt, DateTime.utc(2026, 9, 5, 1));
    expect(proposal.timezoneOffsetSeconds, 32400);
    expect(proposal.evidence.single.id, 'preference');
  });
}
