import 'package:floe_client/features/day/domain/day_models.dart';

import 'service_presentation.dart';

import 'package:floe_client/app/runtime/app_owner_exception.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/features/connections/presentation/gateway_connection_panel.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/connections/presentation/integration_detail_panel.dart';
import 'package:floe_client/features/connections/presentation/source_connection_panel.dart';

final class ConnectorScreen extends StatefulWidget {
  const ConnectorScreen({
    super.key,
    required this.controller,
    this.showServices = true,
    this.initialSourceRef,
    this.calendarCoverage,
  });
  final ConnectionsController? controller;
  final bool showServices;
  final SourceRef? initialSourceRef;
  final DayCalendarCoverage? calendarCoverage;
  @override
  State<ConnectorScreen> createState() => _ConnectorScreenState();
}

final class _ConnectorScreenState extends State<ConnectorScreen> {
  IntegrationRef? selectedIntegration;
  SourceRef? selectedSource;
  ConnectionsController? get controller => widget.controller;
  @override
  void initState() {
    super.initState();
    selectedSource = widget.initialSourceRef;
    controller?.addListener(_changed);
    if (controller != null) unawaited(controller!.load());
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.initialSourceRef != widget.initialSourceRef) {
      selectedIntegration = null;
      selectedSource = widget.initialSourceRef;
    }
    if (oldWidget.controller != controller) {
      selectedIntegration = null;
      selectedSource = null;
      oldWidget.controller?.removeListener(_changed);
      controller?.addListener(_changed);
      if (controller != null) unawaited(controller!.load());
    }
  }

  @override
  void dispose() {
    controller?.removeListener(_changed);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final current = controller;
    if (current == null)
      return const Text('Connections are available in the native Floe app.');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.showServices)
          Row(
            children: [
              const Expanded(
                child: Text('Connections', style: FloeType.pageTitle),
              ),
              FloeButton.text(
                onPressed: current.ready ? current.load : null,
                child: const Text('Refresh'),
              ),
            ],
          ),
        const SizedBox(height: FloeSpace.base),
        if (!current.ready) ...[
          Text(current.storageMessage),
          if (current.storageIncidentId case final incident?)
            SelectableText('Incident: $incident'),
          if (current.runtime.canRecover)
            FloeButton.outlined(
              onPressed: current.runtime.recover,
              child: Text(
                current.runtime.hasPendingOperation
                    ? 'Check the same preparation request'
                    : 'Retry Runtime preparation',
              ),
            ),
          const SizedBox(height: FloeSpace.sm),
        ],
        if (current.failure != null) SelectableText(current.failure!),
        if (current.hasUncertainCommand)
          for (final request in current.pendingRequests)
            FloeButton.outlined(
              onPressed: () => current.retryPendingCommand(request.commandId),
              child: Text(
                !widget.showServices && request.label == 'pairing'
                    ? 'Retry pairing result'
                    : 'Recover ${request.label}',
              ),
            ),
        if (!widget.showServices) GatewayConnectionPanel(controller: current),
        if ((widget.showServices && current.ready ? current.operation : null)
            case final operation?) ...[
          const SizedBox(height: FloeSpace.base),
          Text(
            current.operationLabel ?? 'Connection operation',
            style: FloeType.title,
          ),
          Text(operation.state.replaceAll('_', ' ')),
          if (operation.failure case final failure?)
            Text(failure.reason.replaceAll('_', ' ')),
          if (operation.displayCode != null)
            SelectableText(operation.displayCode!),
          if (operation.launchAction case final launch?)
            ManagementLaunchButton(action: launch),
          FloeButton.text(
            onPressed: current.ready ? current.observeOperation : null,
            child: const Text('Check status'),
          ),
          if (operation.allowedActions.contains('cancel'))
            FloeButton.text(
              onPressed: current.busy ? null : current.cancelOperation,
              child: const Text('Cancel operation'),
            ),
        ],
        const SizedBox(height: FloeSpace.lg),
        if (widget.showServices) _connections(context, current),
      ],
    );
  }

  Widget _connections(BuildContext context, ConnectionsController current) {
    final integrations =
        current.overview?.integrations ?? const <IntegrationSummary>[];
    final sources = current.overview?.sources ?? const <SourceSummary>[];
    if (selectedIntegration != null || selectedSource != null) {
      final integration = integrations
          .where(
            (value) => selectedIntegration != null
                ? value.integrationRef == selectedIntegration
                : value.source?.sourceRef == selectedSource,
          )
          .firstOrNull;
      final source =
          integration?.source ??
          sources
              .where((value) => value.sourceRef == selectedSource)
              .firstOrNull;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: FloeTextLink(
              label: AppLocalizations.of(context).backToConnections,
              icon: LucideIcons.arrowLeft,
              onPressed: () => setState(() {
                selectedIntegration = null;
                selectedSource = null;
              }),
            ),
          ),
          const SizedBox(height: FloeSpace.lg),
          if (integration != null && source == null)
            IntegrationDetailPanel(
              controller: current,
              integration: integration,
            ),
          if (source != null) ...[
            SourceConnectionPanel(
              key: ValueKey(source.sourceRef.value),
              controller: current,
              source: source,
              integration: integration,
              calendarCoverage: widget.calendarCoverage,
            ),
          ],
          if (integration == null && source == null)
            Text(
              current.ready
                  ? 'This connection is no longer in the current list. Refresh Connections.'
                  : current.storageMessage,
            ),
        ],
      );
    }
    final strings = AppLocalizations.of(context);
    final linkedSources = {
      for (final integration in integrations)
        if (integration.source case final source?) source.sourceRef,
    };
    final available = <Widget>[
      for (final integration in integrations)
        if (integration.state != 'unavailable')
          _integrationCard(context, integration),
      for (final source in sources)
        if (!linkedSources.contains(source.sourceRef) &&
            source.availability != 'unavailable')
          _sourceCard(source),
    ];
    final unavailable = <Widget>[
      for (final integration in integrations)
        if (integration.state == 'unavailable')
          _integrationCard(context, integration),
      for (final source in sources)
        if (!linkedSources.contains(source.sourceRef) &&
            source.availability == 'unavailable')
          _sourceCard(source),
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          strings.manageTheServicesThatBringContextTo,
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.lg),
        Text(
          strings.connectedServicesCount(
            integrations.where((value) => value.state == 'connected').length +
                sources
                    .where(
                      (value) =>
                          !linkedSources.contains(value.sourceRef) &&
                          value.availability == 'available',
                    )
                    .length,
          ),
          style: FloeType.titleLarge,
        ),
        const SizedBox(height: FloeSpace.lg),
        if (current.overview == null && current.failure == null)
          const Text('Loading connections…')
        else if (available.isEmpty && unavailable.isEmpty)
          const Text('No services are available yet.'),
        if (available.isNotEmpty) ...[
          Text(strings.availableServices, style: FloeType.title),
          const SizedBox(height: FloeSpace.base),
          _ConnectionCardGrid(cards: available),
        ],
        if (unavailable.isNotEmpty) ...[
          const SizedBox(height: FloeSpace.lg),
          Text(strings.unavailableServices, style: FloeType.title),
          const SizedBox(height: FloeSpace.base),
          _ConnectionCardGrid(cards: unavailable),
        ],
      ],
    );
  }

  Widget _sourceCard(SourceSummary source) => _ConnectionCard(
    key: ValueKey('source-${source.sourceRef.value}'),
    icon: LucideIcons.plug,
    name: source.displayLabels.join(' · '),
    description: 'Manage the resources and access reviewed for this source.',
    status: source.availability.replaceAll('_', ' '),
    tone: source.availability == 'available'
        ? FloeBadgeTone.success
        : FloeBadgeTone.neutral,
    onPressed: () => setState(() => selectedSource = source.sourceRef),
  );

  Widget _integrationCard(
    BuildContext context,
    IntegrationSummary integration,
  ) {
    final presentation = ServicePresentation.forIntegration(
      integration,
      AppLocalizations.of(context),
    );
    return _ConnectionCard(
      key: ValueKey('integration-${integration.integrationRef.value}'),
      icon: presentation.icon,
      name: presentation.name,
      description: presentation.description,
      status: switch (integration.state) {
        'connected' => 'Connected',
        'connecting' => 'Connecting',
        'error' => 'Error',
        'available' => 'Available',
        _ => 'Unavailable',
      },
      tone: switch (integration.state) {
        'connected' => FloeBadgeTone.success,
        'connecting' => FloeBadgeTone.warning,
        'error' => FloeBadgeTone.danger,
        'available' => FloeBadgeTone.info,
        _ => FloeBadgeTone.neutral,
      },
      onPressed: () =>
          setState(() => selectedIntegration = integration.integrationRef),
    );
  }
}

final class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({
    super.key,
    required this.icon,
    required this.name,
    required this.description,
    required this.status,
    required this.tone,
    required this.onPressed,
  });
  final IconData icon;
  final String name;
  final String description;
  final String status;
  final FloeBadgeTone tone;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => FloePressable(
    size: FloeSquircleSize.lg,
    fill: FloePalette.neutral0,
    borderColor: FloePalette.neutral200,
    borderWidth: 1,
    hoverFill: FloePalette.primary50,
    onPressed: onPressed,
    child: Padding(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              FloeSquircle(
                size: FloeSquircleSize.md,
                fill: FloePalette.primary50,
                borderWidth: 0,
                padding: const EdgeInsets.all(14),
                child: Icon(icon, size: 26, color: FloePalette.primary600),
              ),
              const SizedBox(width: FloeSpace.sm),
              Flexible(
                child: FloeBadge(label: status, tone: tone, compact: true),
              ),
            ],
          ),
          const SizedBox(height: 20),
          Text(name, style: FloeType.title),
          const SizedBox(height: FloeSpace.sm),
          Text(
            description,
            style: FloeType.bodySmall.copyWith(
              height: 1.6,
              color: FloePalette.neutral600,
            ),
          ),
        ],
      ),
    ),
  );
}

final class _ConnectionCardGrid extends StatelessWidget {
  const _ConnectionCardGrid({required this.cards});
  final List<Widget> cards;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final width = constraints.maxWidth < 620
          ? constraints.maxWidth
          : constraints.maxWidth < 930
          ? (constraints.maxWidth - FloeSpace.lg) / 2
          : (constraints.maxWidth - FloeSpace.lg * 2) / 3;
      return Wrap(
        spacing: FloeSpace.lg,
        runSpacing: FloeSpace.lg,
        children: [
          for (final card in cards) SizedBox(width: width, child: card),
        ],
      );
    },
  );
}
