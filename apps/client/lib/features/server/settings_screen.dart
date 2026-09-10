import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_button.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_primitives.dart';
import '../../app/floe_loading.dart';
import '../../app/floe_selection.dart';
import '../../app/floe_squircle.dart';
import '../agent/agent_calendar_sources.dart';
import '../agent/agent_connection_settings.dart';
import '../agent/agent_calendar_expert_dialog.dart';
import '../agent/agent_controller.dart';
import '../agent/agent_memory_settings.dart';
import '../agent/agent_vault_gateway.dart';
import '../day_canvas/application/calendar_action_controller.dart';
import '../day_canvas/domain/calendar_action.dart';
import 'local_server_client.dart';
import 'local_server_panel.dart';

part 'settings/data_privacy.dart';
part 'settings/ai_processing.dart';
part 'settings/action_permissions.dart';
part 'settings/navigation.dart';

enum _SettingsPage { actions, dataPrivacy, memory, remoteServer }

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
  final navigationScrollController = ScrollController();
  final contentScrollController = ScrollController();

  List<_SettingsPage> get _availablePages => [
    if (widget.actionController != null) _SettingsPage.actions,
    if (widget.agentController != null) _SettingsPage.dataPrivacy,
    _SettingsPage.remoteServer,
  ];

  @override
  void didUpdateWidget(SettingsScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_availablePages.contains(selectedPage) &&
        !(selectedPage == _SettingsPage.memory &&
            widget.agentController != null)) {
      selectedPage = _availablePages.first;
    }
  }

  @override
  void dispose() {
    navigationScrollController.dispose();
    contentScrollController.dispose();
    super.dispose();
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
      onManageMemory: () => setState(() => selectedPage = _SettingsPage.memory),
    ),
    _SettingsPage.memory => AgentMemorySettings(
      controller: widget.agentController!,
      onBack: () => setState(() => selectedPage = _SettingsPage.dataPrivacy),
    ),
    _SettingsPage.remoteServer => _RemoteServerSettings(client: widget.client),
  };

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final body = LayoutBuilder(
        builder: (context, bodyConstraints) => _buildBody(
          bodyConstraints,
          independentlyScrollable: constraints.hasBoundedHeight,
        ),
      );
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Settings', style: FloeType.pageTitle),
          const SizedBox(height: 10),
          Text(
            'Manage Floe on this device.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: 36),
          if (constraints.hasBoundedHeight) Expanded(child: body) else body,
        ],
      );
    },
  );

  Widget _buildBody(
    BoxConstraints constraints, {
    required bool independentlyScrollable,
  }) {
    final narrow = constraints.maxWidth < 720;
    final navigation = _SettingsNavigation(
      horizontal: narrow,
      controller: narrow ? navigationScrollController : null,
      pages: _availablePages,
      selected: selectedPage == _SettingsPage.memory
          ? _SettingsPage.dataPrivacy
          : selectedPage,
      onSelected: (page) => setState(() => selectedPage = page),
    );
    final content = AnimatedSwitcher(
      duration: const Duration(milliseconds: 180),
      child: KeyedSubtree(key: ValueKey(selectedPage), child: _content()),
    );

    if (!independentlyScrollable) {
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
    }

    final contentScrollView = Scrollbar(
      controller: contentScrollController,
      child: SingleChildScrollView(
        key: const ValueKey('settings-content-scroll'),
        controller: contentScrollController,
        primary: false,
        child: content,
      ),
    );
    if (narrow) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          navigation,
          const SizedBox(height: 28),
          Expanded(child: contentScrollView),
        ],
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          width: 244,
          child: Scrollbar(
            controller: navigationScrollController,
            child: SingleChildScrollView(
              key: const ValueKey('settings-navigation-scroll'),
              controller: navigationScrollController,
              primary: false,
              child: navigation,
            ),
          ),
        ),
        const SizedBox(width: 44),
        Expanded(child: contentScrollView),
      ],
    );
  }
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
