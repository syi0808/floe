import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_primitives.dart';
import '../../app/floe_loading.dart';
import '../../app/floe_selection.dart';
import '../../app/floe_squircle.dart';
import '../agent/agent_calendar_sources.dart';
import '../agent/agent_calendar_expert_dialog.dart';
import '../agent/agent_controller.dart';
import '../agent/agent_vault_gateway.dart';
import '../day_canvas/application/calendar_action_controller.dart';
import '../day_canvas/domain/calendar_action.dart';
import 'local_server_client.dart';
import 'local_server_panel.dart';

enum _SettingsPage { actions, dataPrivacy, remoteServer }

class SettingsScreen extends StatefulWidget {
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
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  late _SettingsPage selectedPage = _availablePages.first;

  List<_SettingsPage> get _availablePages => [
    if (widget.actionController != null) _SettingsPage.actions,
    if (widget.agentController != null) _SettingsPage.dataPrivacy,
    _SettingsPage.remoteServer,
  ];

  @override
  void didUpdateWidget(SettingsScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_availablePages.contains(selectedPage)) {
      selectedPage = _availablePages.first;
    }
  }

  Widget _content() => switch (selectedPage) {
    _SettingsPage.actions => _ActionPermissions(
      controller: widget.actionController!,
    ),
    _SettingsPage.dataPrivacy => _DataPrivacy(
      controller: widget.agentController!,
      serverClient: widget.client,
      calendarSources: widget.calendarSources,
      calendarSourceChanges: widget.calendarSourceChanges,
    ),
    _SettingsPage.remoteServer => _RemoteServerSettings(client: widget.client),
  };

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Text('Settings', style: FloeType.pageTitle),
      const SizedBox(height: 10),
      Text(
        'Manage Floe on this device.',
        style: FloeType.body.copyWith(color: FloePalette.neutral600),
      ),
      const SizedBox(height: 36),
      LayoutBuilder(
        builder: (context, constraints) {
          final narrow = constraints.maxWidth < 720;
          final navigation = _SettingsNavigation(
            horizontal: narrow,
            pages: _availablePages,
            selected: selectedPage,
            onSelected: (page) => setState(() => selectedPage = page),
          );
          final content = AnimatedSwitcher(
            duration: const Duration(milliseconds: 180),
            child: KeyedSubtree(key: ValueKey(selectedPage), child: _content()),
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

class _RemoteServerSettings extends StatelessWidget {
  const _RemoteServerSettings({required this.client});

  final LocalServerClient? client;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
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
      Padding(
        padding: EdgeInsets.symmetric(horizontal: 4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Connection boundary', style: FloeType.controlLabel),
            SizedBox(height: 6),
            Text(
              'Pairing authorizes this app to use assisted features on your server. Sensitive context is still approved per request, and service credentials remain on the server.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
          ],
        ),
      ),
    ],
  );
}

class _DataPrivacy extends StatefulWidget {
  const _DataPrivacy({
    required this.controller,
    required this.serverClient,
    this.calendarSources,
    this.calendarSourceChanges,
  });

  final AgentController controller;
  final LocalServerClient? serverClient;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;

  @override
  State<_DataPrivacy> createState() => _DataPrivacyState();
}

class _DataPrivacyState extends State<_DataPrivacy> {
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
  void didUpdateWidget(_DataPrivacy oldWidget) {
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
    builder: (context, _) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Data & privacy',
          style: FloeType.headlineLarge.copyWith(fontSize: 22),
        ),
        const SizedBox(height: FloeSpace.sm),
        Text(
          'Control what Floe may use and where assisted processing may happen.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.lg),
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: controller.hasCalendarExpertManagement
              ? AgentCalendarSettings(
                  controller: controller,
                  sources: widget.calendarSources,
                  sourceChanges: widget.calendarSourceChanges,
                )
              : const Text('No connected data sources are available yet.'),
        ),
        const SizedBox(height: FloeSpace.lg),
        _AiProcessing(client: widget.serverClient),
        if (controller.vaultState != AgentVaultState.ready) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(
            'Data access will appear when your private data is unlocked.',
            style: FloeType.body.copyWith(
              color: FloePalette.neutral600,
              height: 1.4,
            ),
          ),
        ],
      ],
    ),
  );
}

class _AiProcessing extends StatefulWidget {
  const _AiProcessing({required this.client});
  final LocalServerClient? client;

  @override
  State<_AiProcessing> createState() => _AiProcessingState();
}

class _AiProcessingState extends State<_AiProcessing> {
  ServerConnection? connection;
  Map<InferencePurpose, InferencePurposeAvailability>? purposes;
  List<InferenceAuditRecord>? activity;
  bool loading = true;
  String? failure;

  Set<String> get _externalRecipients =>
      purposes?.values
          .map((purpose) => purpose.recipient)
          .whereType<String>()
          .toSet() ??
      const {};

  bool get _externalConsentActive {
    final saved = connection;
    final recipients = _externalRecipients;
    return saved != null &&
        recipients.isNotEmpty &&
        recipients.every(saved.coversExternalRecipient);
  }

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final saved = await widget.client?.connection();
      if (!mounted) return;
      Map<InferencePurpose, InferencePurposeAvailability>? availability;
      List<InferenceAuditRecord>? recentActivity;
      String? routeFailure;
      if (saved != null && widget.client != null) {
        try {
          availability = await widget.client!.purposes(saved);
        } on Object {
          routeFailure = 'Paired, but route availability could not be checked.';
        }
        try {
          recentActivity = await widget.client!.privacyActivity(saved);
        } on Object {
          routeFailure ??=
              'Paired, but recent processing activity is unavailable.';
        }
      }
      if (!mounted) return;
      setState(() {
        connection = saved;
        purposes = availability;
        activity = recentActivity;
        loading = false;
        failure = routeFailure;
      });
    } on Object {
      if (mounted) {
        setState(() {
          loading = false;
          failure = 'Processing settings could not be loaded.';
        });
      }
    }
  }

  Future<void> _setExternal(bool enabled) async {
    final current = connection;
    final client = widget.client;
    if (current == null || client == null) return;
    setState(() => loading = true);
    try {
      final updated = current.withExternalConsent(
        enabled,
        recipients: _externalRecipients,
      );
      await client.save(updated);
      if (mounted) {
        setState(() {
          connection = updated;
          failure = null;
        });
      }
    } on Object {
      if (mounted) {
        setState(
          () => failure = 'External processing consent could not be saved.',
        );
      }
    } finally {
      if (mounted) setState(() => loading = false);
    }
  }

  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text('AI processing', style: FloeType.title),
        const SizedBox(height: FloeSpace.xs),
        Text(
          'Floe chooses a permitted route for each task. Conversations do not select a model.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.base),
        const _ProcessingRow(
          title: 'On this device',
          detail: 'Local data preparation and available local intelligence',
          status: 'Preferred',
        ),
        const SizedBox(height: FloeSpace.sm),
        _ProcessingRow(
          title: 'On your Floe Server',
          detail: connection == null
              ? 'Pair a server in Floe Server settings to add assisted routes.'
              : 'Paired server may process only the context required for a task.',
          status: connection == null ? 'Not paired' : 'Paired',
        ),
        if (connection != null && purposes != null) ...[
          const SizedBox(height: FloeSpace.sm),
          for (final purpose in InferencePurpose.values)
            Padding(
              padding: const EdgeInsets.only(top: FloeSpace.xs),
              child: _ProcessingRow(
                title: switch (purpose) {
                  InferencePurpose.quickResponse => 'Quick responses',
                  InferencePurpose.everydayAssistance => 'Everyday assistance',
                  InferencePurpose.deepWork => 'Deep work',
                },
                detail: purposes![purpose]!.recipient != null
                    ? 'External recipient: ${purposes![purpose]!.recipient}. Selected automatically when needed.'
                    : 'Processed on your Floe Server when selected automatically.',
                status: !purposes![purpose]!.available
                    ? 'Unavailable'
                    : purposes![purpose]!.requiresExternalConsent &&
                          !connection!.coversExternalRecipient(
                            purposes![purpose]!.recipient,
                          )
                    ? 'Needs consent'
                    : 'Available',
              ),
            ),
        ],
        if (connection != null) ...[
          const FloeDivider(height: FloeSpace.xl),
          FloeSwitchTile(
            key: const ValueKey('external-model-consent'),
            title: 'Allow external model providers',
            subtitle: 'Allows the paired server to send the minimum required context to a provider it manages. Turn this off to withdraw consent without disconnecting the server.',
            value: _externalConsentActive,
            onChanged: loading || _externalRecipients.isEmpty
                ? null
                : _setExternal,
          ),
        ],
        if (failure case final message?) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(
            message,
            style: FloeType.body.copyWith(color: FloePalette.error600),
          ),
        ],
        if (connection != null && activity != null) ...[
          const FloeDivider(height: FloeSpace.xl),
          const Text('Recent data use', style: FloeType.controlLabel),
          const SizedBox(height: FloeSpace.xs),
          if (activity!.isEmpty)
            Text(
              'No server model processing has been recorded since the server started.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            )
          else
            for (final record in activity!.take(5))
              Padding(
                padding: const EdgeInsets.only(top: FloeSpace.sm),
                child: _ProcessingRow(
                  title: switch (record.purpose) {
                    'quick_response' => 'Quick response',
                    'everyday_assistance' => 'Everyday assistance',
                    'deep_work' => 'Deep work',
                    _ => 'Assisted processing',
                  },
                  detail:
                      '${record.dataClasses.join(', ')} · ${record.placement == 'remote' ? 'External provider' : 'Floe Server'} · Trace ${record.traceId.substring(0, 8)}',
                  status: record.outcome == 'completed'
                      ? 'Completed'
                      : 'Failed',
                ),
              ),
        ],
      ],
    ),
  );
}

class _ProcessingRow extends StatelessWidget {
  const _ProcessingRow({
    required this.title,
    required this.detail,
    required this.status,
  });
  final String title;
  final String detail;
  final String status;

  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: FloeType.controlLabel),
            const SizedBox(height: FloeSpace.xxs),
            Text(
              detail,
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
          ],
        ),
      ),
      const SizedBox(width: FloeSpace.md),
      FloeBadge(label: status, tone: _statusTone(status)),
    ],
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
            const Text('Action permissions', style: FloeType.titleLarge),
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Choose when Floe must ask before changing an external service. OS and connector permissions still apply.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
            const SizedBox(height: 20),
            FloeRadioGroup<_ActionPermissionPreset>(
              value: preset,
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
            Text(
              'Currently this preset covers Calendar event creation only. It never bypasses macOS permission or safety checks.',
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
            const SizedBox(height: FloeSpace.md),
            Text(
              controller.writesEnabled
                  ? 'Calendar writing is available in this build.'
                  : 'Calendar writing is unavailable in this build.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
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

FloeBadgeTone _statusTone(String status) => switch (status) {
  'Paired' ||
  'Available' ||
  'Completed' ||
  'Preferred' => FloeBadgeTone.success,
  'Needs consent' || 'Not paired' || 'Unavailable' => FloeBadgeTone.warning,
  'Failed' => FloeBadgeTone.danger,
  _ => FloeBadgeTone.neutral,
};

class _SettingsNavigation extends StatelessWidget {
  const _SettingsNavigation({
    required this.horizontal,
    required this.pages,
    required this.selected,
    required this.onSelected,
  });

  final bool horizontal;
  final List<_SettingsPage> pages;
  final _SettingsPage selected;
  final ValueChanged<_SettingsPage> onSelected;

  @override
  Widget build(BuildContext context) {
    final sections = [
      for (final page in pages)
        _SettingsSection(
          key: ValueKey('settings-${page.name}'),
          icon: switch (page) {
            _SettingsPage.actions => LucideIcons.slidersHorizontal,
            _SettingsPage.dataPrivacy => LucideIcons.shieldCheck,
            _SettingsPage.remoteServer => LucideIcons.server,
          },
          label: switch (page) {
            _SettingsPage.actions => 'Action permissions',
            _SettingsPage.dataPrivacy => 'Data & privacy',
            _SettingsPage.remoteServer => 'Remote server',
          },
          selected: page == selected,
          onPressed: () => onSelected(page),
        ),
    ];
    return horizontal
        ? SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(children: sections),
          )
        : Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: sections,
          );
  }
}

class _SettingsSection extends StatelessWidget {
  const _SettingsSection({
    required this.icon,
    required this.label,
    required this.onPressed,
    this.selected = false,
    super.key,
  });
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: selected ? FloePalette.primary100 : Colors.transparent,
      borderWidth: 0,
      child: FloePressable(
        size: FloeSquircleSize.md,
        onPressed: onPressed,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                icon,
                size: 18,
                color: selected
                    ? FloePalette.primary700
                    : FloePalette.neutral600,
              ),
              const SizedBox(width: 10),
              Flexible(
                child: Text(
                  label,
                  overflow: TextOverflow.ellipsis,
                  style: FloeType.body.copyWith(
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
    ),
  );
}
