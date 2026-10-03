import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/experts/presentation/agent_registry_dialog.dart';
import 'package:floe_client/features/settings/presentation/agent_memory_settings.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

part 'data_privacy.dart';
part 'action_permissions.dart';
part 'navigation.dart';

enum _SettingsPage { actions, dataPrivacy, experts, memory, remoteServer }

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.connectionsController,
    this.actionController,
    this.agentController,
    this.expertBindingTarget,
    this.platform,
  });

  final ConnectionsController? connectionsController;
  final CalendarActionController? actionController;
  final ConversationController? agentController;
  final AgentExpertBindingTarget? expertBindingTarget;
  final TargetPlatform? platform;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  late _SettingsPage selectedPage = widget.expertBindingTarget == null
      ? _availablePages.first
      : _SettingsPage.experts;
  final navigationScrollController = ScrollController();
  final contentScrollController = ScrollController();

  List<_SettingsPage> get _availablePages => [
    if (widget.actionController != null) _SettingsPage.actions,
    if (widget.agentController != null) _SettingsPage.dataPrivacy,
    if (widget.agentController != null) _SettingsPage.experts,
    _SettingsPage.remoteServer,
  ];

  @override
  void didUpdateWidget(SettingsScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.expertBindingTarget != oldWidget.expertBindingTarget &&
        widget.expertBindingTarget != null) {
      selectedPage = _SettingsPage.experts;
    }
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
      platform: widget.platform,
      onManageMemory: () => setState(() => selectedPage = _SettingsPage.memory),
    ),
    _SettingsPage.experts => AgentRegistrySettings(
      controller: widget.agentController!,
      focus: widget.expertBindingTarget,
    ),
    _SettingsPage.memory => AgentMemorySettings(
      controller: widget.agentController!,
      onBack: () => setState(() => selectedPage = _SettingsPage.dataPrivacy),
    ),
    _SettingsPage.remoteServer => ConnectorScreen(
      controller: widget.connectionsController,
    ),
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
