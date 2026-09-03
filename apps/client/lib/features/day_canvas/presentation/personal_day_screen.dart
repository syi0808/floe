import 'package:flutter/material.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_mascot.dart';
import '../../../app/floe_motion.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_theme.dart';
import '../application/day_gateway.dart';
import '../application/personal_day_controller.dart';
import '../domain/day_models.dart';

class PersonalDayScreen extends StatefulWidget {
  const PersonalDayScreen({
    super.key,
    required this.gateway,
    required this.query,
  });
  final DayGateway gateway;
  final DayQuery query;
  @override
  State<PersonalDayScreen> createState() => _PersonalDayScreenState();
}

class _PersonalDayScreenState extends State<PersonalDayScreen> {
  late final PersonalDayController controller;
  final captureController = TextEditingController();

  @override
  void initState() {
    super.initState();
    controller = PersonalDayController(
      gateway: widget.gateway,
      query: widget.query,
    )..load();
  }

  @override
  void dispose() {
    captureController.dispose();
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) => LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth < 720;
        return Scaffold(
          bottomNavigationBar: narrow ? const _MobileDestinations() : null,
          body: SafeArea(
            child: Column(
              children: [
                _ApplicationHeader(narrow: narrow),
                Expanded(
                  child: SingleChildScrollView(
                    child: Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 1240),
                        child: Padding(
                          padding: EdgeInsets.fromLTRB(
                            narrow ? FloeSpace.base : FloeSpace.xl,
                            FloeSpace.lg,
                            narrow ? FloeSpace.base : FloeSpace.xl,
                            FloeSpace.xxxl,
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              _DayToolbar(controller, narrow: narrow),
                              if (controller.errorMessage case final message?)
                                _ErrorNotice(
                                  message: message,
                                  dismiss: controller.clearError,
                                ),
                              const SizedBox(height: FloeSpace.lg),
                              _content(narrow),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 1240),
                    child: Padding(
                      padding: EdgeInsets.fromLTRB(
                        narrow ? FloeSpace.base : FloeSpace.xl,
                        FloeSpace.sm,
                        narrow ? FloeSpace.base : FloeSpace.xl,
                        FloeSpace.base,
                      ),
                      child: _CaptureBar(
                        textController: captureController,
                        pending: controller.commandPending,
                        submit: _capture,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    ),
  );

  Widget _content(bool narrow) {
    if (controller.loadState == DayLoadState.loading) {
      return const _LoadingDay();
    }
    if (controller.loadState == DayLoadState.failure) {
      return _FailureDay(retry: controller.load);
    }
    final snapshot = controller.snapshot!;
    final primary = _PrimaryDay(
      snapshot: snapshot,
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      delete: controller.deleteItem,
    );
    final rail = _ContextRail(
      snapshot: snapshot,
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      delete: controller.deleteItem,
    );
    if (narrow) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          primary,
          const SizedBox(height: FloeSpace.lg),
          rail,
        ],
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(flex: 7, child: primary),
        const SizedBox(width: FloeSpace.xl),
        SizedBox(width: 320, child: rail),
      ],
    );
  }

  Future<void> _setTaskCompleted(TaskItem task, bool completed) async {
    await controller.setTaskCompleted(task, completed);
    if (!mounted || controller.errorMessage != null) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(completed ? '할 일을 완료했어요.' : '완료를 취소했어요.'),
          action: SnackBarAction(
            label: '실행 취소',
            onPressed: () => controller.setTaskCompleted(task, !completed),
          ),
        ),
      );
  }

  Future<void> _capture() async {
    if (!await controller.submitCapture(captureController.text) || !mounted) {
      return;
    }
    final dialog = _ClassificationDialog(
      capture: controller.pendingCapture!,
      now: controller.pendingCapture!.capturedAt,
      classify: controller.classify,
    );
    final saved = MediaQuery.sizeOf(context).width < 720
        ? await showModalBottomSheet<bool>(
            context: context,
            isScrollControlled: true,
            useSafeArea: true,
            backgroundColor: Colors.transparent,
            builder: (context) => dialog,
          )
        : await showGeneralDialog<bool>(
            context: context,
            barrierDismissible: false,
            barrierColor: FloePalette.neutral950.withValues(alpha: 0.28),
            transitionDuration: FloeMotion.dialogDuration,
            pageBuilder: (context, animation, secondaryAnimation) =>
                Center(child: dialog),
            transitionBuilder: (
              context,
              animation,
              secondaryAnimation,
              child,
            ) => FloeFadeScaleTransition(animation: animation, child: child),
          );
    if (saved == true) captureController.clear();
  }
}

class _ApplicationHeader extends StatelessWidget {
  const _ApplicationHeader({required this.narrow});
  final bool narrow;
  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: const BoxDecoration(
      color: FloePalette.neutral0,
      border: Border(bottom: BorderSide(color: FloePalette.neutral200)),
    ),
    child: Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1240),
        child: SizedBox(
          height: narrow ? 64 : 76,
          child: Padding(
            padding: EdgeInsets.symmetric(
              horizontal: narrow ? FloeSpace.base : FloeSpace.xl,
            ),
            child: Row(
              children: [
                const FloeMascot(size: 44),
                const SizedBox(width: FloeSpace.md),
                const Text(
                  'Floe',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
                ),
                if (!narrow) ...[
                  const SizedBox(width: FloeSpace.xxl),
                  const _Destination(label: 'Today', selected: true),
                  const _Destination(label: 'Tasks'),
                  const _Destination(label: 'Notes'),
                ],
                const Spacer(),
                IconButton(
                  tooltip: '설정',
                  onPressed: () => _showComingSoon(context),
                  icon: const Icon(Icons.settings_outlined),
                ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

class _Destination extends StatelessWidget {
  const _Destination({required this.label, this.selected = false});
  final String label;
  final bool selected;
  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: TextButton(
      onPressed: selected ? null : () => _showComingSoon(context),
      style: TextButton.styleFrom(
        foregroundColor: selected
            ? FloePalette.primary700
            : FloePalette.neutral600,
        disabledForegroundColor: FloePalette.primary700,
        backgroundColor: selected ? FloePalette.primary50 : Colors.transparent,
      ),
      child: Text(label),
    ),
  );
}

class _MobileDestinations extends StatelessWidget {
  const _MobileDestinations();
  @override
  Widget build(BuildContext context) => NavigationBar(
    selectedIndex: 0,
    height: 68,
    backgroundColor: FloePalette.neutral0,
    indicatorColor: FloePalette.primary100,
    onDestinationSelected: (index) {
      if (index != 0) _showComingSoon(context);
    },
    destinations: const [
      NavigationDestination(icon: Icon(Icons.today_outlined), label: '오늘'),
      NavigationDestination(icon: Icon(Icons.check_box_outlined), label: '할 일'),
      NavigationDestination(icon: Icon(Icons.notes_outlined), label: '노트'),
    ],
  );
}

class _DayToolbar extends StatelessWidget {
  const _DayToolbar(this.controller, {required this.narrow});
  final PersonalDayController controller;
  final bool narrow;
  @override
  Widget build(BuildContext context) {
    final date = Text(
      _date(controller.query.date),
      style: FloeType.headlineLarge,
    );
    final movement = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        IconButton(
          tooltip: '이전 날',
          onPressed: () => controller.moveDay(-1),
          icon: const Icon(Icons.chevron_left),
        ),
        TextButton(onPressed: controller.goToday, child: const Text('오늘')),
        IconButton(
          tooltip: '다음 날',
          onPressed: () => controller.moveDay(1),
          icon: const Icon(Icons.chevron_right),
        ),
      ],
    );
    final views = SegmentedButton<int>(
      showSelectedIcon: false,
      segments: const [
        ButtonSegment(value: 0, label: Text('일')),
        ButtonSegment(value: 1, label: Text('주')),
        ButtonSegment(value: 2, label: Text('월')),
      ],
      selected: const {0},
      onSelectionChanged: (value) {
        if (value.first != 0) _showComingSoon(context);
      },
    );
    if (narrow) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          date,
          const SizedBox(height: FloeSpace.md),
          Align(alignment: Alignment.centerLeft, child: movement),
          const SizedBox(height: FloeSpace.sm),
          Align(alignment: Alignment.centerLeft, child: views),
        ],
      );
    }
    return Row(
      children: [
        Expanded(child: date),
        movement,
        const SizedBox(width: FloeSpace.md),
        views,
      ],
    );
  }
}

class _PrimaryDay extends StatelessWidget {
  const _PrimaryDay({
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
  });
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  @override
  Widget build(BuildContext context) {
    final timelineItems = snapshot.items
        .where((item) => item is! TaskItem)
        .toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _NowNext(snapshot),
        const SizedBox(height: FloeSpace.xl),
        Row(
          children: [
            const Expanded(
              child: Text('오늘의 흐름', style: FloeType.headlineLarge),
            ),
            Text('${snapshot.items.length}개 항목', style: FloeType.body),
          ],
        ),
        const SizedBox(height: FloeSpace.base),
        if (snapshot.items.isEmpty)
          const _EmptyState()
        else ...[
          _CurrentTimeMarker(now: snapshot.generatedAt),
          if (timelineItems.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: FloeSpace.xl),
              child: Text('시간이 정해진 일정이나 노트가 없어요.', style: FloeType.body),
            )
          else
            for (final (index, item) in timelineItems.indexed) ...[
              _DayRow(
                item: item,
                snapshot: snapshot,
                disabled: disabled,
                complete: complete,
                delete: delete,
              ),
              if (index < timelineItems.length - 1)
                const Divider(height: 1, indent: 56),
            ],
        ],
      ],
    );
  }
}

class _ContextRail extends StatelessWidget {
  const _ContextRail({
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
  });
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  @override
  Widget build(BuildContext context) {
    final tasks = snapshot.items.whereType<TaskItem>().toList();
    final notes = snapshot.items.whereType<NoteItem>().toList();
    final relatedNote = notes.isEmpty ? null : notes.first;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  const Expanded(
                    child: Text('오늘의 할 일', style: FloeType.headline),
                  ),
                  Text('${tasks.length}', style: FloeType.numeric),
                ],
              ),
              const SizedBox(height: FloeSpace.md),
              if (tasks.isEmpty)
                const Text('오늘 처리할 할 일이 없어요.', style: FloeType.body)
              else
                for (final task in tasks)
                  _DayRow(
                    item: task,
                    snapshot: snapshot,
                    disabled: disabled,
                    complete: complete,
                    delete: delete,
                    compact: true,
                  ),
              const SizedBox(height: FloeSpace.sm),
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(
                  onPressed: () => _showComingSoon(context),
                  child: const Text('모두 보기'),
                ),
              ),
            ],
          ),
        ),
        if (relatedNote != null) ...[
          const SizedBox(height: FloeSpace.base),
          FloeSquircle(
            fill: FloePalette.neutral50,
            padding: const EdgeInsets.all(FloeSpace.lg),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Row(
                  children: [
                    Icon(
                      Icons.notes_outlined,
                      size: 18,
                      color: FloePalette.neutral500,
                    ),
                    SizedBox(width: FloeSpace.sm),
                    Text('관련 노트', style: FloeType.label),
                  ],
                ),
                const SizedBox(height: FloeSpace.md),
                Text(relatedNote.title, style: FloeType.bodyLarge),
                const SizedBox(height: FloeSpace.xs),
                const Text('오늘의 흐름과 연결된 기록', style: FloeType.body),
              ],
            ),
          ),
        ],
        const SizedBox(height: FloeSpace.base),
        const _FloePresence(),
      ],
    );
  }
}

class _FloePresence extends StatelessWidget {
  const _FloePresence();
  @override
  Widget build(BuildContext context) => FloeSquircle(
    fill: FloePalette.primary50,
    borderColor: FloePalette.primary100,
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const FloeMascot(size: 36),
        const SizedBox(width: FloeSpace.md),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('Floe가 도와드릴게요', style: FloeType.bodyLarge),
              const SizedBox(height: FloeSpace.xs),
              const Text('필요할 때 오늘의 흐름을 함께 정리해요.', style: FloeType.body),
              const SizedBox(height: FloeSpace.sm),
              TextButton(
                onPressed: () => _showComingSoon(context),
                child: const Text('Floe 열기'),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}

class _NowNext extends StatelessWidget {
  const _NowNext(this.snapshot);
  final DaySnapshot snapshot;
  EventItem? _find(String? id) {
    for (final event in snapshot.items.whereType<EventItem>()) {
      if (event.id == id) return event;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final current = _find(snapshot.nowEventId);
    final next = _find(snapshot.nextEventId);
    final now = _Hero(
      label: '지금',
      title: current?.title ?? '여유로운 시간',
      meta: current == null ? '진행 중인 일정이 없어요' : '${_time(current.endsAt)}까지',
      primary: true,
    );
    final upcoming = _Hero(
      label: '다음',
      title: next?.title ?? '예정된 일정 없음',
      meta: next == null ? '하루를 가볍게 계획해 보세요' : _time(next.startsAt),
    );
    return ClipPath(
      clipper: ShapeBorderClipper(
        shape: floeSquircleBorder(FloeSquircleSize.xl),
      ),
      child: DecoratedBox(
        decoration: const BoxDecoration(gradient: FloePalette.glacialField),
        child: FloeSquircle(
          size: FloeSquircleSize.xl,
          fill: Colors.transparent,
          borderColor: FloePalette.primary100,
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: LayoutBuilder(
            builder: (context, constraints) => constraints.maxWidth < 520
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      now,
                      const SizedBox(height: FloeSpace.base),
                      _NextSurface(child: upcoming),
                    ],
                  )
                : Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(flex: 6, child: now),
                      const SizedBox(width: FloeSpace.lg),
                      Expanded(flex: 4, child: _NextSurface(child: upcoming)),
                    ],
                  ),
          ),
        ),
      ),
    );
  }
}

class _NextSurface extends StatelessWidget {
  const _NextSurface({required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.md,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: child,
  );
}

class _Hero extends StatelessWidget {
  const _Hero({
    required this.label,
    required this.title,
    required this.meta,
    this.primary = false,
  });
  final String label;
  final String title;
  final String meta;
  final bool primary;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        label,
        style: FloeType.label.copyWith(color: FloePalette.primary700),
      ),
      const SizedBox(height: FloeSpace.sm),
      Text(title, style: primary ? FloeType.display : FloeType.headline),
      const SizedBox(height: FloeSpace.xs),
      Text(meta, style: FloeType.body),
    ],
  );
}

class _CurrentTimeMarker extends StatelessWidget {
  const _CurrentTimeMarker({required this.now});
  final DateTime now;
  @override
  Widget build(BuildContext context) => Semantics(
    label: '현재 시간 ${_time(now)}',
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: FloeSpace.md),
      child: Row(
        children: [
          Text(
            '현재 ${_time(now)}',
            style: FloeType.numeric.copyWith(color: FloePalette.primary700),
          ),
          const SizedBox(width: FloeSpace.md),
          const Expanded(
            child: Divider(color: FloePalette.primary400, thickness: 1.5),
          ),
        ],
      ),
    ),
  );
}

class _DayRow extends StatelessWidget {
  const _DayRow({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    this.compact = false,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final bool compact;
  @override
  Widget build(BuildContext context) {
    final task = item is TaskItem ? item as TaskItem : null;
    final overdue =
        task != null &&
        !task.isCompleted &&
        task.deadline?.isBefore(snapshot.generatedAt) == true;
    final subtitle = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        '${_time(startsAt)}–${_time(endsAt)}',
      TaskItem(:final deadline) =>
        deadline == null ? '시간 미정' : '마감 ${_time(deadline)}',
      NoteItem() => '오늘의 생각',
    };
    final label = switch (item) {
      EventItem() => '일정',
      TaskItem() => '할 일',
      NoteItem() => '노트',
    };
    return ListTile(
      minTileHeight: compact ? 60 : 72,
      contentPadding: EdgeInsets.zero,
      leading: SizedBox.square(
        dimension: 44,
        child: Center(
          child: task != null
              ? Checkbox(
                  value: task.isCompleted,
                  onChanged: disabled
                      ? null
                      : (value) => complete(task, value ?? false),
                )
              : Icon(
                  item is EventItem
                      ? Icons.calendar_today_outlined
                      : Icons.notes_outlined,
                  color: item is EventItem
                      ? FloePalette.blue500
                      : FloePalette.mint700,
                  size: 20,
                ),
        ),
      ),
      title: Text(
        item.title,
        maxLines: compact ? 2 : 3,
        overflow: TextOverflow.ellipsis,
        style: FloeType.bodyLarge.copyWith(
          decoration: task?.isCompleted == true
              ? TextDecoration.lineThrough
              : null,
        ),
      ),
      subtitle: Text(
        overdue ? '$label · $subtitle · 기한 지남' : '$label · $subtitle',
        style: FloeType.body.copyWith(
          color: overdue ? FloePalette.warning600 : FloePalette.neutral500,
        ),
      ),
      trailing: IconButton(
        tooltip: '${item.title} 삭제',
        onPressed: disabled ? null : () => _confirmDelete(context),
        icon: const Icon(Icons.more_horiz, size: 20),
      ),
    );
  }

  Future<void> _confirmDelete(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('항목을 삭제할까요?'),
        content: Text('“${item.title}” 항목이 오늘의 흐름에서 제거됩니다.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('취소'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            style: FloeTheme.destructiveButtonStyle,
            child: const Text('삭제'),
          ),
        ],
      ),
    );
    if (confirmed == true) await delete(item);
  }
}

class _CaptureBar extends StatelessWidget {
  const _CaptureBar({
    required this.textController,
    required this.pending,
    required this.submit,
  });
  final TextEditingController textController;
  final bool pending;
  final VoidCallback submit;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.md,
    padding: const EdgeInsets.all(FloeSpace.md),
    elevation: 1,
    child: Row(
      children: [
        const Icon(Icons.add_circle_outline, color: FloePalette.primary600),
        const SizedBox(width: FloeSpace.md),
        Expanded(
          child: TextField(
            key: const Key('capture-field'),
            controller: textController,
            enabled: !pending,
            onSubmitted: (value) {
              if (!pending && value.trim().isNotEmpty) submit();
            },
            decoration: const InputDecoration(
              labelText: '빠른 캡처',
              hintText: '일정, 할 일, 생각을 담아보세요',
              filled: false,
              border: InputBorder.none,
              enabledBorder: InputBorder.none,
              focusedBorder: InputBorder.none,
              contentPadding: EdgeInsets.zero,
            ),
          ),
        ),
        const SizedBox(width: FloeSpace.sm),
        ValueListenableBuilder<TextEditingValue>(
          valueListenable: textController,
          builder: (context, value, _) {
            final enabled = !pending && value.text.trim().isNotEmpty;
            return Tooltip(
              message: '캡처 저장',
              child: PressableScale(
                enabled: enabled,
                child: FilledButton(
                  onPressed: enabled ? submit : null,
                  child: pending
                      ? const SizedBox.square(
                          dimension: 16,
                          child: CircularProgressIndicator(
                            strokeWidth: 2,
                            color: FloePalette.neutral0,
                          ),
                        )
                      : const Text('담기'),
                ),
              ),
            );
          },
        ),
      ],
    ),
  );
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();
  @override
  Widget build(BuildContext context) => const Padding(
    padding: EdgeInsets.symmetric(vertical: FloeSpace.xxxl),
    child: Column(
      children: [
        Icon(
          Icons.water_drop_outlined,
          size: 36,
          color: FloePalette.primary500,
        ),
        SizedBox(height: FloeSpace.base),
        Text('오늘은 아직 비어 있어요', style: FloeType.headline),
        SizedBox(height: FloeSpace.sm),
        Text('아래에서 일정, 할 일, 생각을 담아보세요.', style: FloeType.body),
      ],
    ),
  );
}

class _LoadingDay extends StatelessWidget {
  const _LoadingDay();
  @override
  Widget build(BuildContext context) => const FloeSquircle(
    size: FloeSquircleSize.xl,
    fill: FloePalette.neutral50,
    padding: EdgeInsets.all(FloeSpace.xxxl),
    child: Center(child: CircularProgressIndicator()),
  );
}

class _FailureDay extends StatelessWidget {
  const _FailureDay({required this.retry});
  final VoidCallback retry;
  @override
  Widget build(BuildContext context) => Center(
    child: FilledButton.icon(
      onPressed: retry,
      icon: const Icon(Icons.refresh),
      label: const Text('다시 불러오기'),
    ),
  );
}

class _ErrorNotice extends StatelessWidget {
  const _ErrorNotice({required this.message, required this.dismiss});
  final String message;
  final VoidCallback dismiss;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: FloeSpace.base),
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: FloePalette.error50,
      borderColor: FloePalette.coral100,
      padding: const EdgeInsets.symmetric(
        horizontal: FloeSpace.base,
        vertical: FloeSpace.sm,
      ),
      child: Row(
        children: [
          const Icon(Icons.error_outline, color: FloePalette.error600),
          const SizedBox(width: FloeSpace.md),
          Expanded(child: Text(message)),
          TextButton(onPressed: dismiss, child: const Text('닫기')),
        ],
      ),
    ),
  );
}

enum _Kind { event, task, note }

class _ClassificationDialog extends StatefulWidget {
  const _ClassificationDialog({
    required this.capture,
    required this.now,
    required this.classify,
  });
  final CaptureReceipt capture;
  final DateTime now;
  final Future<bool> Function(ClassificationDraft) classify;
  @override
  State<_ClassificationDialog> createState() => _ClassificationDialogState();
}

class _ClassificationDialogState extends State<_ClassificationDialog> {
  _Kind? kind;
  late final TextEditingController text = TextEditingController(
    text: widget.capture.originalInput,
  );
  bool pending = false;
  late TimeOfDay startTime = TimeOfDay.fromDateTime(
    widget.now.add(const Duration(hours: 1)),
  );
  late TimeOfDay endTime = TimeOfDay.fromDateTime(
    widget.now.add(const Duration(hours: 2)),
  );
  DateTime? taskDeadline;
  String? validationError;

  @override
  void dispose() {
    text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Material(
    color: Colors.transparent,
    child: FloeSquircle(
      size: FloeSquircleSize.xl,
      padding: const EdgeInsets.all(FloeSpace.xl),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('어디에 담을까요?', style: FloeType.headlineLarge),
              const SizedBox(height: FloeSpace.lg),
              const Text('입력 원문', style: FloeType.label),
              const SizedBox(height: FloeSpace.xs),
              Text(widget.capture.originalInput, style: FloeType.bodyLarge),
              const SizedBox(height: FloeSpace.lg),
              SegmentedButton<_Kind>(
                segments: const [
                  ButtonSegment(value: _Kind.event, label: Text('일정')),
                  ButtonSegment(value: _Kind.task, label: Text('할 일')),
                  ButtonSegment(value: _Kind.note, label: Text('생각')),
                ],
                selected: kind == null ? {} : {kind!},
                emptySelectionAllowed: true,
                onSelectionChanged: pending
                    ? null
                    : (value) => setState(() {
                        kind = value.isEmpty ? null : value.first;
                        validationError = null;
                      }),
              ),
              const SizedBox(height: FloeSpace.base),
              TextField(
                controller: text,
                decoration: InputDecoration(
                  labelText: kind == _Kind.event ? '제목' : '내용',
                  errorText: validationError,
                ),
              ),
              const SizedBox(height: FloeSpace.md),
              FloeAnimatedSwap(
                child: switch (kind) {
                  _Kind.event => _EventFields(
                    key: const ValueKey('event-fields'),
                    start: startTime,
                    end: endTime,
                    onStart: (value) => setState(() => startTime = value),
                    onEnd: (value) => setState(() => endTime = value),
                  ),
                  _Kind.task => _TaskFields(
                    key: const ValueKey('task-fields'),
                    deadline: taskDeadline,
                    onChanged: (value) => setState(() => taskDeadline = value),
                    baseDate: widget.now,
                  ),
                  _Kind.note ||
                  null => const SizedBox.shrink(key: ValueKey('note-fields')),
                },
              ),
              const SizedBox(height: FloeSpace.lg),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: pending
                        ? null
                        : () => Navigator.pop(context, false),
                    child: const Text('나중에'),
                  ),
                  const SizedBox(width: FloeSpace.sm),
                  FilledButton(
                    onPressed: pending || kind == null ? null : _submit,
                    child: const Text('분류하여 추가'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    ),
  );

  Future<void> _submit() async {
    final value = text.text.trim();
    if (value.isEmpty) {
      setState(() => validationError = '내용을 입력해 주세요.');
      return;
    }
    final start = _onDate(widget.now, startTime);
    final end = _onDate(widget.now, endTime);
    if (kind == _Kind.event && !end.isAfter(start)) {
      setState(() => validationError = '종료 시간은 시작 시간보다 늦어야 해요.');
      return;
    }
    setState(() {
      pending = true;
      validationError = null;
    });
    final draft = switch (kind!) {
      _Kind.event => EventDraft(title: value, startsAt: start, endsAt: end),
      _Kind.task => TaskDraft(title: value, deadline: taskDeadline),
      _Kind.note => NoteDraft(content: value),
    };
    final success = await widget.classify(draft);
    if (mounted && success) Navigator.pop(context, true);
    if (mounted) setState(() => pending = false);
  }
}

class _EventFields extends StatelessWidget {
  const _EventFields({
    required this.start,
    required this.end,
    required this.onStart,
    required this.onEnd,
    super.key,
  });
  final TimeOfDay start;
  final TimeOfDay end;
  final ValueChanged<TimeOfDay> onStart;
  final ValueChanged<TimeOfDay> onEnd;
  @override
  Widget build(BuildContext context) => Row(
    children: [
      Expanded(
        child: _TimeField(label: '시작', value: start, onChanged: onStart),
      ),
      const SizedBox(width: FloeSpace.md),
      Expanded(
        child: _TimeField(label: '종료', value: end, onChanged: onEnd),
      ),
    ],
  );
}

class _TimeField extends StatelessWidget {
  const _TimeField({
    required this.label,
    required this.value,
    required this.onChanged,
  });
  final String label;
  final TimeOfDay value;
  final ValueChanged<TimeOfDay> onChanged;
  @override
  Widget build(BuildContext context) => OutlinedButton(
    onPressed: () async {
      final selected = await showTimePicker(
        context: context,
        initialTime: value,
      );
      if (selected != null) onChanged(selected);
    },
    child: Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [Text(label), Text(value.format(context))],
    ),
  );
}

class _TaskFields extends StatelessWidget {
  const _TaskFields({
    required this.deadline,
    required this.onChanged,
    required this.baseDate,
    super.key,
  });
  final DateTime? deadline;
  final ValueChanged<DateTime?> onChanged;
  final DateTime baseDate;
  @override
  Widget build(BuildContext context) => Row(
    children: [
      Checkbox(
        value: deadline != null,
        onChanged: (checked) => checked == true
            ? onChanged(
                DateTime(baseDate.year, baseDate.month, baseDate.day, 18),
              )
            : onChanged(null),
      ),
      const Text('마감 설정'),
      const Spacer(),
      if (deadline != null)
        TextButton(
          onPressed: () async {
            final selected = await showDatePicker(
              context: context,
              firstDate: DateTime(baseDate.year, baseDate.month, baseDate.day),
              lastDate: DateTime(baseDate.year + 5),
              initialDate: deadline!,
            );
            if (selected != null) {
              onChanged(
                DateTime(selected.year, selected.month, selected.day, 18),
              );
            }
          },
          child: Text('${deadline!.month}월 ${deadline!.day}일 18:00'),
        ),
    ],
  );
}

void _showComingSoon(BuildContext context) {
  ScaffoldMessenger.of(context)
      .showSnackBar(const SnackBar(content: Text('이 화면은 다음 단계에서 연결할게요.')));
}

DateTime _onDate(DateTime date, TimeOfDay time) =>
    DateTime(date.year, date.month, date.day, time.hour, time.minute);

String _time(DateTime value) =>
    '${value.hour.toString().padLeft(2, '0')}:${value.minute.toString().padLeft(2, '0')}';

String _date(DateTime value) =>
    '${value.month}월 ${value.day}일 ${['월', '화', '수', '목', '금', '토', '일'][value.weekday - 1]}요일';
