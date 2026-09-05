import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_squircle.dart';
import 'local_server_client.dart';
import 'local_server_panel.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.client});

  final LocalServerClient? client;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      const Text(
        'Settings',
        style: TextStyle(
          fontSize: 28,
          fontWeight: FontWeight.w700,
          letterSpacing: -1,
        ),
      ),
      const SizedBox(height: 10),
      const Text(
        'Manage Floe on this device.',
        style: TextStyle(color: FloePalette.neutral600),
      ),
      const SizedBox(height: 36),
      LayoutBuilder(
        builder: (context, constraints) {
          final narrow = constraints.maxWidth < 720;
          final navigation = _SettingsNavigation(horizontal: narrow);
          final content = Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (client case final serverClient?)
                LocalServerPanel(client: serverClient)
              else
                const FloeSquircle(
                  padding: EdgeInsets.all(24),
                  child: Text(
                    'Remote server connection is available in the native Floe app.',
                  ),
                ),
              const SizedBox(height: 24),
              const Padding(
                padding: EdgeInsets.symmetric(horizontal: 4),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Connection boundary',
                      style: TextStyle(fontWeight: FontWeight.w600),
                    ),
                    SizedBox(height: 6),
                    Text(
                      'Pairing authorizes this app to use assisted features on your server. Sensitive context is still approved per request, and service credentials remain on the server.',
                      style: TextStyle(
                        color: FloePalette.neutral600,
                        height: 1.5,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
          if (narrow) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [navigation, const SizedBox(height: 28), content],
            );
          }
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(width: 244, child: navigation),
              const SizedBox(width: 44),
              Expanded(child: content),
            ],
          );
        },
      ),
    ],
  );
}

class _SettingsNavigation extends StatelessWidget {
  const _SettingsNavigation({required this.horizontal});
  final bool horizontal;

  @override
  Widget build(BuildContext context) {
    const sections = [
      _SettingsSection(icon: LucideIcons.slidersHorizontal, label: 'General'),
      _SettingsSection(
        icon: LucideIcons.server,
        label: 'Remote server',
        selected: true,
      ),
    ];
    return horizontal
        ? const SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(children: sections),
          )
        : const Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: sections,
          );
  }
}

class _SettingsSection extends StatelessWidget {
  const _SettingsSection({
    required this.icon,
    required this.label,
    this.selected = false,
  });
  final IconData icon;
  final String label;
  final bool selected;

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: selected ? FloePalette.primary100 : Colors.transparent,
      borderWidth: 0,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              icon,
              size: 18,
              color: selected ? FloePalette.primary700 : FloePalette.neutral600,
            ),
            const SizedBox(width: 10),
            Text(
              label,
              style: TextStyle(
                fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                color: selected
                    ? FloePalette.primary700
                    : FloePalette.neutral600,
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
