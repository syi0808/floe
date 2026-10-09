import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/application/conversation_command_replay.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

class AgentProposalCard extends StatefulWidget {
  const AgentProposalCard({
    super.key,
    required this.controller,
    required this.message,
    required this.dayGateway,
  });

  final ConversationController controller;
  final AgentCapabilityMessage message;
  final DayGateway dayGateway;

  @override
  State<AgentProposalCard> createState() => _AgentProposalCardState();
}

class _AgentProposalCardState extends State<AgentProposalCard> {
  static const proposalMediaType =
      'application/vnd.floe.actions.calendar-proposal+json;version=1';

  final form = GlobalKey<FormState>();
  bool expanded = false;
  List<ManualCalendarDestination> destinations = const [];
  String? destinationRef;
  String? artifactId;
  ConversationCommandReplay? pendingCommands;
  final Map<String, CalendarAction> actionsByIntent = {};
  String? error;
  bool loadingDestinations = false;
  bool destinationsLoaded = false;
  bool submitting = false;
  int _messageEpoch = 0;
  int _destinationEpoch = 0;

  List<AgentArtifact> get proposals => widget.message.artifacts
      .where((artifact) => artifact.mediaTypes.contains(proposalMediaType))
      .toList(growable: false);

  bool get receiptMatches {
    final receipt = widget.message.executionReceipt;
    return receipt != null && receipt.execution.taskId == widget.message.callId;
  }

  bool get calendarChangesAvailable =>
      pendingCommands != null &&
      destinationsLoaded &&
      !loadingDestinations &&
      destinations.isNotEmpty;

  CalendarAction? get selectedAction {
    final receipt = widget.message.executionReceipt;
    final selectedArtifactId = artifactId;
    if (receipt == null || selectedArtifactId == null) return null;
    final key = _actionKey(receipt, selectedArtifactId);
    final current = actionsByIntent[key];
    if (current != null) return current;
    for (final interaction in widget.controller.interactionSnapshots) {
      final target = interaction.target;
      if (interaction.originRunId == widget.message.turnId &&
          interaction.kind == AgentInteractionKind.operationApproval &&
          target is AgentOperationApprovalTarget) {
        return target.operation;
      }
    }
    return null;
  }

  String _actionKey(TaskExecutionReceiptReference receipt, String id) =>
      jsonEncode([receipt.toJson(), id]);

  @override
  void initState() {
    super.initState();
    pendingCommands = ConversationCommandReplay.forGateway(
      widget.controller.gateway,
    );
    final artifacts = proposals;
    artifactId = artifacts.length == 1 ? artifacts.single.id : null;
  }

  @override
  void didUpdateWidget(AgentProposalCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    final gatewayChanged =
        oldWidget.controller.gateway != widget.controller.gateway;
    if (gatewayChanged) {
      pendingCommands = ConversationCommandReplay.forGateway(
        widget.controller.gateway,
      );
      actionsByIntent.clear();
    }
    if (gatewayChanged ||
        oldWidget.message.callId != widget.message.callId ||
        oldWidget.message.executionReceipt != widget.message.executionReceipt) {
      _messageEpoch++;
      final artifacts = proposals;
      artifactId = artifacts.length == 1 ? artifacts.single.id : null;
      destinations = const [];
      destinationRef = null;
      expanded = false;
      error = null;
      submitting = false;
      loadingDestinations = false;
      destinationsLoaded = false;
    }
  }

  Future<void> _loadDestinations() async {
    final messageEpoch = _messageEpoch;
    final destinationEpoch = ++_destinationEpoch;
    final receipt = widget.message.executionReceipt;
    final selectedArtifact = artifactId;
    if (receipt == null || selectedArtifact == null) return;
    setState(() {
      loadingDestinations = true;
      destinationsLoaded = false;
      destinations = const [];
      destinationRef = null;
      error = null;
    });
    try {
      final loaded = await widget.dayGateway.loadExternalCalendarDestinations();
      if (!mounted ||
          messageEpoch != _messageEpoch ||
          destinationEpoch != _destinationEpoch ||
          artifactId != selectedArtifact) {
        return;
      }
      setState(() {
        destinations = loaded;
        destinationRef = loaded.length == 1 ? loaded.single.destinationRef : null;
        loadingDestinations = false;
        destinationsLoaded = true;
      });
    } on Object catch (failure) {
      if (!mounted ||
          messageEpoch != _messageEpoch ||
          destinationEpoch != _destinationEpoch ||
          artifactId != selectedArtifact) {
        return;
      }
      setState(() {
        error = failure.toString();
        loadingDestinations = false;
      });
    }
  }

  Future<void> _submit(AgentArtifact artifact) async {
    final commands = pendingCommands;
    final receipt = widget.message.executionReceipt;
    final session = widget.controller.session;
    final destination = destinationRef;
    if (commands == null ||
        !calendarChangesAvailable ||
        session == null ||
        receipt == null ||
        receipt.execution.taskId != widget.message.callId ||
        destination == null ||
        !destinations.any((choice) => choice.destinationRef == destination) ||
        submitting ||
        !form.currentState!.validate()) {
      return;
    }
    final payload = <String, Object?>{
      'kind': 'conversation.calendar_proposal.submit',
      'session_id': session.id,
      'origin_run_id': widget.message.turnId,
      'receipt': receipt.toJson(),
      'artifact_id': artifact.id,
      'destination_ref': destination,
    };
    final payloadKey = jsonEncode(payload);
    final epoch = _messageEpoch;
    setState(() {
      submitting = true;
      error = null;
    });
    try {
      final commandId = commands.retain(payloadKey);
      final result = await widget.controller.submitCalendarProposal(
        commandId: commandId,
        originRunId: widget.message.turnId,
        receipt: receipt.toJson(),
        artifactId: artifact.id,
        destinationRef: destination,
      );
      commands.acknowledge(payloadKey, commandId);
      if (!mounted || epoch != _messageEpoch) return;
      setState(() {
        actionsByIntent[_actionKey(receipt, artifact.id)] = result.operation;
        submitting = false;
      });
    } on Object catch (failure) {
      if (!mounted || epoch != _messageEpoch) return;
      setState(() {
        error = failure.toString();
        submitting = false;
      });
    }
  }

  void _acceptAction(String key, CalendarAction observed) {
    final current = actionsByIntent[key];
    if (current != null) {
      if (observed.isOlderObservationThan(current)) return;
      if (!observed.follows(current)) {
        throw StateError('The Calendar operation observation regressed.');
      }
    }
    actionsByIntent[key] = observed;
  }

  Future<void> _refreshAction(CalendarAction action) async {
    final receipt = widget.message.executionReceipt;
    final selectedArtifact = artifactId;
    if (submitting || receipt == null || selectedArtifact == null) return;
    final key = _actionKey(receipt, selectedArtifact);
    final epoch = _messageEpoch;
    setState(() {
      submitting = true;
      error = null;
    });
    try {
      await widget.controller.refreshInteractions();
      final observed = widget.controller.interactionSnapshots
          .map((snapshot) => snapshot.target)
          .whereType<AgentOperationApprovalTarget>()
          .map((target) => target.operation)
          .where((operation) => operation.actionRef == action.actionRef)
          .firstOrNull;
      if (observed == null) {
        throw StateError('Calendar operation status is not available yet.');
      }
      if (!mounted || epoch != _messageEpoch) return;
      setState(() {
        _acceptAction(key, observed);
        submitting = false;
      });
    } on Object catch (failure) {
      if (!mounted || epoch != _messageEpoch) return;
      setState(() {
        error = failure.toString();
        submitting = false;
      });
    }
  }

  String _statusText(AppLocalizations strings, CalendarAction? action) {
    if (action == null) {
      if (widget.message.executionReceipt == null || !receiptMatches) {
        return 'This proposal is not ready to submit yet.';
      }
      if (error case final currentError?) return currentError;
      if (loadingDestinations) return strings.actionLoading;
      if (!expanded) return 'Review this proposal.';
      if (destinationRef != null) return 'Ready to add to your calendar.';
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
      CalendarActionState.failed => 'Calendar change was not applied.',
      CalendarActionState.unknown => strings.actionUnknown,
      CalendarActionState.succeeded =>
        action.status.collection == ActionCollectionStatus.pending
            ? 'Calendar change succeeded; Day collection is pending.'
            : strings.actionSucceeded,
    };
  }

  FloeBadgeTone _statusTone(CalendarAction? action) =>
      switch (action?.status.state) {
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
              'Review this proposal before adding it to your calendar. Your Calendar permissions still apply.',
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
            ] else if (!expanded && currentAction == null) ...[
              const SizedBox(height: 8),
              FloeButton.outlined(
                onPressed: submitting
                    ? null
                    : () {
                        setState(() => expanded = true);
                        unawaited(_loadDestinations());
                      },
                child: const Text('Review proposal'),
              ),
            ] else ...[
              const SizedBox(height: 12),
              if (artifacts.length > 1)
                FloeSelect<String>(
                  label: 'Proposal',
                  value: artifacts.any((artifact) => artifact.id == artifactId)
                      ? artifactId
                      : null,
                  options: artifacts
                      .map((artifact) => FloeSelectOption(
                            value: artifact.id,
                            label: artifact.name,
                          ))
                      .toList(growable: false),
                  enabled: !submitting,
                  onChanged: (value) {
                    setState(() {
                      artifactId = value;
                      destinations = const [];
                      destinationRef = null;
                      destinationsLoaded = false;
                      loadingDestinations = false;
                    });
                    unawaited(_loadDestinations());
                  },
                  validator: (value) =>
                      value == null ? strings.actionFormInvalid : null,
                ),
              if (currentAction == null) ...[
                if (destinations.length == 1)
                  Text('${strings.actionDestination}: ${destinations.single.label}')
                else
                  FloeSelect<String>(
                    label: strings.actionDestination,
                    value: destinations.any(
                      (destination) => destination.destinationRef == destinationRef,
                    )
                        ? destinationRef
                        : null,
                    options: destinations
                        .map((destination) => FloeSelectOption(
                              value: destination.destinationRef,
                              label: destination.label,
                            ))
                        .toList(growable: false),
                    enabled: !loadingDestinations && !submitting,
                    onChanged: (value) => setState(() => destinationRef = value),
                    validator: (value) =>
                        value == null ? strings.actionFormInvalid : null,
                  ),
                if (destinationsLoaded &&
                    destinations.isEmpty &&
                    !loadingDestinations &&
                    error == null) ...[
                  const SizedBox(height: 8),
                  const Text('No Calendar destinations are currently available.'),
                ],
                if (error case final failure?) ...[
                  const SizedBox(height: 8),
                  Text(failure),
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
                  child: const Text('Add to calendar'),
                ),
              ] else ...[
                Text(currentAction.title),
                Text(DateFormat.yMMMd(
                  Localizations.localeOf(context).toLanguageTag(),
                ).add_jm().format(currentAction.schedule.startsAt.toLocal())),
                if (error case final failure?) Text(failure),
                FloeButton.text(
                  onPressed: submitting ? null : () => _refreshAction(currentAction),
                  child: const Text('Refresh approval status'),
                ),
              ],
            ],
          ],
        ),
      ),
    );
  }
}
