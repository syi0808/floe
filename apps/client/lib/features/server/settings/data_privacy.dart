part of '../settings_screen.dart';

class _DataPrivacy extends StatefulWidget {
  const _DataPrivacy({
    required this.controller,
    required this.serverClient,
    required this.onManageMemory,
    this.calendarSources,
    this.calendarSourceChanges,
  });

  final AgentController controller;
  final LocalServerClient? serverClient;
  final VoidCallback onManageMemory;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;

  @override
  State<_DataPrivacy> createState() => _DataPrivacyState();
}

class _DataPrivacyState extends State<_DataPrivacy> {
  bool loading = false;
  bool registryRequested = false;
  bool calendarRequested = false;
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
      registryRequested = false;
      calendarRequested = false;
      memoryRequested = false;
      savedMemoryRequested = false;
      _load();
    }
  }

  void _controllerChanged() {
    if (!controller.busy) _load();
  }

  Future<void> _load() async {
    if (loading ||
        !controller.canManageRegistry &&
            !controller.canReviewMemory &&
            !controller.canReadMemory) {
      return;
    }
    loading = true;
    if (controller.canManageRegistry && !registryRequested) {
      registryRequested = true;
      await controller.loadRegistry();
    }
    if (controller.hasCalendarExpertManagement &&
        !calendarRequested &&
        controller.canManageCalendarExperts) {
      calendarRequested = true;
      await controller.loadCalendarExperts();
    }
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
    loading = false;
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
          'Control what Floe may use and where assisted processing may happen.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.lg),
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: controller.hasCalendarExpertManagement
              ? AgentCalendarSettings(
                  controller: controller,
                  sources: widget.calendarSources,
                  sourceChanges: widget.calendarSourceChanges,
                )
              : const Text('No connected data sources are available yet.'),
        ),
        if (controller.hasMemory) ...[
          const SizedBox(height: FloeSpace.lg),
          AgentMemorySettingsCard(
            controller: controller,
            onManage: widget.onManageMemory,
          ),
        ],
        const SizedBox(height: FloeSpace.lg),
        _AiProcessing(client: widget.serverClient),
        if (controller.vaultState != AgentVaultState.ready) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(
            'Data access will appear when your private data is unlocked.',
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
