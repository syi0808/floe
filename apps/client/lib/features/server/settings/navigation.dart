part of '../settings_screen.dart';

class _SettingsNavigation extends StatelessWidget {
  const _SettingsNavigation({
    required this.horizontal,
    required this.controller,
    required this.pages,
    required this.selected,
    required this.onSelected,
  });

  final bool horizontal;
  final ScrollController? controller;
  final List<_SettingsPage> pages;
  final _SettingsPage selected;
  final ValueChanged<_SettingsPage> onSelected;

  @override
  Widget build(BuildContext context) {
    final sections = [
      for (final page in pages)
        _SettingsSection(
          key: ValueKey('settings-${page.name}'),
          icon: switch (page) {
            _SettingsPage.actions => LucideIcons.slidersHorizontal,
            _SettingsPage.dataPrivacy => LucideIcons.shieldCheck,
            _SettingsPage.memory => LucideIcons.brain,
            _SettingsPage.remoteServer => LucideIcons.server,
          },
          label: switch (page) {
            _SettingsPage.actions => 'Action permissions',
            _SettingsPage.dataPrivacy => 'Data & privacy',
            _SettingsPage.memory => 'Memory',
            _SettingsPage.remoteServer => 'Remote server',
          },
          selected: page == selected,
          onPressed: () => onSelected(page),
        ),
    ];
    return horizontal
        ? SingleChildScrollView(
            key: const ValueKey('settings-navigation-scroll'),
            controller: controller,
            primary: false,
            scrollDirection: Axis.horizontal,
            child: Row(children: sections),
          )
        : Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: sections,
          );
  }
}

class _SettingsSection extends StatelessWidget {
  const _SettingsSection({
    required this.icon,
    required this.label,
    required this.onPressed,
    this.selected = false,
    super.key,
  });
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: selected ? FloePalette.primary100 : Colors.transparent,
      borderWidth: 0,
      child: FloePressable(
        size: FloeSquircleSize.md,
        onPressed: onPressed,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                icon,
                size: 18,
                color: selected
                    ? FloePalette.primary700
                    : FloePalette.neutral600,
              ),
              const SizedBox(width: 10),
              Flexible(
                child: Text(
                  label,
                  overflow: TextOverflow.ellipsis,
                  style: FloeType.body.copyWith(
                    fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                    color: selected
                        ? FloePalette.primary700
                        : FloePalette.neutral600,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}
