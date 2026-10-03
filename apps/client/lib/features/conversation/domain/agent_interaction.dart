import 'package:floe_client/features/connections/domain/connection_models.dart';

enum AgentInteractionKind { sourceAccess, expertBinding }

enum AgentInteractionState { pending, resolving, resolved, denied, dismissed, superseded, expired, stale, wrongDevice }

enum AgentInteractionAction {
  allow,
  deny,
  dismiss,
  refresh,
  openConnection,
  reviewSource,
  requestPermission,
  openExpertSettings,
}

enum AgentInteractionDecision { approve, deny, dismiss }

enum AgentInteractionResolveOutcome { pending, resolved, resolving, denied, dismissed, superseded, expired, stale, wrongDevice }
enum AgentInteractionRefreshOutcome { pending, denied, dismissed, resolved, resolving, superseded, expired, stale, wrongDevice }

sealed class AgentInteractionTarget {
  const AgentInteractionTarget();

  factory AgentInteractionTarget.parse(Map<String, dynamic> json) {
    String field(String name) {
      final value = json[name];
      if (value is! String || value.isEmpty) {
        throw FormatException('Invalid interaction target $name.');
      }
      return value;
    }

    switch (json['kind']) {
      case 'source_review':
        if (json.length != 2 || !json.containsKey('review')) {
          throw const FormatException('Invalid source review target.');
        }
        return AgentSourceReviewTarget(review: ObserveReview.fromJson(json['review']));
      case 'navigation_only':
        if (json.length != 3 || !{'connection_settings','system_permission','resource_picker'}.contains(json['destination'])) {
          throw const FormatException('Invalid source navigation target.');
        }
        return AgentNavigationTarget(destination: field('destination'), sourceLabel: field('source_label'));
      case 'expert_binding':
        if (json.length != 6) throw const FormatException('Invalid Expert binding target.');
        return AgentExpertBindingTarget(
          assignmentId: field('assignment_id'),
          packageId: field('package_id'),
          packageVersion: field('package_version'),
          requirementKey: field('requirement_key'),
          capability: field('capability'),
        );
      default:
        throw const FormatException('Unknown interaction target.');
    }
  }
}

final class AgentSourceReviewTarget extends AgentInteractionTarget {
  const AgentSourceReviewTarget({required this.review});
  final ObserveReview review;
}

final class AgentNavigationTarget extends AgentInteractionTarget {
  const AgentNavigationTarget({required this.destination, required this.sourceLabel});
  final String destination;
  final String sourceLabel;
}

final class AgentExpertBindingTarget extends AgentInteractionTarget {
  const AgentExpertBindingTarget({
    required this.assignmentId,
    required this.packageId,
    required this.packageVersion,
    required this.requirementKey,
    required this.capability,
  });

  final String assignmentId;
  final String packageId;
  final String packageVersion;
  final String requirementKey;
  final String capability;
}

final class AgentInteractionSnapshot {
  const AgentInteractionSnapshot({
    required this.id,
    required this.sessionId,
    required this.originRunId,
    required this.kind,
    required this.state,
    required this.revision,
    required this.targetDigest,
    required this.createdAtUnixMs,
    required this.expiresAtUnixMs,
    required this.target,
    required this.actions,
  });

  factory AgentInteractionSnapshot.parse(Map<String, dynamic> json) {
    const fields = {'interaction_id','session_id','origin_run_id','interaction_kind','state','revision','target_digest','created_at','expires_at','target','actions'};
    if (json.length != fields.length || !json.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid interaction snapshot fields.');
    }
    String id(String name) {
      final value = json[name];
      if (value is! String || !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(value)) {
        throw FormatException('Invalid interaction $name.');
      }
      return value;
    }

    int number(String name) {
      final value = json[name];
      if (value is! int) {
        throw FormatException('Invalid interaction $name.');
      }
      return value;
    }

    final digest = json['target_digest'];
    if (digest is! String || !RegExp(r'^[0-9a-f]{64}$').hasMatch(digest)) {
      throw const FormatException('Invalid interaction digest.');
    }
    DateTime timestamp(String name) {
      final value = json[name];
      if (value is! String || !value.endsWith('Z')) throw FormatException('Invalid interaction $name.');
      final parsed = DateTime.tryParse(value);
      if (parsed == null || !parsed.isUtc) throw FormatException('Invalid interaction $name.');
      return parsed;
    }
    final created = timestamp('created_at');
    final expires = timestamp('expires_at');
    final actions = json['actions'];
    if (actions is! List || actions.length > 16 || actions.toSet().length != actions.length) {
      throw const FormatException('Invalid interaction actions.');
    }
    final target = json['target'];
    if (target is! Map) {
      throw const FormatException('Invalid interaction target.');
    }
    if (number('revision') < 1 || created.millisecondsSinceEpoch < 0 || !expires.isAfter(created) ||
        json['interaction_kind'] == 'source_access' && !{'source_review','navigation_only'}.contains(target['kind']) ||
        json['interaction_kind'] == 'expert_binding' && target['kind'] != 'expert_binding') {
      throw const FormatException('Invalid interaction owner projection.');
    }
    return AgentInteractionSnapshot(
      id: id('interaction_id'),
      sessionId: id('session_id'),
      originRunId: id('origin_run_id'),
      kind: switch (json['interaction_kind']) {
        'source_access' => AgentInteractionKind.sourceAccess,
        'expert_binding' => AgentInteractionKind.expertBinding,
        _ => throw const FormatException('Unknown interaction kind.'),
      },
      state: switch (json['state']) {
        'pending' => AgentInteractionState.pending,
        'resolving' => AgentInteractionState.resolving,
        'resolved' => AgentInteractionState.resolved,
        'denied' => AgentInteractionState.denied,
        'dismissed' => AgentInteractionState.dismissed,
        'stale' => AgentInteractionState.stale,
        'wrong_device' => AgentInteractionState.wrongDevice,
        'superseded' => AgentInteractionState.superseded,
        'expired' => AgentInteractionState.expired,
        _ => throw const FormatException('Unknown interaction state.'),
      },
      revision: number('revision'),
      targetDigest: digest,
      createdAtUnixMs: created.millisecondsSinceEpoch,
      expiresAtUnixMs: expires.millisecondsSinceEpoch,
      target: AgentInteractionTarget.parse(Map<String, dynamic>.from(target)),
      actions: List<AgentInteractionAction>.unmodifiable(
        actions.map(
          (action) => switch (action) {
            'allow' => AgentInteractionAction.allow,
            'deny' => AgentInteractionAction.deny,
            'dismiss' => AgentInteractionAction.dismiss,
            'refresh' => AgentInteractionAction.refresh,
            'open_connection' => AgentInteractionAction.openConnection,
            'review_source' => AgentInteractionAction.reviewSource,
            'request_permission' => AgentInteractionAction.requestPermission,
            'open_expert_settings' => AgentInteractionAction.openExpertSettings,
            _ => throw const FormatException('Unknown interaction action.'),
          },
        ),
      ),
    );
  }

  bool get terminal =>
      state != AgentInteractionState.pending &&
      state != AgentInteractionState.resolving;

  final String id;
  final String sessionId;
  final String originRunId;
  final AgentInteractionKind kind;
  final AgentInteractionState state;
  final int revision;
  final String targetDigest;
  final int createdAtUnixMs;
  final int expiresAtUnixMs;
  final AgentInteractionTarget target;
  final List<AgentInteractionAction> actions;
}

final class AgentInteractionResolveResult {
  const AgentInteractionResolveResult({
    required this.commandId,
    required this.outcome,
    required this.snapshot,
    this.replacementId,
    this.linkedRun,
  });

  factory AgentInteractionResolveResult.parse(Map<String, dynamic> json) {
    if (json['kind'] != 'interaction_operation') {
      throw const FormatException('Invalid interaction result.');
    }
    return AgentInteractionResolveResult(
      commandId: _commandId(json),
      outcome: switch (json['outcome']) {
        'pending' => AgentInteractionResolveOutcome.pending,
        'resolved' => AgentInteractionResolveOutcome.resolved,
        'resolving' => AgentInteractionResolveOutcome.resolving,
        'denied' => AgentInteractionResolveOutcome.denied,
        'dismissed' => AgentInteractionResolveOutcome.dismissed,
        'superseded' => AgentInteractionResolveOutcome.superseded,
        'expired' => AgentInteractionResolveOutcome.expired,
        'stale' => AgentInteractionResolveOutcome.stale,
        'wrong_device' => AgentInteractionResolveOutcome.wrongDevice,
        _ => throw const FormatException('Unknown resolve outcome.'),
      },
      snapshot: AgentInteractionSnapshot.parse(
        _map(json['snapshot'], 'Invalid interaction snapshot.'),
      ),
      replacementId: _optionalId(json['replacement_id']),
      linkedRun: _optionalReceipt(json['linked_run']),
    );
  }

  final String commandId;
  final AgentInteractionResolveOutcome outcome;
  final AgentInteractionSnapshot snapshot;
  final String? replacementId;
  final AgentLinkedRun? linkedRun;
}

final class AgentInteractionRefreshResult {
  const AgentInteractionRefreshResult({
    required this.commandId,
    required this.outcome,
    required this.snapshot,
    this.replacementId,
    this.linkedRun,
  });

  factory AgentInteractionRefreshResult.parse(Map<String, dynamic> json) {
    if (json['kind'] != 'interaction_refresh') {
      throw const FormatException('Invalid interaction refresh result.');
    }
    return AgentInteractionRefreshResult(
      commandId: _commandId(json),
      outcome: switch (json['outcome']) {
        'resolved' => AgentInteractionRefreshOutcome.resolved,
        'pending' => AgentInteractionRefreshOutcome.pending,
        'denied' => AgentInteractionRefreshOutcome.denied,
        'dismissed' => AgentInteractionRefreshOutcome.dismissed,
        'resolving' => AgentInteractionRefreshOutcome.resolving,
        'superseded' => AgentInteractionRefreshOutcome.superseded,
        'expired' => AgentInteractionRefreshOutcome.expired,
        'stale' => AgentInteractionRefreshOutcome.stale,
        'wrong_device' => AgentInteractionRefreshOutcome.wrongDevice,
        _ => throw const FormatException('Unknown refresh outcome.'),
      },
      snapshot: AgentInteractionSnapshot.parse(
        _map(json['snapshot'], 'Invalid interaction snapshot.'),
      ),
      replacementId: _optionalId(json['replacement_id']),
      linkedRun: _optionalReceipt(json['linked_run']),
    );
  }

  final String commandId;
  final AgentInteractionRefreshOutcome outcome;
  final AgentInteractionSnapshot snapshot;
  final String? replacementId;
  final AgentLinkedRun? linkedRun;
}

/// One linked child Run/command receipt, as an interaction response
/// carries it. The runtime gateway maps this to its own receipt shape
/// for observation; the fields are identical by wire contract.
final class AgentLinkedRun {
  const AgentLinkedRun({
    required this.commandId,
    required this.runId,
    required this.sessionRevision,
    required this.runtimeEpoch,
  });

  final String commandId;
  final String runId;
  final int sessionRevision;
  final int runtimeEpoch;
}

String _commandId(Map<String, dynamic> json) {
  final commandId = json['command_id'];
  if (commandId is! String || commandId.isEmpty) {
    throw const FormatException('Invalid interaction command id.');
  }
  return commandId;
}

String? _optionalId(Object? value) {
  if (value == null) return null;
  if (value is! String || value.isEmpty) {
    throw const FormatException('Invalid interaction reference.');
  }
  return value;
}

AgentLinkedRun? _optionalReceipt(Object? value) {
  if (value == null) return null;
  if (value is! Map) {
    throw const FormatException('Invalid linked Run receipt.');
  }
  final receipt = Map<String, dynamic>.from(value);
  final commandId = receipt['command_id'];
  final runId = receipt['run_id'];
  final sessionRevision = receipt['session_revision'];
  final runtimeEpoch = receipt['runtime_epoch'];
  if (commandId is! String ||
      commandId.isEmpty ||
      runId is! String ||
      runId.isEmpty ||
      sessionRevision is! int ||
      runtimeEpoch is! int ||
      runtimeEpoch <= 0 ||
      receipt['admission'] != 'accepted') {
    throw const FormatException('Invalid linked Run receipt.');
  }
  return AgentLinkedRun(
    commandId: commandId,
    runId: runId,
    sessionRevision: sessionRevision,
    runtimeEpoch: runtimeEpoch,
  );
}

Map<String, dynamic> _map(Object? value, String message) {
  if (value is! Map) throw FormatException(message);
  return Map<String, dynamic>.from(value);
}
