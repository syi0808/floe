import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/floe_feedback.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_squircle.dart';
import '../application/calendar_gateway.dart';
import '../domain/day_models.dart';

class CalendarPanel extends StatefulWidget {
  const CalendarPanel({
    super.key,
    required this.gateway,
    required this.query,
    required this.connection,
    required this.onChanged,
  });
  final CalendarGateway gateway;
  final DayQuery query;
  final CalendarConnection? connection;
  final Future<void> Function() onChanged;

  @override
  State<CalendarPanel> createState() => _CalendarPanelState();
}

class _CalendarPanelState extends State<CalendarPanel> {
  bool busy = false;
  String? error;

  Future<void> _run(Future<void> Function() operation) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await operation();
    } on Object {
      if (mounted) {
        setState(() => error = '연결 또는 수집에 실패했습니다. 권한을 확인하고 다시 시도해 주세요.');
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _connect() => _run(() async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => AlertDialog(
        title: const Text('Calendar 연결'),
        content: const Text(
          '선택한 캘린더의 일정만 이 기기에 저장합니다. macOS는 읽기에도 읽기·쓰기 전체 접근을 요구하지만, Floe는 외부 일정을 변경하지 않습니다. 캘린더를 변경하면 이전 캘린더의 로컬 사본은 교체됩니다.',
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('취소'),
          ),
          FloeButton.filled(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('계속'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final calendars = await widget.gateway.calendars();
    if (!mounted) return;
    if (calendars.isEmpty) {
      setState(
        () => error = '사용 가능한 캘린더가 없습니다. macOS Calendar에서 캘린더를 추가해 주세요.',
      );
      return;
    }
    final choice = await showFloeDialog<CalendarChoice>(
      context,
      (context) => SimpleDialog(
        title: const Text('캘린더 선택'),
        children: calendars
            .map(
              (calendar) => SimpleDialogOption(
                onPressed: () => Navigator.pop(context, calendar),
                child: Text(calendar.name),
              ),
            )
            .toList(),
      ),
    );
    if (choice == null || !mounted) return;
    final query = widget.query;
    await widget.gateway.selectCalendar(choice, query);
    try {
      await widget.gateway.syncCalendar(query);
    } finally {
      await widget.onChanged();
    }
  });

  @override
  Widget build(BuildContext context) {
    final connection = widget.connection;
    final failure = connection?.error;
    final status = switch (failure) {
      'permission_denied' => '권한이 거절되었거나 철회되었습니다. 설정에서 허용한 뒤 재시도해 주세요.',
      'calendar_unavailable' => '선택한 캘린더를 찾을 수 없습니다. 다시 연결해 주세요.',
      'provider_unavailable' => '일정 수집에 실패했습니다. 마지막 저장 데이터를 표시합니다.',
      _ =>
        connection?.lastSuccessAt == null
            ? '아직 수집하지 않았습니다.'
            : '마지막 수집 ${connection!.lastSuccessAt!.toLocal()} · 저장된 데이터',
    };
    return FloeSquircle(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const FloeSquircle(
                size: FloeSquircleSize.md,
                fill: FloePalette.primary50,
                borderWidth: 0,
                padding: EdgeInsets.all(14),
                child: Icon(
                  LucideIcons.calendarDays,
                  size: 26,
                  color: FloePalette.primary600,
                ),
              ),
              const SizedBox(width: 16),
              const Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'macOS Calendar',
                      style: TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    SizedBox(height: 6),
                    Text(
                      'Calendars already on this Mac',
                      style: TextStyle(
                        fontSize: 13,
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ),
              ),
              const FloeReadOnlyPill(),
            ],
          ),
          const SizedBox(height: 24),
          const Text(
            'Bring your calendar into one day. Floe reads events; it never creates, edits, or deletes anything in Calendar.',
            style: TextStyle(
              fontSize: 14,
              height: 1.7,
              color: FloePalette.neutral600,
            ),
          ),
          const SizedBox(height: 28),
          const Divider(height: 1),
          const SizedBox(height: 28),
          const Text(
            'Connected calendar',
            style: TextStyle(
              fontSize: 11,
              letterSpacing: 1,
              color: FloePalette.neutral600,
            ),
          ),
          const SizedBox(height: 12),
          Text(
            connection?.name ?? 'Make room for your day.',
            style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 12),
          Text(
            connection == null
                ? 'Choose a calendar to start. This client currently supports one calendar at a time.'
                : status,
            style: const TextStyle(
              fontSize: 13,
              height: 1.7,
              color: FloePalette.neutral600,
            ),
          ),
          if (connection != null) ...[
            const SizedBox(height: 28),
            const Divider(height: 1),
            const SizedBox(height: 20),
            for (final entry in <String, String>{
              'Person': 'You · this device',
              'Stored range': connection.rangeStart == null
                  ? 'Not collected yet'
                  : '${connection.rangeStart} – ${connection.rangeEnd} (exclusive)',
              'Last successful read':
                  connection.lastSuccessAt?.toLocal().toString() ??
                  'Not collected yet',
            }.entries)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 12),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(
                      child: Text(
                        entry.key,
                        style: const TextStyle(
                          fontSize: 13,
                          color: FloePalette.neutral600,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        entry.value,
                        style: const TextStyle(fontSize: 13),
                      ),
                    ),
                  ],
                ),
              ),
          ],
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(top: 16),
              child: Text(
                error!,
                style: const TextStyle(color: FloePalette.coral700),
              ),
            ),
          const SizedBox(height: 24),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              if (connection != null)
                FloeButton.filled(
                  onPressed: busy
                      ? null
                      : () => _run(() async {
                          try {
                            await widget.gateway.syncCalendar(widget.query);
                          } finally {
                            await widget.onChanged();
                          }
                        }),
                  child: const Text('선택한 날짜 새로고침'),
                ),
              FloeButton.outlined(
                onPressed: busy ? null : _connect,
                child: Text(connection == null ? '연결' : '다시 연결 / 변경'),
              ),
              FloeButton.text(
                onPressed: busy
                    ? null
                    : () => _run(widget.gateway.openCalendarSettings),
                child: const Text('권한 설정'),
              ),
            ],
          ),
          if (busy)
            const Padding(
              padding: EdgeInsets.only(top: 20),
              child: Center(child: FloeDotSpinner()),
            ),
        ],
      ),
    );
  }
}
