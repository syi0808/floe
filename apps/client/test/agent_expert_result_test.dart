import 'dart:convert';

import 'package:floe_client/features/agent/agent_expert_result.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/expert_result.dart';

AgentExpertResult? parse(Map<String, Object?> result) =>
    AgentExpertResult.tryParse(
      jsonEncode(result),
      callId: 'call-1',
      personId: 'test',
    );

void main() {
  test('versioned Expert evidence has bounded typed sample insights', () {
    final result = parse(expertResultFixture())!;
    expect(result.expert, 'floe.schedule');
    expect(result.source, 'fixture.synthetic.timeline');
    expect(result.summary, contains('one-hour focus window'));
    expect(result.insights.first.title, 'Design review');
    expect(result.insights.last.start!.hour, 11);
    expect(result.insights.last.end!.hour, 12);
    final empty = expertResultFixture()
      ..['insights'] = [
        {'kind': 'no_focus_window'},
      ];
    expect(parse(empty)!.insights.single.kind, 'no_focus_window');
  });

  test(
    'wrong scope, version, private fields and personal payloads fail closed',
    () {
      for (final change in [
        {'schema_version': 2},
        {'invocation_id': 'other'},
        {'person_id': 'other'},
        {'data_class': 'personal'},
        {'reasoning': 'must not show'},
        {'state_revision': -1},
        {'view_calls': 2},
        {'summary': 'summary without calls', 'model_calls': 0},
        {'summary': null, 'model_calls': 1},
        {'summary': null, 'model_calls': 2},
        {'summary': 'too many calls', 'model_calls': 11},
        {'summary': 'x' * 2049, 'model_calls': 2},
        {'insights': <Object?>[]},
        {
          'action_proposals': [
            {'kind': 'execute'},
          ],
        },
        {'source_handle': 'x' * 129},
      ]) {
        expect(parse(expertResultFixture()..addAll(change)), isNull);
      }
    },
  );

  test('malformed and oversized evidence never reaches display', () {
    for (final output in [null, '', 'legacy text', '[1]', '{', 'x' * 16385]) {
      expect(
        AgentExpertResult.tryParse(output, callId: 'call-1', personId: 'test'),
        isNull,
      );
    }
    for (final insight in [
      {'kind': 'focus_window', 'starts_at_unix_ms': 1, 'ends_at_unix_ms': 0},
      {'kind': 'focus_window', 'starts_at_unix_ms': -1, 'ends_at_unix_ms': 10},
      {
        'kind': 'focus_window',
        'starts_at_unix_ms': 0,
        'ends_at_unix_ms': 86400001,
      },
      {'kind': 'focus_window', 'starts_at_unix_ms': '0', 'ends_at_unix_ms': 10},
      {'kind': 'execute', 'starts_at_unix_ms': 0, 'ends_at_unix_ms': 10},
    ]) {
      expect(parse(expertResultFixture()..['insights'] = [insight]), isNull);
    }
  });
}
