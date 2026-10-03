import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';

final class GatewayConnectionPanel extends StatefulWidget {
  const GatewayConnectionPanel({super.key, required this.controller});
  final ConnectionsController controller;
  @override
  State<GatewayConnectionPanel> createState() => _GatewayConnectionPanelState();
}

final class _GatewayConnectionPanelState extends State<GatewayConnectionPanel> {
  final address = TextEditingController(text: 'http://127.0.0.1:8431');
  @override
  void dispose() { address.dispose(); super.dispose(); }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final pairing = controller.pairing;
    final setup = controller.setup;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const Text('Gateway', style: FloeType.title),
      const SizedBox(height: FloeSpace.sm),
      const Text('Connect a local Floe Gateway. Compare the code before confirming on both devices.'),
      const SizedBox(height: FloeSpace.base),
      TextField(controller: address, enabled: !controller.busy, decoration: const InputDecoration(labelText: 'Local Gateway address')),
      const SizedBox(height: FloeSpace.sm),
      FloeButton.outlined(onPressed: controller.busy ? null : () => controller.prepareGateway(address.text), child: const Text('Prepare connection')),
      if (setup != null) ...[
        const SizedBox(height: FloeSpace.sm),
        Text(setup.displayAddress),
        FloeButton.outlined(onPressed: controller.busy ? null : controller.startPairing, child: const Text('Start pairing')),
      ],
      if (pairing != null) ...[
        const SizedBox(height: FloeSpace.base),
        Text(_pairingLabel(pairing.state)),
        if (pairing.failure case final failure?) Text(failure.reason.replaceAll('_', ' ')),
        if (pairing.displayCode != null) SelectableText(pairing.displayCode!, style: FloeType.headline),
        const SizedBox(height: FloeSpace.sm),
        Wrap(spacing: FloeSpace.sm, children: [
          if (pairing.allowedActions.contains('confirm'))
            FloeButton.outlined(onPressed: controller.busy ? null : controller.confirmPairing, child: const Text('Codes match')),
          if (pairing.allowedActions.contains('cancel'))
            FloeButton.text(onPressed: controller.busy ? null : controller.cancelPairing, child: const Text('Cancel pairing')),
          FloeButton.text(onPressed: controller.observePairing, child: const Text('Check status')),
        ]),
      ],
      for (final gateway in controller.overview?.gateways ?? const <GatewaySummary>[]) ...[
        const SizedBox(height: FloeSpace.base),
        FloeSquircle(padding: const EdgeInsets.all(FloeSpace.base), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(gateway.displayName, style: FloeType.title),
          Text(gateway.state.replaceAll('_', ' ')),
          if (gateway.failure case final failure?) Text(failure.reason.replaceAll('_', ' ')),
          if (gateway.remoteRevocationPending) const Text('Remote revocation still needs to be completed on the Gateway.'),
          Wrap(spacing: FloeSpace.sm, children: [
            if (gateway.allowedActions.contains('manage'))
              FloeButton.outlined(onPressed: controller.busy ? null : () => controller.prepareManagement(gateway), child: const Text('Prepare management page')),
            if (gateway.allowedActions.contains('forget'))
              FloeButton.text(onPressed: controller.busy ? null : () => controller.forgetGateway(gateway), child: const Text('Forget this Gateway')),
          ]),
        ])),
      ],
      if (controller.launchAction case final launch?) ...[
        const SizedBox(height: FloeSpace.sm),
        ManagementLaunchButton(action: launch),
      ],
    ]);
  }
}

final class ManagementLaunchButton extends StatelessWidget {
  const ManagementLaunchButton({super.key, required this.action});
  final LaunchAction action;
  @override
  Widget build(BuildContext context) => FloeButton.outlined(
    onPressed: () async {
      if (!action.expiresAt.isAfter(DateTime.now().toUtc())) {
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('This launch has expired. Prepare a new management action.')));
        return;
      }
      final opened = await launchUrl(action.validatedUrl, mode: LaunchMode.externalApplication);
      if (!opened && context.mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('The management page could not be opened.')));
    },
    child: Text(action.purpose == 'manage_gateway' ? 'Open Gateway management' : 'Open authorization'),
  );
}

String _pairingLabel(String state) => switch (state) {
  'starting' => 'Starting pairing…',
  'awaiting_local_confirmation' => 'Compare the displayed code with your Gateway.',
  'awaiting_gateway_approval' => 'Waiting for confirmation on the Gateway.',
  'verifying' => 'Verifying the Gateway identity…',
  'committing' => 'Saving and checking the connection…',
  'connected' => 'Gateway connected.',
  'rejected' => 'Pairing was rejected.',
  'expired' => 'Pairing expired.',
  'cancelled' => 'Pairing was cancelled.',
  'repair_required' => 'The Gateway connection needs repair.',
  _ => throw const FormatException('Unknown pairing state.'),
};
