import 'dart:convert';

const _maxActionRevision = 0x7fffffffffffffff;
const _maxActionTitleBytes = 1024;
const _maxObservedTitleBytes = 4096;
const _maxDestinationLabelBytes = 512;
const _maxReceiptJournalRevision = 512;
const _maxObservationAfterMs = 60000;

enum ActionAuthorityMode { allow, ask, deny }

enum CalendarActionDecision { approve, reject, cancel }

enum ActionAllowedAction { approve, reject, cancel, reconcile }

enum CalendarActionOrigin { direct, expert }

enum CalendarActionState {
  pendingReview,
  approved,
  rejected,
  cancelled,
  expired,
  executing,
  blocked,
  failed,
  unknown,
  succeeded,
}

enum ActionBlockedReason {
  permissionDenied,
  policyDenied,
  sourceChanged,
  executorUnavailable,
  scheduleConflict,
}

enum ActionNotAppliedReason {
  permissionDenied,
  providerRejected,
  providerUnavailable,
  sourceChanged,
  cancelled,
  timeout,
}

enum ActionUnknownReason {
  timeout,
  responseLost,
  invalidReceipt,
  cancelledAfterDispatch,
  inconclusiveLookup,
  nativeOperationPending,
  nativeReceiptUnavailable,
}

enum ActionCollectionStatus { pending, collected }

/// A timezone and UTC interval used for a new Actions write.
final class ActionSchedule {
  const ActionSchedule({
    required this.startsAt,
    required this.endsAt,
    required this.timezone,
  }) : _startsAtWire = null,
       _endsAtWire = null;

  const ActionSchedule._({
    required this.startsAt,
    required this.endsAt,
    required this.timezone,
    required String startsAtWire,
    required String endsAtWire,
  }) : _startsAtWire = startsAtWire,
       _endsAtWire = endsAtWire;

  factory ActionSchedule.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {'starts_at', 'ends_at', 'timezone'});
    final startsAt = _string(json['starts_at'], 'schedule.starts_at');
    final endsAt = _string(json['ends_at'], 'schedule.ends_at');
    return ActionSchedule._(
      startsAt: _utcInstant(startsAt, 'schedule.starts_at'),
      endsAt: _utcInstant(endsAt, 'schedule.ends_at'),
      timezone: _string(json['timezone'], 'schedule.timezone'),
      startsAtWire: startsAt,
      endsAtWire: endsAt,
    );
  }

  final DateTime startsAt;
  final DateTime endsAt;
  final String timezone;
  final String? _startsAtWire;
  final String? _endsAtWire;

  void validateNewAction() {
    if (!startsAt.isUtc ||
        !endsAt.isUtc ||
        !endsAt.isAfter(startsAt) ||
        endsAt.difference(startsAt) > const Duration(hours: 24) ||
        !_trimmedText(timezone, 128)) {
      throw const FormatException('Invalid new Action schedule.');
    }
  }

  void validateHistorical() {
    if (!startsAt.isUtc ||
        !endsAt.isUtc ||
        !endsAt.isAfter(startsAt) ||
        utf8.encode(timezone).length > 4 * 1024 * 1024) {
      throw const FormatException('Invalid historical Action schedule.');
    }
  }

  Map<String, Object?> toJson() => {
    'starts_at': _startsAtWire ?? startsAt.toUtc().toIso8601String(),
    'ends_at': _endsAtWire ?? endsAt.toUtc().toIso8601String(),
    'timezone': timezone,
  };
}

sealed class ActionIntent {
  const ActionIntent();

  Map<String, Object?> toJson();
}

final class DirectCreate extends ActionIntent {
  const DirectCreate({
    required this.destinationRef,
    required this.title,
    required this.schedule,
  });

  final String destinationRef;
  final String title;
  final ActionSchedule schedule;

  @override
  Map<String, Object?> toJson() {
    _validateUuid(destinationRef, 'destination_ref');
    _validateNewTitle(title);
    schedule.validateNewAction();
    return {
      'kind': 'direct_create',
      'destination_ref': destinationRef,
      'title': title,
      'schedule': schedule.toJson(),
    };
  }
}

final class DirectUpdate extends ActionIntent {
  const DirectUpdate({
    required this.eventRef,
    required this.expectedRevision,
    required this.title,
    required this.schedule,
  });

  final String eventRef;
  final int expectedRevision;
  final String title;
  final ActionSchedule schedule;

  @override
  Map<String, Object?> toJson() {
    _validateUuid(eventRef, 'event_ref');
    _validateRevision(expectedRevision, 'expected_revision');
    _validateNewTitle(title);
    schedule.validateNewAction();
    return {
      'kind': 'direct_update',
      'event_ref': eventRef,
      'expected_revision': expectedRevision,
      'title': title,
      'schedule': schedule.toJson(),
    };
  }
}

final class DirectDelete extends ActionIntent {
  const DirectDelete({required this.eventRef, required this.expectedRevision});

  final String eventRef;
  final int expectedRevision;

  @override
  Map<String, Object?> toJson() {
    _validateUuid(eventRef, 'event_ref');
    _validateRevision(expectedRevision, 'expected_revision');
    return {
      'kind': 'direct_delete',
      'event_ref': eventRef,
      'expected_revision': expectedRevision,
    };
  }
}

final class ExpertProposal extends ActionIntent {
  const ExpertProposal({
    required this.receipt,
    required this.artifactId,
    required this.destinationRef,
  });

  final TaskExecutionReceiptReference receipt;
  final String artifactId;
  final String destinationRef;

  @override
  Map<String, Object?> toJson() {
    receipt.validate();
    _validateUuid(artifactId, 'artifact_id');
    _validateUuid(destinationRef, 'destination_ref');
    return {
      'kind': 'expert_proposal',
      'receipt': receipt.toJson(),
      'artifact_id': artifactId,
      'destination_ref': destinationRef,
    };
  }
}

/// The immutable Task receipt reference emitted by the safe Conversation DTO.
/// It contains no Task result, artifact contents, or reconstructed proof.
final class TaskExecutionReceiptReference {
  const TaskExecutionReceiptReference({
    required this.execution,
    required this.taskRevision,
    required this.journalRevision,
    required this.digest,
  });

  factory TaskExecutionReceiptReference.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {
      'execution',
      'task_revision',
      'journal_revision',
      'digest',
    });
    final receipt = TaskExecutionReceiptReference(
      execution: TaskExecutionKeyReference.fromJson(
        _object(json['execution'], 'receipt.execution'),
      ),
      taskRevision: _integer(json['task_revision'], 'receipt.task_revision'),
      journalRevision: _integer(
        json['journal_revision'],
        'receipt.journal_revision',
      ),
      digest: _string(json['digest'], 'receipt.digest'),
    );
    receipt.validate();
    return receipt;
  }

  final TaskExecutionKeyReference execution;
  final int taskRevision;
  final int journalRevision;
  final String digest;

  @override
  bool operator ==(Object other) =>
      other is TaskExecutionReceiptReference &&
      execution == other.execution &&
      taskRevision == other.taskRevision &&
      journalRevision == other.journalRevision &&
      digest == other.digest;
  @override
  int get hashCode =>
      Object.hash(execution, taskRevision, journalRevision, digest);

  void validate() {
    if (execution.executorGeneration <= 0 ||
        taskRevision < 2 ||
        journalRevision < 0 ||
        journalRevision > _maxReceiptJournalRevision ||
        !RegExp(r'^[0-9a-f]{64}$').hasMatch(digest) ||
        digest == List.filled(64, '0').join()) {
      throw const FormatException('Invalid Task execution receipt reference.');
    }
    _validateUuid(execution.taskId, 'receipt.execution.task_id');
    _validateUuid(execution.executionId, 'receipt.execution.execution_id');
  }

  Map<String, Object?> toJson() => {
    'execution': execution.toJson(),
    'task_revision': taskRevision,
    'journal_revision': journalRevision,
    'digest': digest,
  };
}

/// The immutable key shared with the acknowledged Task execution receipt.
final class TaskExecutionKeyReference {
  const TaskExecutionKeyReference({
    required this.taskId,
    required this.executionId,
    required this.executorGeneration,
  });

  factory TaskExecutionKeyReference.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {'task_id', 'execution_id', 'executor_generation'});
    return TaskExecutionKeyReference(
      taskId: _string(json['task_id'], 'receipt.execution.task_id'),
      executionId: _string(
        json['execution_id'],
        'receipt.execution.execution_id',
      ),
      executorGeneration: _integer(
        json['executor_generation'],
        'receipt.execution.executor_generation',
      ),
    );
  }

  final String taskId;
  final String executionId;
  final int executorGeneration;

  @override
  bool operator ==(Object other) =>
      other is TaskExecutionKeyReference &&
      taskId == other.taskId &&
      executionId == other.executionId &&
      executorGeneration == other.executorGeneration;
  @override
  int get hashCode => Object.hash(taskId, executionId, executorGeneration);

  Map<String, Object?> toJson() => {
    'task_id': taskId,
    'execution_id': executionId,
    'executor_generation': executorGeneration,
  };
}

final class ActionDestinationChoice {
  const ActionDestinationChoice({
    required this.destinationRef,
    required this.label,
  });

  factory ActionDestinationChoice.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {'destination_ref', 'label'});
    final destination = ActionDestinationChoice(
      destinationRef: _string(json['destination_ref'], 'destination_ref'),
      label: _string(json['label'], 'destination.label'),
    );
    _validateUuid(destination.destinationRef, 'destination_ref');
    if (!_trimmedText(destination.label, _maxDestinationLabelBytes)) {
      throw const FormatException('Invalid Action destination label.');
    }
    return destination;
  }

  final String destinationRef;
  final String label;

  Map<String, Object?> toJson() => {
    'destination_ref': destinationRef,
    'label': label,
  };
}

final class ActionAuthority {
  const ActionAuthority({required this.revision, required this.calendarCreate});

  factory ActionAuthority.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {'revision', 'calendar_create'});
    final revision = _integer(json['revision'], 'authority.revision');
    _validateRevision(revision, 'authority.revision');
    return ActionAuthority(
      revision: revision,
      calendarCreate: _wireEnum(
        ActionAuthorityMode.values,
        json['calendar_create'],
        'authority.calendar_create',
      ),
    );
  }

  final int revision;
  final ActionAuthorityMode calendarCreate;

  Map<String, Object?> toJson() => {
    'revision': revision,
    'calendar_create': calendarCreate.name,
  };
}

final class ActionReviewReference {
  const ActionReviewReference({
    required this.id,
    required this.actionId,
    required this.effectDigest,
    required this.sourceDigest,
    required this.authorityRevision,
    required this.expiresAt,
  });

  factory ActionReviewReference.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {
      'id',
      'action_id',
      'effect_digest',
      'source_digest',
      'authority_revision',
      'expires_at',
    });
    final reference = ActionReviewReference(
      id: _string(json['id'], 'review_ref.id'),
      actionId: _string(json['action_id'], 'review_ref.action_id'),
      effectDigest: _string(json['effect_digest'], 'review_ref.effect_digest'),
      sourceDigest: _string(json['source_digest'], 'review_ref.source_digest'),
      authorityRevision: _integer(
        json['authority_revision'],
        'review_ref.authority_revision',
      ),
      expiresAt: _string(json['expires_at'], 'review_ref.expires_at'),
    );
    _utcInstant(reference.expiresAt, 'review_ref.expires_at');
    _validateUuid(reference.id, 'review_ref.id');
    _validateUuid(reference.actionId, 'review_ref.action_id');
    _validateRevision(
      reference.authorityRevision,
      'review_ref.authority_revision',
    );
    _validateDigest(reference.effectDigest, 'review_ref.effect_digest');
    _validateDigest(reference.sourceDigest, 'review_ref.source_digest');
    return reference;
  }

  final String id;
  final String actionId;
  final String effectDigest;
  final String sourceDigest;
  final int authorityRevision;
  final String expiresAt;

  Map<String, Object?> toJson() => {
    'id': id,
    'action_id': actionId,
    'effect_digest': effectDigest,
    'source_digest': sourceDigest,
    'authority_revision': authorityRevision,
    'expires_at': expiresAt,
  };
}

sealed class ActionEffectSummary {
  const ActionEffectSummary();

  String get title;
  String get destinationLabel;
  ActionSchedule get schedule;
  Map<String, Object?> toJson();
}

final class CreateActionEffect extends ActionEffectSummary {
  const CreateActionEffect({
    required this.destinationLabel,
    required this.title,
    required this.schedule,
  });

  final String destinationLabel;
  @override
  final String title;
  @override
  final ActionSchedule schedule;

  @override
  Map<String, Object?> toJson() => {
    'kind': 'create',
    'destination_label': destinationLabel,
    'title': title,
    'schedule': schedule.toJson(),
  };
}

final class UpdateActionEffect extends ActionEffectSummary {
  const UpdateActionEffect({
    required this.eventRef,
    required this.expectedRevision,
    required this.destinationLabel,
    required this.previousTitle,
    required this.previousSchedule,
    required this.title,
    required this.schedule,
  });

  final String eventRef;
  final int expectedRevision;
  final String destinationLabel;
  final String previousTitle;
  final ActionSchedule previousSchedule;
  @override
  final String title;
  @override
  final ActionSchedule schedule;

  @override
  Map<String, Object?> toJson() => {
    'kind': 'update',
    'event_ref': eventRef,
    'expected_revision': expectedRevision,
    'destination_label': destinationLabel,
    'previous_title': previousTitle,
    'previous_schedule': previousSchedule.toJson(),
    'title': title,
    'schedule': schedule.toJson(),
  };
}

final class DeleteActionEffect extends ActionEffectSummary {
  const DeleteActionEffect({
    required this.eventRef,
    required this.expectedRevision,
    required this.destinationLabel,
    required this.title,
    required this.schedule,
  });

  final String eventRef;
  final int expectedRevision;
  final String destinationLabel;
  @override
  final String title;
  @override
  final ActionSchedule schedule;

  @override
  Map<String, Object?> toJson() => {
    'kind': 'delete',
    'event_ref': eventRef,
    'expected_revision': expectedRevision,
    'destination_label': destinationLabel,
    'title': title,
    'schedule': schedule.toJson(),
  };
}

ActionEffectSummary _actionEffectFromJson(Map<String, dynamic> json) {
  final kind = _string(json['kind'], 'effect.kind');
  switch (kind) {
    case 'create':
      _expectKeys(json, const {
        'kind',
        'destination_label',
        'title',
        'schedule',
      });
      final effect = CreateActionEffect(
        destinationLabel: _string(
          json['destination_label'],
          'effect.destination_label',
        ),
        title: _string(json['title'], 'effect.title'),
        schedule: ActionSchedule.fromJson(
          _object(json['schedule'], 'effect.schedule'),
        ),
      );
      _validateDestinationLabel(effect.destinationLabel);
      _validateNewTitle(effect.title);
      effect.schedule.validateNewAction();
      return effect;
    case 'update':
      _expectKeys(json, const {
        'kind',
        'event_ref',
        'expected_revision',
        'destination_label',
        'previous_title',
        'previous_schedule',
        'title',
        'schedule',
      });
      final effect = UpdateActionEffect(
        eventRef: _string(json['event_ref'], 'effect.event_ref'),
        expectedRevision: _integer(
          json['expected_revision'],
          'effect.expected_revision',
        ),
        destinationLabel: _string(
          json['destination_label'],
          'effect.destination_label',
        ),
        previousTitle: _string(json['previous_title'], 'effect.previous_title'),
        previousSchedule: ActionSchedule.fromJson(
          _object(json['previous_schedule'], 'effect.previous_schedule'),
        ),
        title: _string(json['title'], 'effect.title'),
        schedule: ActionSchedule.fromJson(
          _object(json['schedule'], 'effect.schedule'),
        ),
      );
      _validateUuid(effect.eventRef, 'effect.event_ref');
      _validateRevision(effect.expectedRevision, 'effect.expected_revision');
      _validateDestinationLabel(effect.destinationLabel);
      _validateObservedTitle(effect.previousTitle);
      effect.previousSchedule.validateHistorical();
      _validateNewTitle(effect.title);
      effect.schedule.validateNewAction();
      return effect;
    case 'delete':
      _expectKeys(json, const {
        'kind',
        'event_ref',
        'expected_revision',
        'destination_label',
        'title',
        'schedule',
      });
      final effect = DeleteActionEffect(
        eventRef: _string(json['event_ref'], 'effect.event_ref'),
        expectedRevision: _integer(
          json['expected_revision'],
          'effect.expected_revision',
        ),
        destinationLabel: _string(
          json['destination_label'],
          'effect.destination_label',
        ),
        title: _string(json['title'], 'effect.title'),
        schedule: ActionSchedule.fromJson(
          _object(json['schedule'], 'effect.schedule'),
        ),
      );
      _validateUuid(effect.eventRef, 'effect.event_ref');
      _validateRevision(effect.expectedRevision, 'effect.expected_revision');
      _validateDestinationLabel(effect.destinationLabel);
      _validateObservedTitle(effect.title);
      effect.schedule.validateHistorical();
      return effect;
    default:
      throw const FormatException('Unknown Action effect kind.');
  }
}

final class CalendarActionStatus {
  const CalendarActionStatus({
    required this.state,
    this.blockedReason,
    this.failedReason,
    this.unknownReason,
    this.collection,
  });

  final CalendarActionState state;
  final ActionBlockedReason? blockedReason;
  final ActionNotAppliedReason? failedReason;
  final ActionUnknownReason? unknownReason;
  final ActionCollectionStatus? collection;

  Map<String, Object?> toJson() => {
    'state': _wireName(state),
    if (blockedReason != null) 'reason': _wireName(blockedReason!),
    if (failedReason != null) 'reason': _wireName(failedReason!),
    if (unknownReason != null) 'reason': _wireName(unknownReason!),
    if (collection != null) 'collection': _wireName(collection!),
  };
}

CalendarActionStatus _actionStatusFromJson(Map<String, dynamic> json) {
  final state = _string(json['state'], 'status.state');
  switch (state) {
    case 'pending_review':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(
        state: CalendarActionState.pendingReview,
      );
    case 'approved':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(state: CalendarActionState.approved);
    case 'rejected':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(state: CalendarActionState.rejected);
    case 'cancelled':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(state: CalendarActionState.cancelled);
    case 'expired':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(state: CalendarActionState.expired);
    case 'executing':
      _expectKeys(json, const {'state'});
      return const CalendarActionStatus(state: CalendarActionState.executing);
    case 'blocked':
      _expectKeys(json, const {'state', 'reason'});
      return CalendarActionStatus(
        state: CalendarActionState.blocked,
        blockedReason: _wireEnum(
          ActionBlockedReason.values,
          json['reason'],
          'status.reason',
        ),
      );
    case 'failed':
      _expectKeys(json, const {'state', 'reason'});
      return CalendarActionStatus(
        state: CalendarActionState.failed,
        failedReason: _wireEnum(
          ActionNotAppliedReason.values,
          json['reason'],
          'status.reason',
        ),
      );
    case 'unknown':
      _expectKeys(json, const {'state', 'reason'});
      return CalendarActionStatus(
        state: CalendarActionState.unknown,
        unknownReason: _wireEnum(
          ActionUnknownReason.values,
          json['reason'],
          'status.reason',
        ),
      );
    case 'succeeded':
      _expectKeys(json, const {'state', 'collection'});
      return CalendarActionStatus(
        state: CalendarActionState.succeeded,
        collection: _wireEnum(
          ActionCollectionStatus.values,
          json['collection'],
          'status.collection',
        ),
      );
    default:
      throw const FormatException('Unknown Action status.');
  }
}

final class CalendarAction {
  const CalendarAction({
    required this.actionRef,
    required this.revision,
    required this.origin,
    required this.effect,
    required this.reviewRef,
    required this.createdAt,
    required this.expiresAt,
    required this.status,
    required this.allowedActions,
    required this.nextObservationAfterMs,
  });

  factory CalendarAction.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {
      'action_ref',
      'revision',
      'origin',
      'effect',
      'review_ref',
      'created_at',
      'expires_at',
      'status',
      'allowed_actions',
      'next_observation_after_ms',
    });
    final revision = _integer(json['revision'], 'action.revision');
    _validateRevision(revision, 'action.revision');
    final action = CalendarAction(
      actionRef: _string(json['action_ref'], 'action.action_ref'),
      revision: revision,
      origin: _wireEnum(
        CalendarActionOrigin.values,
        json['origin'],
        'action.origin',
      ),
      effect: _actionEffectFromJson(_object(json['effect'], 'action.effect')),
      reviewRef: ActionReviewReference.fromJson(
        _object(json['review_ref'], 'action.review_ref'),
      ),
      createdAt: _string(json['created_at'], 'action.created_at'),
      expiresAt: _string(json['expires_at'], 'action.expires_at'),
      status: _actionStatusFromJson(_object(json['status'], 'action.status')),
      allowedActions: _allowedActions(json['allowed_actions']),
      nextObservationAfterMs: _optionalInteger(
        json['next_observation_after_ms'],
        'action.next_observation_after_ms',
      ),
    );
    final createdAt = _utcInstant(action.createdAt, 'action.created_at');
    final expiresAt = _utcInstant(action.expiresAt, 'action.expires_at');
    final reviewExpiresAt = _utcInstant(
      action.reviewRef.expiresAt,
      'review_ref.expires_at',
    );
    _validateUuid(action.actionRef, 'action.action_ref');
    if (action.reviewRef.actionId != action.actionRef ||
        !expiresAt.isAfter(createdAt) ||
        reviewExpiresAt != expiresAt ||
        action.nextObservationAfterMs != null &&
            (action.nextObservationAfterMs! <= 0 ||
                action.nextObservationAfterMs! > _maxObservationAfterMs)) {
      throw const FormatException('Invalid Action snapshot.');
    }
    return action;
  }

  final String actionRef;
  final int revision;
  final CalendarActionOrigin origin;
  final ActionEffectSummary effect;
  final ActionReviewReference reviewRef;
  final String createdAt;
  final String expiresAt;
  final CalendarActionStatus status;
  final List<ActionAllowedAction> allowedActions;
  final int? nextObservationAfterMs;

  String get title => effect.title;
  String get destinationLabel => effect.destinationLabel;
  ActionSchedule get schedule => effect.schedule;
  bool get isWaitingForReview =>
      status.state == CalendarActionState.pendingReview;

  Map<String, Object?> toJson() => {
    'action_ref': actionRef,
    'revision': revision,
    'origin': _wireName(origin),
    'effect': effect.toJson(),
    'review_ref': reviewRef.toJson(),
    'created_at': createdAt,
    'expires_at': expiresAt,
    'status': status.toJson(),
    'allowed_actions': allowedActions.map(_wireName).toList(growable: false),
    'next_observation_after_ms': nextObservationAfterMs,
  };

  bool hasSameImmutableIdentity(CalendarAction other) =>
      actionRef == other.actionRef &&
      origin == other.origin &&
      jsonEncode(effect.toJson()) == jsonEncode(other.effect.toJson()) &&
      jsonEncode(reviewRef.toJson()) == jsonEncode(other.reviewRef.toJson()) &&
      createdAt == other.createdAt &&
      expiresAt == other.expiresAt;

  bool hasSameSnapshot(CalendarAction other) =>
      jsonEncode(toJson()) == jsonEncode(other.toJson());

  /// Poll hints and allowed controls are owner projections, not stored revision
  /// changes. The owner can also project expiry without mutating the record.
  bool follows(CalendarAction previous) =>
      hasSameImmutableIdentity(previous) &&
      revision >= previous.revision &&
      (revision > previous.revision ||
          jsonEncode(status.toJson()) == jsonEncode(previous.status.toJson()) ||
          (status.state == CalendarActionState.expired &&
              {
                CalendarActionState.pendingReview,
                CalendarActionState.approved,
              }.contains(previous.status.state)));

  bool isOlderObservationThan(CalendarAction current) =>
      hasSameImmutableIdentity(current) &&
      (revision < current.revision ||
          (revision == current.revision &&
              current.status.state == CalendarActionState.expired &&
              {
                CalendarActionState.pendingReview,
                CalendarActionState.approved,
              }.contains(status.state)));
}

final class ActionsPage {
  const ActionsPage({required this.actions, required this.nextCursor});

  factory ActionsPage.fromJson(Map<String, dynamic> json) {
    _expectKeys(json, const {'actions', 'next_cursor'});
    final raw = json['actions'];
    if (raw is! List || raw.length > 100) {
      throw const FormatException('Invalid Actions page.');
    }
    final actions = raw
        .map((value) => CalendarAction.fromJson(_object(value, 'page.action')))
        .toList(growable: false);
    final refs = actions.map((action) => action.actionRef).toSet();
    if (refs.length != actions.length) {
      throw const FormatException('Duplicate Action references.');
    }
    final cursor = json['next_cursor'];
    if (cursor != null) _validateUuid(cursor, 'page.next_cursor');
    return ActionsPage(
      actions: List.unmodifiable(actions),
      nextCursor: cursor as String?,
    );
  }

  final List<CalendarAction> actions;
  final String? nextCursor;
}

List<ActionAllowedAction> _allowedActions(Object? value) {
  if (value is! List || value.length > 4) {
    throw const FormatException('Invalid allowed Actions.');
  }
  final actions = value
      .map(
        (item) => _wireEnum(
          ActionAllowedAction.values,
          item,
          'action.allowed_actions',
        ),
      )
      .toList(growable: false);
  if (actions.toSet().length != actions.length) {
    throw const FormatException('Duplicate allowed Actions.');
  }
  return List.unmodifiable(actions);
}

T _wireEnum<T extends Enum>(List<T> values, Object? value, String field) {
  if (value is! String) throw FormatException('Invalid $field.');
  for (final candidate in values) {
    if (_wireName(candidate) == value) return candidate;
  }
  throw FormatException('Unknown $field.');
}

String _wireName(Object value) => switch (value) {
  CalendarActionState.pendingReview => 'pending_review',
  CalendarActionState.approved => 'approved',
  CalendarActionState.rejected => 'rejected',
  CalendarActionState.cancelled => 'cancelled',
  CalendarActionState.expired => 'expired',
  CalendarActionState.executing => 'executing',
  CalendarActionState.blocked => 'blocked',
  CalendarActionState.failed => 'failed',
  CalendarActionState.unknown => 'unknown',
  CalendarActionState.succeeded => 'succeeded',
  ActionBlockedReason.permissionDenied => 'permission_denied',
  ActionBlockedReason.policyDenied => 'policy_denied',
  ActionBlockedReason.sourceChanged => 'source_changed',
  ActionBlockedReason.executorUnavailable => 'executor_unavailable',
  ActionBlockedReason.scheduleConflict => 'schedule_conflict',
  ActionNotAppliedReason.permissionDenied => 'permission_denied',
  ActionNotAppliedReason.providerRejected => 'provider_rejected',
  ActionNotAppliedReason.providerUnavailable => 'provider_unavailable',
  ActionNotAppliedReason.sourceChanged => 'source_changed',
  ActionNotAppliedReason.cancelled => 'cancelled',
  ActionNotAppliedReason.timeout => 'timeout',
  ActionUnknownReason.timeout => 'timeout',
  ActionUnknownReason.responseLost => 'response_lost',
  ActionUnknownReason.invalidReceipt => 'invalid_receipt',
  ActionUnknownReason.cancelledAfterDispatch => 'cancelled_after_dispatch',
  ActionUnknownReason.inconclusiveLookup => 'inconclusive_lookup',
  ActionUnknownReason.nativeOperationPending => 'native_operation_pending',
  ActionUnknownReason.nativeReceiptUnavailable => 'native_receipt_unavailable',
  ActionCollectionStatus.pending => 'pending',
  ActionCollectionStatus.collected => 'collected',
  ActionAuthorityMode.allow => 'allow',
  ActionAuthorityMode.ask => 'ask',
  ActionAuthorityMode.deny => 'deny',
  CalendarActionOrigin.direct => 'direct',
  CalendarActionOrigin.expert => 'expert',
  ActionAllowedAction.approve => 'approve',
  ActionAllowedAction.reject => 'reject',
  ActionAllowedAction.cancel => 'cancel',
  ActionAllowedAction.reconcile => 'reconcile',
  _ => throw ArgumentError.value(value, 'value', 'Unsupported Actions enum.'),
};

Map<String, dynamic> _object(Object? value, String field) {
  if (value is! Map) throw FormatException('Invalid $field.');
  try {
    return Map<String, dynamic>.from(value);
  } on Object {
    throw FormatException('Invalid $field.');
  }
}

void _expectKeys(Map<String, dynamic> value, Set<String> keys) {
  if (value.length != keys.length || !value.keys.toSet().containsAll(keys)) {
    throw const FormatException('Unexpected Actions DTO fields.');
  }
}

String _string(Object? value, String field) {
  if (value is! String) throw FormatException('Invalid $field.');
  return value;
}

int _integer(Object? value, String field) {
  if (value is! int || value < 0) throw FormatException('Invalid $field.');
  return value;
}

int? _optionalInteger(Object? value, String field) =>
    value == null ? null : _integer(value, field);

DateTime _utcInstant(Object? value, String field) {
  if (value is! String ||
      value.isEmpty ||
      value.length > 64 ||
      !RegExp(
        r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]00:00)$',
      ).hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  try {
    final parsed = DateTime.parse(value).toUtc();
    final fields = RegExp(r'^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})')
        .firstMatch(value)!;
    final actual = [
      parsed.year,
      parsed.month,
      parsed.day,
      parsed.hour,
      parsed.minute,
      parsed.second,
    ];
    for (var index = 0; index < actual.length; index++) {
      if (actual[index] != int.parse(fields.group(index + 1)!)) {
        throw FormatException('Invalid $field.');
      }
    }
    return parsed;
  } on FormatException {
    throw FormatException('Invalid $field.');
  }
}

void _validateUuid(Object? value, String field) {
  if (value is! String ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value) ||
      value == '00000000-0000-0000-0000-000000000000') {
    throw FormatException('Invalid $field.');
  }
}

void _validateRevision(int value, String field) {
  if (value <= 0 || value > _maxActionRevision) {
    throw FormatException('Invalid $field.');
  }
}

void _validateDigest(String value, String field) {
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
}

bool _trimmedText(String value, int maximumBytes) =>
    value.isNotEmpty &&
    utf8.encode(value).length <= maximumBytes &&
    value.trim() == value &&
    !value.runes.any((rune) => rune < 32 || (rune >= 127 && rune <= 159));

void _validateNewTitle(String value) {
  if (!_trimmedText(value, _maxActionTitleBytes)) {
    throw const FormatException('Invalid new Action title.');
  }
}

void _validateObservedTitle(String value) {
  if (utf8.encode(value).length > _maxObservedTitleBytes) {
    throw const FormatException('Invalid observed Action title.');
  }
}

void _validateDestinationLabel(String value) {
  if (!_trimmedText(value, _maxDestinationLabelBytes)) {
    throw const FormatException('Invalid Action destination label.');
  }
}

sealed class ActionProposalPreview {
  const ActionProposalPreview();
  factory ActionProposalPreview.fromJson(Map<String, dynamic> json) {
    switch (json['kind']) {
      case 'ready':
        _expectKeys(json, const {'kind', 'title', 'schedule', 'destinations'});
        final title = _string(json['title'], 'proposal.title');
        _validateNewTitle(title);
        final raw = json['destinations'];
        if (raw is! List || raw.length > 256)
          throw const FormatException('Invalid proposal choices.');
        final choices = raw
            .map(
              (value) => ActionDestinationChoice.fromJson(
                _object(value, 'proposal.destination'),
              ),
            )
            .toList();
        if (choices.map((value) => value.destinationRef).toSet().length !=
            choices.length)
          throw const FormatException('Duplicate proposal choice.');
        final schedule = ActionSchedule.fromJson(
          _object(json['schedule'], 'proposal.schedule'),
        );
        schedule.validateNewAction();
        return ReadyActionProposal(
          title: title,
          schedule: schedule,
          destinations: List.unmodifiable(choices),
        );
      case 'existing':
        _expectKeys(json, const {'kind', 'action'});
        final action = CalendarAction.fromJson(
          _object(json['action'], 'proposal.action'),
        );
        if (action.origin != CalendarActionOrigin.expert)
          throw const FormatException('Invalid proposal action origin.');
        return ExistingActionProposal(action);
      default:
        throw const FormatException('Invalid proposal preview.');
    }
  }
}

final class ReadyActionProposal extends ActionProposalPreview {
  const ReadyActionProposal({
    required this.title,
    required this.schedule,
    required this.destinations,
  });
  final String title;
  final ActionSchedule schedule;
  final List<ActionDestinationChoice> destinations;
}

final class ExistingActionProposal extends ActionProposalPreview {
  const ExistingActionProposal(this.action);
  final CalendarAction action;
}
