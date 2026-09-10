import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_squircle.dart';
import 'agent_controller.dart';
import 'agent_memory_review.dart';

final class AgentMemoryReviewSettings extends StatelessWidget {
  const AgentMemoryReviewSettings({super.key, required this.controller});

  final AgentController controller;

  @override
  Widget build(BuildContext context) {
    final candidates = controller.memoryCandidates;
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
          if (controller.memoryReviewFailure != null)
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
        ],
      ),
    );
  }
}

final class _Candidate extends StatelessWidget {
  const _Candidate({required this.controller, required this.candidate});

  final AgentController controller;
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
          '${candidate.memoryKind} · ${candidate.epistemicStatus} · '
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
              onPressed: controller.canReviewMemory
                  ? () => controller.decideMemoryCandidate(
                      candidate.id,
                      AgentMemoryDecision.approve,
                    )
                  : null,
              child: const Text('Approve'),
            ),
            FloeButton.text(
              key: ValueKey('memory-reject-${candidate.id}'),
              size: FloeButtonSize.compact,
              onPressed: controller.canReviewMemory
                  ? () => controller.decideMemoryCandidate(
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
