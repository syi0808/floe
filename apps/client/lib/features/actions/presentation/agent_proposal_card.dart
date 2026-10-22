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
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/l10n/app_localizations.dart';

class AgentProposalCard extends StatefulWidget {
  const AgentProposalCard({
    super.key,
    required this.controller,
    required this.message,
  });

  final ConversationController controller;
  final AgentCapabilityMessage message;

  @override
  State<AgentProposalCard> createState() => _AgentProposalCardState();
}

class _AgentProposalCardState extends State<AgentProposalCard> {
  static const proposalMediaType =
      'application/vnd.floe.actions.calendar-proposal+json;version=1';

  final form = GlobalKey<FormState>();
  bool expanded = false;
  ReadyActionProposal? proposalDetails;
  List<ActionDestinationChoice> get destinations =>
      proposalDetails?.destinations ?? const [];
  String? destinationRef;
  String? artifactId;
  ConversationCommandReplay? pendingCommands;
  final Map<String, CalendarAction> actionsByIntent = {};
  String? error;
  bool loadingPreview = false;
  bool previewLoaded = false;
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
      previewLoaded &&
      !loadingPreview &&
      destinations.isNotEmpty;

  CalendarAction? get selectedAction {
    final receipt = widget.message.executionReceipt;
    final selectedArtifactId = artifactId;
    if (receipt == null || selectedArtifactId == null) return null;
    final key = _actionKey(receipt, selectedArtifactId);
    return actionsByIntent[key];
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
        oldWidget.controller != widget.controller ||
        oldWidget.controller.gateway != widget.controller.gateway;
    if (gatewayChanged) {
      pendingCommands = ConversationCommandReplay.forGateway(
        widget.controller.gateway,
      );
      actionsByIntent.clear();
    }
    if (gatewayChanged ||
        oldWidget.message.messageId != widget.message.messageId ||
        oldWidget.message.callId != widget.message.callId ||
        oldWidget.message.executionReceipt != widget.message.executionReceipt) {
      _messageEpoch++;
      _destinationEpoch++;
      final artifacts = proposals;
      artifactId = artifacts.length == 1 ? artifacts.single.id : null;
      proposalDetails = null;
      destinationRef = null;
      expanded = false;
      error = null;
      submitting = false;
      loadingPreview = false;
      previewLoaded = false;
    }
  }

  Future<void> _loadPreview() async {
    final messageEpoch = _messageEpoch;
    final destinationEpoch = ++_destinationEpoch;
    final receipt = widget.message.executionReceipt;
    final selectedArtifact = artifactId;
    if (receipt == null || selectedArtifact == null) return;
    setState(() {
      loadingPreview = true;
      previewLoaded = false;
      proposalDetails = null;
      destinationRef = null;
      error = null;
    });
    try {
      final preview = await widget.controller.previewCalendarProposal(
        receipt: receipt.toJson(),
        artifactId: selectedArtifact,
      );
      if (!mounted ||
          messageEpoch != _messageEpoch ||
          destinationEpoch != _destinationEpoch ||
          artifactId != selectedArtifact) {
        return;
      }
      setState(() {
        switch (preview) {
          case ExistingActionProposal(:final action):
            _acceptAction(_actionKey(receipt, selectedArtifact), action);
            proposalDetails = null;
            destinationRef = null;
          case final ReadyActionProposal ready:
            proposalDetails = ready;
            destinationRef = ready.destinations.length == 1
                ? ready.destinations.single.destinationRef
                : null;
        }
        loadingPreview = false;
        previewLoaded = true;
      });
    } on Object catch (failure) {
      if (!mounted ||
          messageEpoch != _messageEpoch ||
          destinationEpoch != _destinationEpoch ||
          artifactId != selectedArtifact) {
        return;
      }
      setState(() {
        proposalDetails = null;
        destinationRef = null;
        error = failure.toString();
        loadingPreview = false;
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
      final preview = await widget.controller.previewCalendarProposal(
        receipt: receipt.toJson(),
        artifactId: selectedArtifact,
      );
      if (preview is! ExistingActionProposal ||
          preview.action.actionRef != action.actionRef) {
        throw StateError('Calendar proposal operation identity changed.');
      }
      final observed = preview.action;
      if (!mounted ||
          epoch != _messageEpoch ||
          artifactId != selectedArtifact) {
        return;
      }
      final currentObservation = observed;
      setState(() {
        _acceptAction(key, currentObservation);
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

  String? _statusText(AppLocalizations strings, CalendarAction? action) {
    if (action == null) {
      if (loadingPreview) return strings.actionLoading;
      return null;
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
    final statusText = _statusText(strings, currentAction);
    return FloeSquircle(
      padding: const EdgeInsets.all(12),
      child: Form(
        key: form,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(strings.agentProposalTitle, style: FloeType.controlLabel),
            if (currentAction == null) ...[
              const SizedBox(height: 8),
              Text(
                strings.agentProposalReviewReminder,
                style: FloeType.bodySmall.copyWith(fontSize: 12),
              ),
            ],
            if (statusText != null) ...[
              const SizedBox(height: 8),
              Semantics(
                liveRegion: true,
                child: Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: FloeBadge(
                    label: statusText,
                    tone: error == null
                        ? _statusTone(currentAction)
                        : FloeBadgeTone.danger,
                  ),
                ),
              ),
            ],
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
                        unawaited(_loadPreview());
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
                      .map(
                        (artifact) => FloeSelectOption(
                          value: artifact.id,
                          label: artifact.name,
                        ),
                      )
                      .toList(growable: false),
                  enabled: !submitting,
                  onChanged: (value) {
                    setState(() {
                      artifactId = value;
                      _destinationEpoch++;
                      proposalDetails = null;
                      destinationRef = null;
                      error = null;
                      previewLoaded = false;
                      loadingPreview = false;
                    });
                    unawaited(_loadPreview());
                  },
                  validator: (value) =>
                      value == null ? strings.actionFormInvalid : null,
                ),
              if (currentAction == null) ...[
                if (proposalDetails case final ready?) ...[
                  if (artifacts.length > 1) const SizedBox(height: 12),
                  _ProposalPresentation(
                    title: ready.title,
                    schedule: ready.schedule,
                  ),
                ],
                if (destinations.length == 1)
                  Text(
                    '${strings.actionDestination}: ${destinations.single.label}',
                  )
                else
                  FloeSelect<String>(
                    label: strings.actionDestination,
                    value:
                        destinations.any(
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
                    enabled: !loadingPreview && !submitting,
                    onChanged: (value) =>
                        setState(() => destinationRef = value),
                    validator: (value) =>
                        value == null ? strings.actionFormInvalid : null,
                  ),
                if (previewLoaded &&
                    destinations.isEmpty &&
                    !loadingPreview &&
                    error == null) ...[
                  const SizedBox(height: 8),
                  const Text(
                    'No Calendar destinations are currently available.',
                  ),
                ],
                if (error case final failure?) ...[
                  const SizedBox(height: 8),
                  Text(failure),
                  FloeButton.text(
                    onPressed: loadingPreview
                        ? null
                        : () => unawaited(_loadPreview()),
                    child: Text(strings.actionReload),
                  ),
                ],
                FloeButton.filled(
                  onPressed:
                      selectedArtifact == null ||
                          !calendarChangesAvailable ||
                          submitting ||
                          destinationRef == null
                      ? null
                      : () => _submit(selectedArtifact!),
                  loading: submitting,
                  child: const Text('Add to calendar'),
                ),
              ] else ...[
                _ProposalPresentation(
                  title: currentAction.title,
                  schedule: currentAction.schedule,
                ),
                if (error case final failure?) Text(failure),
                FloeButton.text(
                  onPressed: submitting
                      ? null
                      : () => _refreshAction(currentAction),
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

final class _ProposalPresentation extends StatelessWidget {
  const _ProposalPresentation({required this.title, required this.schedule});

  final String title;
  final ActionSchedule schedule;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final start = schedule.startsAt.toLocal();
    final end = schedule.endsAt.toLocal();
    final dateFormat = DateFormat.yMMMEd(strings.localeName);
    final alwaysUse24HourFormat = MediaQuery.alwaysUse24HourFormatOf(context);
    String offsetLabel(int seconds) {
      final minutes = seconds ~/ 60;
      final sign = minutes < 0 ? '−' : '+';
      return 'UTC$sign${(minutes.abs() ~/ 60).toString().padLeft(2, '0')}:${(minutes.abs() % 60).toString().padLeft(2, '0')}';
    }

    String time(DateTime value) => MaterialLocalizations.of(context)
        .formatTimeOfDay(
          TimeOfDay.fromDateTime(value),
          alwaysUse24HourFormat: alwaysUse24HourFormat,
        );

    String dateTime(DateTime value) {
      var clock = time(value);
      if (start.timeZoneOffset != end.timeZoneOffset) {
        clock += start.timeZoneName != end.timeZoneName
            ? ' ${value.timeZoneName}'
            : ' ${offsetLabel(value.timeZoneOffset.inSeconds)}';
      }
      return '${dateFormat.format(value)} · $clock';
    }

    return Semantics(
      container: true,
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(title, style: FloeType.title, softWrap: true),
            const SizedBox(height: 4),
            Text(strings.agentProposalStartsLocal(dateTime(start))),
            Text(strings.agentProposalEndsLocal(dateTime(end))),
          ],
        ),
      ),
    );
  }
}
