import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
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
  Widget build(BuildContext context) {
    final review = controller.integrationReview;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(integration.displayName, style: FloeType.title),
        Text(integration.state.replaceAll('_', ' ')),
        if (integration.capabilities.contains('prepare_review'))
          FloeButton.outlined(
            onPressed: controller.busy
                ? null
                : () => controller.prepareIntegration(integration),
            child: const Text('Review setup'),
          ),
        if (review != null &&
            review.integrationRef == integration.integrationRef) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(switch (review.setupKind) {
            'native_permission' => 'Review system access on this device.',
            'device_code' =>
              'Complete the displayed device authorization on your Gateway.',
            'gateway_managed_secret' =>
              'Manage provider credentials on your Gateway.',
            _ => 'Authorize this connection on your Gateway.',
          }),
          if (review.allowedActions.contains('start'))
            FloeButton.outlined(
              onPressed: controller.busy
                  ? null
                  : () => controller.startIntegration(review),
              child: const Text('Continue setup'),
            ),
          FloeButton.text(
            onPressed: controller.dismissReview,
            child: const Text('Close review'),
          ),
        ],
      ],
    );
  }
}
