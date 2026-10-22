part of 'settings_screen.dart';

class _OperationPolicySettings extends StatelessWidget {
  const _OperationPolicySettings({required this.controller});

  final OperationPolicyController controller;

  Future<void> selectMode(ActionAuthorityMode? mode) async {
    if (mode == null ||
        controller.busy ||
        controller.hasPendingChange ||
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
            const Text('Expert Calendar policy', style: FloeType.titleLarge),
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Choose whether Floe can create events suggested by Experts. Calendar permissions still apply; direct changes use your current permissions.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
            const SizedBox(height: 20),
            if (controller.authority case final authority?)
              FloeSelect<ActionAuthorityMode>(
                label: 'Create Calendar events',
                value: authority.calendarCreate,
                deferSelectionUntilConfirmed: true,
                enabled:
                    !controller.busy &&
                    !controller.hasPendingChange &&
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
                        : 'Load Access to view the Expert Calendar policy.'),
              ),
            if (controller.hasPendingChange) ...[
              const SizedBox(height: FloeSpace.md),
              const Text(
                'A previous policy change has no confirmed result. Reloading '
                'reads the current value; it does not resolve that change.',
              ),
              TextButton(
                onPressed: controller.busy
                    ? null
                    : () async {
                        try {
                          await controller.retryPendingChange();
                        } on Object {
                          // Keep the exact pending command and owner failure visible.
                        }
                      },
                child: const Text('Retry pending policy change'),
              ),
            ],
            if (controller.error case final error?) ...[
              const SizedBox(height: FloeSpace.md),
              Text(error.message),
              TextButton(
                onPressed: controller.busy ? null : controller.load,
                child: const Text('Reload Access policy'),
              ),
            ],
          ],
        ),
      ),
    ),
  );
}
