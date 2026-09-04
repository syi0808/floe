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
              label: 'Back to connections',
              icon: LucideIcons.arrowLeft,
              onPressed: () => setState(() => detail = false),
            ),
          ),
          const SizedBox(height: 24),
          if (widget.gateway != null)
            CalendarPanel(
              gateway: widget.gateway!,
              query: widget.query,
              connection: widget.connection,
              onChanged: widget.onChanged,
            )
          else
            const FloeSquircle(
              padding: EdgeInsets.all(24),
              child: Text(
                'Calendar integration is unavailable in this preview. Use the native macOS client to connect.',
              ),
            ),
          const SizedBox(height: 24),
          LayoutBuilder(
            builder: (context, constraints) {
              final cards = [
                _InfoCard(
                  icon: LucideIcons.shieldCheck,
                  title: 'A clear boundary.',
                  text: 'Events are saved on this Mac. Floe never creates, edits, or deletes events in your connected calendar.',
                  note: 'macOS calls this “Full Access,” even for reading. That OS permission does not enable writes in Floe.',
                ),
                _InfoCard(
                  icon: LucideIcons.wifiOff,
                  title: 'What happens offline?',
                  text: 'Your last saved events remain visible, with their collection time. Revoking permission stops new reads; it does not erase the saved copy.',
                ),
              ];
              return constraints.maxWidth < 700
                  ? Column(
                      children: [
                        cards[0],
                        const SizedBox(height: 20),
                        cards[1],
                      ],
                    )
                  : Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(child: cards[0]),
                        const SizedBox(width: 20),
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
        const Text(
          'Connections',
          style: TextStyle(
            fontSize: 28,
            fontWeight: FontWeight.w700,
            letterSpacing: -1,
          ),
        ),
        const SizedBox(height: 12),
        const Text(
          'Manage the services that bring context to your day.',
          style: TextStyle(color: FloePalette.neutral600),
        ),
        const SizedBox(height: 36),
        Text(
          widget.connection == null
              ? 'Available services'
              : 'Connected services   1',
          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 16),
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 280),
          child: FloeSquircle(
            child: InkWell(
              onTap: () => setState(() => detail = true),
              hoverColor: FloePalette.primary50,
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const FloeSquircle(
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
                    const SizedBox(height: 20),
                    const Text(
                      'macOS Calendar',
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 12),
                    const Text(
                      'Bring events from your Mac into your day.',
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
    padding: const EdgeInsets.all(24),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, color: FloePalette.primary600, size: 23),
        const SizedBox(height: 20),
        Text(
          title,
          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 16),
        Text(
          text,
          style: const TextStyle(
            fontSize: 13,
            height: 1.8,
            color: FloePalette.neutral600,
          ),
        ),
        if (note != null) ...[
          const SizedBox(height: 20),
          FloeInfoNote(text: note!),
        ],
      ],
    ),
  );
}
