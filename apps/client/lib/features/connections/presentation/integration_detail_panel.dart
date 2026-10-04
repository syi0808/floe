import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';

final class IntegrationDetailPanel extends StatelessWidget {
  const IntegrationDetailPanel({
    super.key,
    required this.controller,
    required this.integration,
  });
  final ConnectionsController controller;
  final IntegrationSummary integration;
  @override
  Widget build(BuildContext context) => FloeCard(
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(integration.displayName, style: FloeType.titleLarge),
        const SizedBox(height: FloeSpace.sm),
        Text(switch (integration.category) {
          'calendar' => 'Connect your calendars to see your schedule in Floe.',
          'contacts' => 'Choose the contacts you want to use with Floe.',
          'health' => 'Connect Apple Health for a private wellbeing summary.',
          'attention' => 'Connect device attention signals to Floe.',
          _ => 'Connect this service to Floe.',
        }, style: FloeType.body.copyWith(color: FloePalette.neutral600)),
        const SizedBox(height: FloeSpace.lg),
        if (integration.capabilities.contains('prepare_review'))
          FloeButton.filled(
            onPressed: controller.busy
                ? null
                : () => controller.connectIntegration(integration),
            child: const Text('Connect'),
          ),
        if (integration.state == 'unavailable')
          const Text('This service is not available on this device or server.'),
      ],
    ),
  );
}
