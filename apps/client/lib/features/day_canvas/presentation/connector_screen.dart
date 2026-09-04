import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_squircle.dart';
import '../application/calendar_gateway.dart';
import '../domain/day_models.dart';
import 'calendar_panel.dart';

class ConnectorScreen extends StatefulWidget {
  const ConnectorScreen({
    super.key,
    required this.gateway,
    required this.query,
    required this.connection,
    required this.onChanged,
  });
  final CalendarGateway? gateway;
  final DayQuery query;
  final CalendarConnection? connection;
  final Future<void> Function() onChanged;
  @override
  State<ConnectorScreen> createState() => _ConnectorScreenState();
}

class _ConnectorScreenState extends State<ConnectorScreen> {
  bool detail = false;
  @override
  Widget build(BuildContext context) {
    if (detail) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: FloeTextLink(
              label: AppLocalizations.of(context).backToConnections,
              icon: LucideIcons.arrowLeft,
              onPressed: () => setState(() => detail = false),
            ),
          ),
          SizedBox(height: 24),
          if (widget.gateway != null)
            CalendarPanel(
              gateway: widget.gateway!,
              query: widget.query,
              connection: widget.connection,
              onChanged: widget.onChanged,
            )
          else
            FloeSquircle(
              padding: EdgeInsets.all(24),
              child: Text(
                AppLocalizations.of(context)
                    .calendarIntegrationIsUnavailableInThisPreview,
              ),
            ),
          SizedBox(height: 24),
          LayoutBuilder(
            builder: (context, constraints) {
              final cards = [
                _InfoCard(
                  icon: LucideIcons.shieldCheck,
                  title: AppLocalizations.of(context).aClearBoundary,
                  text: AppLocalizations.of(context)
                      .eventsAreSavedOnThisMacFloe,
                  note: AppLocalizations.of(context)
                      .macosCallsThisFullAccessEvenFor,
                ),
                _InfoCard(
                  icon: LucideIcons.wifiOff,
                  title: AppLocalizations.of(context).whatHappensOffline,
                  text: AppLocalizations.of(context)
                      .yourLastSavedEventsRemainVisibleWith,
                ),
              ];
              return constraints.maxWidth < 700
                  ? Column(children: [cards[0], SizedBox(height: 20), cards[1]])
                  : Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(child: cards[0]),
                        SizedBox(width: 20),
                        Expanded(child: cards[1]),
                      ],
                    );
            },
          ),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          AppLocalizations.of(context).connections,
          style: TextStyle(
            fontSize: 28,
            fontWeight: FontWeight.w700,
            letterSpacing: -1,
          ),
        ),
        SizedBox(height: 12),
        Text(
          AppLocalizations.of(context).manageTheServicesThatBringContextTo,
          style: TextStyle(color: FloePalette.neutral600),
        ),
        SizedBox(height: 36),
        Text(
          widget.connection == null
              ? AppLocalizations.of(context).availableServices
              : AppLocalizations.of(context).connectedServicesCount(1),
          style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        SizedBox(height: 16),
        ConstrainedBox(
          constraints: BoxConstraints(maxWidth: 280),
          child: FloeSquircle(
            child: InkWell(
              onTap: () => setState(() => detail = true),
              hoverColor: FloePalette.primary50,
              child: Padding(
                padding: EdgeInsets.all(24),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    FloeSquircle(
                      size: FloeSquircleSize.md,
                      fill: FloePalette.primary50,
                      borderWidth: 0,
                      padding: EdgeInsets.all(14),
                      child: Icon(
                        LucideIcons.calendarDays,
                        size: 26,
                        color: FloePalette.primary600,
                      ),
                    ),
                    SizedBox(height: 20),
                    Text(
                      AppLocalizations.of(context).macosCalendar,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    SizedBox(height: 12),
                    Text(
                      AppLocalizations.of(context)
                          .bringEventsFromYourMacIntoYour,
                      style: TextStyle(
                        fontSize: 12,
                        height: 1.6,
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard({
    required this.icon,
    required this.title,
    required this.text,
    this.note,
  });
  final IconData icon;
  final String title;
  final String text;
  final String? note;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: EdgeInsets.all(24),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, color: FloePalette.primary600, size: 23),
        SizedBox(height: 20),
        Text(
          title,
          style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        SizedBox(height: 16),
        Text(
          text,
          style: TextStyle(
            fontSize: 13,
            height: 1.8,
            color: FloePalette.neutral600,
          ),
        ),
        if (note != null) ...[SizedBox(height: 20), FloeInfoNote(text: note!)],
      ],
    ),
  );
}
