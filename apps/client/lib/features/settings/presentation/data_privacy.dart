part of 'settings_screen.dart';

class _DataPrivacy extends StatefulWidget {
  const _DataPrivacy({
    required this.controller,
    required this.vault,
    required this.onManageMemory,
    this.platform,
  });

  final AgentMemoryController controller;
  final VaultController vault;
  final VoidCallback onManageMemory;
  final TargetPlatform? platform;

  @override
  State<_DataPrivacy> createState() => _DataPrivacyState();
}

class _DataPrivacyState extends State<_DataPrivacy> {
  bool memoryRequested = false;
  bool savedMemoryRequested = false;

  AgentMemoryController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    controller.addListener(_controllerChanged);
    widget.vault.addListener(_controllerChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void didUpdateWidget(_DataPrivacy oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      oldWidget.controller.removeListener(_controllerChanged);
      controller.addListener(_controllerChanged);
      memoryRequested = false;
      savedMemoryRequested = false;
    }
    if (oldWidget.vault != widget.vault) {
      oldWidget.vault.removeListener(_controllerChanged);
      widget.vault.addListener(_controllerChanged);
    }
    _load();
  }

  void _controllerChanged() {
    if (!widget.vault.ready) {
      memoryRequested = false;
      savedMemoryRequested = false;
      return;
    }
    if (!controller.busy) _load();
  }

  Future<void> _load() async {
    if (!mounted || !widget.vault.ready) return;
    if (controller.hasReview &&
        !memoryRequested &&
        controller.canReadReview) {
      memoryRequested = true;
      await controller.loadReview();
    }
    if (!mounted || !widget.vault.ready) return;
    if (controller.hasMemory &&
        !savedMemoryRequested &&
        controller.canRead) {
      savedMemoryRequested = true;
      await controller.load();
    }
  }

  @override
  void dispose() {
    controller.removeListener(_controllerChanged);
    widget.vault.removeListener(_controllerChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: Listenable.merge([controller, widget.vault]),
    builder: (context, _) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Data & privacy',
          style: FloeType.headlineLarge.copyWith(fontSize: 22),
        ),
        const SizedBox(height: FloeSpace.sm),
        Text(
          'Manage source access from Connections. External actions use separate Action permissions, and source reviews control whether processing stays on this device or may use your verified Gateway.',
          style: FloeType.body.copyWith(
            color: FloePalette.neutral600,
            height: 1.5,
          ),
        ),
        const SizedBox(height: FloeSpace.lg),
        const FloeInfoNote(
          key: ValueKey('connections-privacy-navigation'),
          text: 'Open Connections to connect sources, choose resources, or change Use with Floe.',
        ),
        if (controller.hasMemory) ...[
          const SizedBox(height: FloeSpace.lg),
          AgentMemorySettingsCard(
            controller: controller,
            onManage: widget.onManageMemory,
          ),
        ],
        if (!widget.vault.ready) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(
            'Private data controls will appear when your vault is unlocked.',
            style: FloeType.body.copyWith(
              color: FloePalette.neutral600,
              height: 1.4,
            ),
          ),
        ],
      ],
    ),
  );
}
