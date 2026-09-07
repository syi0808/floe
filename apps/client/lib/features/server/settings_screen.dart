import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_loading.dart';
import '../../app/floe_selection.dart';
import '../../app/floe_squircle.dart';
import '../agent/agent_calendar_sources.dart';
import '../agent/agent_calendar_expert_dialog.dart';
import '../agent/agent_controller.dart';
import '../agent/agent_registry_dialog.dart';
import '../agent/agent_vault_gateway.dart';
import '../day_canvas/application/calendar_action_controller.dart';
import '../day_canvas/domain/calendar_action.dart';
import 'local_server_client.dart';
import 'local_server_panel.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({
    super.key,
    required this.client,
    this.actionController,
    this.agentController,
    this.calendarSources,
    this.calendarSourceChanges,
  });

  final LocalServerClient? client;
  final CalendarActionController? actionController;
  final AgentController? agentController;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      const Text(
        'Settings',
        style: TextStyle(
          fontSize: 28,
          fontWeight: FontWeight.w700,
          letterSpacing: -1,
        ),
      ),
      const SizedBox(height: 10),
      const Text(
        'Manage Floe on this device.',
        style: TextStyle(color: FloePalette.neutral600),
      ),
      const SizedBox(height: 36),
      LayoutBuilder(
        builder: (context, constraints) {
          final narrow = constraints.maxWidth < 720;
          final navigation = _SettingsNavigation(horizontal: narrow);
          final content = Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (actionController case final controller?) ...[
                _ActionPermissions(controller: controller),
                const SizedBox(height: FloeSpace.lg),
              ],
              if (agentController case final controller?) ...[
                _AgentPermissions(
                  controller: controller,
                  calendarSources: calendarSources,
                  calendarSourceChanges: calendarSourceChanges,
                ),
                const SizedBox(height: FloeSpace.lg),
              ],
              if (client case final serverClient?)
                LocalServerPanel(client: serverClient)
              else
                const FloeSquircle(
                  padding: EdgeInsets.all(FloeSpace.lg),
                  child: Text(
                    'Remote server connection is available in the native Floe app.',
                  ),
                ),
              const SizedBox(height: FloeSpace.lg),
              const Padding(
                padding: EdgeInsets.symmetric(horizontal: 4),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Connection boundary',
                      style: TextStyle(fontWeight: FontWeight.w600),
                    ),
                    SizedBox(height: 6),
                    Text(
                      'Pairing authorizes this app to use assisted features on your server. Sensitive context is still approved per request, and service credentials remain on the server.',
                      style: TextStyle(
                        color: FloePalette.neutral600,
                        height: 1.5,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
          if (narrow) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [navigation, const SizedBox(height: 28), content],
            );
          }
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(width: 244, child: navigation),
              const SizedBox(width: 44),
              Expanded(child: content),
            ],
          );
        },
      ),
    ],
  );
}

class _AgentPermissions extends StatefulWidget {
  const _AgentPermissions({
    required this.controller,
    this.calendarSources,
    this.calendarSourceChanges,
  });

  final AgentController controller;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;

  @override
  State<_AgentPermissions> createState() => _AgentPermissionsState();
}

class _AgentPermissionsState extends State<_AgentPermissions> {
  bool loading = false;
  bool registryRequested = false;
  bool calendarRequested = false;

  AgentController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    controller.addListener(_controllerChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void didUpdateWidget(_AgentPermissions oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      oldWidget.controller.removeListener(_controllerChanged);
      controller.addListener(_controllerChanged);
      registryRequested = false;
      calendarRequested = false;
      _load();
    }
  }

  void _controllerChanged() {
    if (!controller.busy) _load();
  }

  Future<void> _load() async {
    if (loading || !controller.canManageRegistry) return;
    loading = true;
    if (!registryRequested) {
      registryRequested = true;
      await controller.loadRegistry();
    }
    if (controller.hasCalendarExpertManagement &&
        !calendarRequested &&
        controller.canManageCalendarExperts) {
      calendarRequested = true;
      await controller.loadCalendarExperts();
    }
    loading = false;
  }

  @override
  void dispose() {
    controller.removeListener(_controllerChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) => FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Floe access',
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: FloeSpace.sm),
          const Text(
            'Choose what Floe can use when helping you. You can change these choices at any time.',
            style: TextStyle(color: FloePalette.neutral600, height: 1.5),
          ),
          const SizedBox(height: FloeSpace.lg),
          AgentRegistrySettings(controller: controller),
          if (controller.hasCalendarExpertManagement) ...[
            const Divider(height: FloeSpace.xxl),
            AgentCalendarSettings(
              controller: controller,
              sources: widget.calendarSources,
              sourceChanges: widget.calendarSourceChanges,
            ),
          ],
          if (controller.vaultState != AgentVaultState.ready) ...[
            const SizedBox(height: FloeSpace.sm),
            const Text(
              'Floe access will appear automatically when your private data is ready.',
              style: TextStyle(color: FloePalette.neutral600, height: 1.4),
            ),
          ],
        ],
      ),
    ),
  );
}

enum _ActionPermissionPreset { all, customize }

class _ActionPermissions extends StatefulWidget {
  const _ActionPermissions({required this.controller});

  final CalendarActionController controller;

  @override
  State<_ActionPermissions> createState() => _ActionPermissionsState();
}

class _ActionPermissionsState extends State<_ActionPermissions> {
  late _ActionPermissionPreset preset;

  CalendarActionController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    preset = controller.authority.calendarCreate == ActionAuthorityMode.allow
        ? _ActionPermissionPreset.all
        : _ActionPermissionPreset.customize;
  }

  @override
  void didUpdateWidget(_ActionPermissions oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      preset = controller.authority.calendarCreate == ActionAuthorityMode.allow
          ? _ActionPermissionPreset.all
          : _ActionPermissionPreset.customize;
    }
  }

  Future<void> selectPreset(_ActionPermissionPreset? nextPreset) async {
    if (nextPreset == null || controller.busy) return;
    setState(() => preset = nextPreset);
    if (nextPreset == _ActionPermissionPreset.all) {
      await controller.setCalendarCreateAuthority(ActionAuthorityMode.allow);
      if (mounted &&
          controller.authority.calendarCreate != ActionAuthorityMode.allow) {
        setState(() => preset = _ActionPermissionPreset.customize);
      }
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) => FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: FloeLoadingOverlay(
        loading: controller.busy,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text(
              'Action permissions',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: FloeSpace.sm),
            const Text(
              'Choose when Floe must ask before changing an external service. OS and connector permissions still apply.',
              style: TextStyle(color: FloePalette.neutral600, height: 1.5),
            ),
            const SizedBox(height: 20),
            RadioGroup<_ActionPermissionPreset>(
              groupValue: preset,
              onChanged: selectPreset,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  FloeRadioTile<_ActionPermissionPreset>(
                    value: _ActionPermissionPreset.all,
                    enabled: !controller.busy,
                    title: const Text('Allow all supported actions'),
                  ),
                  FloeRadioTile<_ActionPermissionPreset>(
                    value: _ActionPermissionPreset.customize,
                    enabled: !controller.busy,
                    title: const Text('Customize permissions'),
                  ),
                ],
              ),
            ),
            const SizedBox(height: FloeSpace.base),
            FloeSelect<ActionAuthorityMode>(
              label: 'Create Calendar events',
              value: controller.authority.calendarCreate,
              enabled:
                  preset == _ActionPermissionPreset.customize &&
                  !controller.busy,
              options: const [
                FloeSelectOption(
                  value: ActionAuthorityMode.allow,
                  label: 'Allow automatically',
                ),
                FloeSelectOption(
                  value: ActionAuthorityMode.ask,
                  label: 'Ask every time',
                ),
                FloeSelectOption(
                  value: ActionAuthorityMode.deny,
                  label: 'Do not allow',
                ),
              ],
              onChanged: (value) {
                if (value != null) {
                  controller.setCalendarCreateAuthority(value);
                }
              },
            ),
            const SizedBox(height: FloeSpace.sm),
            const Text(
              'Currently this preset covers Calendar event creation only. It never bypasses macOS permission or safety checks.',
              style: TextStyle(color: FloePalette.neutral600, height: 1.4),
            ),
            const SizedBox(height: FloeSpace.md),
            Text(
              controller.writesEnabled
                  ? 'Calendar writing is available in this build.'
                  : 'Calendar writing is unavailable in this build.',
              style: const TextStyle(color: FloePalette.neutral600),
            ),
            if (controller.failed) ...[
              const SizedBox(height: FloeSpace.sm),
              const Text('The permission could not be saved. Try again.'),
            ],
          ],
        ),
      ),
    ),
  );
}

class _SettingsNavigation extends StatelessWidget {
  const _SettingsNavigation({required this.horizontal});
  final bool horizontal;

  @override
  Widget build(BuildContext context) {
    const sections = [
      _SettingsSection(
        icon: LucideIcons.slidersHorizontal,
        label: 'Action permissions',
        selected: true,
      ),
      _SettingsSection(icon: LucideIcons.server, label: 'Remote server'),
    ];
    return horizontal
        ? const SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(children: sections),
          )
        : const Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: sections,
          );
  }
}

class _SettingsSection extends StatelessWidget {
  const _SettingsSection({
    required this.icon,
    required this.label,
    this.selected = false,
  });
  final IconData icon;
  final String label;
  final bool selected;

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: selected ? FloePalette.primary100 : Colors.transparent,
      borderWidth: 0,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              icon,
              size: 18,
              color: selected ? FloePalette.primary700 : FloePalette.neutral600,
            ),
            const SizedBox(width: 10),
            Flexible(
              child: Text(
                label,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                  color: selected
                      ? FloePalette.primary700
                      : FloePalette.neutral600,
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
