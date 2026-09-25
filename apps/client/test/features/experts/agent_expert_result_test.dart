import 'dart:convert';

import 'package:floe_client/features/experts/domain/agent_expert_result.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/expert_result.dart';

AgentExpertResult? parse(Map<String, Object?> result) =>
    AgentExpertResult.tryParse(
      jsonEncode(result),
      callId: '00000000-0000-4000-8000-000000000003',
      personId: '00000000-0000-4000-8000-000000000001',
      allowedDataClasses: const ['personal'],
    );

void main() {
  test('Rust-settled one-call Schedule result parses', () {
    final result = parse(rustExpertResultFixture())!;
    expect(result.expert, 'floe.builtin.schedule');
    expect(result.source, startsWith('calendar.observe:'));
    expect(result.summary, 'One focus window');
    expect(result.insights.single.kind, 'focus_window');
    expect(result.proposal, isNotNull);
  });

  test('Rust-valid multi-view and summary-only results parse', () {
    expect(parse(rustExpertResultFixture()..['view_calls'] = 2), isNotNull);
    final summaryOnly = rustExpertResultFixture()
      ..['insights'] = <Object?>[]
      ..['action_proposals'] = <Object?>[];
    expect(parse(summaryOnly)!.insights, isEmpty);
  });

  test(
    'wrong scope, version, private fields and personal payloads fail closed',
    () {
      for (final change in [
        {'schema_version': 2},
        {'invocation_id': 'other'},
        {'person_id': 'other'},
        {'data_class': 'synthetic'},
        {'reasoning': 'must not show'},
        {'state_revision': -1},
        {'view_calls': 0},
        {'view_calls': 9},
        {'summary': 'summary without calls', 'model_calls': 0},
        {'summary': null, 'model_calls': 1},
        {'summary': null, 'model_calls': 2},
        {'summary': 'too many calls', 'model_calls': 11},
        {'summary': 'x' * 2049, 'model_calls': 2},
        {'insights': <Object?>[], 'summary': null, 'model_calls': 0},
        {
          'action_proposals': [
            {'kind': 'execute'},
          ],
        },
        {'source_handle': 'x' * 129},
      ]) {
        expect(parse(rustExpertResultFixture()..addAll(change)), isNull);
      }
    },
  );

  test('malformed and oversized evidence never reaches display', () {
    for (final output in [null, '', 'plain text', '[1]', '{', 'x' * 16385]) {
      expect(
        AgentExpertResult.tryParse(
          output,
          callId: '00000000-0000-4000-8000-000000000003',
          personId: '00000000-0000-4000-8000-000000000001',
        ),
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
      expect(
        parse(rustExpertResultFixture()..['insights'] = [insight]),
        isNull,
      );
    }
  });
}
