import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_switch.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

class AgentRegistrySettings extends StatelessWidget {
  const AgentRegistrySettings({
    super.key,
    required this.controller,
    this.focus,
  });

  final AgentController controller;
  final AgentExpertBindingTarget? focus;

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
          if (focus != null) ...[
            const SizedBox(height: FloeSpace.xs),
            Text(
              '${focus!.packageId} · ${focus!.requirementKey}',
              style: FloeType.body,
            ),
          ],
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
                  focus: focus,
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
    required this.focus,
  });

  final AgentController controller;
  final AgentRegistryView registry;
  final AgentInstallation installation;
  final AgentExpertBindingTarget? focus;

  @override
  Widget build(BuildContext context) {
    final assignments = registry.assignments
        .where((entry) => entry.installationId == installation.id)
        .toList();
    final definition = registry.definitions.singleWhere(
      (entry) =>
          entry.packageId == installation.packageId &&
          entry.version == installation.version,
    );
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
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          FloeSwitch(
            key: ValueKey('capability-${installation.id}'),
            value: enabled,
            onChanged: assignments.isNotEmpty && controller.canManageRegistry
                ? (value) =>
                      controller.configureCapability(installation.id, value)
                : null,
            label: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(definition.name, style: FloeType.controlLabel),
                const SizedBox(height: FloeSpace.xxs),
                Text(
                  definition.description,
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                    fontSize: 12,
                    height: 1.4,
                  ),
                ),
              ],
            ),
          ),
          for (final assignment in assignments)
            for (final requirement in assignment.requirements)
              _RequirementPicker(
                controller: controller,
                installation: installation,
                definition: definition,
                assignment: assignment,
                requirement: requirement,
                focused:
                    focus?.assignmentId == assignment.id &&
                    focus?.requirementKey == requirement.key,
              ),
        ],
      ),
    );
  }
}

class _RequirementPicker extends StatefulWidget {
  const _RequirementPicker({
    required this.controller,
    required this.installation,
    required this.definition,
    required this.assignment,
    required this.requirement,
    required this.focused,
  });

  final AgentController controller;
  final AgentInstallation installation;
  final AgentExpertDefinition definition;
  final AgentAssignment assignment;
  final AgentSourceRequirement requirement;
  final bool focused;

  @override
  State<_RequirementPicker> createState() => _RequirementPickerState();
}

class _RequirementPickerState extends State<_RequirementPicker> {
  bool expanded = false;
  int? loadedRevision;
  Set<String> draft = {};

  @override
  void initState() {
    super.initState();
    if (widget.focused) {
      expanded = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _load();
      });
    }
  }

  void _load() {
    widget.controller.loadExpertCandidates(
      widget.assignment.id,
      widget.requirement.key,
    );
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final catalog = widget.controller.expertCandidates;
    final current =
        catalog?.assignmentId == widget.assignment.id &&
            catalog?.requirementKey == widget.requirement.key
        ? catalog
        : null;
    if (current != null && loadedRevision != current.bindingRevision) {
      loadedRevision = current.bindingRevision;
      draft = current.candidates
          .where(
            (candidate) =>
                candidate.selected && candidate.availability == 'available',
          )
          .map((candidate) => candidate.id)
          .toSet();
    }
    return ExpansionTile(
      key: ValueKey(
        'requirement-${widget.assignment.id}-${widget.requirement.key}',
      ),
      initiallyExpanded: expanded,
      onExpansionChanged: (value) {
        setState(() => expanded = value);
        if (value) _load();
      },
      title: Text(widget.requirement.key),
      subtitle: Text(
        '${widget.requirement.minimumSources == 0 ? strings.expertSourceOptional : strings.expertSourceRequired} · ${widget.requirement.selectedCount} ${strings.expertSourceSelected}',
      ),
      children: [
        Text(strings.expertSourceSelectionBoundary),
        if (widget.controller.expertCandidateBusy && current == null)
          const CircularProgressIndicator(),
        if (widget.controller.expertCandidateFailure != null)
          Text(strings.agentRegistryFailure),
        if (current != null && current.candidates.isEmpty)
          Text(strings.expertSourceNoCompatible),
        if (current != null)
          for (final candidate in current.candidates)
            CheckboxListTile(
              value:
                  draft.contains(candidate.id) ||
                  (candidate.availability == 'unavailable' &&
                      candidate.selected),
              onChanged:
                  candidate.availability == 'available' &&
                      widget.controller.canManageRegistry &&
                      !widget.controller.expertCandidateBusy
                  ? (value) => setState(() {
                      if (value == true) {
                        if (widget.requirement.maximumSources == 1) {
                          draft.clear();
                        }
                        if (draft.length < widget.requirement.maximumSources) {
                          draft.add(candidate.id);
                        }
                      } else {
                        draft.remove(candidate.id);
                      }
                    })
                  : null,
              title: Text(candidate.title),
              subtitle: Text(
                candidate.availability == 'unavailable'
                    ? strings.expertSourceUnavailable
                    : candidate.detail,
              ),
            ),
        if (current != null)
          Row(
            children: [
              FloeButton.text(
                onPressed:
                    widget.controller.canManageRegistry &&
                        !widget.controller.expertCandidateBusy
                    ? () => widget.controller.replaceExpertSelection(
                        widget.installation,
                        widget.definition,
                        widget.assignment,
                        widget.requirement,
                        draft.toList()..sort(),
                      )
                    : null,
                child: Text(strings.expertSourceSave),
              ),
              FloeButton.text(
                onPressed:
                    widget.controller.canManageRegistry &&
                        !widget.controller.expertCandidateBusy
                    ? () => widget.controller.replaceExpertSelection(
                        widget.installation,
                        widget.definition,
                        widget.assignment,
                        widget.requirement,
                        const [],
                      )
                    : null,
                child: Text(strings.expertSourceRemove),
              ),
            ],
          ),
      ],
    );
  }
}
