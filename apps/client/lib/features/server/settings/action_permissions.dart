part of '../settings_screen.dart';

enum _ActionPermissionPreset { all, customize }

class _ActionPermissions extends StatefulWidget {
  const _ActionPermissions({required this.controller});

  final CalendarActionController controller;

  @override
  State<_ActionPermissions> createState() => _ActionPermissionsState();
}

class _ActionPermissionsState extends State<_ActionPermissions> {
  late _ActionPermissionPreset preset;

  CalendarActionController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    preset = controller.authority.calendarCreate == ActionAuthorityMode.allow
        ? _ActionPermissionPreset.all
        : _ActionPermissionPreset.customize;
  }

  @override
  void didUpdateWidget(_ActionPermissions oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      preset = controller.authority.calendarCreate == ActionAuthorityMode.allow
          ? _ActionPermissionPreset.all
          : _ActionPermissionPreset.customize;
    }
  }

  Future<void> selectPreset(_ActionPermissionPreset? nextPreset) async {
    if (nextPreset == null || controller.busy) return;
    setState(() => preset = nextPreset);
    if (nextPreset == _ActionPermissionPreset.all) {
      await controller.setCalendarCreateAuthority(ActionAuthorityMode.allow);
      if (mounted &&
          controller.authority.calendarCreate != ActionAuthorityMode.allow) {
        setState(() => preset = _ActionPermissionPreset.customize);
      }
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) => FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: FloeLoadingOverlay(
        loading: controller.busy,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text('Action permissions', style: FloeType.titleLarge),
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Choose when Floe must ask before changing an external service. OS and connector permissions still apply.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
            const SizedBox(height: 20),
            FloeRadioGroup<_ActionPermissionPreset>(
              value: preset,
              onChanged: selectPreset,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  FloeRadioTile<_ActionPermissionPreset>(
                    value: _ActionPermissionPreset.all,
                    enabled: !controller.busy,
                    title: const Text('Allow all supported actions'),
                  ),
                  FloeRadioTile<_ActionPermissionPreset>(
                    value: _ActionPermissionPreset.customize,
                    enabled: !controller.busy,
                    title: const Text('Customize permissions'),
                  ),
                ],
              ),
            ),
            const SizedBox(height: FloeSpace.base),
            FloeSelect<ActionAuthorityMode>(
              label: 'Create Calendar events',
              value: controller.authority.calendarCreate,
              enabled:
                  preset == _ActionPermissionPreset.customize &&
                  !controller.busy,
              options: const [
                FloeSelectOption(
                  value: ActionAuthorityMode.allow,
                  label: 'Allow automatically',
                ),
                FloeSelectOption(
                  value: ActionAuthorityMode.ask,
                  label: 'Ask every time',
                ),
                FloeSelectOption(
                  value: ActionAuthorityMode.deny,
                  label: 'Do not allow',
                ),
              ],
              onChanged: (value) {
                if (value != null) {
                  controller.setCalendarCreateAuthority(value);
                }
              },
            ),
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Currently this preset covers Calendar event creation only. It never bypasses macOS permission or safety checks.',
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
            const SizedBox(height: FloeSpace.md),
            Text(
              controller.writesEnabled
                  ? 'Calendar writing is available in this build.'
                  : 'Calendar writing is unavailable in this build.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
            if (controller.failed) ...[
              const SizedBox(height: FloeSpace.sm),
              const Text('The permission could not be saved. Try again.'),
            ],
          ],
        ),
      ),
    ),
  );
}

FloeBadgeTone _statusTone(String status) => switch (status) {
  'Paired' ||
  'Available' ||
  'Completed' ||
  'Preferred' => FloeBadgeTone.success,
  'Needs consent' || 'Not paired' || 'Unavailable' => FloeBadgeTone.warning,
  'Failed' => FloeBadgeTone.danger,
  _ => FloeBadgeTone.neutral,
};
