import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';

import '../features/day_canvas/application/day_gateway.dart';
import '../features/day_canvas/domain/day_models.dart';
import '../features/day_canvas/presentation/personal_day_screen.dart';
import '../features/agent/agent_fixture_gateway.dart';
import '../features/server/local_server_client.dart';
import '../infrastructure/native/android_context_gateway.dart';
import '../infrastructure/native/apple_context_gateway.dart';
import 'floe_theme.dart';
import 'floe_toast.dart';
import 'local_identity.dart';

class FloeApp extends StatefulWidget {
  const FloeApp({
    super.key,
    required this.gateway,
    this.query,
    this.agentGateway,
    this.serverClient,
    this.androidContext,
    this.appleContext,
    this.onDisposeGateway,
    this.locale = const Locale('en'),
    this.builder,
  });
  final Locale locale;
  final DayGateway gateway;
  final DayQuery? query;
  final AgentFixtureStreamingGateway? agentGateway;
  final LocalServerClient? serverClient;
  final AndroidContextApi? androidContext;
  final AppleContextApi? appleContext;
  final Future<void> Function()? onDisposeGateway;
  final TransitionBuilder? builder;

  @override
  State<FloeApp> createState() => _FloeAppState();
}

class _FloeAppState extends State<FloeApp> {
  @override
  void dispose() {
    final onDisposeGateway = widget.onDisposeGateway;
    if (onDisposeGateway != null) unawaited(onDisposeGateway());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final now = DateTime.now();
    final effectiveQuery =
        widget.query ??
        DayQuery.local(
          personId: defaultLocalPersonId,
          date: DateTime(now.year, now.month, now.day),
          now: now,
        );
    final home = FloeToastHost(
      child: PersonalDayScreen(
        gateway: widget.gateway,
        query: effectiveQuery,
        agentGateway: widget.agentGateway,
        serverClient: widget.serverClient,
        androidContext: widget.androidContext,
        appleContext: widget.appleContext,
      ),
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
