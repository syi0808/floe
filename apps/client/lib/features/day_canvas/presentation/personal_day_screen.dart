import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_mascot.dart';
import '../../../app/floe_motion.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_theme.dart';
import '../application/day_gateway.dart';
import '../application/calendar_gateway.dart';
import '../application/personal_day_controller.dart';
import '../domain/day_models.dart';
import 'day_appearance.dart';
import 'calendar_agenda.dart';
import 'calendar_context_rail.dart';
import 'connector_screen.dart';
import '../../../app/floe_feedback.dart';

enum _DestinationView { today, tasks, notes, connections }

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
  String? capturedText;

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
        final narrow = constraints.maxWidth <= 780;
        return Scaffold(
          backgroundColor: FloePalette.neutral25,
          body: SafeArea(
            child: Padding(
              padding: EdgeInsets.all(narrow ? 0 : 16),
              child: FloeSquircle(
                size: FloeSquircleSize.frame,
                fill: FloePalette.neutral25,
                child: Stack(
                  children: [
                    Positioned.fill(
                      child: SingleChildScrollView(
                        padding: EdgeInsets.fromLTRB(
                          narrow ? FloeSpace.md : 120,
                          narrow ? (constraints.maxWidth <= 430 ? 52 : 58) : 72,
                          narrow ? FloeSpace.md : 36,
                          narrow ? 112 : 28,
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            FloeScreenEntrance(
                              identity: selectedTaskId ?? destination,
                              child: _workspace(narrow),
                            ),
                            if (destination == _DestinationView.today &&
                                selectedTaskId == null)
                              Padding(
                                padding: EdgeInsets.only(top: narrow ? 16 : 22),
                                child: _CaptureBar(
                                  textController: captureController,
                                  pending: controller.commandPending,
                                  submit: _capture,
                                  capturedText: capturedText,
                                  dismiss: () =>
                                      setState(() => capturedText = null),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                    _AdaptiveNavigation(
                      narrow: narrow,
                      selected: destination,
                      onSelected: _selectDestination,
                    ),
                    Positioned(
                      top: narrow ? 10 : 30,
                      right: narrow ? 8 : 36,
                      child: FloeButton.icon(
                        tooltip: '설정',
                        style: const ButtonStyle(
                          backgroundColor: WidgetStatePropertyAll(
                            Colors.transparent,
                          ),
                          overlayColor: WidgetStatePropertyAll(
                            Colors.transparent,
                          ),
                        ),
                        onPressed: () =>
                            _selectDestination(_DestinationView.connections),
                        icon: const Icon(LucideIcons.settings, size: 20),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    ),
  );

  Widget _workspace(bool narrow) {
    if (destination == _DestinationView.connections) {
      return ConnectorScreen(
        gateway: widget.gateway is CalendarGateway
            ? widget.gateway as CalendarGateway
            : null,
        query: controller.query,
        connection: controller.snapshot?.calendar,
        onChanged: controller.load,
      );
    }
    if (controller.loadState == DayLoadState.failure) {
      return _FailureDay(
        retry: controller.load,
        message: controller.errorMessage,
      );
    }
    final snapshot =
        controller.snapshot ??
        DaySnapshot(
          personId: controller.query.personId,
          date: controller.query.date,
          generatedAt: controller.query.now,
          timezoneOffsetSeconds: controller.query.timezoneOffsetSeconds,
          items: const [],
        );
    final selectedTask = _taskById(snapshot, selectedTaskId);
    if (selectedTask != null) {
      return _TaskDetailScreen(
        task: selectedTask,
        snapshot: snapshot,
        narrow: narrow,

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
              if (snapshot.calendar?.error != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 16),
                  child: FloeSquircle(
                    fill: FloePalette.amber50,
                    padding: const EdgeInsets.all(16),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          snapshot.calendar!.lastSuccessAt == null
                              ? 'Calendar could not be collected. Check access and try again.'
                              : 'Showing saved events. Calendar changes could not be collected.',
                          style: const TextStyle(
                            fontSize: 13,
                            color: FloePalette.neutral600,
                          ),
                        ),
                        FloeTextLink(
                          label: 'Manage connection',
                          onPressed: () =>
                              _selectDestination(_DestinationView.connections),
                        ),
                      ],
                    ),
                  ),
                ),
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
          _DestinationView.connections => const SizedBox.shrink(),
          _DestinationView.notes => _NotesScreen(
            notes: snapshot.items.whereType<NoteItem>().toList(),
            narrow: narrow,
            onCreate: _createNote,
            pending: controller.commandPending,
          ),
        },
      ],
    );
  }

  Widget _content(bool narrow, DaySnapshot snapshot) {
    final primary = CalendarAgenda(
      key: const PageStorageKey('calendar-agenda'),
      snapshot: snapshot,
      loading: controller.loadState == DayLoadState.loading,
      onConnections: () => _selectDestination(_DestinationView.connections),
    );
    final rail = CalendarContextRail(
      snapshot: snapshot,
      disabled: controller.commandPending,
      complete: _setTaskCompleted,
      onTasks: () => _selectDestination(_DestinationView.tasks),
      onOpenTask: (task) => setState(() => selectedTaskId = task.id),
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        if (MediaQuery.sizeOf(context).width <= 960) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              primary,
              SizedBox(height: narrow ? 16 : 24),
              rail,
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(flex: 7, child: primary),
            const SizedBox(width: 24),
            SizedBox(
              width: ((constraints.maxWidth - 24) * 0.3).clamp(
                288,
                double.infinity,
              ),
              child: rail,
            ),
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
    final input = captureController.text.trim();
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
        : await showFloeDialog<bool>(
            context,
            (context) => Center(child: dialog),
            barrierDismissible: false,
          );
    if (saved == true && mounted) {
      captureController.clear();
      setState(() => capturedText = input);
    }
  }

  Future<bool> _createNote(String content) async {
    if (controller.commandPending) return false;
    if (!await controller.submitCapture(content)) return false;
    return controller.classify(NoteDraft(content: content));
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
        bottom: 10,
        left: FloeSpace.md,
        child: FloeSquircle(
          size: FloeSquircleSize.lg,
          elevation: 4,
          padding: const EdgeInsets.all(7),
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
      top: 28,
      bottom: 28,
      left: 18,
      width: 64,
      child: Column(
        children: [
          const SizedBox(height: 4),
          const FloeMascot(size: 40),
          const SizedBox(height: 32),
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

class _DestinationButton extends StatefulWidget {
  const _DestinationButton({
    required this.view,
    required this.selected,
    required this.onPressed,
  });

  final _DestinationView view;
  final bool selected;
  final VoidCallback onPressed;

  @override
  State<_DestinationButton> createState() => _DestinationButtonState();
}

class _DestinationButtonState extends State<_DestinationButton> {
  bool hovered = false;
  bool focused = false;
  bool get selected => widget.selected;
  _DestinationView get view => widget.view;

  String get label => switch (view) {
    _DestinationView.today => 'Today',
    _DestinationView.tasks => 'Tasks',
    _DestinationView.notes => 'Notes',
    _DestinationView.connections => 'Connect',
  };

  IconData get icon => switch (view) {
    _DestinationView.today => LucideIcons.calendarDays,
    _DestinationView.tasks => LucideIcons.listTodo,
    _DestinationView.notes => LucideIcons.notebookPen,
    _DestinationView.connections => LucideIcons.link,
  };

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    label: label,
    button: true,
    child: PressableScale(
      scale: 0.98,
      builder: (states) => FloeSquircle(
        size: FloeSquircleSize.md,
        fill: selected
            ? FloePalette.primary100
            : hovered
            ? FloePalette.neutral100
            : Colors.transparent,
        borderColor: focused ? FloePalette.primary600 : Colors.transparent,
        borderWidth: focused ? 2 : 0,
        child: InkWell(
          statesController: states,
          mouseCursor: WidgetStateMouseCursor.clickable,
          onTap: widget.onPressed,
          onHover: (value) => setState(() => hovered = value),
          onFocusChange: (value) => setState(() => focused = value),
          customBorder: floeSquircleBorder(FloeSquircleSize.md),
          child: SizedBox(
            width: MediaQuery.sizeOf(context).width <= 780 ? null : 58,
            height: 40 + MediaQuery.textScalerOf(context).scale(20),
            child: Tooltip(
              message: label,
              child: Center(
                child: Icon(
                  icon,
                  size: 20,
                  color: selected
                      ? FloePalette.primary600
                      : FloePalette.neutral600,
                ),
              ),
            ),
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
    final compact = MediaQuery.sizeOf(context).width <= 430;
    final leading = Row(
      children: [
        for (final direction in [-1, 1]) ...[
          FloeSquircle(
            size: FloeSquircleSize.md,
            child: FloeButton.icon(
              tooltip: direction == -1 ? '이전 날' : '다음 날',
              constraints: BoxConstraints.tightFor(
                width: compact ? 40 : 44,
                height: compact ? 40 : 44,
              ),
              style: IconButton.styleFrom(
                fixedSize: Size.square(compact ? 40 : 44),
                minimumSize: Size.square(compact ? 40 : 44),
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
              padding: EdgeInsets.zero,
              onPressed: () => controller.moveDay(direction),
              icon: Icon(
                direction == -1
                    ? LucideIcons.chevronLeft
                    : LucideIcons.chevronRight,
                size: 20,
              ),
            ),
          ),
          SizedBox(width: compact ? 6 : 12),
        ],
        SizedBox(width: narrow ? 2 : 14),
        Flexible(
          child: Text(
            _date(controller.query.date),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: FloeType.headline.copyWith(
              fontSize: compact ? 17 : 20,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
        SizedBox(width: narrow ? 8 : 12),
        FloeButton.text(
          style: TextButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: const Size(0, 40),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle: TextStyle(
              fontFamily: 'Pretendard',
              fontSize: compact ? 12 : 16,
              fontWeight: FontWeight.w400,
            ),
          ),
          onPressed: controller.goToday,
          child: const Text('Today'),
        ),
      ],
    );
    return Padding(
      padding: const EdgeInsets.only(top: 12, bottom: 16),
      child: Row(
        children: [
          Expanded(child: leading),
          IconButton(
            tooltip: 'Refresh calendar',
            onPressed: controller.loadState == DayLoadState.loading
                ? null
                : controller.refresh,
            icon: const Icon(LucideIcons.refreshCw, size: 18),
          ),
        ],
      ),
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
            FloeButton.filled(
              onPressed: () => _showComingSoon(context),
              icon: const Icon(Icons.add),
              child: const Text('새 할 일'),
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
  const _NotesScreen({
    required this.notes,
    required this.narrow,
    required this.onCreate,
    required this.pending,
  });
  final List<NoteItem> notes;
  final bool narrow;
  final Future<bool> Function(String) onCreate;
  final bool pending;
  @override
  State<_NotesScreen> createState() => _NotesScreenState();
}

class _NotesScreenState extends State<_NotesScreen> {
  final search = TextEditingController();
  bool personalOnly = false;

  Future<void> _create() async {
    final saved = await showFloeDialog<bool>(
      context,
      (context) => _NewNoteDialog(save: widget.onCreate),
      barrierDismissible: false,
    );
    if (saved == true && mounted) {
      setState(() {
        search.clear();
        personalOnly = false;
      });
    }
  }

  @override
  void dispose() {
    search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = search.text.trim().toLowerCase();
    final appearances = DayAppearance.of(context)?.notes ?? {};
    final notes = widget.notes.where((note) {
      final appearance = appearances[note.id];
      return (!personalOnly ||
              (appearance?.category ?? 'Personal') == 'Personal') &&
          [
            note.title,
            appearance?.excerpt ?? '',
            appearance?.category ?? '',
          ].any((text) => text.toLowerCase().contains(query));
    }).toList();
    final heading = Text(
      'All notes · ${widget.notes.length}',
      style: const TextStyle(
        fontSize: 18,
        height: 1.2,
        fontWeight: FontWeight.w600,
      ),
    );
    final searchField = FloeSquircle(
      size: FloeSquircleSize.field,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      child: SizedBox(
        height: 50,
        child: Row(
          children: [
            const Icon(
              LucideIcons.search,
              size: 19,
              color: FloePalette.neutral600,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: TextField(
                controller: search,
                onChanged: (_) => setState(() {}),
                style: const TextStyle(fontSize: 16),
                decoration: const InputDecoration(
                  hintText: 'Search notes',
                  filled: false,
                  border: InputBorder.none,
                  enabledBorder: InputBorder.none,
                  focusedBorder: InputBorder.none,
                  contentPadding: EdgeInsets.zero,
                ),
              ),
            ),
          ],
        ),
      ),
    );
    final filter = FloeButton.outlined(
      style: OutlinedButton.styleFrom(
        shape: floeSquircleBorder(FloeSquircleSize.md),
        backgroundColor: personalOnly
            ? FloePalette.primary100
            : FloePalette.neutral0,
        side: const BorderSide(color: FloePalette.neutral200),
      ),
      onPressed: () => setState(() => personalOnly = !personalOnly),
      icon: const Icon(LucideIcons.filter, size: 18),
      child: const Text('Filter'),
    );
    final create = FloeButton.filled(
      style: FilledButton.styleFrom(
        shape: floeSquircleBorder(FloeSquircleSize.md),
      ),
      onPressed: widget.pending ? null : _create,
      icon: const Icon(LucideIcons.plus, size: 18),
      child: const Text('New note'),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.only(top: 12, bottom: widget.narrow ? 16 : 12),
          child: widget.narrow || MediaQuery.sizeOf(context).width < 1000
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    heading,
                    const SizedBox(height: 12),
                    searchField,
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        Expanded(child: filter),
                        const SizedBox(width: 10),
                        Expanded(child: create),
                      ],
                    ),
                  ],
                )
              : Row(
                  children: [
                    Expanded(child: heading),
                    SizedBox(width: 240, child: searchField),
                    const SizedBox(width: 10),
                    filter,
                    const SizedBox(width: 10),
                    create,
                  ],
                ),
        ),
        SizedBox(height: widget.narrow ? 4 : 8),
        if (notes.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 120),
            child: Center(
              child: Column(
                children: [
                  const Icon(LucideIcons.search, size: 24),
                  const SizedBox(height: 12),
                  const Text('No notes found', style: FloeType.headline),
                  const SizedBox(height: 8),
                  const Text(
                    'Try a different search or clear the current filter.',
                    textAlign: TextAlign.center,
                  ),
                  FloeButton.text(
                    onPressed: () => setState(() {
                      search.clear();
                      personalOnly = false;
                    }),
                    child: const Text('Clear filters'),
                  ),
                ],
              ),
            ),
          )
        else
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = widget.narrow
                  ? 1
                  : MediaQuery.sizeOf(context).width > 1080
                  ? 3
                  : 2;
              final gap = widget.narrow ? 14.0 : 22.0;
              final width =
                  (constraints.maxWidth - gap * (columns - 1)) / columns;
              return Wrap(
                spacing: gap,
                runSpacing: gap,
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

class _NewNoteDialog extends StatefulWidget {
  const _NewNoteDialog({required this.save});
  final Future<bool> Function(String) save;

  @override
  State<_NewNoteDialog> createState() => _NewNoteDialogState();
}

class _NewNoteDialogState extends State<_NewNoteDialog> {
  final content = TextEditingController();
  bool pending = false;
  bool failed = false;

  @override
  void dispose() {
    content.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (pending || content.text.trim().isEmpty) return;
    setState(() {
      pending = true;
      failed = false;
    });
    final saved = await widget.save(content.text.trim());
    if (!mounted) return;
    if (saved) {
      Navigator.pop(context, true);
    } else {
      setState(() {
        pending = false;
        failed = true;
      });
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !pending,
    child: AlertDialog(
      title: const Text('New note'),
      content: SizedBox(
        width: 420,
        child: TextField(
          key: const Key('new-note-content'),
          controller: content,
          autofocus: true,
          enabled: !pending,
          minLines: 3,
          maxLines: 8,
          onChanged: (_) => setState(() {}),
          decoration: InputDecoration(
            hintText: 'Write a thought, decision, or detail to remember.',
            errorText: failed ? 'Could not save. Please try again.' : null,
          ),
        ),
      ),
      actions: [
        FloeButton.text(
          onPressed: pending ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FloeButton.filled(
          onPressed: pending || content.text.trim().isEmpty ? null : _save,
          child: Text(pending ? 'Saving…' : 'Save note'),
        ),
      ],
    ),
  );
}

class _NotePreviewCard extends StatelessWidget {
  const _NotePreviewCard({required this.note, required this.onOpen});
  final NoteItem note;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final appearance = DayAppearance.of(context)?.notes[note.id];
    final tone = appearance?.tone ?? ItemTone.violet;
    final mobile = MediaQuery.sizeOf(context).width <= 780;
    return FloeSquircle(
      fill: Color.lerp(Colors.white, tone.fill, .4)!,
      borderColor: tone.border,
      child: InkWell(
        onTap: onOpen,
        customBorder: floeSquircleBorder(FloeSquircleSize.lg),
        child: Padding(
          padding: EdgeInsets.all(mobile ? 25 : 29),
          child: IntrinsicHeight(
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: mobile ? 160 : 187),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      _ToneDot(color: tone.accent),
                      const SizedBox(width: 10),
                      Text(
                        appearance?.category ?? 'Personal',
                        style: const TextStyle(
                          fontSize: 12,
                          height: 1.2,
                          fontWeight: FontWeight.w600,
                          color: FloePalette.neutral600,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 20),
                  Text(
                    note.title,
                    style: FloeType.headline.copyWith(
                      fontWeight: FontWeight.w600,
                      height: 1.2,
                    ),
                  ),
                  const SizedBox(height: 14),
                  Text(
                    appearance?.excerpt ?? '',
                    style: FloeType.body.copyWith(height: 1.65),
                  ),
                  const Spacer(),
                  const SizedBox(height: 24),
                  Text(
                    appearance?.timestamp ?? _date(note.createdAt),
                    style: const TextStyle(
                      fontSize: 12,
                      height: 1.2,
                      color: FloePalette.neutral500,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _TaskDetailScreen extends StatefulWidget {
  const _TaskDetailScreen({
    required this.task,
    required this.snapshot,
    required this.narrow,
    required this.onComplete,
  });
  final TaskItem task;
  final DaySnapshot snapshot;
  final bool narrow;
  final Future<void> Function(TaskItem, bool) onComplete;
  @override
  State<_TaskDetailScreen> createState() => _TaskDetailScreenState();
}

class _TaskDetailScreenState extends State<_TaskDetailScreen> {
  final Map<String, bool> subtaskChecks = {};
  bool suggestionVisible = true;

  @override
  Widget build(BuildContext context) {
    final appearance = DayAppearance.of(context)?.tasks[widget.task.id];
    final primary = FloeSquircle(
      padding: widget.narrow
          ? EdgeInsets.symmetric(
              horizontal: MediaQuery.sizeOf(context).width <= 430 ? 21 : 23,
              vertical: MediaQuery.sizeOf(context).width <= 430 ? 25 : 27,
            )
          : const EdgeInsets.all(43),
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: widget.narrow ? 0 : 634),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                const _ToneDot(color: FloePalette.blue500),
                const SizedBox(width: 12),
                Text('Task', style: FloeType.body.copyWith(height: 1.15)),
              ],
            ),
            const SizedBox(height: 18),
            Text(
              widget.task.title,
              style: TextStyle(
                fontSize: widget.narrow ? 39 : 48,
                fontWeight: FontWeight.w700,
                letterSpacing: -1.6,
                height: 1.2,
                color: FloePalette.neutral950,
              ),
            ),
            const SizedBox(height: 24),
            Text(
              appearance?.description ?? '아직 설명이 없어요.',
              style: FloeType.body.copyWith(fontSize: 16, height: 1.65),
            ),
            const SizedBox(height: 34),
            _LabeledValue(
              label: 'Due',
              value: widget.task.deadline == null ? 'No due date' : 'Today',
              color: FloePalette.primary600,
            ),
            const SizedBox(height: 16),
            _LabeledValue(
              label: 'Time context',
              value: appearance?.timeContext ?? 'Not scheduled',
            ),
            const SizedBox(height: 16),
            _LabeledValue(
              label: 'Calendar',
              value: appearance?.project ?? 'Personal',
              color: FloePalette.mint700,
            ),
            const SizedBox(height: 32),
            const Divider(height: 1, color: FloePalette.neutral200),
            const SizedBox(height: 26),
            const Text(
              'Subtasks',
              style: TextStyle(
                fontSize: 17,
                height: 1.2,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 10),
            if (appearance == null || appearance.subtasks.isEmpty)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 20),
                child: Text('아직 하위 작업이 없어요.', style: FloeType.body),
              ),
            for (final subtask
                in appearance?.subtasks ??
                    <({String title, String duration, bool done})>[])
              Container(
                constraints: const BoxConstraints(minHeight: 62),
                decoration: const BoxDecoration(
                  border: Border(
                    bottom: BorderSide(color: FloePalette.neutral200),
                  ),
                ),
                child: Row(
                  children: [
                    SizedBox(
                      width: 44,
                      child: Checkbox(
                        value: subtaskChecks[subtask.title] ?? subtask.done,
                        onChanged: (value) => setState(
                          () => subtaskChecks[subtask.title] = value ?? false,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        subtask.title,
                        style: FloeType.body.copyWith(
                          color: (subtaskChecks[subtask.title] ?? subtask.done)
                              ? FloePalette.neutral500
                              : FloePalette.neutral950,
                          decoration:
                              (subtaskChecks[subtask.title] ?? subtask.done)
                              ? TextDecoration.lineThrough
                              : null,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      subtask.duration,
                      style: const TextStyle(
                        fontSize: 13,
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ),
              ),
            const SizedBox(height: 8),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.text(
                onPressed: () => _showComingSoon(context),
                style: TextButton.styleFrom(
                  padding: EdgeInsets.zero,
                  foregroundColor: FloePalette.primary600,
                ),
                icon: const Icon(LucideIcons.plus, size: 18),
                child: const Text('Add a subtask'),
              ),
            ),
          ],
        ),
      ),
    );
    final rail = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (suggestionVisible) ...[
          FloeSquircle(
            padding: EdgeInsets.all(widget.narrow ? 23 : 27),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    const FloeMascot(size: 38),
                    const SizedBox(width: 10),
                    const Expanded(
                      child: Text(
                        'Floe suggests',
                        style: TextStyle(
                          fontSize: 13,
                          color: FloePalette.primary600,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    FloeSquircle(
                      size: FloeSquircleSize.md,
                      child: FloeButton.icon(
                        tooltip: 'Dismiss suggestion',
                        style: IconButton.styleFrom(
                          fixedSize: const Size.square(36),
                          minimumSize: const Size.square(36),
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        ),
                        onPressed: () =>
                            setState(() => suggestionVisible = false),
                        icon: const Icon(LucideIcons.x, size: 18),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 18),
                Text(
                  appearance?.suggestion ??
                      'Review the context before starting this task?',
                  style: FloeType.body.copyWith(height: 1.55),
                ),
                const SizedBox(height: 18),
                Wrap(
                  alignment: WrapAlignment.end,
                  spacing: 12,
                  children: [
                    FloeButton.filled(
                      onPressed: () =>
                          setState(() => suggestionVisible = false),
                      child: const Text('Review now'),
                    ),
                    FloeButton.text(
                      style: TextButton.styleFrom(
                        minimumSize: const Size(0, 40),
                        padding: const EdgeInsets.symmetric(horizontal: 12),
                        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        textStyle: const TextStyle(
                          fontFamily: 'Pretendard',
                          fontSize: 13,
                        ),
                      ),
                      onPressed: () =>
                          setState(() => suggestionVisible = false),
                      child: const Text('Snooze'),
                    ),
                  ],
                ),
              ],
            ),
          ),
          SizedBox(height: widget.narrow ? 16 : 20),
        ],
        FloeSquircle(
          fill: FloePalette.primary50,
          borderColor: FloePalette.primary100,
          padding: EdgeInsets.all(widget.narrow ? 23 : 27),
          child: ConstrainedBox(
            constraints: const BoxConstraints(),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Notes',
                  style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 18),
                Text(
                  appearance?.note ?? '연결된 노트가 없어요.',
                  style: FloeType.body.copyWith(height: 1.7),
                ),
                const SizedBox(height: 20),
                const Text(
                  'Updated this morning',
                  style: TextStyle(fontSize: 12, color: FloePalette.neutral500),
                ),
              ],
            ),
          ),
        ),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 12),
          child: Row(
            children: [
              const Spacer(),
              FloeSquircle(
                size: FloeSquircleSize.md,
                child: PopupMenuButton<bool>(
                  tooltip: 'Task options',
                  onSelected: (completed) =>
                      widget.onComplete(widget.task, completed),
                  itemBuilder: (context) => [
                    PopupMenuItem(
                      value: !widget.task.isCompleted,
                      child: Text(
                        widget.task.isCompleted
                            ? 'Mark incomplete'
                            : 'Complete task',
                      ),
                    ),
                  ],
                  shape: floeSquircleBorder(FloeSquircleSize.md),
                  icon: const Icon(LucideIcons.ellipsis, size: 21),
                ),
              ),
            ],
          ),
        ),
        LayoutBuilder(
          builder: (context, constraints) {
            if (MediaQuery.sizeOf(context).width <= 1080) {
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  primary,
                  SizedBox(height: widget.narrow ? 16 : 24),
                  rail,
                ],
              );
            }
            return Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: primary),
                const SizedBox(width: 24),
                SizedBox(
                  width: ((constraints.maxWidth - 24) * .3).clamp(
                    288,
                    double.infinity,
                  ),
                  child: rail,
                ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _LabeledValue extends StatelessWidget {
  const _LabeledValue({
    required this.label,
    required this.value,
    this.color = FloePalette.neutral950,
  });
  final String label;
  final String value;
  final Color color;
  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      SizedBox(
        width: MediaQuery.sizeOf(context).width <= 430
            ? 92
            : MediaQuery.sizeOf(context).width <= 780
            ? 110
            : 150,
        child: Text(label, style: FloeType.body.copyWith(height: 1.15)),
      ),
      Expanded(
        child: Text(
          value,
          style: FloeType.body.copyWith(color: color, height: 1.15),
        ),
      ),
    ],
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
    await showFloeDialog<void>(context, (context) => Dialog(child: detail));
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
              FloeButton.icon(
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
          FloeButton.outlined(
            onPressed: () => _showComingSoon(context),
            icon: const FloeMascot(size: 24),
            child: const Text('Floe와 함께 보기'),
          ),
        ],
      ),
    ),
  );
}

class _ToneDot extends StatelessWidget {
  const _ToneDot({required this.color});
  final Color color;
  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    child: const SizedBox.square(dimension: 10),
  );
}

class _DayRow extends StatelessWidget {
  const _DayRow({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    this.onOpen,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
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
      minTileHeight: 72,
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
        maxLines: 3,
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
      trailing: item is EventItem && (item as EventItem).externalId != null
          ? const Tooltip(
              message: '외부 캘린더에서 관리하는 읽기 전용 일정',
              child: Icon(Icons.lock_outline, size: 18),
            )
          : FloeButton.icon(
              tooltip: '${item.title} 삭제',
              onPressed: disabled ? null : () => _confirmDelete(context),
              icon: const Icon(Icons.more_horiz, size: 20),
            ),
    );
  }

  Future<void> _confirmDelete(BuildContext context) async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => AlertDialog(
        title: const Text('항목을 삭제할까요?'),
        content: Text('“${item.title}” 항목이 오늘의 흐름에서 제거됩니다.'),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('취소'),
          ),
          FloeButton.filled(
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
    required this.capturedText,
    required this.dismiss,
  });
  final TextEditingController textController;
  final bool pending;
  final VoidCallback submit;
  final String? capturedText;
  final VoidCallback dismiss;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.field,
    padding: const EdgeInsets.fromLTRB(18, 9, 11, 9),
    child: capturedText != null
        ? Semantics(
            liveRegion: true,
            child: SizedBox(
              height: 48,
              child: Row(
                children: [
                  const Icon(
                    LucideIcons.check,
                    size: 18,
                    color: FloePalette.primary600,
                  ),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Text(
                      'Captured “$capturedText”',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  FloeButton.icon(
                    tooltip: 'Dismiss capture',
                    onPressed: dismiss,
                    icon: const Icon(LucideIcons.x, size: 17),
                  ),
                ],
              ),
            ),
          )
        : Row(
            children: [
              const Icon(
                LucideIcons.plus,
                size: 22,
                color: FloePalette.primary600,
              ),
              const SizedBox(width: 14),
              Expanded(
                child: TextField(
                  key: const Key('capture-field'),
                  controller: textController,
                  enabled: !pending,
                  style: TextStyle(
                    fontSize: MediaQuery.sizeOf(context).width <= 430 ? 13 : 16,
                  ),
                  onSubmitted: (value) {
                    if (!pending && value.trim().isNotEmpty) submit();
                  },
                  decoration: const InputDecoration(
                    hintText: 'A thought for your day...',
                    hintStyle: TextStyle(color: FloePalette.neutral500),
                    filled: false,
                    border: InputBorder.none,
                    enabledBorder: InputBorder.none,
                    focusedBorder: InputBorder.none,
                    contentPadding: EdgeInsets.zero,
                  ),
                ),
              ),
              const SizedBox(width: 14),
              ValueListenableBuilder<TextEditingValue>(
                valueListenable: textController,
                builder: (context, value, _) {
                  final enabled = !pending && value.text.trim().isNotEmpty;
                  return Tooltip(
                    message: '캡처 저장',
                    child: SizedBox.square(
                      dimension: 44,
                      child: FloeButton.outlined(
                        style: OutlinedButton.styleFrom(
                          padding: EdgeInsets.zero,
                          shape: floeSquircleBorder(FloeSquircleSize.md),
                        ),
                        onPressed: enabled ? submit : null,
                        child: pending
                            ? const SizedBox.square(
                                dimension: 16,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                ),
                              )
                            : const Icon(LucideIcons.arrowRight, size: 19),
                      ),
                    ),
                  );
                },
              ),
            ],
          ),
  );
}

class _FailureDay extends StatelessWidget {
  const _FailureDay({required this.retry, required this.message});
  final VoidCallback retry;
  final String? message;
  @override
  Widget build(BuildContext context) => Center(
    child: FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.xl),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.error_outline, color: FloePalette.error600),
          const SizedBox(height: 12),
          const Text('하루 데이터를 불러오지 못했어요'),
          const SizedBox(height: 8),
          SelectableText(
            message ?? '알 수 없는 오류입니다.',
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 16),
          FloeButton.filled(
            onPressed: retry,
            icon: const Icon(Icons.refresh),
            child: const Text('다시 불러오기'),
          ),
        ],
      ),
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
          FloeButton.text(onPressed: dismiss, child: const Text('닫기')),
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
                  FloeButton.text(
                    onPressed: pending
                        ? null
                        : () => Navigator.pop(context, false),
                    child: const Text('나중에'),
                  ),
                  const SizedBox(width: FloeSpace.sm),
                  FloeButton.filled(
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
  Widget build(BuildContext context) => FloeButton.outlined(
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
        FloeButton.text(
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
    '${['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'][value.weekday - 1]}, ${['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][value.month - 1]} ${value.day}';

TaskItem? _taskById(DaySnapshot snapshot, String? id) {
  if (id == null) return null;
  for (final task in snapshot.items.whereType<TaskItem>()) {
    if (task.id == id) return task;
  }
  return null;
}
