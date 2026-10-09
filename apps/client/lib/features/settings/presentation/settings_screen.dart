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
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/settings/presentation/assistant_feature_settings.dart';
import 'package:floe_client/features/settings/presentation/agent_memory_settings.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

part 'data_privacy.dart';
part 'operation_policy_settings.dart';
part 'navigation.dart';

enum _SettingsPage {
  operationPolicy,
  dataPrivacy,
  assistantFeatures,
  memory,
  remoteServer,
}

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.connectionsController,
    this.operationPolicyController,
    this.runtime,
    this.assistantFeatureController,
    this.memoryController,
    this.assistantFeatureSourceTarget,
    this.onAssistantFeatureConfigured,
    this.platform,
  });

  final ConnectionsController? connectionsController;
  final OperationPolicyController? operationPolicyController;
  final RuntimeController? runtime;
  final AssistantFeatureController? assistantFeatureController;
  final AgentMemoryController? memoryController;
  final AgentAssistantFeatureSourceTarget? assistantFeatureSourceTarget;
  final Future<void> Function()? onAssistantFeatureConfigured;
  final TargetPlatform? platform;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  late _SettingsPage selectedPage = widget.assistantFeatureSourceTarget == null
      ? _availablePages.first
      : _SettingsPage.assistantFeatures;
  final navigationScrollController = ScrollController();
  final contentScrollController = ScrollController();

  List<_SettingsPage> get _availablePages => [
    if (widget.operationPolicyController != null) _SettingsPage.operationPolicy,
    if (widget.memoryController != null && widget.runtime != null)
      _SettingsPage.dataPrivacy,
    if (widget.assistantFeatureController != null && widget.runtime != null)
      _SettingsPage.assistantFeatures,
    _SettingsPage.remoteServer,
  ];

  @override
  void didUpdateWidget(SettingsScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.assistantFeatureSourceTarget !=
            oldWidget.assistantFeatureSourceTarget &&
        widget.assistantFeatureSourceTarget != null) {
      selectedPage = _SettingsPage.assistantFeatures;
    }
    if (!_availablePages.contains(selectedPage) &&
        !(selectedPage == _SettingsPage.memory &&
            widget.memoryController != null)) {
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
    _SettingsPage.operationPolicy => _OperationPolicySettings(
      controller: widget.operationPolicyController!,
    ),
    _SettingsPage.dataPrivacy => _DataPrivacy(
      runtime: widget.runtime!,
      controller: widget.memoryController!,
      platform: widget.platform,
      onManageMemory: () => setState(() => selectedPage = _SettingsPage.memory),
    ),
    _SettingsPage.assistantFeatures => AssistantFeatureSettings(
      controller: widget.assistantFeatureController!,
      runtime: widget.runtime!,
      focus: widget.assistantFeatureSourceTarget,
      onConfigured: widget.onAssistantFeatureConfigured,
    ),
    _SettingsPage.memory => AgentMemorySettings(
      controller: widget.memoryController!,
      runtime: widget.runtime!,
      onBack: () => setState(() => selectedPage = _SettingsPage.dataPrivacy),
    ),
    _SettingsPage.remoteServer => ConnectorScreen(
      controller: widget.connectionsController,
      showServices: false,
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
