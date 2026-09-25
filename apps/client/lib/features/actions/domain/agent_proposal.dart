import 'package:floe_client/features/actions/domain/calendar_action.dart';

abstract interface class AgentProposalGateway {
  Future<AgentProposalInspection> inspectProposal({
    required String personId,
    required String sessionId,
    required String invocationId,
  });
}

final class AgentProposalInspection {
  AgentProposalInspection.fromJson(Map<String, dynamic> json)
    : personId = _uuid(json['person_id']),
      sessionId = _uuid(json['session_id']),
      invocationId = _uuid(json['invocation_id']),
      action = json['action'] == null
          ? null
          : AgentProposalAction.fromJson(
              Map<String, dynamic>.from(json['action'] as Map),
            ) {
    if (json['schema_version'] != 1 ||
        !json.containsKey('action') ||
        action != null && action!.id != invocationId) {
      throw const FormatException('Invalid proposal inspection');
    }
  }

  final String personId;
  final String sessionId;
  final String invocationId;
  final AgentProposalAction? action;
}

final class AgentProposalAction {
  AgentProposalAction.fromJson(Map<String, dynamic> json)
    : id = _uuid(json['action_id']),
      executionId = _uuid(json['execution_id']),
      status = CalendarActionStatus.values.byName(json['status'] as String),
      expiresAt = DateTime.parse(json['expires_at'] as String),
      startsAt = DateTime.parse(json['starts_at'] as String),
      endsAt = DateTime.parse(json['ends_at'] as String) {
    if (!expiresAt.isUtc || !startsAt.isUtc || !endsAt.isUtc || !endsAt.isAfter(startsAt)) {
      throw const FormatException('Proposal expiry must have a UTC offset');
    }
  }

  final String id;
  final String executionId;
  final CalendarActionStatus status;
  final DateTime expiresAt;
  final DateTime startsAt;
  final DateTime endsAt;
}

String _uuid(Object? value) {
  if (value is! String ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value)) {
    throw const FormatException('Invalid proposal identifier');
  }
  return value;
}
