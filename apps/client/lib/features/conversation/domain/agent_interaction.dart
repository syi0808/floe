import 'dart:convert';

import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

enum AgentInteractionKind {
  sourceAccess,
  assistantFeatureSources,
  operationApproval,
}

enum AgentInteractionState {
  pending,
  resolving,
  resolved,
  denied,
  dismissed,
  superseded,
  expired,
  stale,
  wrongDevice,
}

enum AgentInteractionAction {
  allow,
  deny,
  dismiss,
  refresh,
  openConnection,
  reviewSource,
  requestPermission,
  openAssistantFeatureSettings,
}

enum AgentInteractionDecision { approve, deny, dismiss }

enum AgentSourceAccessReason {
  enableObserve,
  reviewChangedSource,
  requestSystemPermission,
  reconnect,
  reviewProcessing,
  selectResource,
}

sealed class AgentInteractionRequirement {
  const AgentInteractionRequirement();

  factory AgentInteractionRequirement.parse(Map<String, dynamic> json) {
    String text(String name, int maxBytes) {
      final value = json[name];
      if (value is! String ||
          value.isEmpty ||
          value.trim() != value ||
          utf8.encode(value).length > maxBytes ||
          value.contains(RegExp(r'[\x00-\x1f\x7f]'))) {
        throw FormatException('Invalid interaction requirement $name.');
      }
      return value;
    }

    switch (json['kind']) {
      case 'source_access':
        const keys = {
          'kind',
          'reason',
          'source_id',
          'connection_id',
          'consumer',
          'purpose',
          'inline',
        };
        if (json.length != keys.length ||
            !json.keys.toSet().containsAll(keys)) {
          throw const FormatException('Invalid source access requirement.');
        }
        final connection = json['connection_id'];
        final inline = json['inline'];
        if ((connection != null && connection is! String) || inline is! bool) {
          throw const FormatException('Invalid source access requirement.');
        }
        return AgentSourceAccessRequirement(
          reason: switch (json['reason']) {
            'enable_observe' => AgentSourceAccessReason.enableObserve,
            'review_changed_source' =>
              AgentSourceAccessReason.reviewChangedSource,
            'request_system_permission' =>
              AgentSourceAccessReason.requestSystemPermission,
            'reconnect' => AgentSourceAccessReason.reconnect,
            'review_processing' => AgentSourceAccessReason.reviewProcessing,
            'select_resource' => AgentSourceAccessReason.selectResource,
            _ => throw const FormatException('Unknown source access reason.'),
          },
          sourceId: text('source_id', 128),
          connectionId: connection == null
              ? null
              : _validateInteractionIdentifier(connection, 'connection_id'),
          consumer: text('consumer', 256),
          purpose: text('purpose', 64),
          inline: inline,
        );
      case 'assistant_feature_source_review':
        const keys = {'kind', 'source_requirement_ref', 'review_ref'};
        if (json.length != keys.length ||
            !json.keys.toSet().containsAll(keys)) {
          throw const FormatException(
            'Invalid assistant feature source requirement.',
          );
        }
        final review = json['review_ref'];
        if (review is! Map || review.keys.any((key) => key is! String)) {
          throw const FormatException(
            'Invalid assistant feature source review reference.',
          );
        }
        return AgentAssistantFeatureSourceRequirement(
          sourceRequirementRef: text('source_requirement_ref', 128),
          reviewRef: AssistantFeatureSourceReviewRef.fromJson(
            Map<String, dynamic>.from(review),
          ),
        );
      case 'operation_approval':
        const keys = {'kind', 'review_ref'};
        if (json.length != keys.length ||
            !json.keys.toSet().containsAll(keys)) {
          throw const FormatException(
            'Invalid operation approval requirement.',
          );
        }
        final review = json['review_ref'];
        if (review is! Map || review.keys.any((key) => key is! String)) {
          throw const FormatException('Invalid operation review reference.');
        }
        return AgentOperationApprovalRequirement(
          reviewRef: ActionReviewReference.fromJson(
            Map<String, dynamic>.from(review),
          ),
        );
      default:
        throw const FormatException('Unknown interaction requirement.');
    }
  }
}

final class AgentSourceAccessRequirement extends AgentInteractionRequirement {
  const AgentSourceAccessRequirement({
    required this.reason,
    required this.sourceId,
    required this.connectionId,
    required this.consumer,
    required this.purpose,
    required this.inline,
  });

  final AgentSourceAccessReason reason;
  final String sourceId;
  final String? connectionId;
  final String consumer;
  final String purpose;
  final bool inline;
}

final class AgentAssistantFeatureSourceRequirement
    extends AgentInteractionRequirement {
  const AgentAssistantFeatureSourceRequirement({
    required this.sourceRequirementRef,
    required this.reviewRef,
  });

  final String sourceRequirementRef;
  final AssistantFeatureSourceReviewRef reviewRef;
}

final class AgentOperationApprovalRequirement
    extends AgentInteractionRequirement {
  const AgentOperationApprovalRequirement({required this.reviewRef});

  final ActionReviewReference reviewRef;
}

enum AgentInteractionResolveOutcome {
  pending,
  resolved,
  resolving,
  denied,
  dismissed,
  superseded,
  expired,
  stale,
  wrongDevice,
}

enum AgentInteractionRefreshOutcome {
  pending,
  denied,
  dismissed,
  resolved,
  resolving,
  superseded,
  expired,
  stale,
  wrongDevice,
}

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
        return AgentSourceReviewTarget(
          review: ObserveReview.fromJson(json['review']),
        );
      case 'navigation_only':
        if (json.length != 4 ||
            !json.containsKey('source_ref') ||
            !{
              'connection_settings',
              'system_permission',
              'resource_picker',
            }.contains(json['destination'])) {
          throw const FormatException('Invalid source navigation target.');
        }
        return AgentNavigationTarget(
          destination: field('destination'),
          sourceLabel: field('source_label'),
          sourceRef: json['source_ref'] == null
              ? null
              : SourceRef(field('source_ref')),
        );
      case 'assistant_feature_source_review':
        if (json.length != 2 || !json.containsKey('review'))
          throw const FormatException(
            'Invalid assistant feature source target.',
          );
        final reviewJson = json['review'];
        if (reviewJson is! Map ||
            reviewJson.keys.any((key) => key is! String)) {
          throw const FormatException(
            'Invalid assistant feature source review.',
          );
        }
        return AgentAssistantFeatureSourceTarget(
          review: AssistantFeatureSourceReview.fromJson(
            Map<String, dynamic>.from(reviewJson),
          ),
        );
      case 'operation_approval':
        if (json.length != 2 || !json.containsKey('operation')) {
          throw const FormatException('Invalid operation approval target.');
        }
        final operation = json['operation'];
        if (operation is! Map || operation.keys.any((key) => key is! String)) {
          throw const FormatException('Invalid Calendar operation snapshot.');
        }
        return AgentOperationApprovalTarget(
          operation: CalendarAction.fromJson(
            Map<String, dynamic>.from(operation),
          ),
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
  const AgentNavigationTarget({
    required this.destination,
    required this.sourceLabel,
    this.sourceRef,
  });
  final String destination;
  final String sourceLabel;
  final SourceRef? sourceRef;
}

final class AgentAssistantFeatureSourceTarget extends AgentInteractionTarget {
  const AgentAssistantFeatureSourceTarget({required this.review});

  final AssistantFeatureSourceReview review;
}

final class AgentOperationApprovalTarget extends AgentInteractionTarget {
  const AgentOperationApprovalTarget({required this.operation});
  final CalendarAction operation;
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
    required this.requirement,
    required this.target,
    required this.actions,
  });

  factory AgentInteractionSnapshot.parse(Map<String, dynamic> json) {
    const fields = {
      'interaction_id',
      'session_id',
      'origin_run_id',
      'interaction_kind',
      'state',
      'revision',
      'target_digest',
      'created_at',
      'expires_at',
      'requirement',
      'target',
      'actions',
    };
    if (json.length != fields.length ||
        !json.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid interaction snapshot fields.');
    }
    String id(String name) {
      final value = json[name];
      if (value is! String ||
          !RegExp(
            r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
          ).hasMatch(value)) {
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
      if (value is! String || !value.endsWith('Z'))
        throw FormatException('Invalid interaction $name.');
      final parsed = DateTime.tryParse(value);
      if (parsed == null || !parsed.isUtc)
        throw FormatException('Invalid interaction $name.');
      return parsed;
    }

    final created = timestamp('created_at');
    final expires = timestamp('expires_at');
    final actions = json['actions'];
    if (actions is! List ||
        actions.length > 16 ||
        actions.toSet().length != actions.length) {
      throw const FormatException('Invalid interaction actions.');
    }
    final target = json['target'];
    final requirement = json['requirement'];
    if (target is! Map ||
        requirement is! Map ||
        target.keys.any((key) => key is! String) ||
        requirement.keys.any((key) => key is! String)) {
      throw const FormatException('Invalid interaction target.');
    }
    final typedTarget = AgentInteractionTarget.parse(
      Map<String, dynamic>.from(target),
    );
    final typedRequirement = AgentInteractionRequirement.parse(
      Map<String, dynamic>.from(requirement),
    );
    if (number('revision') < 1 ||
        created.millisecondsSinceEpoch < 0 ||
        !expires.isAfter(created) ||
        json['interaction_kind'] == 'source_access' &&
            !{'source_review', 'navigation_only'}.contains(target['kind']) ||
        json['interaction_kind'] == 'assistant_feature_sources' &&
            target['kind'] != 'assistant_feature_source_review' ||
        json['interaction_kind'] == 'operation_approval' &&
            target['kind'] != 'operation_approval' ||
        json['interaction_kind'] == 'source_access' &&
            typedRequirement is! AgentSourceAccessRequirement ||
        json['interaction_kind'] == 'assistant_feature_sources' &&
            typedRequirement is! AgentAssistantFeatureSourceRequirement ||
        json['interaction_kind'] == 'operation_approval' &&
            typedRequirement is! AgentOperationApprovalRequirement) {
      throw const FormatException('Invalid interaction owner projection.');
    }
    if (typedRequirement case AgentAssistantFeatureSourceRequirement(
      :final reviewRef,
    )) {
      if (typedTarget is! AgentAssistantFeatureSourceTarget ||
          !typedTarget.review.reviewRef.matches(reviewRef) ||
          typedTarget.review.sourceRequirementRef !=
              typedRequirement.sourceRequirementRef) {
        throw const FormatException(
          'Mismatched assistant feature source requirement.',
        );
      }
    }
    if (typedRequirement case AgentOperationApprovalRequirement(
      :final reviewRef,
    )) {
      if (typedTarget is! AgentOperationApprovalTarget ||
          typedTarget.operation.reviewRef.id != reviewRef.id ||
          typedTarget.operation.reviewRef.operationId !=
              reviewRef.operationId ||
          typedTarget.operation.reviewRef.effectDigest !=
              reviewRef.effectDigest ||
          typedTarget.operation.reviewRef.sourceDigest !=
              reviewRef.sourceDigest ||
          typedTarget.operation.reviewRef.personId != reviewRef.personId ||
          typedTarget.operation.reviewRef.deviceId != reviewRef.deviceId ||
          typedTarget.operation.reviewRef.authorityRevision !=
              reviewRef.authorityRevision ||
          typedTarget.operation.reviewRef.expiresAt != reviewRef.expiresAt) {
        throw const FormatException(
          'Mismatched Calendar operation requirement.',
        );
      }
    }
    return AgentInteractionSnapshot(
      id: id('interaction_id'),
      sessionId: id('session_id'),
      originRunId: id('origin_run_id'),
      kind: switch (json['interaction_kind']) {
        'source_access' => AgentInteractionKind.sourceAccess,
        'assistant_feature_sources' =>
          AgentInteractionKind.assistantFeatureSources,
        'operation_approval' => AgentInteractionKind.operationApproval,
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
      requirement: typedRequirement,
      target: typedTarget,
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
            'open_assistant_feature_settings' =>
              AgentInteractionAction.openAssistantFeatureSettings,
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
  final AgentInteractionRequirement requirement;
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

String _validateInteractionIdentifier(Object? value, String field) {
  if (value is! String ||
      value.isEmpty ||
      value.trim() != value ||
      utf8.encode(value).length > 256 ||
      value.contains(RegExp(r'[\x00-\x1f\x7f]'))) {
    throw FormatException('Invalid interaction requirement $field.');
  }
  return value;
}
