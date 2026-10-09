import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// One durable review card, rendered from the backend snapshot only.
///
/// Buttons come only from the snapshot's backend-projected actions; this
/// widget never derives its own. Navigation actions open the owning
/// surface when the shell provides one, then always reconcile, because
/// only the backend can observe the owner's new state.
final class AgentInteractionCard extends StatefulWidget {
  const AgentInteractionCard({
    super.key,
    required this.controller,
    required this.interactionId,
    this.onOpenSourceReview,
    this.onOpenConnections,
    this.onOpenExpertSettings,
  });

  final ConversationController controller;
  final String interactionId;
  final void Function(AgentInteractionTarget? target)? onOpenSourceReview;
  final VoidCallback? onOpenConnections;
  final void Function(
    AgentExpertBindingTarget target,
    Future<void> Function() reconcileAfterReplacement,
  )?
  onOpenExpertSettings;

  @override
  State<AgentInteractionCard> createState() => _AgentInteractionCardState();
}

final class _AgentInteractionCardState extends State<AgentInteractionCard> {
  @override
  void initState() {
    super.initState();
    _ensure();
  }

  @override
  void didUpdateWidget(AgentInteractionCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.interactionId != widget.interactionId) {
      _ensure();
    }
  }

  void _ensure() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.controller.ensureInteraction(widget.interactionId);
    });
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final controller = widget.controller;
      final snapshot = controller.interactionFor(widget.interactionId);
      final busy = controller.interactionBusyFor(widget.interactionId);
      final failure = controller.interactionFailureFor(
        widget.interactionId,
      );
      if (snapshot == null) {
        return FloeSquircle(
          size: FloeSquircleSize.md,
          fill: FloePalette.neutral50,
          borderWidth: 0,
          padding: const EdgeInsets.all(FloeSpace.md),
          child: failure != null
              ? SelectableText(
                  _failureText(strings, failure),
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                  ),
                )
              : const Center(
                  child: SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                ),
        );
      }
      final stateLabel = switch (snapshot.state) {
        AgentInteractionState.pending => strings.agentInteractionStatePending,
        AgentInteractionState.resolving =>
          strings.agentInteractionStateResolving,
        AgentInteractionState.resolved => strings.agentInteractionStateResolved,
        AgentInteractionState.denied => strings.agentInteractionStateDenied,
        AgentInteractionState.dismissed =>
          strings.agentInteractionStateCancelled,
        AgentInteractionState.superseded =>
          strings.agentInteractionStateSuperseded,
        AgentInteractionState.expired => strings.agentInteractionStateExpired,
        AgentInteractionState.stale => strings.agentInteractionStale,
        AgentInteractionState.wrongDevice =>
          strings.agentInteractionWrongDevice,
      };
      return FloeSquircle(
        size: FloeSquircleSize.md,
        fill: FloePalette.primary50,
        borderWidth: 0,
        padding: const EdgeInsets.all(FloeSpace.md),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(switch (snapshot.kind) {
                    AgentInteractionKind.sourceAccess =>
                      strings.agentInteractionSourceTitle,
                    AgentInteractionKind.expertBinding =>
                      strings.agentInteractionExpertBindingTitle,
                    AgentInteractionKind.operationApproval =>
                      'Calendar change approval',
                  }, style: FloeType.label),
                ),
                FloeBadge(
                  label: stateLabel,
                  tone: snapshot.terminal
                      ? FloeBadgeTone.neutral
                      : FloeBadgeTone.info,
                ),
              ],
            ),
            const SizedBox(height: FloeSpace.sm),
            ..._targetRows(strings, snapshot.target),
            if (busy) ...[
              const SizedBox(height: FloeSpace.sm),
              const SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ],
            if (failure != null) ...[
              const SizedBox(height: FloeSpace.sm),
              SelectableText(
                _failureText(strings, failure),
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                ),
              ),
            ],
            if (snapshot.actions.isNotEmpty) ...[
              const SizedBox(height: FloeSpace.sm),
              Wrap(
                spacing: FloeSpace.sm,
                runSpacing: FloeSpace.sm,
                children: [
                  for (final action in snapshot.actions)
                    _actionButton(strings, controller, snapshot, action, busy),
                ],
              ),
            ],
          ],
        ),
      );
    },
  );

  List<Widget> _targetRows(
    AppLocalizations strings,
    AgentInteractionTarget target,
  ) => switch (target) {
    AgentSourceReviewTarget(:final review) => [
      _row(strings.agentInteractionMembers, review.displayMembers.join(', ')),
      for (final view in review.processingDisclosure.views) ...[
        _row('${view.viewId} · sensitivity', view.dataClassLabel),
        _row('${view.viewId} · data', view.dataCategories.join(', ')),
        _row(
          '${view.viewId} · current',
          view.current?.label ?? 'No Observe permission',
        ),
        _row('${view.viewId} · requested', view.requested!.label),
        if (view.expandsGateway)
          _row(
            'Gateway permission',
            'This review expands processing for ${view.viewId}.',
          ),
      ],
      if (review.processingDisclosure.views.any((view) => view.isDerivedHealth))
        _row(
          'Health',
          'Only locally transformed derived Health data may reach the Gateway. Raw Health data stays on this device.',
        ),
    ],
    AgentNavigationTarget(:final sourceLabel) => [
      _row(strings.agentInteractionSource, sourceLabel),
    ],
    AgentExpertBindingTarget(:final review) => [
      _row(strings.agentInteractionNextStep, review.requirementRef),
      _row(
        'Selected sources',
        review.candidates
            .where((candidate) => candidate.selected)
            .map((candidate) => candidate.label)
            .join(', '),
      ),
      _row(
        'Reviewed options',
        review.candidates.map((candidate) => candidate.label).join(', '),
      ),
    ],
    AgentOperationApprovalTarget(:final operation) => [
      _row('Calendar', operation.destinationLabel),
      _row('Change', operation.title),
      _row(
        'Time',
        '${operation.schedule.startsAt.toLocal()} – ${operation.schedule.endsAt.toLocal()}',
      ),
      _row('Operation status', operation.status.state.name),
    ],
  };

  Widget _row(String label, String value) => Padding(
    padding: const EdgeInsets.only(top: FloeSpace.xs),
    child: SelectableText.rich(
      TextSpan(
        style: FloeType.bodySmall,
        children: [
          TextSpan(
            text: '$label: ',
            style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
          ),
          TextSpan(text: value),
        ],
      ),
    ),
  );

  Widget _actionButton(
    AppLocalizations strings,
    ConversationController controller,
    AgentInteractionSnapshot snapshot,
    AgentInteractionAction action,
    bool busy,
  ) {
    final label = switch (action) {
      AgentInteractionAction.allow => strings.agentInteractionAllow,
      AgentInteractionAction.deny => strings.agentInteractionDeny,
      AgentInteractionAction.dismiss => strings.agentInteractionDismiss,
      AgentInteractionAction.refresh => strings.agentInteractionRefresh,
      AgentInteractionAction.openConnection =>
        strings.agentInteractionOpenConnection,
      AgentInteractionAction.reviewSource =>
        strings.agentInteractionReviewSource,
      AgentInteractionAction.requestPermission =>
        strings.agentInteractionRequestPermission,
      AgentInteractionAction.openExpertSettings =>
        strings.agentInteractionOpenExpertSettings,
    };
    final VoidCallback? onPressed = busy
        ? null
        : switch (action) {
            AgentInteractionAction.allow => () => controller.decideInteraction(
              snapshot,
              AgentInteractionDecision.approve,
            ),
            AgentInteractionAction.deny => () => controller.decideInteraction(
              snapshot,
              AgentInteractionDecision.deny,
            ),
            AgentInteractionAction.dismiss =>
              () => controller.decideInteraction(
                snapshot,
                AgentInteractionDecision.dismiss,
              ),
            AgentInteractionAction.refresh =>
              () => controller.refreshInteraction(snapshot),
            AgentInteractionAction.openConnection => () {
              widget.onOpenConnections?.call();
              controller.refreshInteraction(snapshot);
            },
            AgentInteractionAction.reviewSource => () {
              widget.onOpenSourceReview?.call(snapshot.target);
              controller.refreshInteraction(snapshot);
            },
            AgentInteractionAction.requestPermission => () {
              widget.onOpenSourceReview?.call(snapshot.target);
            },
            AgentInteractionAction.openExpertSettings => () {
              final target = snapshot.target;
              if (target is AgentExpertBindingTarget) {
                widget.onOpenExpertSettings?.call(
                  target,
                  () => controller.refreshInteraction(snapshot),
                );
              }
            },
          };
    if (action == AgentInteractionAction.allow) {
      return FloeButton.filled(
        onPressed: onPressed,
        size: FloeButtonSize.compact,
        child: Text(label),
      );
    }
    return FloeButton.outlined(
      onPressed: onPressed,
      size: FloeButtonSize.compact,
      child: Text(label),
    );
  }

  String _failureText(AppLocalizations strings, String failure) =>
      switch (failure) {
        'interaction_stale' => strings.agentInteractionStale,
        'interaction_wrong_device' => strings.agentInteractionWrongDevice,
        'interaction_expired' => strings.agentInteractionExpired,
        'interaction_unavailable' => strings.agentInteractionUnavailable,
        _ => strings.agentFailure,
      };
}
