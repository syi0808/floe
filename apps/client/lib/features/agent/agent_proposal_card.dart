import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../app/floe_button.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import '../day_canvas/domain/calendar_action.dart';
import 'agent_controller.dart';
import 'agent_fixture_gateway.dart';

class AgentProposalCard extends StatelessWidget {
  const AgentProposalCard({
    super.key,
    required this.controller,
    required this.message,
    this.onOpenAction,
  });

  final AgentController controller;
  final AgentCapabilityMessage message;
  final Future<void> Function(String actionId)? onOpenAction;

  @override
  Widget build(BuildContext context) {
    final proposal = controller.expertResult(message)?.proposal;
    if (proposal == null) return const SizedBox.shrink();
    final strings = AppLocalizations.of(context);
    final inspection = controller.proposalFor(message.callId);
    final action = inspection?.action;
    final failed = controller.proposalFailureFor(message.callId) != null;
    final canInspect = controller.canInspectProposal(message);
    final date = DateFormat.yMMMd(
      Localizations.localeOf(context).toLanguageTag(),
    ).add_jm();
    final status = action == null
        ? null
        : switch (action.status) {
            CalendarActionStatus.pending => strings.actionPending,
            CalendarActionStatus.approved => strings.actionApproved,
            CalendarActionStatus.rejected => strings.actionRejected,
            CalendarActionStatus.executing => strings.actionExecuting,
            CalendarActionStatus.blocked => strings.actionBlocked,
            CalendarActionStatus.unknown => strings.actionUnknown,
            CalendarActionStatus.succeeded => strings.actionSucceeded,
          };
    return FloeSquircle(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.agentProposalTitle,
            style: const TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          Text(
            strings.agentProposalInterval(
              date.format(proposal.start!.toLocal()),
              date.format(proposal.end!.toLocal()),
            ),
          ),
          const SizedBox(height: 8),
          Semantics(
            liveRegion: true,
            child: Text(
              failed
                  ? strings.agentProposalFailed
                  : inspection == null
                  ? strings.agentProposalUnchecked
                  : status == null
                  ? strings.agentProposalUnprepared
                  : strings.agentProposalRecorded(status),
            ),
          ),
          const SizedBox(height: 8),
          Text(
            strings.agentProposalReadOnly,
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              FloeButton.outlined(
                onPressed: canInspect
                    ? () => controller.inspectProposal(message)
                    : null,
                child: Text(strings.agentProposalCheck),
              ),
              if (action != null && onOpenAction != null)
                FloeButton.text(
                  onPressed: canInspect ? () => onOpenAction!(action.id) : null,
                  child: Text(strings.agentProposalOpen),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
