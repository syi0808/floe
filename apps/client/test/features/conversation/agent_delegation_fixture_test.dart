import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Rust delegation fixture projects generic Task result and Actions artifact', () {
    final bytes = File('../../fixtures/expert-report/delegation-v1.json')
        .readAsStringSync();
    final json = jsonDecode(bytes) as Map<String, dynamic>;
    final message = AgentMessage.fromJson(json) as AgentCapabilityMessage;
    expect(message.output, 'One focus window');
    expect(message.input, 'floe.builtin.schedule');
    expect(message.isDelegation, isTrue);
    expect(
      message.hasArtifactMediaType(
        'application/vnd.floe.expert.schedule+json;version=1',
      ),
      isTrue,
    );
    expect(
      message.hasArtifactMediaType(
        'application/vnd.floe.actions.calendar-proposal+json;version=1',
      ),
      isTrue,
    );
    expect(bytes, isNot(contains('grant_id')));
    expect(bytes, isNot(contains('source_authority')));
    expect(bytes, isNot(contains('consumer_policy')));
  });
}
