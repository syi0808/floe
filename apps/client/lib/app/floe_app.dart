import 'package:flutter/material.dart';

import '../features/day_canvas/application/day_gateway.dart';
import '../features/day_canvas/application/fake_day_gateway.dart';
import '../features/day_canvas/domain/day_models.dart';
import '../features/day_canvas/presentation/personal_day_screen.dart';
import 'floe_theme.dart';

class FloeApp extends StatelessWidget {
  const FloeApp({super.key, this.gateway, this.query});
  final DayGateway? gateway;
  final DayQuery? query;

  @override
  Widget build(BuildContext context) {
    final now = DateTime.now();
    final effectiveQuery =
        query ??
        DayQuery(
          personId: 'local-person',
          date: DateTime(now.year, now.month, now.day),
          now: now,
          timezoneOffsetSeconds: now.timeZoneOffset.inSeconds,
        );
    return MaterialApp(
      title: 'Floe',
      debugShowCheckedModeBanner: false,
      theme: FloeTheme.light,
      home: PersonalDayScreen(
        gateway: gateway ?? FakeDayGateway(),
        query: effectiveQuery,
      ),
    );
  }
}
