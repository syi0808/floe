import 'package:floe_client/features/connections/presentation/connections_controller.dart';

import 'dart:async';

import 'package:flutter/material.dart';

import 'package:floe_client/l10n/app_localizations.dart';

import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/personal_day_screen.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_toast.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';

class FloeApp extends StatefulWidget {
  const FloeApp({
    super.key,
    required this.gateway,
    required this.personId,
    required this.ownerGateways,
    this.query,
    this.agentGateway,
    this.connectionsController,
    this.onDisposeGateway,
    this.locale = const Locale('en'),
    this.builder,
  });
  final Locale locale;
  final DayGateway gateway;
  final String personId;

  final DayQuery? query;
  final AgentConversationGateway? agentGateway;
  final ConnectionsController? connectionsController;
  final LocalOwnerGateways ownerGateways;
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
          personId: widget.personId,
          date: DateTime(now.year, now.month, now.day),
          now: now,
        );
    final home = FloeToastHost(
      child: PersonalDayScreen(
        gateway: widget.gateway,
        query: effectiveQuery,
        agentGateway: widget.agentGateway,
        connectionsController: widget.connectionsController,
        ownerGateways: widget.ownerGateways,
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
