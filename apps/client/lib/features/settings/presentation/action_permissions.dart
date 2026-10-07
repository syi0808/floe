part of 'settings_screen.dart';

enum _ActionPermissionPreset { all, customize }

class _ActionPermissions extends StatefulWidget {
  const _ActionPermissions({required this.controller});

  final CalendarActionController controller;

  @override
  State<_ActionPermissions> createState() => _ActionPermissionsState();
}

class _ActionPermissionsState extends State<_ActionPermissions> {
  CalendarActionController get controller => widget.controller;
  late _ActionPermissionPreset? _selectedPreset;

  @override
  void initState() {
    super.initState();
    _selectedPreset = _presetFor(controller.authority?.calendarCreate);
  }

  @override
  void didUpdateWidget(_ActionPermissions oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      _selectedPreset = _presetFor(controller.authority?.calendarCreate);
    }
  }

  _ActionPermissionPreset? _presetFor(ActionAuthorityMode? mode) => mode == null
      ? null
      : mode == ActionAuthorityMode.allow
      ? _ActionPermissionPreset.all
      : _ActionPermissionPreset.customize;

  _ActionPermissionPreset? get preset =>
      _selectedPreset ?? _presetFor(controller.authority?.calendarCreate);

  Future<void> selectPreset(_ActionPermissionPreset? nextPreset) async {
    if (nextPreset == null ||
        controller.busy ||
        controller.authority == null ||
        controller.error != null) {
      return;
    }
    setState(() => _selectedPreset = nextPreset);
    if (nextPreset == _ActionPermissionPreset.all) {
      try {
        await controller.setAuthority(ActionAuthorityMode.allow);
      } on Object {
        if (mounted) {
          setState(() => _selectedPreset = _ActionPermissionPreset.customize);
        }
      }
    }
  }

  Future<void> selectMode(ActionAuthorityMode? mode) async {
    if (mode == null ||
        controller.busy ||
        controller.authority == null ||
        controller.error != null) {
      return;
    }
    try {
      await controller.setAuthority(mode);
    } on Object {
      // The controller retains the owner failure for this view.
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
              'Choose whether Floe can create Calendar events suggested by Experts. Calendar permissions still apply.',
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
                    enabled:
                        !controller.busy &&
                        controller.authority != null &&
                        controller.error == null,
                    title: const Text('Allow Calendar event creation'),
                  ),
                  FloeRadioTile<_ActionPermissionPreset>(
                    value: _ActionPermissionPreset.customize,
                    enabled:
                        !controller.busy &&
                        controller.authority != null &&
                        controller.error == null,
                    title: const Text('Customize permissions'),
                  ),
                ],
              ),
            ),
            const SizedBox(height: FloeSpace.base),
            if (controller.authority case final authority?)
              FloeSelect<ActionAuthorityMode>(
                label: 'Create Calendar events',
                value: authority.calendarCreate,
                enabled:
                    preset == _ActionPermissionPreset.customize &&
                    !controller.busy &&
                    controller.error == null,
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
                onChanged: selectMode,
              )
            else
              Text(
                controller.error?.message ??
                    (controller.loaded
                        ? 'Calendar creation authority is unavailable.'
                        : 'Load Actions to view Calendar creation authority.'),
              ),
            const SizedBox(height: FloeSpace.sm),
            Text(
              'This setting covers Expert suggestions. Changes you request directly still use your current Calendar permissions.',
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
            if (controller.error case final error?) ...[
              const SizedBox(height: FloeSpace.md),
              Text(error.message),
              TextButton(
                onPressed: controller.busy ? null : controller.load,
                child: const Text('Reload Actions'),
              ),
            ],
          ],
        ),
      ),
    ),
  );
}
