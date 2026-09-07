import 'package:flutter/material.dart';

import '../../../app/design_tokens.dart';

enum ItemTone {
  violet(FloePalette.primary50, FloePalette.primary100, FloePalette.primary500),
  blue(FloePalette.blue50, FloePalette.blue100, FloePalette.blue500),
  mint(FloePalette.mint50, FloePalette.mint100, FloePalette.mint500),
  amber(FloePalette.amber50, FloePalette.amber100, FloePalette.amber500),
  coral(FloePalette.coral50, FloePalette.coral100, FloePalette.coral500);

  const ItemTone(this.fill, this.border, this.accent);
  final Color fill;
  final Color border;
  final Color accent;
}

class NoteAppearance {
  const NoteAppearance({
    required this.excerpt,
    required this.category,
    required this.timestamp,
    this.tone = ItemTone.violet,
  });
  final String excerpt;
  final String category;
  final String timestamp;
  final ItemTone tone;
}

class TaskAppearance {
  const TaskAppearance({
    required this.description,
    this.project,
    this.estimate,
    this.timeContext,
    this.suggestion,
    this.note,
    this.subtasks = const [],
  });
  final String description;
  final String? project;
  final String? estimate;
  final String? timeContext;
  final String? suggestion;
  final String? note;
  final List<({String title, String duration, bool done})> subtasks;
}

class DayAppearance extends InheritedWidget {
  const DayAppearance({
    required super.child,
    this.tones = const {},
    this.notes = const {},
    this.tasks = const {},
    this.dailyNote,
    super.key,
  });
  final Map<String, ItemTone> tones;
  final Map<String, NoteAppearance> notes;
  final Map<String, TaskAppearance> tasks;
  final String? dailyNote;

  static DayAppearance? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<DayAppearance>();
  static ItemTone tone(
    BuildContext context,
    String id, [
    ItemTone fallback = ItemTone.violet,
  ]) => of(context)?.tones[id] ?? fallback;

  @override
  bool updateShouldNotify(DayAppearance oldWidget) =>
      tones != oldWidget.tones ||
      notes != oldWidget.notes ||
      tasks != oldWidget.tasks ||
      dailyNote != oldWidget.dailyNote;
}
