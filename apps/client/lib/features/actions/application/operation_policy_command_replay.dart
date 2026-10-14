import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// One unresolved policy change per gateway. A current-value query is not proof
/// of a prior command's outcome, and cannot rebase its immutable intent.
final class OperationPolicyCommandReplay {
  OperationPolicyCommandReplay._();
  static final Expando<OperationPolicyCommandReplay> _byGateway =
      Expando<OperationPolicyCommandReplay>('Access operation policy replay');
  OperationPolicyCommand? _pending;

  static OperationPolicyCommandReplay forGateway(
    OperationAuthorizationGateway gateway,
  ) => _byGateway[gateway] ??= OperationPolicyCommandReplay._();

  OperationPolicyCommand? get pending => _pending;

  OperationPolicyCommand retain({
    required ActionAuthorityMode mode,
    required ActionAuthority before,
  }) {
    final previous = _pending;
    if (previous != null) {
      if (previous.mode != mode ||
          previous.before.revision != before.revision ||
          previous.before.calendarCreate != before.calendarCreate) {
        throw StateError(
          'Resolve the pending policy change before submitting another change.',
        );
      }
      return previous;
    }
    return _pending = OperationPolicyCommand._(
      commandId: newAgentRequestId(),
      mode: mode,
      before: before,
    );
  }

  int begin(OperationPolicyCommand command) {
    if (!identical(_pending, command)) {
      throw StateError('This policy command is no longer pending.');
    }
    return ++command._submissionCount;
  }

  void acknowledge(OperationPolicyCommand command) {
    if (identical(_pending, command)) _pending = null;
  }

  void reject(
    OperationPolicyCommand command,
    int submission,
    CommandOutcome? outcome,
  ) {
    if (identical(_pending, command) &&
        command._submissionCount == submission &&
        mayDiscardPendingCommand(
          outcome,
          previouslySubmitted: submission > 1,
        )) {
      _pending = null;
    }
  }
}

final class OperationPolicyCommand {
  OperationPolicyCommand._({
    required this.commandId,
    required this.mode,
    required this.before,
  });

  final String commandId;
  final ActionAuthorityMode mode;
  final ActionAuthority before;
  int _submissionCount = 0;
}
