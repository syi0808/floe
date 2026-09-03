import 'package:flutter/material.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_mascot.dart';
import '../../../app/floe_motion.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_theme.dart';
import '../application/day_gateway.dart';
import '../application/personal_day_controller.dart';
import '../domain/day_models.dart';

enum _DestinationView { today, tasks, notes }

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
  _DestinationView destination = _DestinationView.today;
  String? selectedTaskId;

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
        final narrow = constraints.maxWidth < 780;
        return Scaffold(
          body: SafeArea(
            minimum: EdgeInsets.all(narrow ? 0 : FloeSpace.base),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 1500),
                child: FloeSquircle(
                  size: FloeSquircleSize.frame,
                  fill: FloePalette.neutral25,
                  child: Stack(
                    children: [
                      Positioned.fill(
                        child: Column(
                          children: [
                            Expanded(
                              child: SingleChildScrollView(
                                padding: EdgeInsets.fromLTRB(
                                  narrow ? FloeSpace.md : 100,
                                  narrow ? 64 : 72,
                                  narrow ? FloeSpace.md : 36,
                                  FloeSpace.lg,
                                ),
                                child: Center(
                                  child: ConstrainedBox(
                                    constraints: const BoxConstraints(
                                      maxWidth: 1240,
                                    ),
                                    child: _workspace(narrow),
                                  ),
                                ),
                              ),
                            ),
                            if (destination == _DestinationView.today &&
                                selectedTaskId == null)
                              Padding(
                                padding: EdgeInsets.fromLTRB(
                                  narrow ? FloeSpace.md : 100,
                                  FloeSpace.sm,
                                  narrow ? FloeSpace.md : 36,
                                  narrow ? 88 : FloeSpace.base,
                                ),
                                child: Center(
                                  child: ConstrainedBox(
                                    constraints: const BoxConstraints(
                                      maxWidth: 1240,
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
                      _AdaptiveNavigation(
                        narrow: narrow,
                        selected: destination,
                        onSelected: _selectDestination,
                      ),
                      Positioned(
                        top: narrow ? 8 : 22,
                        right: narrow ? 8 : 24,
                        child: IconButton(
                          tooltip: '설정',
                          style: const ButtonStyle(
                            backgroundColor: WidgetStatePropertyAll(
                              Colors.transparent,
                            ),
                            overlayColor: WidgetStatePropertyAll(
                              Colors.transparent,
                            ),
                          ),
                          onPressed: () => _showComingSoon(context),
                          icon: const Icon(Icons.settings_outlined),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
      },
    ),
  );

  Widget _workspace(bool narrow) {
    if (controller.loadState == DayLoadState.loading) {
      return const _LoadingDay();
    }
    if (controller.loadState == DayLoadState.failure) {
      return _FailureDay(retry: controller.load);
    }
    final snapshot = controller.snapshot!;
    final selectedTask = _taskById(snapshot, selectedTaskId);
    if (selectedTask != null) {
      return _TaskDetailScreen(
        task: selectedTask,
        snapshot: snapshot,
        narrow: narrow,
        onBack: () => setState(() => selectedTaskId = null),
        onComplete: _setTaskCompleted,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (controller.errorMessage case final message?)
          _ErrorNotice(message: message, dismiss: controller.clearError),
        switch (destination) {
          _DestinationView.today => Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _DayToolbar(controller, narrow: narrow),
              const SizedBox(height: FloeSpace.lg),
              _content(narrow, snapshot),
            ],
          ),
          _DestinationView.tasks => _TasksScreen(
            snapshot: snapshot,
            disabled: controller.commandPending,
            onComplete: _setTaskCompleted,
            onOpen: (task) => setState(() => selectedTaskId = task.id),
            onDelete: controller.deleteItem,
          ),
          _DestinationView.notes => _NotesScreen(
            notes: snapshot.items.whereType<NoteItem>().toList(),
            narrow: narrow,
          ),
        },
      ],
    );
  }

  Widget _content(bool narrow, DaySnapshot snapshot) {
    final primary = _PrimaryDay(
      snapshot: snapshot,
      onOpenTask: (task) => setState(() => selectedTaskId = task.id),
    );
    final rail = _ContextRail(
      snapshot: snapshot,
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      delete: controller.deleteItem,
      onOpenTask: (task) => setState(() => selectedTaskId = task.id),
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        if (narrow || constraints.maxWidth < 900) {
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
      },
    );
  }

  void _selectDestination(_DestinationView value) {
    setState(() {
      destination = value;
      selectedTaskId = null;
    });
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

class _AdaptiveNavigation extends StatelessWidget {
  const _AdaptiveNavigation({
    required this.narrow,
    required this.selected,
    required this.onSelected,
  });
  final bool narrow;
  final _DestinationView selected;
  final ValueChanged<_DestinationView> onSelected;

  @override
  Widget build(BuildContext context) {
    if (narrow) {
      return Positioned(
        right: FloeSpace.md,
        bottom: FloeSpace.sm,
        left: FloeSpace.md,
        child: FloeSquircle(
          size: FloeSquircleSize.xl,
          elevation: 4,
          padding: const EdgeInsets.all(FloeSpace.xs),
          child: Row(
            children: [
              for (final view in _DestinationView.values)
                Expanded(
                  child: _DestinationButton(
                    view: view,
                    selected: selected == view,
                    onPressed: () => onSelected(view),
                  ),
                ),
            ],
          ),
        ),
      );
    }
    return Positioned(
      top: 22,
      bottom: 22,
      left: 14,
      width: 68,
      child: Column(
        children: [
          const SizedBox(height: 4),
          const FloeMascot(size: 40),
          const SizedBox(height: FloeSpace.lg),
          for (final view in _DestinationView.values) ...[
            _DestinationButton(
              view: view,
              selected: selected == view,
              onPressed: () => onSelected(view),
            ),
            const SizedBox(height: FloeSpace.sm),
          ],
        ],
      ),
    );
  }
}

class _DestinationButton extends StatelessWidget {
  const _DestinationButton({
    required this.view,
    required this.selected,
    required this.onPressed,
  });

  final _DestinationView view;
  final bool selected;
  final VoidCallback onPressed;

  String get label => switch (view) {
    _DestinationView.today => 'Today',
    _DestinationView.tasks => 'Tasks',
    _DestinationView.notes => 'Notes',
  };

  IconData get icon => switch (view) {
    _DestinationView.today => Icons.calendar_today_outlined,
    _DestinationView.tasks => Icons.checklist_rounded,
    _DestinationView.notes => Icons.edit_note_outlined,
  };

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: FloeSquircle(
      size: FloeSquircleSize.md,
      fill: selected ? FloePalette.primary100 : Colors.transparent,
      borderWidth: 0,
      child: InkWell(
        onTap: selected ? null : onPressed,
        customBorder: floeSquircleBorder(FloeSquircleSize.md),
        child: SizedBox(
          height: 58,
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(
                icon,
                size: 19,
                color: selected
                    ? FloePalette.primary700
                    : FloePalette.neutral600,
              ),
              const SizedBox(height: FloeSpace.xs),
              Text(
                label,
                style: FloeType.label.copyWith(
                  fontSize: 10,
                  color: selected
                      ? FloePalette.primary700
                      : FloePalette.neutral600,
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

class _DayToolbar extends StatelessWidget {
  const _DayToolbar(this.controller, {required this.narrow});
  final PersonalDayController controller;
  final bool narrow;
  @override
  Widget build(BuildContext context) {
    final leading = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        IconButton(
          tooltip: '이전 날',
          onPressed: () => controller.moveDay(-1),
          icon: const Icon(Icons.chevron_left),
        ),
        const SizedBox(width: FloeSpace.xs),
        IconButton(
          tooltip: '다음 날',
          onPressed: () => controller.moveDay(1),
          icon: const Icon(Icons.chevron_right),
        ),
        const SizedBox(width: FloeSpace.md),
        Flexible(
          child: Text(
            _date(controller.query.date),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: FloeType.headline,
          ),
        ),
        const SizedBox(width: FloeSpace.sm),
        TextButton(onPressed: controller.goToday, child: const Text('오늘')),
      ],
    );
    final views = _CalendarViewSelector(
      onUnavailable: () => _showComingSoon(context),
    );
    if (narrow) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          leading,
          const SizedBox(height: FloeSpace.md),
          views,
        ],
      );
    }
    return Row(
      children: [
        Expanded(child: leading),
        const SizedBox(width: FloeSpace.lg),
        SizedBox(width: 300, child: views),
      ],
    );
  }
}

class _CalendarViewSelector extends StatelessWidget {
  const _CalendarViewSelector({required this.onUnavailable});

  final VoidCallback onUnavailable;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.md,
    padding: const EdgeInsets.all(FloeSpace.xs),
    child: Row(
      children: [
        for (final (index, label) in ['일', '주', '월'].indexed)
          Expanded(
            child: FloeSquircle(
              size: FloeSquircleSize.sm,
              fill: index == 0 ? FloePalette.primary100 : Colors.transparent,
              borderWidth: 0,
              child: TextButton(
                onPressed: index == 0 ? null : onUnavailable,
                style: TextButton.styleFrom(
                  foregroundColor: index == 0
                      ? FloePalette.primary700
                      : FloePalette.neutral600,
                  disabledForegroundColor: FloePalette.primary700,
                  backgroundColor: Colors.transparent,
                  minimumSize: const Size(44, 40),
                ),
                child: Text(label),
              ),
            ),
          ),
      ],
    ),
  );
}

class _TasksScreen extends StatelessWidget {
  const _TasksScreen({
    required this.snapshot,
    required this.disabled,
    required this.onComplete,
    required this.onOpen,
    required this.onDelete,
  });

  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) onComplete;
  final ValueChanged<TaskItem> onOpen;
  final Future<void> Function(DayItem) onDelete;

  @override
  Widget build(BuildContext context) {
    final tasks = snapshot.items.whereType<TaskItem>().toList();
    final remaining = tasks.where((task) => !task.isCompleted).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('할 일', style: FloeType.display),
                  const SizedBox(height: FloeSpace.xs),
                  Text(
                    '남은 할 일 $remaining개 · 전체 ${tasks.length}개',
                    style: FloeType.body,
                  ),
                ],
              ),
            ),
            FilledButton.icon(
              onPressed: () => _showComingSoon(context),
              icon: const Icon(Icons.add),
              label: const Text('새 할 일'),
            ),
          ],
        ),
        const SizedBox(height: FloeSpace.xl),
        FloeSquircle(
          padding: const EdgeInsets.symmetric(
            horizontal: FloeSpace.lg,
            vertical: FloeSpace.sm,
          ),
          child: tasks.isEmpty
              ? const Padding(
                  padding: EdgeInsets.symmetric(vertical: FloeSpace.xxxl),
                  child: Center(
                    child: Text('아직 할 일이 없어요.', style: FloeType.body),
                  ),
                )
              : Column(
                  children: [
                    for (final (index, task) in tasks.indexed) ...[
                      _DayRow(
                        item: task,
                        snapshot: snapshot,
                        disabled: disabled,
                        complete: onComplete,
                        delete: onDelete,
                        onOpen: () => onOpen(task),
                      ),
                      if (index < tasks.length - 1)
                        const Divider(height: 1, indent: 56),
                    ],
                  ],
                ),
        ),
      ],
    );
  }
}

class _NotesScreen extends StatefulWidget {
  const _NotesScreen({required this.notes, required this.narrow});

  final List<NoteItem> notes;
  final bool narrow;

  @override
  State<_NotesScreen> createState() => _NotesScreenState();
}

class _NotesScreenState extends State<_NotesScreen> {
  final search = TextEditingController();

  @override
  void dispose() {
    search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = search.text.trim().toLowerCase();
    final notes = query.isEmpty
        ? widget.notes
        : widget.notes
              .where((note) => note.title.toLowerCase().contains(query))
              .toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          spacing: FloeSpace.base,
          runSpacing: FloeSpace.base,
          alignment: WrapAlignment.spaceBetween,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('모든 노트', style: FloeType.display),
                const SizedBox(height: FloeSpace.xs),
                Text('${widget.notes.length}개의 기록', style: FloeType.body),
              ],
            ),
            FilledButton.icon(
              onPressed: () => _showComingSoon(context),
              icon: const Icon(Icons.add),
              label: const Text('새 노트'),
            ),
          ],
        ),
        const SizedBox(height: FloeSpace.lg),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: search,
                onChanged: (_) => setState(() {}),
                decoration: const InputDecoration(
                  labelText: '노트 검색',
                  prefixIcon: Icon(Icons.search),
                ),
              ),
            ),
            if (!widget.narrow) ...[
              const SizedBox(width: FloeSpace.md),
              OutlinedButton.icon(
                onPressed: () => _showComingSoon(context),
                icon: const Icon(Icons.tune),
                label: const Text('필터'),
              ),
              const SizedBox(width: FloeSpace.sm),
              OutlinedButton.icon(
                onPressed: () => _showComingSoon(context),
                icon: const Icon(Icons.swap_vert),
                label: const Text('최근 수정순'),
              ),
            ],
          ],
        ),
        const SizedBox(height: FloeSpace.xl),
        if (notes.isEmpty)
          FloeSquircle(
            fill: FloePalette.neutral50,
            padding: const EdgeInsets.symmetric(vertical: FloeSpace.xxxl),
            child: Center(
              child: Text(
                query.isEmpty ? '첫 노트를 남겨보세요.' : '검색 결과가 없어요.',
                style: FloeType.body,
              ),
            ),
          )
        else
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = widget.narrow
                  ? 1
                  : constraints.maxWidth >= 980
                  ? 3
                  : 2;
              final width =
                  (constraints.maxWidth - FloeSpace.base * (columns - 1)) /
                  columns;
              return Wrap(
                spacing: FloeSpace.base,
                runSpacing: FloeSpace.base,
                children: [
                  for (final note in notes)
                    SizedBox(
                      width: width,
                      child: _NotePreviewCard(
                        note: note,
                        onOpen: () => _openNote(context, note, widget.narrow),
                      ),
                    ),
                ],
              );
            },
          ),
      ],
    );
  }
}

class _NotePreviewCard extends StatelessWidget {
  const _NotePreviewCard({required this.note, required this.onOpen});

  final NoteItem note;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    fill: FloePalette.neutral0,
    child: InkWell(
      onTap: onOpen,
      customBorder: floeSquircleBorder(FloeSquircleSize.lg),
      child: Padding(
        padding: const EdgeInsets.all(FloeSpace.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Row(
              children: [
                Icon(
                  Icons.notes_outlined,
                  size: 18,
                  color: FloePalette.mint700,
                ),
                SizedBox(width: FloeSpace.sm),
                Text('개인 노트', style: FloeType.label),
              ],
            ),
            const SizedBox(height: FloeSpace.base),
            Text(
              note.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: FloeType.headline,
            ),
            const SizedBox(height: FloeSpace.sm),
            Text(
              '오늘 기록한 내용이에요. 열어서 원문과 연결된 맥락을 확인하세요.',
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
              style: FloeType.body,
            ),
            const SizedBox(height: FloeSpace.lg),
            Text(_date(note.createdAt), style: FloeType.numeric),
          ],
        ),
      ),
    ),
  );
}

class _TaskDetailScreen extends StatelessWidget {
  const _TaskDetailScreen({
    required this.task,
    required this.snapshot,
    required this.narrow,
    required this.onBack,
    required this.onComplete,
  });

  final TaskItem task;
  final DaySnapshot snapshot;
  final bool narrow;
  final VoidCallback onBack;
  final Future<void> Function(TaskItem, bool) onComplete;

  @override
  Widget build(BuildContext context) {
    final primary = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          '할 일',
          style: FloeType.label.copyWith(color: FloePalette.primary700),
        ),
        const SizedBox(height: FloeSpace.sm),
        Text(
          task.title,
          style: narrow ? FloeType.headlineLarge : FloeType.display,
        ),
        const SizedBox(height: FloeSpace.lg),
        Row(
          children: [
            Checkbox(
              value: task.isCompleted,
              onChanged: (value) => onComplete(task, value ?? false),
            ),
            Text(
              task.isCompleted ? '완료됨' : '완료로 표시',
              style: FloeType.bodyLarge,
            ),
          ],
        ),
        const SizedBox(height: FloeSpace.xl),
        const Divider(),
        const SizedBox(height: FloeSpace.lg),
        const Text('세부 정보', style: FloeType.headline),
        const SizedBox(height: FloeSpace.base),
        const _LabeledValue(label: '설명', value: '아직 설명이 없어요.'),
        _LabeledValue(
          label: '마감',
          value: task.deadline == null ? '마감 없음' : _date(task.deadline!),
        ),
        const _LabeledValue(label: '시간 맥락', value: '시간이 지정되지 않았어요.'),
        const _LabeledValue(label: '출처', value: 'Floe 로컬 캘린더'),
        const SizedBox(height: FloeSpace.xl),
        Row(
          children: [
            const Expanded(child: Text('하위 작업', style: FloeType.headline)),
            Text('0개', style: FloeType.numeric),
          ],
        ),
        const SizedBox(height: FloeSpace.md),
        const Text('아직 하위 작업이 없어요.', style: FloeType.body),
        const SizedBox(height: FloeSpace.sm),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            onPressed: () => _showComingSoon(context),
            icon: const Icon(Icons.add),
            label: const Text('하위 작업 추가'),
          ),
        ),
      ],
    );
    final relatedNotes = snapshot.items.whereType<NoteItem>().toList();
    final rail = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _FloeSuggestion(task: task),
        const SizedBox(height: FloeSpace.base),
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('관련 노트', style: FloeType.headline),
              const SizedBox(height: FloeSpace.md),
              if (relatedNotes.isEmpty)
                const Text('연결된 노트가 없어요.', style: FloeType.body)
              else
                for (final note in relatedNotes.take(2))
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    leading: const Icon(
                      Icons.notes_outlined,
                      color: FloePalette.mint700,
                    ),
                    title: Text(note.title),
                    subtitle: const Text('오늘의 맥락'),
                  ),
            ],
          ),
        ),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            TextButton.icon(
              onPressed: onBack,
              icon: const Icon(Icons.arrow_back),
              label: const Text('오늘로 돌아가기'),
            ),
            const Spacer(),
            IconButton(
              tooltip: '할 일 메뉴',
              onPressed: () => _showComingSoon(context),
              icon: const Icon(Icons.more_horiz),
            ),
          ],
        ),
        const SizedBox(height: FloeSpace.lg),
        LayoutBuilder(
          builder: (context, constraints) {
            final primaryPanel = FloeSquircle(
              size: FloeSquircleSize.lg,
              padding: EdgeInsets.all(narrow ? FloeSpace.lg : FloeSpace.xl),
              child: primary,
            );
            if (narrow || constraints.maxWidth < 900) {
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  primaryPanel,
                  const SizedBox(height: FloeSpace.xl),
                  rail,
                ],
              );
            }
            return Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(flex: 7, child: primaryPanel),
                const SizedBox(width: FloeSpace.xl),
                SizedBox(width: 320, child: rail),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _LabeledValue extends StatelessWidget {
  const _LabeledValue({required this.label, required this.value});
  final String label;
  final String value;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: FloeSpace.md),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(width: 104, child: Text(label, style: FloeType.label)),
        Expanded(child: Text(value, style: FloeType.bodyLarge)),
      ],
    ),
  );
}

class _FloeSuggestion extends StatelessWidget {
  const _FloeSuggestion({required this.task});
  final TaskItem task;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    fill: FloePalette.primary50,
    borderColor: FloePalette.primary100,
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Row(
          children: [
            FloeMascot(size: 32),
            SizedBox(width: FloeSpace.md),
            Text('Floe가 제안해요', style: FloeType.label),
          ],
        ),
        const SizedBox(height: FloeSpace.md),
        Text(
          '“${task.title}”을 시작하기 전에 필요한 맥락을 검토할까요?',
          style: FloeType.bodyLarge,
        ),
        const SizedBox(height: FloeSpace.base),
        FilledButton(
          onPressed: () => _showSuggestionProposal(context, task),
          child: const Text('계획 검토'),
        ),
        TextButton(onPressed: () {}, child: const Text('지금은 괜찮아요')),
      ],
    ),
  );
}

Future<void> _openNote(BuildContext context, NoteItem note, bool narrow) async {
  final detail = _NoteDetail(note: note);
  if (narrow) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: Colors.transparent,
      builder: (context) => detail,
    );
  } else {
    await showDialog<void>(
      context: context,
      builder: (context) => Dialog(child: detail),
    );
  }
}

class _NoteDetail extends StatelessWidget {
  const _NoteDetail({required this.note});
  final NoteItem note;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.xl,
    padding: const EdgeInsets.all(FloeSpace.xl),
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 640),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Text('개인 노트', style: FloeType.label),
              const Spacer(),
              IconButton(
                tooltip: '닫기',
                onPressed: () => Navigator.pop(context),
                icon: const Icon(Icons.close),
              ),
            ],
          ),
          const SizedBox(height: FloeSpace.base),
          Text(note.title, style: FloeType.display),
          const SizedBox(height: FloeSpace.sm),
          Text(_date(note.createdAt), style: FloeType.numeric),
          const SizedBox(height: FloeSpace.xl),
          const Text(
            '사용자가 남긴 원문입니다. 편집 기능은 다음 단계에서 연결됩니다.',
            style: FloeType.bodyLarge,
          ),
          const SizedBox(height: FloeSpace.xl),
          OutlinedButton.icon(
            onPressed: () => _showComingSoon(context),
            icon: const FloeMascot(size: 24),
            label: const Text('Floe와 함께 보기'),
          ),
        ],
      ),
    ),
  );
}

Future<void> _showSuggestionProposal(
  BuildContext context,
  TaskItem task,
) async {
  await showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('계획을 검토할까요?'),
      content: Text('“${task.title}”의 관련 일정과 노트를 확인합니다. 아직 어떤 항목도 변경하지 않아요.'),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('취소'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('검토 시작'),
        ),
      ],
    ),
  );
}

class _PrimaryDay extends StatelessWidget {
  const _PrimaryDay({required this.snapshot, required this.onOpenTask});
  final DaySnapshot snapshot;
  final ValueChanged<TaskItem> onOpenTask;
  @override
  Widget build(BuildContext context) {
    if (snapshot.items.isEmpty) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _NowNext(snapshot),
          const SizedBox(height: FloeSpace.lg),
          const _EmptyState(),
        ],
      );
    }
    return _Timeline(
      items: snapshot.items,
      snapshot: snapshot,
      onOpenTask: onOpenTask,
    );
  }
}

class _ContextRail extends StatelessWidget {
  const _ContextRail({
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    required this.onOpenTask,
  });
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final ValueChanged<TaskItem> onOpenTask;
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
                    onOpen: () => onOpenTask(task),
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
      ],
    );
  }
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

class _Timeline extends StatelessWidget {
  const _Timeline({
    required this.items,
    required this.snapshot,
    required this.onOpenTask,
  });

  static const firstHour = 8;
  static const lastHour = 19;
  static const hourExtent = 64.0;

  final List<DayItem> items;
  final DaySnapshot snapshot;
  final ValueChanged<TaskItem> onOpenTask;

  @override
  Widget build(BuildContext context) {
    final allDayEvents = items
        .whereType<EventItem>()
        .where((event) => event.isAllDay)
        .toList();
    final scheduled = items.where((item) => _startOf(item) != null).toList()
      ..sort((left, right) => _startOf(left)!.compareTo(_startOf(right)!));
    final suggestionItem = _suggestionItem(scheduled);
    const stageHeight = (lastHour - firstHour) * hourExtent;
    return FloeSquircle(
      size: FloeSquircleSize.lg,
      clipBehavior: Clip.none,
      child: Column(
        children: [
          _AllDayStrip(events: allDayEvents),
          SizedBox(
            height: stageHeight,
            child: LayoutBuilder(
              builder: (context, constraints) => Stack(
                clipBehavior: Clip.none,
                children: [
                  for (var hour = firstHour; hour <= lastHour; hour++)
                    _HourGuide(
                      hour: hour,
                      top: (hour - firstHour) * hourExtent,
                    ),
                  if (scheduled.isEmpty)
                    const Positioned(
                      top: 108,
                      right: FloeSpace.lg,
                      left: 76,
                      child: Text(
                        '시간이 지정된 일정이 없어요.',
                        style: FloeType.body,
                        textAlign: TextAlign.center,
                      ),
                    ),
                  for (final item in scheduled)
                    _TimelineBlock(
                      item: item,
                      top: _topFor(item),
                      height: _heightFor(item),
                      showSuggestion: identical(item, suggestionItem),
                      onOpenTask: onOpenTask,
                    ),
                  if (_showsCurrentTime)
                    _TimelineNowMarker(
                      now: snapshot.generatedAt,
                      top:
                          _minutesFromStart(snapshot.generatedAt) /
                          60 *
                          hourExtent,
                    ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  bool get _showsCurrentTime {
    final now = snapshot.generatedAt;
    final date = snapshot.date;
    return now.year == date.year &&
        now.month == date.month &&
        now.day == date.day &&
        now.hour >= firstHour &&
        now.hour < lastHour;
  }

  DayItem? _suggestionItem(List<DayItem> scheduled) {
    final now = snapshot.generatedAt;
    for (final event in scheduled.whereType<EventItem>()) {
      if (!event.startsAt.isAfter(now) && event.endsAt.isAfter(now)) {
        return event;
      }
    }
    for (final item in scheduled) {
      if (item.id == snapshot.nowEventId || item.id == snapshot.nextEventId) {
        return item;
      }
    }
    return scheduled.whereType<EventItem>().firstOrNull;
  }

  static DateTime? _startOf(DayItem item) => switch (item) {
    EventItem(:final startsAt, :final isAllDay) => isAllDay ? null : startsAt,
    TaskItem(:final deadline) => deadline,
    NoteItem() => null,
  };

  static int _minutesFromStart(DateTime value) =>
      value.hour * 60 + value.minute - firstHour * 60;

  static double _topFor(DayItem item) {
    final minutes = _minutesFromStart(_startOf(item)!);
    return (minutes / 60 * hourExtent).clamp(
      4,
      (lastHour - firstHour) * hourExtent - 44,
    );
  }

  static double _heightFor(DayItem item) {
    final minutes = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        endsAt.difference(startsAt).inMinutes,
      TaskItem() => 45,
      NoteItem() => 45,
    };
    return (minutes / 60 * hourExtent - 8).clamp(42, 88);
  }
}

extension<T> on Iterable<T> {
  T? get firstOrNull {
    final iterator = this.iterator;
    return iterator.moveNext() ? iterator.current : null;
  }
}

class _AllDayStrip extends StatelessWidget {
  const _AllDayStrip({required this.events});

  final List<EventItem> events;

  @override
  Widget build(BuildContext context) => Container(
    constraints: const BoxConstraints(minHeight: 50),
    padding: const EdgeInsets.symmetric(
      horizontal: FloeSpace.base,
      vertical: FloeSpace.sm,
    ),
    decoration: const BoxDecoration(
      border: Border(bottom: BorderSide(color: FloePalette.neutral200)),
    ),
    child: Row(
      children: [
        const SizedBox(
          width: 68,
          child: Text('하루 종일', style: FloeType.numeric),
        ),
        Expanded(
          child: events.isEmpty
              ? const SizedBox.shrink()
              : FloeSquircle(
                  size: FloeSquircleSize.md,
                  fill: FloePalette.mint50,
                  borderColor: FloePalette.mint100,
                  padding: const EdgeInsets.symmetric(
                    horizontal: FloeSpace.md,
                    vertical: FloeSpace.sm,
                  ),
                  child: Row(
                    children: [
                      const _ToneDot(color: FloePalette.mint700),
                      const SizedBox(width: FloeSpace.sm),
                      Flexible(
                        child: Text(
                          events.first.title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: FloeType.label.copyWith(
                            color: FloePalette.neutral950,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
        ),
      ],
    ),
  );
}

class _HourGuide extends StatelessWidget {
  const _HourGuide({required this.hour, required this.top});

  final int hour;
  final double top;

  @override
  Widget build(BuildContext context) => Positioned(
    top: top,
    right: FloeSpace.md,
    left: FloeSpace.md,
    child: Row(
      children: [
        SizedBox(
          width: 58,
          child: Text(_hourLabel(hour), style: FloeType.numeric),
        ),
        const Expanded(child: Divider(height: 1)),
      ],
    ),
  );
}

class _TimelineBlock extends StatelessWidget {
  const _TimelineBlock({
    required this.item,
    required this.top,
    required this.height,
    required this.showSuggestion,
    required this.onOpenTask,
  });

  final DayItem item;
  final double top;
  final double height;
  final bool showSuggestion;
  final ValueChanged<TaskItem> onOpenTask;

  @override
  Widget build(BuildContext context) {
    final task = item is TaskItem ? item as TaskItem : null;
    final (fill, border, accent) = switch (item) {
      EventItem() => (
        FloePalette.blue50,
        FloePalette.blue100,
        FloePalette.blue500,
      ),
      TaskItem() => (
        FloePalette.primary50,
        FloePalette.primary100,
        FloePalette.primary500,
      ),
      NoteItem() => (
        FloePalette.mint50,
        FloePalette.mint100,
        FloePalette.mint700,
      ),
    };
    final time = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        '${_time(startsAt)} – ${_time(endsAt)}',
      TaskItem(:final deadline) =>
        deadline == null ? '시간 미정' : '마감 ${_time(deadline)}',
      NoteItem() => '노트',
    };
    return Positioned(
      top: top,
      right: 18,
      left: 76,
      height: height,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned.fill(
            child: FloeSquircle(
              size: FloeSquircleSize.md,
              fill: fill,
              borderColor: border,
              child: InkWell(
                onTap: task == null ? null : () => onOpenTask(task),
                customBorder: floeSquircleBorder(FloeSquircleSize.md),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: FloeSpace.md,
                    vertical: FloeSpace.sm,
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.only(top: FloeSpace.xs),
                        child: _ToneDot(color: accent),
                      ),
                      const SizedBox(width: FloeSpace.sm),
                      Expanded(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              item.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: FloeType.label.copyWith(
                                color: FloePalette.neutral950,
                              ),
                            ),
                            Text(time, style: FloeType.numeric),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (showSuggestion)
            Positioned(
              right: -8,
              bottom: -8,
              child: _TimelineSuggestionButton(item: item),
            ),
        ],
      ),
    );
  }
}

class _ToneDot extends StatelessWidget {
  const _ToneDot({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    child: const SizedBox.square(dimension: 9),
  );
}

class _TimelineSuggestionButton extends StatelessWidget {
  const _TimelineSuggestionButton({required this.item});

  final DayItem item;

  @override
  Widget build(BuildContext context) => Tooltip(
    message: 'Floe 제안 열기',
    child: FloeSquircle(
      size: FloeSquircleSize.xl,
      elevation: 4,
      child: InkWell(
        onTap: () => _showTimelineSuggestion(context, item),
        customBorder: floeSquircleBorder(FloeSquircleSize.xl),
        child: const SizedBox.square(
          dimension: 48,
          child: Center(child: FloeMascot(size: 32)),
        ),
      ),
    ),
  );
}

class _TimelineNowMarker extends StatelessWidget {
  const _TimelineNowMarker({required this.now, required this.top});

  final DateTime now;
  final double top;

  @override
  Widget build(BuildContext context) => Positioned(
    top: top,
    right: FloeSpace.md,
    left: FloeSpace.sm,
    child: Semantics(
      label: '현재 시간 ${_time(now)}',
      child: Row(
        children: [
          SizedBox(
            width: 64,
            child: Text(
              _time(now),
              style: FloeType.numeric.copyWith(
                color: FloePalette.primary700,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          const _ToneDot(color: FloePalette.primary600),
          const SizedBox(width: FloeSpace.sm),
          const Expanded(
            child: Divider(color: FloePalette.primary600, thickness: 1.5),
          ),
        ],
      ),
    ),
  );
}

Future<void> _showTimelineSuggestion(BuildContext context, DayItem item) async {
  final panel = _TimelineSuggestionPanel(item: item);
  if (MediaQuery.sizeOf(context).width < 780) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: Colors.transparent,
      builder: (context) => panel,
    );
    return;
  }
  await showDialog<void>(
    context: context,
    barrierColor: FloePalette.neutral950.withValues(alpha: 0.18),
    builder: (context) =>
        Dialog(backgroundColor: Colors.transparent, elevation: 0, child: panel),
  );
}

class _TimelineSuggestionPanel extends StatelessWidget {
  const _TimelineSuggestionPanel({required this.item});

  final DayItem item;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.xl,
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 380),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const FloeMascot(size: 32),
              const SizedBox(width: FloeSpace.sm),
              Text(
                'Floe suggestion',
                style: FloeType.label.copyWith(color: FloePalette.primary700),
              ),
              const Spacer(),
              IconButton(
                tooltip: '닫기',
                onPressed: () => Navigator.pop(context),
                icon: const Icon(Icons.close),
              ),
            ],
          ),
          const SizedBox(height: FloeSpace.base),
          const Text('20분의 여유를 확보할까요?', style: FloeType.headlineLarge),
          const SizedBox(height: FloeSpace.sm),
          Text(
            '“${item.title}” 이후 일정 사이에 짧은 휴식 시간을 제안해요.',
            style: FloeType.body,
          ),
          const SizedBox(height: FloeSpace.base),
          FloeSquircle(
            size: FloeSquircleSize.md,
            padding: const EdgeInsets.symmetric(
              horizontal: FloeSpace.md,
              vertical: FloeSpace.sm,
            ),
            child: const Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.schedule_outlined, size: 18),
                SizedBox(width: FloeSpace.sm),
                Text('20분 휴식', style: FloeType.label),
              ],
            ),
          ),
          const SizedBox(height: FloeSpace.base),
          Row(
            children: [
              Expanded(
                child: FilledButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('휴식 추가'),
                ),
              ),
              const SizedBox(width: FloeSpace.sm),
              Expanded(
                child: OutlinedButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('현재대로'),
                ),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}

String _hourLabel(int hour) {
  final suffix = hour < 12 ? 'AM' : 'PM';
  final normalized = hour % 12 == 0 ? 12 : hour % 12;
  return '$normalized $suffix';
}

class _DayRow extends StatelessWidget {
  const _DayRow({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    this.compact = false,
    this.onOpen,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final bool compact;
  final VoidCallback? onOpen;
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
      onTap: onOpen,
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

TaskItem? _taskById(DaySnapshot snapshot, String? id) {
  if (id == null) return null;
  for (final task in snapshot.items.whereType<TaskItem>()) {
    if (task.id == id) return task;
  }
  return null;
}
