import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
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
    required this.message,
    this.onOpenSourceReview,
    this.onOpenConnections,
  });

  final AgentController controller;
  final AgentInteractionMessage message;
  final VoidCallback? onOpenSourceReview;
  final VoidCallback? onOpenConnections;

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
    if (oldWidget.message.interactionId != widget.message.interactionId) {
      _ensure();
    }
  }

  void _ensure() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.controller.ensureInteraction(widget.message.interactionId);
    });
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final controller = widget.controller;
      final snapshot = controller.interactionFor(widget.message.interactionId);
      final busy = controller.interactionBusyFor(widget.message.interactionId);
      final failure = controller.interactionFailureFor(
        widget.message.interactionId,
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
        AgentInteractionState.cancelled =>
          strings.agentInteractionStateCancelled,
        AgentInteractionState.superseded =>
          strings.agentInteractionStateSuperseded,
        AgentInteractionState.expired => strings.agentInteractionStateExpired,
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
                    AgentInteractionKind.processingRecipient =>
                      strings.agentInteractionConsentTitle,
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
    AgentInlineObserveTarget(
      :final connectionId,
      :final sourceId,
      :final purpose,
      :final consumer,
      :final members,
    ) =>
      [
        _row(strings.agentInteractionConnection, connectionId),
        _row(strings.agentInteractionSource, sourceId),
        _row(strings.agentInteractionPurpose, purpose),
        _row(strings.agentInteractionConsumer, consumer),
        if (members.isNotEmpty)
          _row(
            strings.agentInteractionMembers,
            members.map((member) => member.resource).join(', '),
          ),
      ],
    AgentNavigationOnlyTarget(
      :final destination,
      :final sourceId,
      :final purpose,
    ) =>
      [
        _row(strings.agentInteractionNextStep, switch (destination) {
          AgentNavigationDestination.connectionSettings =>
            strings.agentInteractionOpenConnection,
          AgentNavigationDestination.systemPermission =>
            strings.agentInteractionRequestPermission,
          AgentNavigationDestination.resourcePicker =>
            strings.agentInteractionReviewSource,
        }),
        _row(strings.agentInteractionSource, sourceId),
        _row(strings.agentInteractionPurpose, purpose),
      ],
    AgentRecipientConsentTarget(
      :final recipient,
      :final profileId,
      :final purpose,
      :final consumer,
      :final inputDataClasses,
      :final sourceScopes,
    ) =>
      [
        _row(strings.agentInteractionRecipient, recipient),
        _row(strings.agentInteractionProfile, profileId),
        _row(strings.agentInteractionPurpose, purpose),
        _row(strings.agentInteractionConsumer, consumer),
        if (inputDataClasses.isNotEmpty)
          _row(strings.agentInteractionData, inputDataClasses.join(', ')),
        for (final scope in sourceScopes)
          _row(
            strings.agentInteractionScopes,
            '${scope.connectionId} · ${scope.resources.join(', ')} · ${scope.operation}',
          ),
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
    AgentController controller,
    AgentInteractionSnapshot snapshot,
    AgentInteractionAction action,
    bool busy,
  ) {
    final label = switch (action) {
      AgentInteractionAction.allow => strings.agentInteractionAllow,
      AgentInteractionAction.deny => strings.agentInteractionDeny,
      AgentInteractionAction.dismiss => strings.agentInteractionDismiss,
      AgentInteractionAction.refresh => strings.agentInteractionRefresh,
      AgentInteractionAction.continueRequest =>
        strings.agentInteractionContinue,
      AgentInteractionAction.openConnection =>
        strings.agentInteractionOpenConnection,
      AgentInteractionAction.reviewSource =>
        strings.agentInteractionReviewSource,
      AgentInteractionAction.requestPermission =>
        strings.agentInteractionRequestPermission,
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
            AgentInteractionAction.continueRequest =>
              () => controller.continueInteraction(snapshot),
            AgentInteractionAction.openConnection => () {
              widget.onOpenConnections?.call();
              controller.refreshInteraction(snapshot);
            },
            AgentInteractionAction.reviewSource => () {
              widget.onOpenSourceReview?.call();
              controller.refreshInteraction(snapshot);
            },
            AgentInteractionAction.requestPermission =>
              () => controller.refreshInteraction(snapshot),
          };
    if (action == AgentInteractionAction.allow ||
        action == AgentInteractionAction.continueRequest) {
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
