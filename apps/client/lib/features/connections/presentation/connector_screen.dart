import 'dart:async';
import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
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
  ConnectionsController? get controller => widget.controller;
  @override
  void initState() {
    super.initState();
    controller?.addListener(_changed);
    if (controller != null) unawaited(controller!.load());
  }
  void _changed() { if (mounted) setState(() {}); }
  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
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
    if (current == null) return const Text('Connections are available in the native Floe app.');
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Row(children: [
        const Expanded(child: Text('Connections', style: FloeType.headline)),
        FloeButton.text(onPressed: current.load, child: const Text('Refresh')),
      ]),
      const SizedBox(height: FloeSpace.base),
      if (current.failure != null) Text(current.failure!),
      if (current.hasUncertainCommand)
        FloeButton.outlined(onPressed: current.retryPendingCommand, child: const Text('Recover the same request')),
      GatewayConnectionPanel(controller: current),
      if (current.operation case final operation?) ...[
        const SizedBox(height: FloeSpace.base),
        Text(current.operationLabel ?? 'Connection operation', style: FloeType.title),
        Text(operation.state.replaceAll('_', ' ')),
        if (operation.failure case final failure?) Text(failure.reason.replaceAll('_', ' ')),
        if (operation.displayCode != null) SelectableText(operation.displayCode!),
        if (operation.launchAction case final launch?) ManagementLaunchButton(action: launch),
        FloeButton.text(onPressed: current.observeOperation, child: const Text('Check status')),
        if (operation.allowedActions.contains('cancel'))
          FloeButton.text(onPressed: current.busy ? null : current.cancelOperation, child: const Text('Cancel operation')),
      ],
      for (final integration in current.overview?.integrations ?? const <IntegrationSummary>[]) ...[
        const SizedBox(height: FloeSpace.lg),
        FloeSquircle(padding: const EdgeInsets.all(FloeSpace.base), child: IntegrationDetailPanel(controller: current, integration: integration)),
      ],
      for (final source in current.overview?.sources ?? const <SourceSummary>[]) ...[
        const SizedBox(height: FloeSpace.lg),
        _SourceCard(key: ValueKey(source.sourceRef.value), controller: current, source: source),
      ],
    ]);
  }
}

final class _SourceCard extends StatefulWidget {
  const _SourceCard({super.key, required this.controller, required this.source});
  final ConnectionsController controller;
  final SourceSummary source;
  @override
  State<_SourceCard> createState() => _SourceCardState();
}

final class _SourceCardState extends State<_SourceCard> {
  SourceProcessing processing = SourceProcessing.deviceOnly;
  SourceReviewRef? selectionReview;
  final selected = <ResourceRef>{};
  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final source = widget.source;
    final review = controller.sourceReview?.sourceRef == source.sourceRef ? controller.sourceReview : null;
    final observe = controller.observeReview?.sourceRef == source.sourceRef ? controller.observeReview : null;
    if (review != null && selectionReview?.id != review.reviewRef.id) {
      selectionReview = review.reviewRef;
      selected..clear()..addAll(review.permittedChoices.where((choice) => choice.selected).map((choice) => choice.resourceRef));
    }
    return FloeSquircle(padding: const EdgeInsets.all(FloeSpace.base), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(source.displayLabels.join(' · '), style: FloeType.title),
      Text('${source.availability.replaceAll('_', ' ')} · ${source.observeState.replaceAll('_', ' ')}'),
      for (final resource in source.selectedResources) Text(resource.label),
      const SizedBox(height: FloeSpace.sm),
      Wrap(spacing: FloeSpace.sm, children: [
        if (source.allowedActions.contains('configure'))
          FloeButton.outlined(onPressed: controller.busy ? null : () => controller.prepareSource(source), child: const Text('Choose resources')),
        if (source.allowedActions.contains('pause_observe'))
          FloeButton.text(onPressed: controller.busy ? null : () => controller.pauseObserve(source), child: const Text('Pause Observe')),
        if (source.allowedActions.contains('disconnect'))
          FloeButton.text(onPressed: controller.busy ? null : () => controller.disconnectSource(source), child: const Text('Disconnect source')),
      ]),
      if (source.allowedActions.contains('prepare_observe_review')) ...[
        DropdownButton<SourceProcessing>(value: processing, items: const [
          DropdownMenuItem(value: SourceProcessing.deviceOnly, child: Text('Process on this device')),
          DropdownMenuItem(value: SourceProcessing.gatewayAllowed, child: Text('Allow processing on my Gateway')),
        ], onChanged: controller.busy ? null : (value) { if (value != null) setState(() => processing = value); }),
        FloeButton.outlined(onPressed: controller.busy ? null : () => controller.prepareObserve(source, processing), child: const Text('Review Observe access')),
      ],
      if (review != null) ...[
        const SizedBox(height: FloeSpace.base),
        for (final choice in review.permittedChoices)
          CheckboxListTile(title: Text(choice.label), value: selected.contains(choice.resourceRef),
            onChanged: controller.busy ? null : (value) => setState(() { if (value == true) { selected.add(choice.resourceRef); } else { selected.remove(choice.resourceRef); } })),
        if (review.allowedActions.contains('configure'))
          FloeButton.outlined(onPressed: controller.busy ? null : () => controller.configureSource(review, selected.toList(growable: false)), child: const Text('Save selected resources')),
        FloeButton.text(onPressed: controller.dismissReview, child: const Text('Close review')),
      ],
      if (observe != null) ...[
        const SizedBox(height: FloeSpace.base),
        for (final member in observe.displayMembers) Text(member),
        Text(observe.processingDisclosure.requested == SourceProcessing.deviceOnly
          ? 'These sources may be processed only on this device.'
          : 'These sources may be processed on this device or your verified Gateway.'),
        for (final label in observe.processingDisclosure.scopeLabels) Text(label),
        if (observe.processingDisclosure.categories.contains('highly_sensitive'))
          const Text('This includes highly sensitive data. Health is reduced privately on this device before use.'),
        if (observe.allowedActions.contains('allow'))
          FloeButton.outlined(onPressed: controller.busy ? null : () => controller.allowObserve(observe), child: const Text('Allow this access')),
        FloeButton.text(onPressed: controller.dismissReview, child: const Text('Close review')),
      ],
    ]));
  }
}
