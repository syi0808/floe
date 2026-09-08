import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_squircle.dart';
import '../../app/floe_switch.dart';
import '../../l10n/app_localizations.dart';
import 'agent_capability_label.dart';
import 'agent_controller.dart';
import 'agent_registry.dart';
import 'agent_vault_gateway.dart';

class AgentRegistrySettings extends StatelessWidget {
  const AgentRegistrySettings({super.key, required this.controller});

  final AgentController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final registry = controller.registry;
      final ready = controller.vaultState == AgentVaultState.ready;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(strings.agentRegistryTitle, style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            strings.agentRegistryBoundary,
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: FloeSpace.base),
          if (!ready)
            Text(
              controller.vaultState == AgentVaultState.unavailable
                  ? strings.agentStorageUnavailable
                  : strings.agentStorageLocked,
            )
          else if (controller.registryFailure != null)
            Semantics(
              liveRegion: true,
              child: Text(strings.agentRegistryFailure),
            )
          else if (controller.registryLoaded &&
              (registry == null || registry.installations.isEmpty))
            Text(strings.agentRegistryEmpty)
          else if (registry != null)
            for (final installation in registry.installations)
              Padding(
                padding: const EdgeInsets.only(bottom: FloeSpace.sm),
                child: _CapabilityPermission(
                  controller: controller,
                  registry: registry,
                  installation: installation,
                ),
              ),
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: controller.canManageRegistry
                  ? controller.loadRegistry
                  : null,
              child: Text(strings.agentRegistryRefresh),
            ),
          ),
        ],
      );
    },
  );
}

class _CapabilityPermission extends StatelessWidget {
  const _CapabilityPermission({
    required this.controller,
    required this.registry,
    required this.installation,
  });

  final AgentController controller;
  final AgentRegistryView registry;
  final AgentInstallation installation;

  @override
  Widget build(BuildContext context) {
    final assignments = registry.assignments
        .where((entry) => entry.installationId == installation.id)
        .toList();
    final enabled =
        installation.enabled &&
        assignments.isNotEmpty &&
        assignments.every((entry) => entry.enabled);
    return FloeSquircle(
      size: FloeSquircleSize.md,
      fill: FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.symmetric(
        horizontal: FloeSpace.base,
        vertical: FloeSpace.sm,
      ),
      child: FloeSwitch(
        key: ValueKey('capability-${installation.id}'),
        value: enabled,
        onChanged: assignments.isNotEmpty && controller.canManageRegistry
            ? (value) => controller.configureCapability(installation.id, value)
            : null,
        label: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              agentCapabilityTitle(
                installation.packageId,
                kind: installation.kind,
              ),
              style: FloeType.controlLabel,
            ),
            const SizedBox(height: FloeSpace.xxs),
            Text(
              agentCapabilityDescription(
                installation.packageId,
                kind: installation.kind,
              ),
              style: FloeType.bodySmall.copyWith(
                color: FloePalette.neutral600,
                fontSize: 12,
                height: 1.4,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
