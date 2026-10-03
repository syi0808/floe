import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';

final class AgentMemoryReviewSettings extends StatelessWidget {
  const AgentMemoryReviewSettings({super.key, required this.controller});

  final AgentMemoryController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
    final candidates = controller.candidates;
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Memory review', style: FloeType.titleLarge),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Floe only saves proposed personal knowledge after you approve it.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: FloeSpace.md),
          if (controller.reviewFailure != null)
            Text(
              'Memory review is temporarily unavailable.',
              style: FloeType.body.copyWith(color: FloePalette.error600),
            )
          else if (candidates == null)
            Text(
              'Loading proposed memories…',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            )
          else if (candidates.isEmpty)
            Text(
              'No pending memory changes.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            )
          else
            for (final candidate in candidates) ...[
              _Candidate(controller: controller, candidate: candidate),
              if (candidate != candidates.last)
                const SizedBox(height: FloeSpace.sm),
            ],
          if (controller.acknowledgement case final acknowledgement?) ...[
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Confirmed ${acknowledgement.decision.name} · '
              '${acknowledgement.committedAt.toLocal()}',
              key: const ValueKey('memory-decision-acknowledgement'),
              style: FloeType.bodySmall.copyWith(
                color: FloePalette.neutral600,
              ),
            ),
          ],
          if (controller.canRetryDecision) ...[
            const SizedBox(height: FloeSpace.sm),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.outlined(
                key: const ValueKey('memory-decision-retry'),
                onPressed: controller.retryPendingDecision,
                child: const Text('Retry pending decision'),
              ),
            ),
          ],
        ],
      ),
    );
    },
  );
}

final class _Candidate extends StatelessWidget {
  const _Candidate({required this.controller, required this.candidate});

  final AgentMemoryController controller;
  final AgentMemoryCandidate candidate;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.md,
    fill: FloePalette.neutral50,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(candidate.statement, style: FloeType.body),
        const SizedBox(height: FloeSpace.xs),
        Text(
          '${candidate.operation} · ${candidate.memoryKind} · ${candidate.epistemicStatus} · '
          '${candidate.sourceCount} source${candidate.sourceCount == 1 ? '' : 's'}',
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.sm),
        Wrap(
          spacing: FloeSpace.sm,
          children: [
            FloeButton.filled(
              key: ValueKey('memory-approve-${candidate.id}'),
              size: FloeButtonSize.compact,
              onPressed: controller.canReview &&
                      candidate.allowedActions.contains(
                        AgentMemoryDecision.approve,
                      )
                  ? () => controller.decide(
                      candidate.id,
                      AgentMemoryDecision.approve,
                    )
                  : null,
              child: const Text('Approve'),
            ),
            FloeButton.text(
              key: ValueKey('memory-reject-${candidate.id}'),
              size: FloeButtonSize.compact,
              onPressed: controller.canReview &&
                      candidate.allowedActions.contains(
                        AgentMemoryDecision.reject,
                      )
                  ? () => controller.decide(
                      candidate.id,
                      AgentMemoryDecision.reject,
                    )
                  : null,
              child: const Text('Reject'),
            ),
          ],
        ),
      ],
    ),
  );
}
