import 'package:flutter/material.dart';

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
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
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
    final choice = await showDialog<CalendarChoice>(
      context: context,
      builder: (context) => SimpleDialog(
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
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: FloeSquircle(
        size: FloeSquircleSize.md,
        fill: FloePalette.neutral0,
        borderColor: FloePalette.neutral200,
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              connection == null
                  ? '외부 Calendar 연결'
                  : '${connection.provider == 'fixture' ? 'Fixture' : 'Calendar'} · ${connection.name}',
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            if (connection != null) Text(status),
            if (failure != null && connection?.lastSuccessAt != null)
              Text(
                '마지막 저장 데이터를 표시합니다 · ${connection!.lastSuccessAt!.toLocal()}',
              ),
            if (connection?.rangeStart != null)
              Text(
                '최근 수집 범위: ${connection!.rangeStart} ~ ${connection.rangeEnd} (종료일 제외)',
              ),
            if (error != null)
              Text(error!, style: const TextStyle(color: FloePalette.coral700)),
            Wrap(
              spacing: 8,
              children: [
                FloeButton.text(
                  onPressed: busy ? null : _connect,
                  child: Text(connection == null ? '연결' : '다시 연결 / 변경'),
                ),
                if (connection != null)
                  FloeButton.text(
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
                FloeButton.text(
                  onPressed: busy
                      ? null
                      : () => _run(widget.gateway.openCalendarSettings),
                  child: const Text('권한 설정'),
                ),
              ],
            ),
            if (busy) const LinearProgressIndicator(),
          ],
        ),
      ),
    );
  }
}
