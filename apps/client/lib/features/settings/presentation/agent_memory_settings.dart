import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/settings/presentation/agent_memory_review_settings.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';

final class AgentMemorySettingsCard extends StatelessWidget {
  const AgentMemorySettingsCard({
    super.key,
    required this.controller,
    required this.onManage,
  });

  final AgentMemoryController controller;
  final VoidCallback onManage;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final overview = controller.overview;
      return FloeSquircle(
        padding: const EdgeInsets.all(FloeSpace.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Memory', style: FloeType.titleLarge),
            const SizedBox(height: FloeSpace.xs),
            Text(
              'Review and manage the personal details Floe may use in future conversations.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
            const SizedBox(height: FloeSpace.md),
            if (controller.failure != null)
              Text(
                'Memory is temporarily unavailable.',
                style: FloeType.body.copyWith(color: FloePalette.error600),
              )
            else if (overview == null)
              Text(
                'Loading memory…',
                style: FloeType.body.copyWith(color: FloePalette.neutral600),
              )
            else
              Text(
                '${overview.savedCount} saved · ${overview.pendingCount} pending',
                key: const ValueKey('memory-summary-counts'),
                style: FloeType.body,
              ),
            const SizedBox(height: FloeSpace.md),
            FloeButton.outlined(
              key: const ValueKey('manage-memory'),
              onPressed: overview == null ? null : onManage,
              child: const Text('Manage memory'),
            ),
          ],
        ),
      );
    },
  );
}

final class AgentMemorySettings extends StatefulWidget {
  const AgentMemorySettings({
    super.key,
    required this.controller,
    required this.runtime,
    required this.onBack,
  });

  final AgentMemoryController controller;
  final RuntimeController runtime;
  final VoidCallback onBack;

  @override
  State<AgentMemorySettings> createState() => _AgentMemorySettingsState();
}

final class _AgentMemorySettingsState extends State<AgentMemorySettings> {
  AgentMemoryController get controller => widget.controller;
  bool _wasReady = false;

  @override
  void initState() {
    super.initState();
    widget.runtime.addListener(_readinessChanged);
    _readinessChanged();
  }

  @override
  void didUpdateWidget(AgentMemorySettings oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.runtime != widget.runtime ||
        oldWidget.controller != controller) {
      oldWidget.runtime.removeListener(_readinessChanged);
      widget.runtime.addListener(_readinessChanged);
      _wasReady = false;
      _readinessChanged();
    }
  }

  void _readinessChanged() {
    final ready = widget.runtime.ready;
    if (!_wasReady && ready) {
      WidgetsBinding.instance.addPostFrameCallback((_) async {
        if (!mounted || !widget.runtime.ready) return;
        await controller.loadReview();
        if (!mounted || !widget.runtime.ready) return;
        await controller.load();
      });
    }
    _wasReady = ready;
  }

  @override
  void dispose() {
    widget.runtime.removeListener(_readinessChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final overview = controller.overview;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              key: const ValueKey('memory-back'),
              icon: const Icon(LucideIcons.arrowLeft, size: 18),
              onPressed: widget.onBack,
              child: const Text('Data & privacy'),
            ),
          ),
          const SizedBox(height: FloeSpace.sm),
          Text('Memory', style: FloeType.headlineLarge.copyWith(fontSize: 22)),
          const SizedBox(height: FloeSpace.sm),
          Text(
            'These are the personal details Floe can use to make future conversations more useful.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          if (controller.hasReview) ...[
            const SizedBox(height: FloeSpace.lg),
            AgentMemoryReviewSettings(controller: controller),
          ],
          const SizedBox(height: FloeSpace.lg),
          FloeSquircle(
            padding: const EdgeInsets.all(FloeSpace.lg),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text('Saved memories', style: FloeType.titleLarge),
                const SizedBox(height: FloeSpace.xs),
                Text(
                  'Only approved memories appear here.',
                  style: FloeType.body.copyWith(color: FloePalette.neutral600),
                ),
                const SizedBox(height: FloeSpace.md),
                if (controller.failure != null)
                  Text(
                    'Saved memories are temporarily unavailable.',
                    style: FloeType.body.copyWith(color: FloePalette.error600),
                  )
                else if (overview == null)
                  Text(
                    'Loading saved memories…',
                    style: FloeType.body.copyWith(
                      color: FloePalette.neutral600,
                    ),
                  )
                else if (overview.memories.isEmpty)
                  Text(
                    'No saved memories yet. Ask Floe to remember something to create a review item.',
                    key: const ValueKey('memory-empty'),
                    style: FloeType.body.copyWith(
                      color: FloePalette.neutral600,
                    ),
                  )
                else ...[
                  for (final memory in overview.memories) ...[
                    _MemoryRow(memory: memory),
                    if (memory != overview.memories.last)
                      const SizedBox(height: FloeSpace.sm),
                  ],
                  if (overview.savedCount > overview.memories.length) ...[
                    const SizedBox(height: FloeSpace.md),
                    Text(
                      'Showing ${overview.memories.length} of ${overview.savedCount} memories.',
                      style: FloeType.bodySmall.copyWith(
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ],
              ],
            ),
          ),
        ],
      );
    },
  );
}

final class _MemoryRow extends StatelessWidget {
  const _MemoryRow({required this.memory});

  final AgentMemory memory;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    key: ValueKey('memory-${memory.targetId}'),
    size: FloeSquircleSize.md,
    fill: FloePalette.neutral50,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(memory.statement, style: FloeType.body),
        const SizedBox(height: FloeSpace.xs),
        Text(
          '${memory.category} · ${memory.origin == AgentMemoryOrigin.userProvided ? 'Provided by you' : 'Learned with your approval'} · ${DateFormat.yMMMd().format(memory.createdAt.toLocal())}',
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
      ],
    ),
  );
}
