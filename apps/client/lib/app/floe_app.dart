import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';

import '../features/day_canvas/application/day_gateway.dart';
import '../features/day_canvas/application/ffi_day_gateway.dart';
import '../features/day_canvas/domain/day_models.dart';
import '../features/day_canvas/presentation/personal_day_screen.dart';
import 'floe_theme.dart';

class FloeApp extends StatefulWidget {
  const FloeApp({
    super.key,
    required this.gateway,
    this.query,
    this.locale = const Locale('en'),
  });
  final Locale locale;
  final DayGateway gateway;
  final DayQuery? query;

  @override
  State<FloeApp> createState() => _FloeAppState();
}

class _FloeAppState extends State<FloeApp> {
  @override
  void dispose() {
    if (widget.gateway case FfiDayGateway gateway) {
      unawaited(gateway.close());
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final now = DateTime.now();
    final effectiveQuery =
        widget.query ??
        DayQuery(
          personId: localPersonId,
          date: DateTime(now.year, now.month, now.day),
          now: now,
          timezoneOffsetSeconds: now.timeZoneOffset.inSeconds,
        );
    return MaterialApp(
      title: 'Floe',
      debugShowCheckedModeBanner: false,
      theme: FloeTheme.light,
      locale: widget.locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: PersonalDayScreen(gateway: widget.gateway, query: effectiveQuery),
    );
  }
}
