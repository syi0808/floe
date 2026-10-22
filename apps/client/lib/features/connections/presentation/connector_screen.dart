import 'package:floe_client/features/day/domain/day_models.dart';

import 'service_presentation.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
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
  final Map<String, FocusNode> _cardFocusNodes = {};
  _ConnectionNavigationReturn? _connectionReturn;
  int _navigationGeneration = 0;
  ConnectionsController? get controller => widget.controller;
  @override
  void initState() {
    super.initState();
    selectedSource = widget.initialSourceRef;
    controller?.addListener(_changed);
    if (controller != null) unawaited(controller!.load());
  }

  void _changed() {
    if (!mounted) return;
    _pruneCardFocusNodes();
    setState(() {});
  }

  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.initialSourceRef != widget.initialSourceRef) {
      _invalidateConnectionReturn();
      selectedIntegration = null;
      selectedSource = widget.initialSourceRef;
    }
    if (oldWidget.controller != controller) {
      _invalidateConnectionReturn(disposeFocusNodes: true);
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
    _invalidateConnectionReturn(disposeFocusNodes: true);
    super.dispose();
  }

  FocusNode _cardFocusNode(String identity) => _cardFocusNodes.putIfAbsent(
    identity,
    () => FocusNode(
      debugLabel: identity.startsWith('source:')
          ? 'Connections source card'
          : 'Connections integration card',
    ),
  );

  String _sourceIdentity(SourceRef source) => 'source:${source.value}';

  String _integrationIdentity(IntegrationRef integration) =>
      'integration:${integration.value}';

  String? get _selectedIdentity =>
      switch ((selectedSource, selectedIntegration)) {
        (final source?, _) => _sourceIdentity(source),
        (_, final integration?) => _integrationIdentity(integration),
        _ => null,
      };

  void _openConnectionDetail({
    required String identity,
    SourceRef? source,
    IntegrationRef? integration,
  }) {
    final current = controller;
    final focusNode = _cardFocusNodes[identity];
    if (current == null || focusNode == null) return;

    final scrollable = Scrollable.maybeOf(context, axis: Axis.vertical);
    final position = scrollable?.position;
    final restore = _ConnectionNavigationReturn(
      controller: current,
      identity: identity,
      scrollable: scrollable,
      scrollController: scrollable?.widget.controller,
      offset: position?.pixels,
      focusNode: focusNode,
    );
    final generation = ++_navigationGeneration;
    setState(() {
      _connectionReturn = restore;
      selectedSource = source;
      selectedIntegration = integration;
    });

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_isCurrentDetailEntry(generation, current, identity) ||
          position == null ||
          !position.hasContentDimensions ||
          !identical(
            Scrollable.maybeOf(context, axis: Axis.vertical)?.position,
            position,
          )) {
        return;
      }
      if (position.pixels != 0) position.jumpTo(0);
    });
  }

  bool _isCurrentDetailEntry(
    int generation,
    ConnectionsController expectedController,
    String expectedIdentity,
  ) =>
      mounted &&
      generation == _navigationGeneration &&
      controller == expectedController &&
      _selectedIdentity == expectedIdentity &&
      _connectionReturn?.identity == expectedIdentity &&
      _connectionReturn?.controller == expectedController &&
      _hasListCard(expectedIdentity);

  void _backToConnections() {
    final restore = _connectionReturn;
    final selectedIdentity = _selectedIdentity;
    final current = controller;
    final generation = ++_navigationGeneration;
    setState(() {
      selectedIntegration = null;
      selectedSource = null;
      _connectionReturn = null;
    });

    if (restore == null ||
        current != restore.controller ||
        selectedIdentity != restore.identity) {
      return;
    }

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          generation != _navigationGeneration ||
          controller != restore.controller ||
          selectedSource != null ||
          selectedIntegration != null ||
          !_hasListCard(restore.identity)) {
        return;
      }

      final currentScrollable = Scrollable.maybeOf(
        context,
        axis: Axis.vertical,
      );
      if (!identical(currentScrollable, restore.scrollable) ||
          currentScrollable?.widget.controller != restore.scrollController) {
        return;
      }
      final position = currentScrollable?.position;
      if (position != null && !position.hasContentDimensions) return;
      if (restore.offset != null && position == null) return;

      if (position != null && restore.offset != null) {
        position.jumpTo(
          restore.offset!.clamp(0, position.maxScrollExtent).toDouble(),
        );
      }

      final focusNode = _cardFocusNodes[restore.identity];
      if (focusNode == restore.focusNode &&
          focusNode?.context != null &&
          focusNode!.canRequestFocus) {
        if (position != null) _revealCardIfNeeded(position, focusNode);
        focusNode.requestFocus();
      }
    });
  }

  void _revealCardIfNeeded(ScrollPosition position, FocusNode focusNode) {
    final target = focusNode.context?.findRenderObject();
    if (target == null || !target.attached) return;
    final viewport = RenderAbstractViewport.maybeOf(target);
    if (viewport == null) {
      return;
    }

    final start = viewport
        .getOffsetToReveal(target, 0, axis: Axis.vertical)
        .offset;
    final end = viewport
        .getOffsetToReveal(target, 1, axis: Axis.vertical)
        .offset;
    final lowerVisibleOffset = start < end ? start : end;
    final upperVisibleOffset = start > end ? start : end;
    final currentOffset = position.pixels;
    final desiredOffset = currentOffset < lowerVisibleOffset
        ? lowerVisibleOffset
        : currentOffset > upperVisibleOffset
        ? upperVisibleOffset
        : null;
    if (desiredOffset == null) return;

    final clampedOffset = desiredOffset
        .clamp(0, position.maxScrollExtent)
        .toDouble();
    if (clampedOffset != currentOffset) position.jumpTo(clampedOffset);
  }

  bool _hasListCard(String identity) {
    final overview = controller?.overview;
    if (overview == null) return false;
    if (identity.startsWith('integration:')) {
      final ref = identity.substring('integration:'.length);
      return overview.integrations.any(
        (value) => value.integrationRef.value == ref,
      );
    }
    if (!identity.startsWith('source:')) return false;
    final ref = identity.substring('source:'.length);
    final linkedSources = {
      for (final integration in overview.integrations)
        if (integration.source case final source?) source.sourceRef.value,
    };
    return !linkedSources.contains(ref) &&
        overview.sources.any((value) => value.sourceRef.value == ref);
  }

  void _pruneCardFocusNodes() {
    final overview = controller?.overview;
    if (overview == null) {
      if (controller?.ready == false) {
        _invalidateConnectionReturn(disposeFocusNodes: true);
      }
      return;
    }
    final linkedSources = {
      for (final integration in overview.integrations)
        if (integration.source case final source?) source.sourceRef.value,
    };
    final identities = <String>{
      for (final integration in overview.integrations)
        _integrationIdentity(integration.integrationRef),
      for (final source in overview.sources)
        if (!linkedSources.contains(source.sourceRef.value))
          _sourceIdentity(source.sourceRef),
    };
    for (final identity in _cardFocusNodes.keys.toList()) {
      if (identities.contains(identity)) continue;
      final node = _cardFocusNodes.remove(identity)!;
      if (identical(_connectionReturn?.focusNode, node)) {
        _invalidateConnectionReturn();
      }
      node.dispose();
    }
    final restore = _connectionReturn;
    if (restore != null &&
        (restore.controller != controller ||
            !identities.contains(restore.identity))) {
      _invalidateConnectionReturn();
    }
  }

  void _invalidateConnectionReturn({bool disposeFocusNodes = false}) {
    _navigationGeneration++;
    _connectionReturn = null;
    if (!disposeFocusNodes) return;
    for (final node in _cardFocusNodes.values) {
      node.dispose();
    }
    _cardFocusNodes.clear();
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
              onPressed: _backToConnections,
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

  Widget _sourceCard(SourceSummary source) {
    final identity = _sourceIdentity(source.sourceRef);
    return _ConnectionCard(
      key: ValueKey(identity),
      focusNode: _cardFocusNode(identity),
      icon: LucideIcons.plug,
      name: source.displayLabels.join(' · '),
      status: ServicePresentation.sourceAvailabilityLabel(source.availability),
      tone: source.availability == 'available'
          ? FloeBadgeTone.success
          : FloeBadgeTone.neutral,
      onPressed: () =>
          _openConnectionDetail(identity: identity, source: source.sourceRef),
    );
  }

  Widget _integrationCard(
    BuildContext context,
    IntegrationSummary integration,
  ) {
    final presentation = ServicePresentation.forIntegration(
      integration,
      AppLocalizations.of(context),
    );
    final identity = _integrationIdentity(integration.integrationRef);
    return _ConnectionCard(
      key: ValueKey(identity),
      focusNode: _cardFocusNode(identity),
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
      onPressed: () => _openConnectionDetail(
        identity: identity,
        integration: integration.integrationRef,
      ),
    );
  }
}

final class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({
    super.key,
    required this.icon,
    required this.name,
    required this.focusNode,
    this.description,
    required this.status,
    required this.tone,
    required this.onPressed,
  });
  final IconData icon;
  final String name;
  final FocusNode focusNode;
  final String? description;
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
    focusNode: focusNode,
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
          if (description case final value?) ...[
            const SizedBox(height: FloeSpace.sm),
            Text(
              value,
              style: FloeType.bodySmall.copyWith(
                height: 1.6,
                color: FloePalette.neutral600,
              ),
            ),
          ],
        ],
      ),
    ),
  );
}

final class _ConnectionNavigationReturn {
  const _ConnectionNavigationReturn({
    required this.controller,
    required this.identity,
    required this.scrollable,
    required this.scrollController,
    required this.offset,
    required this.focusNode,
  });

  final ConnectionsController controller;
  final String identity;
  final ScrollableState? scrollable;
  final ScrollController? scrollController;
  final double? offset;
  final FocusNode focusNode;
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
