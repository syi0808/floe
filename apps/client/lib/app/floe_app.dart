import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';

import '../features/day_canvas/application/day_gateway.dart';
import '../features/day_canvas/application/ffi_day_gateway.dart';
import '../features/day_canvas/domain/day_models.dart';
import '../features/day_canvas/presentation/personal_day_screen.dart';
import 'floe_theme.dart';
import 'floe_toast.dart';

class FloeApp extends StatefulWidget {
  const FloeApp({
    super.key,
    required this.gateway,
    this.query,
    this.locale = const Locale('en'),
    this.builder,
  });
  final Locale locale;
  final DayGateway gateway;
  final DayQuery? query;
  final TransitionBuilder? builder;

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
        DayQuery.local(
          personId: localPersonId,
          date: DateTime(now.year, now.month, now.day),
          now: now,
        );
    final home = FloeToastHost(
      child: PersonalDayScreen(gateway: widget.gateway, query: effectiveQuery),
    );
    return MaterialApp(
      title: 'Floe',
      debugShowCheckedModeBanner: false,
      theme: FloeTheme.light,
      locale: widget.locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: widget.builder == null
          ? home
          : Builder(builder: (context) => widget.builder!(context, home)),
    );
  }
}
