import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_mascot.dart';
import '../../app/floe_selection.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import 'agent_capability_label.dart';
import 'agent_controller.dart';
import 'agent_fixture_gateway.dart';
import 'agent_proposal_card.dart';
import 'agent_vault_gateway.dart';

class AgentPanel extends StatefulWidget {
  const AgentPanel({
    super.key,
    required this.controller,
    required this.onClose,
    this.onOpenAction,
  });

  final AgentController controller;
  final VoidCallback onClose;
  final Future<void> Function(String actionId)? onOpenAction;

  @override
  State<AgentPanel> createState() => _AgentPanelState();
}

class _AgentPanelState extends State<AgentPanel> {
  final _scroll = ScrollController();
  final _actionFocus = FocusNode();
  AgentFixturePrompt _prompt = AgentFixturePrompt.today;
  bool _wasBusy = false;

  @override
  void initState() {
    super.initState();
    _wasBusy = widget.controller.busy;
    widget.controller.addListener(_changed);
  }

  @override
  void didUpdateWidget(AgentPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_changed);
      widget.controller.addListener(_changed);
      _wasBusy = widget.controller.busy;
    }
  }

  void _changed() {
    final restoreFocus = _wasBusy && !widget.controller.busy;
    _wasBusy = widget.controller.busy;
    final follow = !_scroll.hasClients || _scroll.position.extentAfter < 64;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (follow && _scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
      if (restoreFocus) _actionFocus.requestFocus();
    });
  }

  @override
  void dispose() {
    widget.controller.removeListener(_changed);
    _scroll.dispose();
    _actionFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.escape): widget.onClose,
    },
    child: FocusTraversalGroup(
      child: FloeSquircle(
        size: FloeSquircleSize.xl,
        child: AnimatedBuilder(
          animation: widget.controller,
          builder: (context, _) {
            final strings = AppLocalizations.of(context);
            final controller = widget.controller;
            final header = Padding(
              padding: const EdgeInsets.all(FloeSpace.base),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const FloeMascot(size: 32),
                      const SizedBox(width: FloeSpace.sm),
                      const Expanded(
                        child: Text(
                          'Floe',
                          style: TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      FloeButton.icon(
                        tooltip: strings.agentNewConversation,
                        onPressed:
                            !controller.canSend ||
                                controller.needsReload ||
                                controller.needsRecovery
                            ? null
                            : () => controller.load(newSession: true),
                        icon: const Icon(LucideIcons.squarePen, size: 18),
                      ),
                      FloeButton.icon(
                        tooltip: strings.close,
                        onPressed: widget.onClose,
                        icon: const Icon(LucideIcons.x, size: 18),
                      ),
                    ],
                  ),
                  const SizedBox(height: FloeSpace.sm),
                  Text(
                    strings.agentSampleTitle,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: FloeSpace.xs),
                  Text(
                    controller.usesVault
                        ? strings.agentSecureSampleBoundary
                        : strings.agentSampleBoundary,
                    style: const TextStyle(
                      fontSize: 12,
                      color: FloePalette.neutral600,
                    ),
                  ),
                ],
              ),
            );
            final footer = _composer(strings, controller);
            return LayoutBuilder(
              builder: (context, constraints) {
                final compact =
                    constraints.maxHeight < 650 ||
                    MediaQuery.textScalerOf(context).scale(14) > 20;
                final messages = controller.messages;
                final content = messages.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(FloeSpace.base),
                        child: Text(
                          strings.agentEmpty,
                          style: const TextStyle(color: FloePalette.neutral600),
                        ),
                      )
                    : ListView.separated(
                        controller: compact ? null : _scroll,
                        shrinkWrap: compact,
                        physics: compact
                            ? const NeverScrollableScrollPhysics()
                            : null,
                        padding: const EdgeInsets.all(FloeSpace.base),
                        itemCount: messages.length,
                        separatorBuilder: (_, _) =>
                            const SizedBox(height: FloeSpace.base),
                        itemBuilder: (context, index) =>
                            _message(strings, messages[index]),
                      );
                if (compact) {
                  return SingleChildScrollView(
                    controller: _scroll,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        header,
                        const Divider(height: 1),
                        content,
                        footer,
                      ],
                    ),
                  );
                }
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    header,
                    const Divider(height: 1),
                    Expanded(child: content),
                    footer,
                  ],
                );
              },
            );
          },
        ),
      ),
    ),
  );

  Widget _message(
    AppLocalizations strings,
    AgentMessage message,
  ) => switch (message) {
    AgentTextMessage(:final kind, :final text) => FloeSquircle(
      size: FloeSquircleSize.md,
      fill: kind == AgentMessageKind.assistant
          ? FloePalette.primary50
          : FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.all(FloeSpace.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            kind == AgentMessageKind.assistant ? 'Floe' : strings.agentYou,
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: FloeSpace.sm),
          SelectableText(
            text,
            style: const TextStyle(fontSize: 14, height: 1.5),
          ),
        ],
      ),
    ),
    AgentCapabilityMessage() => ExpansionTile(
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: FloeSpace.sm),
      title: Text(
        strings.agentSource,
        style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      ),
      subtitle: Text(
        strings.agentSourceDetails,
        style: const TextStyle(fontSize: 12),
      ),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: SelectableText(
            _sourceText(strings, message),
            style: const TextStyle(fontSize: 13, color: FloePalette.neutral600),
          ),
        ),
        if (widget.controller.expertResult(message)?.proposal != null) ...[
          const SizedBox(height: FloeSpace.md),
          AgentProposalCard(
            controller: widget.controller,
            message: message,
            onOpenAction: widget.onOpenAction,
          ),
        ],
      ],
    ),
  };

  String _sourceText(AppLocalizations strings, AgentCapabilityMessage message) {
    final result = widget.controller.expertResult(message);
    if (result == null) {
      final output = message.output;
      return output == null || output.trimLeft().startsWith('{')
          ? strings.agentSourceUnavailable
          : output;
    }
    String clock(DateTime time) =>
        '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
    return [
      strings.agentExpertSource(agentCapabilityTitle(result.expert)),
      for (final insight in result.insights)
        switch (insight.kind) {
          'commitment' => strings.agentExpertCommitment(
            insight.title!,
            clock(insight.start!),
            clock(insight.end!),
          ),
          'focus_window' => strings.agentExpertFocus(
            clock(insight.start!),
            clock(insight.end!),
          ),
          _ => strings.agentExpertNoFocus,
        },
    ].join('\n');
  }

  Widget _composer(AppLocalizations strings, AgentController controller) {
    final status = _status(strings, controller);
    final storageLocked =
        controller.usesVault && controller.vaultState != AgentVaultState.ready;
    final label = storageLocked
        ? strings.agentReload
        : controller.running
        ? strings.agentStop
        : controller.needsReload
        ? strings.agentReload
        : controller.needsRecovery
        ? strings.agentRecover
        : strings.agentSend;
    final VoidCallback? action = storageLocked
        ? controller.busy
              ? null
              : () => controller.load()
        : controller.running
        ? controller.progress == AgentProgress.stopping
              ? null
              : () => controller.stop()
        : controller.busy
        ? null
        : controller.needsReload
        ? () => controller.load()
        : controller.needsRecovery
        ? () => controller.recover()
        : controller.canSend
        ? () => controller.send(_prompt)
        : null;
    return Padding(
      padding: const EdgeInsets.all(FloeSpace.base),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (status != null) ...[
            Semantics(
              liveRegion: true,
              child: Text(
                status,
                style: const TextStyle(
                  fontSize: 13,
                  color: FloePalette.neutral600,
                ),
              ),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          if (!storageLocked)
            FloeSelect<AgentFixturePrompt>(
              label: strings.agentPrompt,
              value: _prompt,
              enabled: controller.canSend,
              options: [
                FloeSelectOption(
                  value: AgentFixturePrompt.today,
                  label: strings.agentBriefing,
                ),
                FloeSelectOption(
                  value: AgentFixturePrompt.followUp,
                  label: strings.agentFollowUp,
                ),
              ],
              onChanged: (value) {
                if (value != null) setState(() => _prompt = value);
              },
            ),
          const SizedBox(height: FloeSpace.md),
          FloeButton.filled(
            onPressed: action,
            focusNode: _actionFocus,
            child: Text(label),
          ),
          if (controller.canRetry) ...[
            const SizedBox(height: FloeSpace.sm),
            FloeButton.text(
              onPressed: controller.retry,
              child: Text(strings.agentRetry),
            ),
          ],
        ],
      ),
    );
  }

  String? _status(AppLocalizations strings, AgentController controller) {
    if (controller.busy) {
      return switch (controller.progress) {
        AgentProgress.loading => strings.agentLoading,
        AgentProgress.capability => strings.agentReading,
        AgentProgress.stopping => strings.agentStopping,
        _ => strings.agentPreparing,
      };
    }
    if (controller.usesVault &&
        controller.vaultState != AgentVaultState.ready) {
      return switch (controller.vaultState) {
        AgentVaultState.missing => strings.agentStorageMissing,
        AgentVaultState.locked => strings.agentStorageLocked,
        _ => strings.agentStorageUnavailable,
      };
    }
    if (controller.needsReload) return strings.agentReloadNeeded;
    if (controller.needsRecovery) return strings.agentInterrupted;
    return switch (controller.failure) {
      null => null,
      'cancelled' => strings.agentStopped,
      'interrupted' => strings.agentRecovered,
      'model_unavailable' => strings.agentUnavailable,
      'stalled' => strings.agentStalled,
      'budget_exceeded' || 'deadline_exceeded' => strings.agentBudget,
      _ => strings.agentFailure,
    };
  }
}
