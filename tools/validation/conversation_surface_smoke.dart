// SDK-only codec checks. This does not replace Flutter analyze/test or native QA.
import 'dart:convert';

import '../../apps/client/lib/features/conversation/domain/conversation_surface.dart';

var checks = 0;
void check(bool value, String reason) {
  if (!value) throw StateError(reason);
  checks++;
}

void rejects(void Function() action, String reason) {
  var failed = false;
  try {
    action();
  } catch (_) {
    failed = true;
  }
  check(failed, reason);
}

Map<String, Object?> copy(Map<String, Object?> value) =>
    Map<String, Object?>.from(jsonDecode(jsonEncode(value)) as Map);
String id(int n) =>
    '00000000-0000-0000-0000-${n.toRadixString(16).padLeft(12, '0')}';
Map<String, Object?> anchor(int n, {int session = 1}) => {
  'session_id': id(session),
  'sequence': n,
  'digest': n.toRadixString(16).padLeft(64, '0'),
};
Map<String, Object?> input() => {
  'message_id': id(2),
  'before': anchor(0),
  'through': anchor(1),
  'content': {'kind': 'input', 'text': '원래 요청', 'origin': 'text'},
};
Map<String, Object?> receipt() => {
  'command_id': id(3),
  'work_id': id(4),
  'execution_command_id': id(5),
  'input_id': id(2),
  'anchor': anchor(1),
  'origin': {'kind': 'text'},
  'runtime_epoch': 1,
};
void main() {
  final empty = AppSurfaceSnapshot.fromJson({
    'through': anchor(0),
    'messages': <Object?>[],
    'older': null,
  });
  check(empty.messages.isEmpty && empty.older == null, 'empty history');
  final page = <String, Object?>{
    'through': anchor(1),
    'messages': [input()],
    'older': null,
  };
  check(
    (AppSurfaceSnapshot.fromJson(page).messages.single as AppSurfaceInput)
            .text ==
        '원래 요청',
    'original UTF-8 text',
  );
  final malformed = copy(page);
  malformed['through'] = anchor(2);
  rejects(() => AppSurfaceSnapshot.fromJson(malformed), 'head mismatch');
  final injected = input()..['turn_id'] = id(8);
  rejects(
    () => AppSurfaceMessage.fromJson(injected),
    'Surface is not a Core turn',
  );
  final provider = input();
  (provider['content'] as Map)['provider_event_id'] = 'external';
  rejects(
    () => AppSurfaceMessage.fromJson(provider),
    'provider identity exclusion',
  );
  final oversize = input();
  (oversize['content'] as Map)['text'] = '가' * 3000;
  rejects(() => AppSurfaceMessage.fromJson(oversize), 'input byte bound');
  final result = <String, Object?>{
    'message_id': id(6),
    'before': anchor(1),
    'through': anchor(2),
    'content': {
      'kind': 'result',
      'run_id': id(7),
      'completeness': 'partial',
      'text': 'Original partial answer',
    },
  };
  check(
    (AppSurfaceMessage.fromJson(result) as AppSurfaceResult).completeness ==
        AppSurfaceResultCompleteness.partial,
    'partial is not complete',
  );
  final older = AppSurfaceSnapshot.fromJson({
    'through': anchor(2),
    'messages': [result],
    'older': anchor(1),
  });
  check(
    older.older == AppSurfaceAnchor.fromJson(anchor(1)),
    'older prefix exactness',
  );
  rejects(
    () => AppSurfaceSnapshot.fromJson({
      'through': anchor(2),
      'messages': [result],
      'older': null,
    }),
    'missing cursor',
  );
  final duplicate = copy(result)..['message_id'] = id(2);
  rejects(
    () => AppSurfaceSnapshot.fromJson({
      'through': anchor(2),
      'messages': [input(), duplicate],
      'older': null,
    }),
    'duplicate message identity',
  );
  final wrongSession = copy(result)..['through'] = anchor(2, session: 9);
  rejects(
    () => AppSurfaceMessage.fromJson(wrongSession),
    'cross-session chain',
  );
  final work = AppSurfaceWorkSnapshot.fromJson({
    'receipt': receipt(),
    'input_text': '원래 요청',
    'state': {'kind': 'linked', 'run_id': id(7)},
    'current_run_id': id(8),
  });
  check(
    work.initialRunId != work.currentRunId,
    'descendant does not rewrite initial claim',
  );
  rejects(
    () => AppSurfaceWorkSnapshot.fromJson({
      'receipt': receipt(),
      'input_text': '원래 요청',
      'state': {'kind': 'linked', 'run_id': id(7)},
      'current_run_id': null,
    }),
    'linked current Run required',
  );
  rejects(
    () => AppSurfaceWorkSnapshot.fromJson({
      'receipt': receipt(),
      'input_text': '원래 요청',
      'state': {'kind': 'queued'},
      'current_run_id': id(8),
    }),
    'queued cannot invent Run',
  );
  final retry = receipt()
    ..['origin'] = {'kind': 'retry', 'source_run_id': id(7)};
  check(
    AppSurfaceWorkReceipt.fromJson(retry).sourceRunId == id(7),
    'typed retry source',
  );
  final leaked = receipt()
    ..['origin'] = {'kind': 'voice', 'delegation_id': 'external'};
  rejects(() => AppSurfaceWorkReceipt.fromJson(leaked), 'safe Voice origin');
  final outcome = <String, Object?>{
    'message_id': id(9),
    'before': anchor(2),
    'through': anchor(3),
    'content': {
      'kind': 'outcome',
      'run_id': id(7),
      'state': 'failed',
      'issue': null,
      'interaction_refs': <Object?>[],
    },
  };
  rejects(() => AppSurfaceMessage.fromJson(outcome), 'failure issue required');
  (outcome['content'] as Map)['issue'] = {
    'code': 'unavailable',
    'message': 'Stopped',
  };
  check(
    (AppSurfaceMessage.fromJson(outcome) as AppSurfaceOutcome).issueMessage ==
        'Stopped',
    'failure display only',
  );
  rejects(
    () => AppSurfaceSnapshot.fromJson({
      'through': anchor(1),
      'messages': <Object?>[],
      'older': null,
    }),
    'nonempty history cannot be blank',
  );
  check(
    AppSurfaceAnchor.fromJson(anchor(1)).toJson()['sequence'] == 1,
    'immutable anchor round trip',
  );
  print('Conversation Surface SDK checks passed: $checks');
}
