part of 'settings_screen.dart';

class _DataPrivacy extends StatefulWidget {
  const _DataPrivacy({
    required this.controller,
    required this.serverClient,
    required this.onManageMemory,
    this.androidContext,
    this.appleContext,
    this.daySnapshot,
    this.personalAccessGateway,
    this.platform,
  });

  final AgentController controller;
  final LocalServerClient? serverClient;
  final VoidCallback onManageMemory;
  final AndroidContextApi? androidContext;
  final AppleContextApi? appleContext;
  final DaySnapshot? daySnapshot;
  final AgentPersonalAccessGateway? personalAccessGateway;
  final TargetPlatform? platform;

  @override
  State<_DataPrivacy> createState() => _DataPrivacyState();
}

class _DataPrivacyState extends State<_DataPrivacy> {
  bool memoryRequested = false;
  bool savedMemoryRequested = false;

  AgentController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    controller.addListener(_controllerChanged);
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
    _load();
  }

  void _controllerChanged() {
    if (!controller.busy) _load();
  }

  Future<void> _load() async {
    if (controller.hasMemoryReview &&
        !memoryRequested &&
        controller.canReviewMemory) {
      memoryRequested = true;
      await controller.loadMemoryReview();
    }
    if (controller.hasMemory &&
        !savedMemoryRequested &&
        controller.canReadMemory) {
      savedMemoryRequested = true;
      await controller.loadMemory();
    }
  }

  @override
  void dispose() {
    controller.removeListener(_controllerChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Data & privacy',
          style: FloeType.headlineLarge.copyWith(fontSize: 22),
        ),
        const SizedBox(height: FloeSpace.sm),
        Text(
          'Manage source access from Connections. External actions use separate Action permissions, and external model processing requires exact recipient approval when needed.',
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
        if (controller.vaultState != AgentVaultState.ready) ...[
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
