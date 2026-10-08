import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';

void main() {
  test('pending-command recovery follows AppWire disposition only', () {
    const ownerConflictMarkedIndeterminate = AppWireTransportException(
      'conflict',
      'The owner reported a conflict.',
      metadata: {'reason_code': 'conflict'},
      commandOutcome: CommandOutcome.indeterminate,
    );
    const ownerFailureMarkedNotApplied = AppWireTransportException(
      'internal',
      'The App proved the command was not applied.',
      commandOutcome: CommandOutcome.notApplied,
    );
    const notAdmitted = AppWireTransportException(
      'unavailable',
      'The command did not enter App.',
      commandOutcome: CommandOutcome.notAdmitted,
    );

    expect(
      mayDiscardPendingCommand(
        ownerConflictMarkedIndeterminate,
        previouslySubmitted: true,
      ),
      isFalse,
    );
    expect(
      mayDiscardPendingCommand(
        ownerFailureMarkedNotApplied,
        previouslySubmitted: true,
      ),
      isTrue,
    );
    expect(
      mayDiscardPendingCommand(notAdmitted, previouslySubmitted: false),
      isTrue,
    );
    expect(
      mayDiscardPendingCommand(notAdmitted, previouslySubmitted: true),
      isFalse,
    );
  });
}
