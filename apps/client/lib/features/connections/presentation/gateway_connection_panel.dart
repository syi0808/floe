import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';

final class GatewayConnectionPanel extends StatefulWidget {
  const GatewayConnectionPanel({super.key, required this.controller});
  final ConnectionsController controller;
  @override
  State<GatewayConnectionPanel> createState() => _GatewayConnectionPanelState();
}

final class _GatewayConnectionPanelState extends State<GatewayConnectionPanel> {
  late final address = TextEditingController(
    text: widget.controller.pairingAddress ?? 'http://127.0.0.1:8431',
  );
  @override
  void dispose() {
    address.dispose();
    super.dispose();
  }

  Future<void> _openDashboard(GatewaySummary? connected) async {
    Uri? target;
    if (connected != null) {
      final action = await widget.controller.prepareManagement(connected);
      if (!mounted || action == null) return;
      if (action.purpose != 'manage_gateway' ||
          !action.expiresAt.isAfter(DateTime.now().toUtc())) {
        _notice(
          'The dashboard link is unavailable. Check the connection and retry.',
        );
        return;
      }
      target = action.validatedUrl;
    } else {
      // This is only a user-requested browser link. It cannot admit pairing,
      // establish server identity or carry private authentication material.
      target = _localDashboardUri(address.text);
      if (target == null) {
        _notice('Enter a local HTTP address such as http://127.0.0.1:8431.');
        return;
      }
    }
    try {
      final opened = await launchUrl(
        target,
        mode: LaunchMode.externalApplication,
      );
      if (mounted && !opened) {
        _notice('The server dashboard could not be opened.');
      }
    } on Object {
      if (mounted) _notice('The server dashboard could not be opened.');
    }
  }

  void _notice(String text) {
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final pairing = controller.ready ? controller.pairing : null;
    final gateways = controller.ready
        ? controller.overview?.gateways ?? const <GatewaySummary>[]
        : const <GatewaySummary>[];
    final connected = gateways
        .where((item) => item.state == 'paired')
        .firstOrNull;
    final needsRepair = gateways.any((item) => item.state == 'repair_required');
    final activePairing =
        pairing != null &&
        {
          'starting',
          'awaiting_gateway_approval',
          'cancelling',
        }.contains(pairing.state);
    final approvablePairing =
        pairing != null &&
        {'awaiting_gateway_approval'}.contains(pairing.state);
    bool showGatewayRepair(GatewaySummary gateway) =>
        !activePairing ||
        gateway.gatewayRef.value != pairing.operationRef.value ||
        pairing.failure != null;
    final canPair =
        connected == null &&
        !needsRepair &&
        !activePairing &&
        !controller.hasPendingPairingRequest;
    final status = !controller.ready
        ? controller.storageMessage
        : controller.hasPendingPairingRequest
        ? 'Checking pairing result…'
        : connected != null
        ? 'Device paired with Floe server'
        : activePairing
        ? _pairingLabel(pairing.state)
        : needsRepair
        ? 'Connection needs attention'
        : controller.failure != null
        ? 'Connection could not complete'
        : 'Not connected';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FloeCard(
          child: FloeLoadingOverlay(
            loading: controller.commandBusy,
            label: status,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Remote server connection',
                  style: FloeType.headline,
                ),
                const SizedBox(height: 12),
                const Text(
                  'Connect Floe to your server for assisted features. Service credentials stay on the server; app access is saved in your encrypted local vault.',
                ),
                const SizedBox(height: 20),
                if (connected == null || controller.pairingAddress != null)
                  FloeInput(
                    key: const Key('server-address'),
                    label: 'Server address',
                    controller: address,
                    enabled: !controller.busy && canPair,
                    autocorrect: false,
                    enableSuggestions: false,
                  )
                else
                  const Text('Using the saved server connection.'),
                const SizedBox(height: 16),
                Semantics(
                  liveRegion: true,
                  child: FloeBadge(
                    label: status,
                    tone: connected != null
                        ? FloeBadgeTone.success
                        : activePairing
                        ? FloeBadgeTone.info
                        : needsRepair || controller.failure != null
                        ? FloeBadgeTone.danger
                        : FloeBadgeTone.warning,
                  ),
                ),
                if ((approvablePairing ? pairing.displayCode : null)
                    case final code?) ...[
                  const SizedBox(height: 16),
                  SelectableText(
                    code,
                    style: FloeType.display.copyWith(
                      fontSize: 28,
                      letterSpacing: 4,
                    ),
                  ),
                  const SizedBox(height: 12),
                  const FloeInfoNote(
                    text: 'Approve only when this code matches the dashboard. No calendar data is sent when pairing.',
                  ),
                ],
                if (pairing?.failure case final failure?)
                  Text(failure.reason.replaceAll('_', ' ')),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 10,
                  runSpacing: 10,
                  children: [
                    if (canPair)
                      FloeButton.filled(
                        onPressed: controller.busy
                            ? null
                            : () => controller.pairGateway(address.text),
                        child: const Text('Pair this device'),
                      ),
                    FloeButton.outlined(
                      onPressed: controller.busy
                          ? null
                          : () => _openDashboard(connected),
                      child: const Text('Open dashboard'),
                    ),
                    if (pairing?.allowedActions.contains('cancel') == true)
                      FloeButton.text(
                        onPressed: controller.busy
                            ? null
                            : controller.cancelPairing,
                        child: const Text('Cancel pairing'),
                      ),
                    if (connected != null || activePairing || needsRepair)
                      FloeButton.outlined(
                        onPressed: controller.busy
                            ? null
                            : () async {
                                await controller.load();
                                await controller.observePairing();
                              },
                        child: const Text('Check connection'),
                      ),
                    for (final gateway in gateways)
                      if (gateway.allowedActions.contains('forget') &&
                          showGatewayRepair(gateway))
                        FloeButton.text(
                          onPressed: controller.busy
                              ? null
                              : () => controller.forgetGateway(gateway),
                          child: const Text('Forget connection'),
                        ),
                  ],
                ),
                for (final gateway in gateways.where(showGatewayRepair)) ...[
                  if (gateway.failure case final failure?)
                    Text(failure.reason.replaceAll('_', ' ')),
                  if (gateway.remoteRevocationPending)
                    const Text(
                      'Revoke this device in the server dashboard if it no longer needs access.',
                    ),
                ],
              ],
            ),
          ),
        ),
        const SizedBox(height: FloeSpace.lg),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('Connection boundary', style: FloeType.controlLabel),
              const SizedBox(height: 6),
              Text(
                'Pairing connects this device to your server. Connector permissions control which sources may be used; service credentials remain on the server.',
                style: FloeType.body.copyWith(color: FloePalette.neutral600),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

Uri? _localDashboardUri(String input) {
  try {
    final text = input.trim();
    final uri = Uri.tryParse(text);
    if (text.length > 2048 ||
        uri == null ||
        uri.scheme != 'http' ||
        !{'127.0.0.1', 'localhost'}.contains(uri.host) ||
        !uri.hasPort ||
        uri.port < 1 ||
        uri.port > 65535 ||
        uri.userInfo.isNotEmpty ||
        uri.hasQuery ||
        uri.hasFragment ||
        (uri.path.isNotEmpty && uri.path != '/'))
      return null;
    return uri.replace(path: '/manage/');
  } on FormatException {
    return null;
  }
}

final class ManagementLaunchButton extends StatelessWidget {
  const ManagementLaunchButton({super.key, required this.action});
  final LaunchAction action;
  @override
  Widget build(BuildContext context) => FloeButton.outlined(
    onPressed: () async {
      if (!action.expiresAt.isAfter(DateTime.now().toUtc())) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'This launch has expired. Prepare a new management action.',
            ),
          ),
        );
        return;
      }
      final opened = await launchUrl(
        action.validatedUrl,
        mode: LaunchMode.externalApplication,
      );
      if (!opened && context.mounted)
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('The management page could not be opened.'),
          ),
        );
    },
    child: Text(
      action.purpose == 'manage_gateway'
          ? 'Open Gateway management'
          : 'Open authorization',
    ),
  );
}

String _pairingLabel(String state) => switch (state) {
  'starting' => 'Starting pairing…',
  'awaiting_gateway_approval' => 'Waiting for confirmation on the Gateway.',
  'cancelling' => 'Cancelling pairing on the Gateway…',
  'connected' => 'Gateway connected.',
  'rejected' => 'Pairing was rejected.',
  'expired' => 'Pairing expired.',
  'cancelled' => 'Pairing was cancelled.',
  'repair_required' => 'The Gateway connection needs repair.',
  'revocation_pending' =>
    'Pairing is inactive. Remote revocation still needs attention.',
  'forgotten' => 'This Gateway connection was forgotten.',
  _ => throw const FormatException('Unknown pairing state.'),
};
