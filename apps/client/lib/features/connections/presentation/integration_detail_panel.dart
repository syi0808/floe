import 'calendar_system_access_card.dart';
import 'service_presentation.dart';

import 'package:floe_client/l10n/app_localizations.dart';
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
        ServiceDetailHeader(
          presentation: ServicePresentation.forIntegration(
            integration,
            AppLocalizations.of(context),
          ),
        ),
        const SizedBox(height: FloeSpace.lg),
        if (CalendarSystemAccessCard.appliesTo(integration)) ...[
          CalendarSystemAccessCard(
            gateway: controller.calendarSystemAccess,
            enabled: !controller.busy,
          ),
          const SizedBox(height: FloeSpace.lg),
        ],
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
