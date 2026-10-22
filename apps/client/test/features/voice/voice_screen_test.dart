import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/voice/application/voice_gateway.dart';
import 'package:floe_client/features/voice/presentation/voice_screen.dart';

void main() {
  testWidgets('voice cannot be dismissed before confirmed local Stop', (
    tester,
  ) async {
    final gateway = _RecoverableVoiceGateway(VoiceStartState.bound);
    var closed = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: VoiceScreen(
            ownerGateways: LocalOwnerGateways(voice: gateway),
            agentGateway: null,
            personId: 'fixture',
            onClose: () => closed++,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
    await tester.tap(find.byTooltip('Stop voice before closing'));
    await tester.pumpAndSettle();
    expect(closed, 0);
    await tester.tap(find.text('Stop voice'));
    await tester.pumpAndSettle();
    expect(gateway.stopCalls, 1);
    expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);
    await tester.tap(find.byTooltip('Back to Floe'));
    await tester.pumpAndSettle();
    expect(closed, 1);
    expect(tester.takeException(), isNull);
  });
  testWidgets('Voice screen describes the muted native audio boundary', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: VoiceScreen(
            ownerGateways: LocalOwnerGateways(),
            agentGateway: null,
            personId: 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Talk with Floe'), findsOneWidget);
    expect(find.text('Start voice'), findsOneWidget);
    expect(find.textContaining('checks microphone permission'), findsOneWidget);
    expect(
      find.textContaining('OpenAI’s experimental Codex live service'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('Voice screen restores and explicitly cancels a pending Start', (
    tester,
  ) async {
    final gateway = _RecoverableVoiceGateway();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: VoiceScreen(
            ownerGateways: LocalOwnerGateways(voice: gateway),
            agentGateway: null,
            personId: 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(gateway.recoverCalls, 1);
    expect(find.text('Connection pending'), findsOneWidget);
    expect(find.text('Cancel connection'), findsOneWidget);

    await tester.tap(find.text('Cancel connection'));
    await tester.pumpAndSettle();

    expect(gateway.cancelCalls, 1);
    expect(gateway.pendingCommandId, isNull);
    expect(find.text('Connection cancelled'), findsOneWidget);
    expect(find.textContaining('command ID'), findsNothing);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'active voice keeps Stop ahead of disclosure without implying microphone capture',
    (tester) async {
      final gateway = _RecoverableVoiceGateway(VoiceStartState.bound);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: VoiceScreen(
              ownerGateways: LocalOwnerGateways(voice: gateway),
              agentGateway: null,
              personId: 'fixture',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Voice session open'), findsOneWidget);
      expect(find.text('Microphone: not started'), findsOneWidget);
      expect(find.text('Start voice'), findsNothing);
      expect(
        tester.getTopLeft(find.text('Stop voice')).dy,
        lessThan(tester.getTopLeft(find.text('Before you start')).dy),
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets('owner failure retains its specific recovery instruction', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: VoiceScreen(
            ownerGateways: LocalOwnerGateways(
              voice: _UnavailableVoiceGateway(),
            ),
            agentGateway: null,
            personId: 'fixture',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.text('Voice access is disabled for this connection.'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
  for (final width in [320.0, 390.0, 1200.0]) {
    testWidgets('Voice fits $width with large text and preserves disclosures', (
      tester,
    ) async {
      tester.view.physicalSize = Size(width, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          builder: (context, child) => MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: const TextScaler.linear(2)),
            child: child!,
          ),
          home: const Scaffold(
            body: VoiceScreen(
              ownerGateways: LocalOwnerGateways(),
              agentGateway: null,
              personId: 'fixture',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('command journal'), findsNothing);
      expect(find.textContaining('lease'), findsNothing);
      expect(find.textContaining('microphone permission'), findsOneWidget);
      expect(find.textContaining('conversation context'), findsOneWidget);
      expect(
        find.textContaining('OpenAI’s experimental Codex live service'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Replies play on this device'),
        findsOneWidget,
      );
      expect(find.text('Check status'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }
}

final class _RecoverableVoiceGateway implements VoiceGateway {
  static const _commandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
  static const _surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';

  _RecoverableVoiceGateway([this._state = VoiceStartState.preparing]);
  VoiceStartState _state;
  int recoverCalls = 0;
  int cancelCalls = 0;
  int stopCalls = 0;
  bool released = false;

  @override
  String? get pendingCommandId =>
      _state == VoiceStartState.cancelled ? null : _commandId;

  VoiceStartStatus _status() => VoiceStartStatus(
    commandId: _commandId,
    surfaceSessionId: _surfaceId,
    state: _state,
    localLeaseReleased: released,
  );

  @override
  Future<VoiceStartStatus?> recoverPendingStart() async {
    recoverCalls++;
    return _status();
  }

  @override
  Future<VoiceStartStatus> start(String surfaceSessionId) async => _status();

  @override
  Future<VoiceStartStatus> cancelPendingStart() async {
    cancelCalls++;
    _state = VoiceStartState.cancelled;
    return _status();
  }

  @override
  Future<VoiceStartStatus> stop() async {
    stopCalls++;
    released = true;
    return _status();
  }

  @override
  Future<VoiceStartStatus?> readStatus(String commandId) async => _status();
}

final class _UnavailableVoiceGateway extends _RecoverableVoiceGateway {
  @override
  Future<VoiceStartStatus?> recoverPendingStart() async =>
      throw const AppWireTransportException(
        'permission_denied',
        'Voice access is disabled for this connection.',
      );
}
