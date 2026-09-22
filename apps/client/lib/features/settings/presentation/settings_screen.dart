import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/connections/presentation/agent_connection_settings.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/settings/presentation/agent_memory_settings.dart';
import 'package:floe_client/features/settings/domain/agent_personal_access.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/features/connections/presentation/local_server_panel.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';

part 'data_privacy.dart';
part 'ai_processing.dart';
part 'action_permissions.dart';
part 'navigation.dart';

enum _SettingsPage { actions, dataPrivacy, memory, remoteServer }

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.client,
    this.personalAccessGateway,
    this.pairingGateway,
    this.actionController,
    this.agentController,
    this.androidContext,
    this.appleContext,
    this.daySnapshot,
    this.platform,
  });

  final LocalServerClient? client;
  final AgentPersonalAccessGateway? personalAccessGateway;
  final RemotePairingGateway? pairingGateway;
  final CalendarActionController? actionController;
  final AgentController? agentController;
  final AndroidContextApi? androidContext;
  final AppleContextApi? appleContext;
  final DaySnapshot? daySnapshot;
  final TargetPlatform? platform;

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
      androidContext: widget.androidContext,
      appleContext: widget.appleContext,
      daySnapshot: widget.daySnapshot,
      personalAccessGateway: widget.personalAccessGateway,
      platform: widget.platform,
      onManageMemory: () => setState(() => selectedPage = _SettingsPage.memory),
    ),
    _SettingsPage.memory => AgentMemorySettings(
      controller: widget.agentController!,
      onBack: () => setState(() => selectedPage = _SettingsPage.dataPrivacy),
    ),
    _SettingsPage.remoteServer => _RemoteServerSettings(
      client: widget.client,
      personalAccessGateway: widget.personalAccessGateway,
      pairingGateway: widget.pairingGateway,
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

class _RemoteServerSettings extends StatelessWidget {
  const _RemoteServerSettings({
    required this.client,
    required this.personalAccessGateway,
    required this.pairingGateway,
  });

  final LocalServerClient? client;
  final AgentPersonalAccessGateway? personalAccessGateway;
  final RemotePairingGateway? pairingGateway;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (client case final serverClient?)
        LocalServerPanel(client: serverClient, pairingGateway: pairingGateway)
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
