import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import 'agent_controller.dart';
import 'agent_registry.dart';
import 'agent_vault_gateway.dart';

class AgentRegistryDialog extends StatelessWidget {
  const AgentRegistryDialog({super.key, required this.controller});
  final AgentController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final registry = controller.registry;
      final ready = controller.vaultState == AgentVaultState.ready;
      return FloeDetailDialog(
        title: strings.agentRegistryTitle,
        loading: controller.busy,
        children: [
          Text(strings.agentRegistryBoundary),
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
            Text(strings.agentRegistryEmpty),
          if (ready && registry != null)
            for (final installation in registry.installations)
              Padding(
                padding: const EdgeInsets.only(bottom: FloeSpace.base),
                child: FloeSquircle(
                  size: FloeSquircleSize.md,
                  padding: const EdgeInsets.all(FloeSpace.md),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                        installation.packageId,
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      Text('${installation.kind} · ${installation.version}'),
                      SwitchListTile.adaptive(
                        key: ValueKey('installation-${installation.id}'),
                        contentPadding: EdgeInsets.zero,
                        title: Text(
                          strings.agentRegistryInstallation,
                          semanticsLabel:
                              '${installation.packageId}: ${strings.agentRegistryInstallation}',
                        ),
                        value: installation.enabled,
                        onChanged: controller.canManageRegistry
                            ? (enabled) => controller.configureRegistry(
                                AgentRegistryTarget.installation,
                                installation.id,
                                enabled,
                              )
                            : null,
                      ),
                      if (!registry.assignments.any(
                        (entry) => entry.installationId == installation.id,
                      ))
                        Text(strings.agentRegistryUnassigned),
                      for (final assignment in registry.assignments.where(
                        (entry) => entry.installationId == installation.id,
                      )) ...[
                        const Divider(),
                        SwitchListTile.adaptive(
                          key: ValueKey('assignment-${assignment.id}'),
                          contentPadding: EdgeInsets.zero,
                          title: Text(
                            strings.agentRegistryAssignment,
                            semanticsLabel:
                                '${installation.packageId}: ${strings.agentRegistryAssignment}',
                          ),
                          value: assignment.enabled,
                          onChanged: controller.canManageRegistry
                              ? (enabled) => controller.configureRegistry(
                                  AgentRegistryTarget.assignment,
                                  assignment.id,
                                  enabled,
                                )
                              : null,
                        ),
                        if (!installation.enabled)
                          Text(strings.agentRegistryInstallationOff),
                        Text(
                          strings.agentRegistryGrants(
                            assignment.grantedViewCount,
                            assignment.grantedToolCount,
                          ),
                        ),
                        Text(
                          strings.agentRegistryState(
                            assignment.stateRevision,
                            assignment.completedInvocations,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
          const SizedBox(height: FloeSpace.sm),
          FloeButton.outlined(
            onPressed: controller.canManageRegistry
                ? controller.loadRegistry
                : null,
            child: Text(strings.agentRegistryRefresh),
          ),
        ],
      );
    },
  );
}
