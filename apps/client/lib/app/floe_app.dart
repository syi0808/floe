import 'package:floe_client/features/connections/application/remote_access_gateway.dart';
import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';

import 'dart:async';

import 'package:flutter/material.dart';

import 'package:floe_client/l10n/app_localizations.dart';

import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/personal_day_screen.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/infrastructure/native/macos_context_gateway.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_toast.dart';
import 'package:floe_client/app/local_identity.dart';

class FloeApp extends StatefulWidget {
  const FloeApp({
    super.key,
    required this.gateway,
    this.calendarActions,
    this.query,
    this.agentGateway,
    this.pairingGateway,
    this.remoteAccessGateway,
    this.serverClient,
    this.androidContext,
    this.appleContext,
    this.macOSContext,
    this.onDisposeGateway,
    this.locale = const Locale('en'),
    this.builder,
  });
  final Locale locale;
  final DayGateway gateway;

  /// Proposal, approval and execution of calendar actions.
  final CalendarActionExecutionGateway? calendarActions;

  final DayQuery? query;
  final AgentConversationGateway? agentGateway;
  final RemotePairingGateway? pairingGateway;
  final RemoteAccessGateway? remoteAccessGateway;
  final LocalServerClient? serverClient;
  final AndroidContextApi? androidContext;
  final AppleContextApi? appleContext;
  final MacOSContextApi? macOSContext;
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
        pairingGateway: widget.pairingGateway,
        remoteAccessGateway: widget.remoteAccessGateway,
        serverClient: widget.serverClient,
        androidContext: widget.androidContext,
        appleContext: widget.appleContext,
        macOSContext: widget.macOSContext,
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
