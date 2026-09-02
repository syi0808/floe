import 'package:flutter/material.dart';

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
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 780),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(24, 12, 24, 20),
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
                  const SizedBox(height: 12),
                  TextField(
                    key: const Key('capture-field'),
                    controller: captureController,
                    enabled: !controller.commandPending,
                    onSubmitted: (_) => _capture(),
                    decoration: InputDecoration(
                      hintText: '무엇이든 입력…',
                      prefixIcon: const Icon(Icons.add),
                      suffixIcon: IconButton(
                        tooltip: '캡처 저장',
                        onPressed: controller.commandPending ? null : _capture,
                        icon: const Icon(Icons.arrow_upward),
                      ),
                    ),
                  ),
                ],
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
        child: FilledButton.icon(
          onPressed: controller.load,
          icon: const Icon(Icons.refresh),
          label: const Text('다시 불러오기'),
        ),
      );
    }
    final snapshot = controller.snapshot!;
    return Column(
      children: [
        _NowNext(snapshot),
        const SizedBox(height: 20),
        Expanded(
          child: snapshot.items.isEmpty
              ? const _EmptyState()
              : ListView.separated(
                  itemCount: snapshot.items.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (_, index) => _Row(
                    item: snapshot.items[index],
                    snapshot: snapshot,
                    disabled: controller.commandPending,
                    complete: controller.setTaskCompleted,
                    delete: controller.deleteItem,
                  ),
                ),
        ),
      ],
    );
  }

  Future<void> _capture() async {
    if (!await controller.submitCapture(captureController.text) || !mounted) {
      return;
    }
    final saved = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _ClassificationDialog(
        capture: controller.pendingCapture!,
        now: controller.query.now,
        classify: controller.classify,
      ),
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
  Widget build(BuildContext context) => SizedBox(
    height: 56,
    child: Row(
      children: [
        IconButton(
          tooltip: '이전 날',
          onPressed: () => controller.moveDay(-1),
          icon: const Icon(Icons.chevron_left),
        ),
        TextButton(onPressed: controller.load, child: const Text('오늘')),
        IconButton(
          tooltip: '다음 날',
          onPressed: () => controller.moveDay(1),
          icon: const Icon(Icons.chevron_right),
        ),
        const Spacer(),
        Text(
          _date(controller.query.date),
          style: Theme.of(context).textTheme.titleLarge,
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
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        color: FloeColors.sageSoft,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Row(
        children: [
          Expanded(
            child: _Hero(
              '지금',
              current?.title ?? '여유로운 시간',
              current == null ? '진행 중인 일정이 없어요' : '${_time(current.endsAt)}까지',
            ),
          ),
          Container(width: 1, height: 52, color: FloeColors.divider),
          const SizedBox(width: 20),
          Expanded(
            child: _Hero(
              '다음',
              next?.title ?? '예정된 일정 없음',
              next == null ? '하루를 가볍게 계획해 보세요' : _time(next.startsAt),
            ),
          ),
        ],
      ),
    );
  }
}

class _Hero extends StatelessWidget {
  const _Hero(this.label, this.title, this.meta);
  final String label;
  final String title;
  final String meta;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        label,
        style: Theme.of(context).textTheme.bodyMedium
            ?.copyWith(color: FloeColors.sage, fontWeight: FontWeight.w600),
      ),
      const SizedBox(height: 5),
      Text(
        title,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: Theme.of(context).textTheme.titleLarge,
      ),
      const SizedBox(height: 3),
      Text(meta, style: Theme.of(context).textTheme.bodyMedium),
    ],
  );
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();
  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Icon(Icons.water_drop_outlined, color: FloeColors.sage, size: 32),
        const SizedBox(height: 12),
        Text('오늘은 아직 비어 있어요', style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 6),
        Text(
          '아래에서 일정, 할 일, 생각을 담아보세요.',
          style: Theme.of(context).textTheme.bodyMedium,
        ),
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
    return ListTile(
      minTileHeight: 64,
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
              color: FloeColors.secondary,
            ),
      title: Text(
        item.title,
        style: TextStyle(
          decoration: task?.isCompleted == true
              ? TextDecoration.lineThrough
              : null,
        ),
      ),
      subtitle: Text(
        overdue ? '$subtitle · 기한 지남' : subtitle,
        style: TextStyle(
          color: overdue ? FloeColors.overdue : FloeColors.secondary,
        ),
      ),
      trailing: IconButton(
        tooltip: '${item.title} 삭제',
        onPressed: disabled ? null : () => delete(item),
        icon: const Icon(Icons.more_horiz),
      ),
    );
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
                : (value) =>
                      setState(() => kind = value.isEmpty ? null : value.first),
          ),
          const SizedBox(height: 14),
          TextField(
            controller: text,
            decoration: const InputDecoration(labelText: '내용'),
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: pending ? null : () => Navigator.pop(context, false),
        child: const Text('나중에'),
      ),
      FilledButton(
        onPressed: pending || kind == null ? null : _submit,
        child: const Text('분류하여 추가'),
      ),
    ],
  );
  Future<void> _submit() async {
    setState(() => pending = true);
    final draft = switch (kind!) {
      _Kind.event => EventDraft(
        title: text.text,
        startsAt: widget.now.add(const Duration(hours: 1)),
        endsAt: widget.now.add(const Duration(hours: 2)),
      ),
      _Kind.task => TaskDraft(title: text.text),
      _Kind.note => NoteDraft(content: text.text),
    };
    final success = await widget.classify(draft);
    if (mounted && success) Navigator.pop(context, true);
    if (mounted) setState(() => pending = false);
  }
}

String _time(DateTime value) =>
    '${value.hour.toString().padLeft(2, '0')}:${value.minute.toString().padLeft(2, '0')}';
String _date(DateTime value) =>
    '${value.month}월 ${value.day}일 ${['월', '화', '수', '목', '금', '토', '일'][value.weekday - 1]}요일';
