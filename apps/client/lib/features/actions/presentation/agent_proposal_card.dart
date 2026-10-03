import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/application/action_command_replay.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/l10n/app_localizations.dart';

class AgentProposalCard extends StatefulWidget {
  const AgentProposalCard({
    super.key,
    required this.controller,
    required this.message,
    this.onOpenAction,
  });

  final ConversationController controller;
  final AgentCapabilityMessage message;
  final Future<void> Function(String actionRef)? onOpenAction;

  @override
  State<AgentProposalCard> createState() => _AgentProposalCardState();
}

class _AgentProposalCardState extends State<AgentProposalCard> {
  static const proposalMediaType =
      'application/vnd.floe.actions.calendar-proposal+json;version=1';

  final form = GlobalKey<FormState>();
  final timezone = TextEditingController();
  List<ActionDestinationChoice> destinations = const [];
  String? destinationRef;
  String? artifactId;
  ActionCommandReplay? pendingCommands;
  final Map<String, CalendarAction> actionsByIntent = {};
  CalendarActionError? error;
  bool loadingDestinations = false;
  bool destinationsLoaded = false;
  bool submitting = false;
  int _messageEpoch = 0;

  List<AgentArtifact> get proposals => widget.message.artifacts
      .where((artifact) => artifact.mediaTypes.contains(proposalMediaType))
      .toList(growable: false);

  bool get receiptMatches {
    final receipt = widget.message.executionReceipt;
    return receipt != null && receipt.execution.taskId == widget.message.callId;
  }

  bool get calendarChangesAvailable =>
      widget.controller.owners.actions != null &&
      pendingCommands != null && destinationsLoaded &&
      !loadingDestinations && destinations.isNotEmpty;

  CalendarAction? get selectedAction {
    final receipt = widget.message.executionReceipt;
    final selectedArtifactId = artifactId;
    if (receipt == null || selectedArtifactId == null) return null;
    return actionsByIntent[_actionKey(receipt, selectedArtifactId)];
  }

  String _actionKey(TaskExecutionReceiptReference receipt, String id) =>
      jsonEncode([receipt.toJson(), id]);

  @override
  void initState() {
    super.initState();
    final gateway = widget.controller.owners.actions;
    pendingCommands = gateway == null ? null : ActionCommandReplay.forGateway(gateway);
    final proposals = this.proposals;
    artifactId = proposals.length == 1 ? proposals.single.id : null;
    if (receiptMatches && gateway != null) unawaited(_loadDestinations());
  }

  @override
  void didUpdateWidget(AgentProposalCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    final gatewayChanged = oldWidget.controller.owners.actions != widget.controller.owners.actions;
    if (gatewayChanged) {
      final gateway = widget.controller.owners.actions;
      pendingCommands = gateway == null ? null : ActionCommandReplay.forGateway(gateway);
      actionsByIntent.clear();
    }
    if (gatewayChanged || oldWidget.message.callId != widget.message.callId ||
        oldWidget.message.executionReceipt != widget.message.executionReceipt) {
      _messageEpoch++;
      final proposals = this.proposals;
      artifactId = proposals.length == 1 ? proposals.single.id : null;
      destinations = const [];
      destinationRef = null;
      timezone.clear();
      error = null;
      submitting = false;
      loadingDestinations = false;
      destinationsLoaded = false;
      if (receiptMatches && widget.controller.owners.actions != null) unawaited(_loadDestinations());
    }
  }

  @override
  void dispose() {
    timezone.dispose();
    super.dispose();
  }

  Future<void> _loadDestinations() async {
    final messageEpoch = _messageEpoch;
    final gateway = widget.controller.owners.actions;
    if (gateway == null) return;
    setState(() {
      loadingDestinations = true;
      destinationsLoaded = false;
      destinations = const [];
      destinationRef = null;
      error = null;
    });
    try {
      final values = await gateway.loadDestinations();
      if (!mounted || messageEpoch != _messageEpoch) return;
      setState(() {
        destinations = List.unmodifiable(values);
        destinationRef = values.length == 1 ? values.single.destinationRef : null;
        loadingDestinations = false;
        destinationsLoaded = true;
      });
    } on Object catch (failure) {
      if (!mounted || messageEpoch != _messageEpoch) return;
      setState(() {
        error = CalendarActionError.from(failure);
        loadingDestinations = false;
      });
    }
  }

  bool _validTimezone(String? value) {
    if (value == null || value.trim().isEmpty) return false;
    return utf8.encode(value.trim()).length <= 128 &&
        !value.trim().runes.any(
          (rune) => rune < 32 || (rune >= 127 && rune <= 159),
        );
  }

  Future<void> _submit(AgentArtifact artifact) async {
    final gateway = widget.controller.owners.actions;
    final commands = pendingCommands;
    final receipt = widget.message.executionReceipt;
    final destination = destinationRef;
    if (gateway == null || commands == null || !calendarChangesAvailable ||
        receipt == null ||
        receipt.execution.taskId != widget.message.callId ||
        destination == null ||
        !destinations.any((choice) => choice.destinationRef == destination) ||
        submitting ||
        !form.currentState!.validate()) {
      return;
    }
    final intent = ExpertProposal(
      receipt: receipt,
      artifactId: artifact.id,
      destinationRef: destination,
      timezone: timezone.text.trim(),
    );
    final payload = <String, Object?>{
      'kind': 'actions.submit',
      'intent': intent.toJson(),
    };
    final payloadKey = jsonEncode(payload);
    final messageEpoch = _messageEpoch;
    setState(() {
      submitting = true;
      error = null;
    });
    try {
      final commandId = commands.retain(payloadKey);
      final result = await gateway.submit(
        commandId: commandId,
        intent: intent,
      );
      if (result.origin != CalendarActionOrigin.expert) {
        throw StateError('Actions returned a non-Expert snapshot.');
      }
      commands.acknowledge(payloadKey, commandId);
      if (!mounted || messageEpoch != _messageEpoch) return;
      setState(() {
        actionsByIntent[_actionKey(receipt, artifact.id)] = result;
        submitting = false;
      });
    } on Object catch (failure) {
      if (!mounted) return;
      if (messageEpoch != _messageEpoch) return;
      setState(() {
        error = CalendarActionError.from(failure);
        submitting = false;
      });
    }
  }

  Future<void> _refreshAction(CalendarAction action) async {
    final gateway = widget.controller.owners.actions;
    final receipt = widget.message.executionReceipt;
    final selectedArtifact = artifactId;
    if (gateway == null || submitting || receipt == null || selectedArtifact == null) return;
    final key = _actionKey(receipt, selectedArtifact);
    final epoch = _messageEpoch;
    setState(() { submitting = true; error = null; });
    try {
      final observed = await gateway.inspect(action.actionRef);
      if (!mounted || epoch != _messageEpoch) return;
      final current = actionsByIntent[key] ?? action;
      if (!observed.isOlderObservationThan(current) && !observed.follows(current)) {
        throw StateError('The Action observation changed identity or regressed.');
      }
      setState(() {
        if (!observed.isOlderObservationThan(current)) actionsByIntent[key] = observed;
        submitting = false;
      });
    } on Object catch (failure) {
      if (!mounted || epoch != _messageEpoch) return;
      setState(() { error = CalendarActionError.from(failure); submitting = false; });
    }
  }

  String _statusText(AppLocalizations strings, CalendarAction? action) {
    if (action == null) {
      if (widget.controller.owners.actions == null) {
        return 'Calendar Actions are unavailable in this view.';
      }
      if (widget.message.executionReceipt == null) {
        return 'This proposal is not ready to submit yet.';
      }
      if (!receiptMatches) return 'This proposal could not be verified.';
      if (error case final currentError?) return currentError.message;
      if (loadingDestinations) return strings.actionLoading;
      return 'Choose where to add this proposal.';
    }
    return switch (action.status.state) {
      CalendarActionState.pendingReview => strings.actionPending,
      CalendarActionState.approved => strings.actionApproved,
      CalendarActionState.rejected => strings.actionRejected,
      CalendarActionState.cancelled => 'Cancelled',
      CalendarActionState.expired => 'Expired',
      CalendarActionState.executing => strings.actionExecuting,
      CalendarActionState.blocked => strings.actionBlocked,
      CalendarActionState.failed => switch (action.status.failedReason) {
        ActionNotAppliedReason.sourceChanged =>
          'The Calendar source changed before this action. No change was applied.',
        ActionNotAppliedReason.cancelled =>
          'Cancelled before any Calendar change was made.',
        ActionNotAppliedReason.timeout =>
          'Timed out before any Calendar change was made.',
        _ => 'The owner confirmed this change was not applied.',
      },
      CalendarActionState.unknown => strings.actionUnknown,
      CalendarActionState.succeeded => action.status.collection ==
              ActionCollectionStatus.pending
          ? 'Calendar change succeeded; Day collection is pending.'
          : strings.actionSucceeded,
    };
  }

  FloeBadgeTone _statusTone(CalendarAction? action) => switch (
    action?.status.state
  ) {
    CalendarActionState.succeeded => FloeBadgeTone.success,
    CalendarActionState.blocked ||
    CalendarActionState.failed ||
    CalendarActionState.rejected ||
    CalendarActionState.cancelled ||
    CalendarActionState.expired => FloeBadgeTone.danger,
    _ => FloeBadgeTone.info,
  };

  @override
  Widget build(BuildContext context) {
    if (!widget.message.hasArtifactMediaType(proposalMediaType)) {
      return const SizedBox.shrink();
    }
    final strings = AppLocalizations.of(context);
    final artifacts = proposals;
    final currentAction = selectedAction;
    AgentArtifact? selectedArtifact;
    for (final artifact in artifacts) {
      if (artifact.id == artifactId) selectedArtifact = artifact;
    }
    return FloeSquircle(
      padding: const EdgeInsets.all(12),
      child: Form(
        key: form,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(strings.agentProposalTitle, style: FloeType.controlLabel),
            const SizedBox(height: 8),
            Text(
              'Choose a Calendar destination and timezone for this proposal. Your Action permissions determine whether approval is needed.',
              style: FloeType.bodySmall.copyWith(fontSize: 12),
            ),
            const SizedBox(height: 8),
            Semantics(
              liveRegion: true,
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: FloeBadge(
                  label: _statusText(strings, currentAction),
                  tone: error == null
                      ? _statusTone(currentAction)
                      : FloeBadgeTone.danger,
                ),
              ),
            ),
            if (!receiptMatches) ...[
              const SizedBox(height: 8),
              Text(
                widget.message.executionReceipt == null
                    ? 'This proposal is not ready to submit yet. Refresh the conversation.'
                    : 'This proposal could not be verified. Refresh the conversation.',
              ),
            ] else if (widget.controller.owners.actions == null) ...[
              const SizedBox(height: 8),
              const Text('Calendar Actions are unavailable in this view.'),
            ] else ...[
              const SizedBox(height: 12),
              if (artifacts.length > 1)
                FloeSelect<String>(
                  label: 'Proposal',
                  value: artifacts.any((artifact) => artifact.id == artifactId)
                      ? artifactId
                      : null,
                  options: artifacts
                      .map(
                        (artifact) => FloeSelectOption(
                          value: artifact.id,
                          label: artifact.name,
                        ),
                      )
                      .toList(growable: false),
                  enabled: !submitting,
                  onChanged: (value) => setState(() => artifactId = value),
                  validator: (value) =>
                      value == null ? strings.actionFormInvalid : null,
                ),
              if (currentAction == null) ...[
                FloeSelect<String>(
                  label: strings.actionDestination,
                  value: destinations.any(
                    (destination) =>
                        destination.destinationRef == destinationRef,
                  )
                      ? destinationRef
                      : null,
                  options: destinations
                      .map(
                        (destination) => FloeSelectOption(
                          value: destination.destinationRef,
                          label: destination.label,
                        ),
                      )
                      .toList(growable: false),
                  enabled: !loadingDestinations && !submitting,
                  onChanged: (value) => setState(() => destinationRef = value),
                  validator: (value) =>
                      value == null ? strings.actionFormInvalid : null,
                ),
                const SizedBox(height: 8),
                FloeInput(
                  label: 'Timezone',
                  controller: timezone,
                  enabled: !submitting,
                  validator: (value) => _validTimezone(value)
                      ? null
                      : strings.actionFormInvalid,
                ),
                if (destinations.isEmpty &&
                    !loadingDestinations &&
                    error == null) ...[
                  const SizedBox(height: 8),
                  const Text(
                    'No Calendar destinations are currently available.',
                  ),
                ],
                if (error != null) ...[
                  const SizedBox(height: 8),
                  Text(
                    error!.isVaultLocked
                        ? 'Vault locked. Unlock it to submit this proposal.'
                        : error!.message,
                  ),
                  FloeButton.text(
                    onPressed: loadingDestinations
                        ? null
                        : () => unawaited(_loadDestinations()),
                    child: Text(strings.actionReload),
                  ),
                ],
                FloeButton.filled(
                  onPressed: selectedArtifact == null ||
                          !calendarChangesAvailable ||
                          submitting ||
                          destinationRef == null
                      ? null
                      : () => _submit(selectedArtifact!),
                  loading: submitting,
                  child: Text('Submit proposal'),
                ),
              ] else ...[
                Text(currentAction.title),
                if (error case final actionError?) Text(actionError.message),
                FloeButton.text(
                  onPressed: submitting || widget.controller.owners.actions == null
                      ? null : () => _refreshAction(currentAction),
                  child: const Text('Refresh Action'),
                ),
                if (currentAction.status.state ==
                        CalendarActionState.succeeded &&
                    currentAction.status.collection ==
                        ActionCollectionStatus.pending)
                  const Text(
                    'Calendar change succeeded; Day collection is pending.',
                  ),
                if (widget.onOpenAction != null)
                  FloeButton.text(
                    onPressed: () =>
                        widget.onOpenAction!(currentAction.actionRef),
                    child: Text(strings.agentProposalOpen),
                  ),
              ],
            ],
          ],
        ),
      ),
    );
  }
}
