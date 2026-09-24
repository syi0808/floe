enum AgentInteractionKind { sourceAccess, processingRecipient }

enum AgentInteractionState {
  pending,
  resolving,
  resolved,
  denied,
  cancelled,
  superseded,
  expired,
}

enum AgentInteractionAction {
  allow,
  deny,
  dismiss,
  refresh,
  continueRequest,
  openConnection,
  reviewSource,
  requestPermission,
}

enum AgentInteractionDecision { approve, deny, dismiss }

enum AgentInteractionResolveOutcome {
  resolved,
  resolving,
  denied,
  cancelled,
  superseded,
  expired,
  stale,
  terminal,
  wrongDevice,
}

enum AgentInteractionRefreshOutcome {
  resolved,
  stillPending,
  superseded,
  terminal,
  expired,
  stale,
  wrongDevice,
}

enum AgentNavigationDestination {
  connectionSettings,
  systemPermission,
  resourcePicker,
}

final class AgentObservedMember {
  const AgentObservedMember({required this.memberId, required this.resource});

  factory AgentObservedMember.parse(Map<String, dynamic> json) {
    final memberId = json['member_id'];
    final resource = json['resource'];
    if (memberId is! String ||
        memberId.isEmpty ||
        resource is! String ||
        resource.isEmpty) {
      throw const FormatException('Invalid observed member.');
    }
    return AgentObservedMember(memberId: memberId, resource: resource);
  }

  final String memberId;
  final String resource;
}

final class AgentConsentScope {
  const AgentConsentScope({
    required this.connectionId,
    required this.resources,
    required this.categories,
    required this.operation,
    required this.purpose,
    required this.consumer,
  });

  factory AgentConsentScope.parse(Map<String, dynamic> json) {
    String field(String name) {
      final value = json[name];
      if (value is! String || value.isEmpty) {
        throw FormatException('Invalid consent scope $name.');
      }
      return value;
    }

    List<String> list(String name) {
      final value = json[name];
      if (value is! List ||
          value.any((entry) => entry is! String || entry.isEmpty)) {
        throw FormatException('Invalid consent scope $name.');
      }
      return List<String>.unmodifiable(value.cast<String>());
    }

    return AgentConsentScope(
      connectionId: field('connection_id'),
      resources: list('resources'),
      categories: list('categories'),
      operation: field('operation'),
      purpose: field('purpose'),
      consumer: field('consumer'),
    );
  }

  final String connectionId;
  final List<String> resources;
  final List<String> categories;
  final String operation;
  final String purpose;
  final String consumer;
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
      case 'inline_observe':
        final members = json['members'];
        if (members is! List) {
          throw const FormatException('Invalid inline observe members.');
        }
        return AgentInlineObserveTarget(
          connectionId: field('connection_id'),
          sourceId: field('source_id'),
          consumer: field('consumer'),
          purpose: field('purpose'),
          members: List<AgentObservedMember>.unmodifiable(
            members.map(
              (member) => AgentObservedMember.parse(
                _map(member, 'Invalid observed member.'),
              ),
            ),
          ),
        );
      case 'navigation_only':
        return AgentNavigationOnlyTarget(
          destination: switch (json['destination']) {
            'connection_settings' =>
              AgentNavigationDestination.connectionSettings,
            'system_permission' => AgentNavigationDestination.systemPermission,
            'resource_picker' => AgentNavigationDestination.resourcePicker,
            _ => throw const FormatException('Invalid navigation destination.'),
          },
          sourceId: field('source_id'),
          consumer: field('consumer'),
          purpose: field('purpose'),
        );
      case 'recipient_consent':
        final classes = json['input_data_classes'];
        final scopes = json['source_scopes'];
        if (classes is! List ||
            classes.any((entry) => entry is! String || entry.isEmpty) ||
            scopes is! List) {
          throw const FormatException('Invalid recipient consent target.');
        }
        return AgentRecipientConsentTarget(
          recipient: field('recipient'),
          profileId: field('profile_id'),
          purpose: field('purpose'),
          consumer: field('consumer'),
          inputDataClasses: List<String>.unmodifiable(classes.cast<String>()),
          sourceScopes: List<AgentConsentScope>.unmodifiable(
            scopes.map(
              (scope) => AgentConsentScope.parse(
                _map(scope, 'Invalid consent scope.'),
              ),
            ),
          ),
        );
      default:
        throw const FormatException('Unknown interaction target.');
    }
  }
}

final class AgentInlineObserveTarget extends AgentInteractionTarget {
  const AgentInlineObserveTarget({
    required this.connectionId,
    required this.sourceId,
    required this.consumer,
    required this.purpose,
    required this.members,
  });

  final String connectionId;
  final String sourceId;
  final String consumer;
  final String purpose;
  final List<AgentObservedMember> members;
}

final class AgentNavigationOnlyTarget extends AgentInteractionTarget {
  const AgentNavigationOnlyTarget({
    required this.destination,
    required this.sourceId,
    required this.consumer,
    required this.purpose,
  });

  final AgentNavigationDestination destination;
  final String sourceId;
  final String consumer;
  final String purpose;
}

final class AgentRecipientConsentTarget extends AgentInteractionTarget {
  const AgentRecipientConsentTarget({
    required this.recipient,
    required this.profileId,
    required this.purpose,
    required this.consumer,
    required this.inputDataClasses,
    required this.sourceScopes,
  });

  final String recipient;
  final String profileId;
  final String purpose;
  final String consumer;
  final List<String> inputDataClasses;
  final List<AgentConsentScope> sourceScopes;
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
    String id(String name) {
      final value = json[name];
      if (value is! String || value.isEmpty) {
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
    if (digest is! List ||
        digest.length != 32 ||
        digest.any((byte) => byte is! int || byte < 0 || byte > 255)) {
      throw const FormatException('Invalid interaction digest.');
    }
    final actions = json['actions'];
    if (actions is! List) {
      throw const FormatException('Invalid interaction actions.');
    }
    final target = json['target'];
    if (target is! Map) {
      throw const FormatException('Invalid interaction target.');
    }
    return AgentInteractionSnapshot(
      id: id('interaction_id'),
      sessionId: id('session_id'),
      originRunId: id('origin_run_id'),
      kind: switch (json['interaction_kind']) {
        'source_access' => AgentInteractionKind.sourceAccess,
        'processing_recipient' => AgentInteractionKind.processingRecipient,
        _ => throw const FormatException('Unknown interaction kind.'),
      },
      state: switch (json['state']) {
        'pending' => AgentInteractionState.pending,
        'resolving' => AgentInteractionState.resolving,
        'resolved' => AgentInteractionState.resolved,
        'denied' => AgentInteractionState.denied,
        'cancelled' => AgentInteractionState.cancelled,
        'superseded' => AgentInteractionState.superseded,
        'expired' => AgentInteractionState.expired,
        _ => throw const FormatException('Unknown interaction state.'),
      },
      revision: number('revision'),
      targetDigest: List<int>.unmodifiable(digest.cast<int>()),
      createdAtUnixMs: number('created_at_unix_ms'),
      expiresAtUnixMs: number('expires_at_unix_ms'),
      target: AgentInteractionTarget.parse(Map<String, dynamic>.from(target)),
      actions: List<AgentInteractionAction>.unmodifiable(
        actions.map(
          (action) => switch (action) {
            'allow' => AgentInteractionAction.allow,
            'deny' => AgentInteractionAction.deny,
            'dismiss' => AgentInteractionAction.dismiss,
            'refresh' => AgentInteractionAction.refresh,
            'continue_request' => AgentInteractionAction.continueRequest,
            'open_connection' => AgentInteractionAction.openConnection,
            'review_source' => AgentInteractionAction.reviewSource,
            'request_permission' => AgentInteractionAction.requestPermission,
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
  final List<int> targetDigest;
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
        'resolved' => AgentInteractionResolveOutcome.resolved,
        'resolving' => AgentInteractionResolveOutcome.resolving,
        'denied' => AgentInteractionResolveOutcome.denied,
        'cancelled' => AgentInteractionResolveOutcome.cancelled,
        'superseded' => AgentInteractionResolveOutcome.superseded,
        'expired' => AgentInteractionResolveOutcome.expired,
        'stale' => AgentInteractionResolveOutcome.stale,
        'terminal' => AgentInteractionResolveOutcome.terminal,
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
        'still_pending' => AgentInteractionRefreshOutcome.stillPending,
        'superseded' => AgentInteractionRefreshOutcome.superseded,
        'terminal' => AgentInteractionRefreshOutcome.terminal,
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
