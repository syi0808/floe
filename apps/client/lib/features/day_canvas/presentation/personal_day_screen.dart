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
        final narrow = constraints.maxWidth < 720;
        return Scaffold(
          bottomNavigationBar: narrow
              ? _MobileDestinations(
                  selected: destination,
                  onSelected: _selectDestination,
                )
              : null,
          body: SafeArea(
            child: Column(
              children: [
                _ApplicationHeader(
                  narrow: narrow,
                  selected: destination,
                  onSelected: _selectDestination,
                ),
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
                          child: _workspace(narrow),
                        ),
                      ),
                    ),
                  ),
                ),
                if (destination == _DestinationView.today &&
                    selectedTaskId == null)
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
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      delete: controller.deleteItem,
      onOpenTask: (task) => setState(() => selectedTaskId = task.id),
    );
    final rail = _ContextRail(
      snapshot: snapshot,
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      delete: controller.deleteItem,
      onOpenTask: (task) => setState(() => selectedTaskId = task.id),
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

class _ApplicationHeader extends StatelessWidget {
  const _ApplicationHeader({
    required this.narrow,
    required this.selected,
    required this.onSelected,
  });
  final bool narrow;
  final _DestinationView selected;
  final ValueChanged<_DestinationView> onSelected;
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
                  _Destination(
                    label: 'Today',
                    selected: selected == _DestinationView.today,
                    onPressed: () => onSelected(_DestinationView.today),
                  ),
                  _Destination(
                    label: 'Tasks',
                    selected: selected == _DestinationView.tasks,
                    onPressed: () => onSelected(_DestinationView.tasks),
                  ),
                  _Destination(
                    label: 'Notes',
                    selected: selected == _DestinationView.notes,
                    onPressed: () => onSelected(_DestinationView.notes),
                  ),
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
  const _Destination({
    required this.label,
    required this.selected,
    required this.onPressed,
  });
  final String label;
  final bool selected;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    button: true,
    child: TextButton(
      onPressed: selected ? null : onPressed,
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
  const _MobileDestinations({required this.selected, required this.onSelected});
  final _DestinationView selected;
  final ValueChanged<_DestinationView> onSelected;
  @override
  Widget build(BuildContext context) => NavigationBar(
    selectedIndex: selected.index,
    height: 68,
    backgroundColor: FloePalette.neutral0,
    indicatorColor: FloePalette.primary100,
    onDestinationSelected: (index) =>
        onSelected(_DestinationView.values[index]),
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
        if (narrow) ...[
          primary,
          const SizedBox(height: FloeSpace.xl),
          rail,
        ] else
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(flex: 7, child: primary),
              const SizedBox(width: FloeSpace.xl),
              SizedBox(width: 320, child: rail),
            ],
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
  const _PrimaryDay({
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
    final timelineItems = snapshot.items;
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
            _Timeline(
              items: timelineItems,
              snapshot: snapshot,
              disabled: disabled,
              complete: complete,
              delete: delete,
              onOpenTask: onOpenTask,
            ),
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

class _Timeline extends StatelessWidget {
  const _Timeline({
    required this.items,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    required this.onOpenTask,
  });

  final List<DayItem> items;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final ValueChanged<TaskItem> onOpenTask;

  @override
  Widget build(BuildContext context) => Column(
    children: [
      for (final (index, item) in items.indexed)
        _TimelineEntry(
          item: item,
          snapshot: snapshot,
          first: index == 0,
          last: index == items.length - 1,
          disabled: disabled,
          complete: complete,
          delete: delete,
          onOpenTask: onOpenTask,
        ),
    ],
  );
}

class _TimelineEntry extends StatelessWidget {
  const _TimelineEntry({
    required this.item,
    required this.snapshot,
    required this.first,
    required this.last,
    required this.disabled,
    required this.complete,
    required this.delete,
    required this.onOpenTask,
  });

  final DayItem item;
  final DaySnapshot snapshot;
  final bool first;
  final bool last;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final ValueChanged<TaskItem> onOpenTask;

  @override
  Widget build(BuildContext context) {
    final task = item is TaskItem ? item as TaskItem : null;
    final time = switch (item) {
      EventItem(:final startsAt) => _time(startsAt),
      TaskItem(:final deadline) => deadline == null ? '—' : _time(deadline),
      NoteItem() => '노트',
    };
    final meta = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        '${_time(startsAt)}–${_time(endsAt)} · 일정',
      TaskItem(:final deadline) =>
        deadline == null ? '시간 미정 · 할 일' : '마감 ${_time(deadline)} · 할 일',
      NoteItem() => '오늘의 생각 · 노트',
    };
    final accent = switch (item) {
      EventItem() => FloePalette.blue500,
      TaskItem() => FloePalette.primary500,
      NoteItem() => FloePalette.mint700,
    };
    final content = Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: FloeSpace.base,
        vertical: FloeSpace.md,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (task != null)
            SizedBox.square(
              dimension: 44,
              child: Checkbox(
                value: task.isCompleted,
                onChanged: disabled
                    ? null
                    : (value) => complete(task, value ?? false),
              ),
            )
          else
            Padding(
              padding: const EdgeInsets.only(
                top: FloeSpace.xs,
                right: FloeSpace.md,
              ),
              child: Icon(
                item is EventItem
                    ? Icons.calendar_today_outlined
                    : Icons.notes_outlined,
                size: 18,
                color: accent,
              ),
            ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  item.title,
                  style: FloeType.bodyLarge.copyWith(
                    decoration: task?.isCompleted == true
                        ? TextDecoration.lineThrough
                        : null,
                  ),
                ),
                const SizedBox(height: FloeSpace.xs),
                Text(meta, style: FloeType.numeric),
              ],
            ),
          ),
          IconButton(
            tooltip: '${item.title} 삭제',
            onPressed: disabled ? null : () => _confirmTimelineDelete(context),
            icon: const Icon(Icons.more_horiz, size: 20),
          ),
        ],
      ),
    );
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            width: 60,
            child: Padding(
              padding: const EdgeInsets.only(top: FloeSpace.base),
              child: Text(
                time,
                style: FloeType.numeric,
                textAlign: TextAlign.right,
              ),
            ),
          ),
          const SizedBox(width: FloeSpace.md),
          SizedBox(
            width: 12,
            child: Stack(
              alignment: Alignment.topCenter,
              children: [
                Positioned.fill(
                  top: first ? 20 : 0,
                  bottom: last ? 20 : 0,
                  child: const Center(
                    child: VerticalDivider(width: 1, thickness: 1),
                  ),
                ),
                Positioned(
                  top: 20,
                  child: Container(
                    width: 10,
                    height: 10,
                    decoration: BoxDecoration(
                      color: FloePalette.neutral0,
                      border: Border.all(color: accent, width: 2),
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: FloeSpace.md),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(bottom: FloeSpace.sm),
              child: item is EventItem
                  ? FloeSquircle(
                      size: FloeSquircleSize.md,
                      fill: FloePalette.blue50,
                      borderColor: FloePalette.blue100,
                      child: content,
                    )
                  : InkWell(
                      onTap: task == null ? null : () => onOpenTask(task),
                      customBorder: floeSquircleBorder(FloeSquircleSize.md),
                      child: content,
                    ),
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _confirmTimelineDelete(BuildContext context) async {
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
