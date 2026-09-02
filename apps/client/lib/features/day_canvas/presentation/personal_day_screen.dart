import 'package:flutter/material.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_motion.dart';
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
    builder: (context, _) => Scaffold(
      body: SafeArea(
        child: LayoutBuilder(
          builder: (context, constraints) => Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 860),
              child: Padding(
                padding: EdgeInsets.fromLTRB(
                  constraints.maxWidth < 800 ? FloeSpace.lg : FloeSpace.xl,
                  FloeSpace.base,
                  constraints.maxWidth < 800 ? FloeSpace.lg : FloeSpace.xl,
                  FloeSpace.lg,
                ),
                child: Column(
                  children: [
                    _Toolbar(controller),
                    if (controller.errorMessage case final message?)
                      MaterialBanner(
                        content: Text(message),
                        actions: [
                          TextButton(
                            onPressed: controller.clearError,
                            child: const Text('닫기'),
                          ),
                        ],
                      ),
                    Expanded(child: _body()),
                    const SizedBox(height: FloeSpace.base),
                    _CaptureBar(
                      textController: captureController,
                      pending: controller.commandPending,
                      submit: _capture,
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );

  Widget _body() {
    if (controller.loadState == DayLoadState.loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (controller.loadState == DayLoadState.failure) {
      return Center(
        child: PressableScale(
          child: FilledButton.icon(
            onPressed: controller.load,
            icon: const Icon(Icons.refresh),
            label: const Text('다시 불러오기'),
          ),
        ),
      );
    }
    final snapshot = controller.snapshot!;
    return ListView(
      children: [
        _NowNext(snapshot),
        const SizedBox(height: 20),
        if (snapshot.items.isEmpty)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: FloeSpace.xxxl),
            child: _EmptyState(),
          )
        else
          for (final (index, item) in snapshot.items.indexed) ...[
            _Row(
              item: item,
              snapshot: snapshot,
              disabled: controller.commandPending,
              complete: controller.setTaskCompleted,
              delete: controller.deleteItem,
            ),
            if (index < snapshot.items.length - 1) const Divider(height: 1),
          ],
      ],
    );
  }

  Future<void> _capture() async {
    if (!await controller.submitCapture(captureController.text) || !mounted) {
      return;
    }
    final saved = await showGeneralDialog<bool>(
      context: context,
      barrierDismissible: false,
      barrierColor: FloePalette.neutral950.withValues(alpha: 0.28),
      transitionDuration: FloeMotion.dialogDuration,
      pageBuilder: (context, animation, secondaryAnimation) => Center(
        child: _ClassificationDialog(
          capture: controller.pendingCapture!,
          now: controller.query.now,
          classify: controller.classify,
        ),
      ),
      transitionBuilder: (context, animation, secondaryAnimation, child) =>
          FloeFadeScaleTransition(animation: animation, child: child),
    );
    if (saved == true) {
      captureController.clear();
    }
  }
}

class _Toolbar extends StatelessWidget {
  const _Toolbar(this.controller);
  final PersonalDayController controller;
  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final brand = Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 30,
            height: 30,
            decoration: BoxDecoration(
              color: FloePalette.neutral950,
              borderRadius: BorderRadius.circular(FloeRadius.sm),
            ),
            alignment: Alignment.center,
            child: const Text(
              'F',
              style: TextStyle(
                color: FloePalette.neutral0,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          const SizedBox(width: FloeSpace.md),
          Flexible(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('Floe', style: FloeType.label),
                Text(
                  _date(controller.query.date),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.titleLarge,
                ),
              ],
            ),
          ),
        ],
      );
      final navigation = Row(
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
      if (constraints.maxWidth < 520) {
        return Padding(
          padding: const EdgeInsets.symmetric(vertical: FloeSpace.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              brand,
              const SizedBox(height: FloeSpace.sm),
              Align(alignment: Alignment.centerRight, child: navigation),
            ],
          ),
        );
      }
      return SizedBox(
        height: 72,
        child: Row(
          children: [
            Expanded(child: brand),
            navigation,
          ],
        ),
      );
    },
  );
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
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(FloeSpace.md),
    decoration: BoxDecoration(
      color: FloePalette.neutral0,
      border: Border.all(color: FloePalette.neutral200),
      borderRadius: BorderRadius.circular(FloeRadius.lg),
      boxShadow: const [
        BoxShadow(
          color: Color(0x0F111317),
          blurRadius: 24,
          offset: Offset(0, 8),
        ),
      ],
    ),
    child: Row(
      children: [
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: FloeSpace.xs),
          child: Icon(Icons.add_circle_outline, color: FloePalette.primary600),
        ),
        const SizedBox(width: FloeSpace.sm),
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
        const SizedBox(width: FloeSpace.md),
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

class _NowNext extends StatelessWidget {
  const _NowNext(this.snapshot);
  final DaySnapshot snapshot;
  EventItem? _find(String? id) {
    for (final event in snapshot.items.whereType<EventItem>()) {
      if (event.id == id) {
        return event;
      }
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
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(FloeSpace.lg),
      decoration: BoxDecoration(
        gradient: FloePalette.glacialField,
        border: Border.all(color: FloePalette.primary100),
        borderRadius: BorderRadius.circular(FloeRadius.lg),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          if (constraints.maxWidth < 520) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                now,
                const SizedBox(height: FloeSpace.base),
                _NextSurface(child: upcoming),
              ],
            );
          }
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(flex: 6, child: now),
              const SizedBox(width: FloeSpace.lg),
              Expanded(flex: 4, child: _NextSurface(child: upcoming)),
            ],
          );
        },
      ),
    );
  }
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
    mainAxisAlignment: MainAxisAlignment.center,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        label,
        style: FloeType.label.copyWith(color: FloePalette.primary700),
      ),
      const SizedBox(height: FloeSpace.sm),
      Text(
        title,
        maxLines: 2,
        overflow: TextOverflow.fade,
        style: primary ? FloeType.display : FloeType.headline,
      ),
      const SizedBox(height: FloeSpace.xs),
      Text(meta, style: FloeType.body),
    ],
  );
}

class _NextSurface extends StatelessWidget {
  const _NextSurface({required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(FloeSpace.base),
    decoration: BoxDecoration(
      color: FloePalette.neutral0,
      border: Border.all(color: FloePalette.neutral200),
      borderRadius: BorderRadius.circular(FloeRadius.md),
    ),
    child: child,
  );
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();
  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 44,
          height: 44,
          decoration: BoxDecoration(
            color: FloePalette.primary50,
            borderRadius: BorderRadius.circular(FloeRadius.md),
          ),
          child: const Icon(
            Icons.water_drop_outlined,
            color: FloePalette.primary600,
          ),
        ),
        const SizedBox(height: FloeSpace.base),
        const Text('오늘은 아직 비어 있어요', style: FloeType.headline),
        const SizedBox(height: FloeSpace.sm),
        Text('아래에서 일정, 할 일, 생각을 담아보세요.', style: FloeType.body),
      ],
    ),
  );
}

class _Row extends StatelessWidget {
  const _Row({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
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
      NoteItem() => '생각',
    };
    return ListTile(
      minTileHeight: 72,
      contentPadding: const EdgeInsets.symmetric(
        horizontal: FloeSpace.md,
        vertical: FloeSpace.xs,
      ),
      leading: task != null
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
              color: FloePalette.neutral500,
            ),
      title: Row(
        children: [
          Text(label, style: FloeType.label),
          const SizedBox(width: FloeSpace.sm),
          Expanded(
            child: Text(
              item.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: FloeType.bodyLarge.copyWith(
                decoration: task?.isCompleted == true
                    ? TextDecoration.lineThrough
                    : null,
              ),
            ),
          ),
        ],
      ),
      subtitle: Text(
        overdue ? '$subtitle · 기한 지남' : subtitle,
        style: TextStyle(
          color: overdue ? FloePalette.warning600 : FloePalette.neutral500,
        ),
      ),
      trailing: IconButton(
        tooltip: '${item.title} 삭제',
        onPressed: disabled ? null : () => _confirmDelete(context),
        icon: const Icon(Icons.delete_outline, size: 20),
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
          PressableScale(
            child: TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('취소'),
            ),
          ),
          PressableScale(
            child: FilledButton(
              onPressed: () => Navigator.pop(context, true),
              style: FloeTheme.destructiveButtonStyle,
              child: const Text('삭제'),
            ),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await delete(item);
    }
  }
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
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('어디에 담을까요?'),
    content: SizedBox(
      width: 440,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('입력 원문', style: Theme.of(context).textTheme.bodyMedium),
          Text(widget.capture.originalInput),
          const SizedBox(height: 18),
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
          const SizedBox(height: 14),
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
        ],
      ),
    ),
    actions: [
      PressableScale(
        enabled: !pending,
        child: TextButton(
          onPressed: pending ? null : () => Navigator.pop(context, false),
          child: const Text('나중에'),
        ),
      ),
      PressableScale(
        enabled: !pending && kind != null,
        child: FilledButton(
          onPressed: pending || kind == null ? null : _submit,
          child: const Text('분류하여 추가'),
        ),
      ),
    ],
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

DateTime _onDate(DateTime date, TimeOfDay time) =>
    DateTime(date.year, date.month, date.day, time.hour, time.minute);

String _time(DateTime value) =>
    '${value.hour.toString().padLeft(2, '0')}:${value.minute.toString().padLeft(2, '0')}';
String _date(DateTime value) =>
    '${value.month}월 ${value.day}일 ${['월', '화', '수', '목', '금', '토', '일'][value.weekday - 1]}요일';
