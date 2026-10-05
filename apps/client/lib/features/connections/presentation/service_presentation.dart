import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// Copy and icons only. The owner projection supplies identity, state and actions.
final class ServicePresentation {
  const ServicePresentation(this.name, this.description, this.icon);
  final String name;
  final String description;
  final IconData icon;

  factory ServicePresentation.forIntegration(
    IntegrationSummary integration,
    AppLocalizations strings,
  ) {
    if (integration.serviceKind == 'apple_calendar') {
      return ServicePresentation(
        switch (defaultTargetPlatform) {
          TargetPlatform.macOS => strings.macosCalendar,
          TargetPlatform.iOS => strings.appleCalendar,
          _ => integration.displayName,
        },
        switch (defaultTargetPlatform) {
          TargetPlatform.macOS => strings.calendarsAlreadyOnThisMac,
          TargetPlatform.iOS => strings.calendarsAlreadyOnThisIphoneOrIpad,
          _ => strings.calendarsAlreadyOnThisDevice,
        },
        LucideIcons.calendarDays,
      );
    }
    return ServicePresentation(
      integration.displayName,
      switch (integration.serviceKind) {
        'apple_contacts' =>
          'Choose the contacts available on this Apple device.',
        'apple_health' =>
          'Use a private, derived wellbeing summary from Apple Health.',
        'apple_attention' =>
          'Manage coarse attention signals from this device.',
        _ => switch (integration.category) {
          'calendar' => 'Connect your calendars to see your schedule in Floe.',
          'contacts' => 'Choose the contacts you want to use with Floe.',
          _ => 'Manage this service and its reviewed access.',
        },
      },
      switch (integration.category) {
        'calendar' => LucideIcons.calendarDays,
        'contacts' => LucideIcons.contact,
        'health' => LucideIcons.heartPulse,
        'attention' => LucideIcons.focus,
        _ => LucideIcons.plug,
      },
    );
  }

  factory ServicePresentation.forSource(SourceSummary source) =>
      ServicePresentation(
        source.displayLabels.join(' · '),
        'Manage the resources and access reviewed for this source.',
        LucideIcons.plug,
      );
}

/// Shared baseline service identity header for disconnected and connected detail.
final class ServiceDetailHeader extends StatelessWidget {
  const ServiceDetailHeader({
    super.key,
    required this.presentation,
    this.status,
  });
  final ServicePresentation presentation;
  final Widget? status;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          FloeSquircle(
            size: FloeSquircleSize.md,
            fill: FloePalette.primary50,
            borderWidth: 0,
            padding: const EdgeInsets.all(14),
            child: Icon(
              presentation.icon,
              size: 26,
              color: FloePalette.primary600,
            ),
          ),
          const SizedBox(width: FloeSpace.base),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(presentation.name, style: FloeType.titleLarge),
                const SizedBox(height: 6),
                Text(
                  presentation.description,
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
      if (status case final value?) ...[
        const SizedBox(height: FloeSpace.sm),
        value,
      ],
    ],
  );
}
