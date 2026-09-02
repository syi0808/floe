import '../domain/day_models.dart';
import 'day_gateway.dart';

final class FakeDayGateway implements DayGateway {
  FakeDayGateway({List<DayItem> initialItems = const []})
    : _items = List.of(initialItems);

  final List<DayItem> _items;
  int _nextId = 1;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async => _snapshot(query);

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) async {
    final value = input.trim();
    if (value.isEmpty) throw const FormatException('내용을 입력해 주세요.');
    return CaptureReceipt(
      id: 'capture-${_nextId++}',
      originalInput: value,
      capturedAt: query.now,
      revision: 0,
    );
  }

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) async {
    final id = 'item-${_nextId++}';
    final item = switch (classification) {
      EventDraft(:final title, :final startsAt, :final endsAt) => EventItem(
        id: id,
        title: _required(title),
        revision: 0,
        createdAt: query.now,
        startsAt: startsAt,
        endsAt: endsAt,
      ),
      TaskDraft(:final title, :final deadline) => TaskItem(
        id: id,
        title: _required(title),
        revision: 0,
        createdAt: query.now,
        deadline: deadline,
      ),
      NoteDraft(:final content) => NoteItem(
        id: id,
        title: _required(content),
        revision: 0,
        createdAt: query.now,
      ),
    };
    _items.add(item);
    return _snapshot(query);
  }

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async {
    final index = _items.indexWhere((item) => item.id == task.id);
    _items[index] = TaskItem(
      id: task.id,
      title: task.title,
      revision: task.revision + 1,
      createdAt: task.createdAt,
      deadline: task.deadline,
      completedAt: completed ? query.now : null,
      priority: task.priority,
    );
    return _snapshot(query);
  }

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) async {
    _items.removeWhere((candidate) => candidate.id == item.id);
    return _snapshot(query);
  }

  String _required(String value) {
    final trimmed = value.trim();
    if (trimmed.isEmpty) throw const FormatException('내용을 입력해 주세요.');
    return trimmed;
  }

  DaySnapshot _snapshot(DayQuery query) {
    final items = List<DayItem>.of(_items)
      ..sort((left, right) => left.createdAt.compareTo(right.createdAt));
    final events = items.whereType<EventItem>().toList()
      ..sort((left, right) => left.startsAt.compareTo(right.startsAt));
    final currentEvents = events.where(
      (event) =>
          !event.startsAt.isAfter(query.now) && event.endsAt.isAfter(query.now),
    );
    final nextEvents = events.where(
      (event) => event.startsAt.isAfter(query.now),
    );
    final nowId = currentEvents.isEmpty ? null : currentEvents.first.id;
    final nextId = nextEvents.isEmpty ? null : nextEvents.first.id;
    final overdue = items.whereType<TaskItem>().where(
      (task) =>
          !task.isCompleted &&
          task.deadline != null &&
          task.deadline!.isBefore(query.now),
    );
    return DaySnapshot(
      personId: query.personId,
      date: query.date,
      generatedAt: query.now,
      timezoneOffsetSeconds: query.timezoneOffsetSeconds,
      items: List.unmodifiable(items),
      nowEventId: nowId,
      nextEventId: nextId,
      overdueTaskCount: overdue.length,
    );
  }
}
