import 'dart:async';

import 'package:flutter/foundation.dart';
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
import 'package:floe_client/features/connections/presentation/integration_detail_panel.dart';

final class ConnectorScreen extends StatefulWidget {
  const ConnectorScreen({super.key, required this.controller});
  final ConnectionsController? controller;
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
    controller?.addListener(_changed);
    if (controller != null) unawaited(controller!.load());
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
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
        Row(
          children: [
            const Expanded(
              child: Text('Connections', style: FloeType.headline),
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
          if (current.vault.canRecover)
            FloeButton.outlined(
              onPressed: current.vault.recover,
              child: Text(
                current.vault.gateway.hasPendingOperation
                    ? 'Check the same storage request'
                    : current.vault.failure?.safeActions.contains(
                            'reopen_vault',
                          ) ==
                          true
                    ? 'Reopen local storage'
                    : 'Retry local storage',
              ),
            ),
          const SizedBox(height: FloeSpace.sm),
        ],
        if (current.failure != null) SelectableText(current.failure!),
        if (current.hasUncertainCommand)
          for (final request in current.pendingRequests)
            FloeButton.outlined(
              onPressed: () => current.retryPendingCommand(request.commandId),
              child: Text('Recover ${request.label}'),
            ),
        GatewayConnectionPanel(controller: current),
        if ((current.ready ? current.operation : null)
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
        _connections(context, current),
      ],
    );
  }

  Widget _connections(BuildContext context, ConnectionsController current) {
    final integrations =
        current.overview?.integrations ?? const <IntegrationSummary>[];
    final sources = current.overview?.sources ?? const <SourceSummary>[];
    if (selectedIntegration != null || selectedSource != null) {
      final integration = integrations
          .where((value) => value.integrationRef == selectedIntegration)
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
          if (integration != null)
            IntegrationDetailPanel(
              controller: current,
              integration: integration,
            ),
          if (source != null) ...[
            if (integration != null) const SizedBox(height: FloeSpace.lg),
            _SourceCard(
              key: ValueKey(source.sourceRef.value),
              controller: current,
              source: source,
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
    final strings = AppLocalizations.of(context);
    // Platform affects display copy only; existence, status and actions are
    // taken from the current owner's integration projection.
    final macCalendar =
        integration.category == 'calendar' &&
        integration.displayName == 'Calendar' &&
        defaultTargetPlatform == TargetPlatform.macOS;
    return _ConnectionCard(
      key: ValueKey('integration-${integration.integrationRef.value}'),
      icon: switch (integration.category) {
        'calendar' => LucideIcons.calendarDays,
        'contacts' => LucideIcons.contact,
        'health' => LucideIcons.heartPulse,
        'attention' => LucideIcons.focus,
        _ => LucideIcons.plug,
      },
      name: macCalendar ? strings.macosCalendar : integration.displayName,
      description: macCalendar
          ? strings.calendarsAlreadyOnThisMac
          : switch (integration.category) {
              'calendar' => 'Manage the calendars available to Floe.',
              'contacts' => 'Choose the contacts available to Floe.',
              'health' => 'Use a derived wellbeing summary from Apple Health.',
              'attention' => 'Manage coarse device attention signals.',
              _ => 'Manage this service and its reviewed access.',
            },
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

final class _SourceCard extends StatefulWidget {
  const _SourceCard({
    super.key,
    required this.controller,
    required this.source,
  });
  final ConnectionsController controller;
  final SourceSummary source;
  @override
  State<_SourceCard> createState() => _SourceCardState();
}

final class _SourceCardState extends State<_SourceCard> {
  // This is a requested review choice, never the source's current permission.
  SourceProcessing processing = SourceProcessing.gatewayAllowed;
  SourceReviewRef? selectionReview;
  final selected = <ResourceRef>{};
  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final source = widget.source;
    final review = controller.sourceReview?.sourceRef == source.sourceRef
        ? controller.sourceReview
        : null;
    final observe = controller.observeReview?.sourceRef == source.sourceRef
        ? controller.observeReview
        : null;
    if (review != null && selectionReview?.id != review.reviewRef.id) {
      selectionReview = review.reviewRef;
      selected
        ..clear()
        ..addAll(
          review.permittedChoices
              .where((choice) => choice.selected)
              .map((choice) => choice.resourceRef),
        );
    }
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.base),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(source.displayLabels.join(' · '), style: FloeType.title),
          Text(
            '${source.availability.replaceAll('_', ' ')} · ${source.observeState.replaceAll('_', ' ')}',
          ),
          for (final resource in source.selectedResources) Text(resource.label),
          const SizedBox(height: FloeSpace.sm),
          Wrap(
            spacing: FloeSpace.sm,
            children: [
              if (source.allowedActions.contains('configure'))
                FloeButton.outlined(
                  onPressed: controller.busy
                      ? null
                      : () => controller.prepareSource(source),
                  child: const Text('Choose resources'),
                ),
              if (source.allowedActions.contains('pause_observe'))
                FloeButton.text(
                  onPressed: controller.busy
                      ? null
                      : () => controller.pauseObserve(source),
                  child: const Text('Pause Observe'),
                ),
              if (source.allowedActions.contains('disconnect'))
                FloeButton.text(
                  onPressed: controller.busy
                      ? null
                      : () => controller.disconnectSource(source),
                  child: const Text('Disconnect source'),
                ),
            ],
          ),
          if (source.allowedActions.contains('prepare_observe_review')) ...[
            const SizedBox(height: FloeSpace.base),
            const Text('Use with Floe', style: FloeType.title),
            const SizedBox(height: FloeSpace.sm),
            const Text(
              'Review access to the selected resources and where Floe may process them. Nothing changes until you allow the reviewed access.',
            ),
            const SizedBox(height: FloeSpace.sm),
            const Text('Requested processing', style: FloeType.label),
            DropdownButton<SourceProcessing>(
              value: processing,
              isExpanded: true,
              items: const [
                DropdownMenuItem(
                  value: SourceProcessing.deviceOnly,
                  child: Text('This device only'),
                ),
                DropdownMenuItem(
                  value: SourceProcessing.gatewayAllowed,
                  child: Text('This device and my verified Gateway'),
                ),
              ],
              onChanged: controller.busy
                  ? null
                  : (value) {
                      if (value != null) setState(() => processing = value);
                    },
            ),
            Text(
              processing == SourceProcessing.gatewayAllowed
                  ? 'The request includes source access and permission to process its reviewed data on this device or your verified Gateway.'
                  : 'The request includes source access with processing limited to this device.',
            ),
            const Text(
              'For Health sources, only locally transformed derived data may reach the Gateway. Raw Health data stays on this device.',
            ),
            const SizedBox(height: FloeSpace.sm),
            FloeButton.outlined(
              onPressed: controller.busy
                  ? null
                  : () => controller.prepareObserve(source, processing),
              child: const Text('Review access'),
            ),
          ],
          if (review != null) ...[
            const SizedBox(height: FloeSpace.base),
            for (final view in review.processingDisclosure.views) ...[
              Text(
                '${view.viewId}: ${view.current?.label ?? 'No Observe permission'}.',
              ),
              Text('Sensitivity: ${view.dataClassLabel}.'),
              if (view.dataCategories.isNotEmpty)
                Text(
                  'Currently permitted data: ${view.dataCategories.join(', ')}.',
                ),
            ],
            const Text(
              'Saving source resources creates no Observe permission. Review access again after changing resources.',
            ),
            for (final choice in review.permittedChoices)
              CheckboxListTile(
                title: Text(choice.label),
                value: selected.contains(choice.resourceRef),
                onChanged: controller.busy
                    ? null
                    : (value) => setState(() {
                        if (value == true) {
                          selected.add(choice.resourceRef);
                        } else {
                          selected.remove(choice.resourceRef);
                        }
                      }),
              ),
            if (review.allowedActions.contains('configure'))
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => controller.configureSource(
                        review,
                        selected.toList(growable: false),
                      ),
                child: const Text('Save selected resources'),
              ),
            FloeButton.text(
              onPressed: controller.dismissReview,
              child: const Text('Close review'),
            ),
          ],
          if (observe != null) ...[
            const SizedBox(height: FloeSpace.base),
            for (final member in observe.displayMembers) Text(member),
            for (final view in observe.processingDisclosure.views) ...[
              Text(view.viewId),
              Text('Sensitivity: ${view.dataClassLabel}.'),
              Text('Reviewed data: ${view.dataCategories.join(', ')}.'),
              Text(
                'Current reviewed scope: ${view.current?.label ?? 'No Observe permission'}.',
              ),
              Text('Requested processing: ${view.requested!.label}.'),
              if (view.expandsGateway)
                const Text(
                  'This view gains or expands Gateway processing permission.',
                ),
            ],
            if (observe.processingDisclosure.views.any(
              (view) => view.isDerivedHealth,
            ))
              const Text(
                'This includes highly sensitive data. Only locally transformed derived Health data may reach the Gateway; raw Health data stays on this device.',
              ),
            if (observe.allowedActions.contains('allow'))
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => controller.allowObserve(observe),
                child: const Text('Allow this access'),
              ),
            FloeButton.text(
              onPressed: controller.dismissReview,
              child: const Text('Close review'),
            ),
          ],
        ],
      ),
    );
  }
}
