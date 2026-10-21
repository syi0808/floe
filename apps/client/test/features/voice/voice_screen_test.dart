import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/voice/application/voice_gateway.dart';
import 'package:floe_client/features/voice/presentation/voice_screen.dart';

void main() {
  testWidgets('Voice screen describes the muted native audio boundary', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
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

    expect(find.text('Live presentation'), findsOneWidget);
    expect(find.text('Start voice'), findsOneWidget);
    expect(
      find.textContaining('does not attach a microphone track'),
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
    expect(find.text('Status: preparing'), findsOneWidget);
    expect(find.text('Cancel pending Start'), findsOneWidget);

    await tester.tap(find.text('Cancel pending Start'));
    await tester.pumpAndSettle();

    expect(gateway.cancelCalls, 1);
    expect(gateway.pendingCommandId, isNull);
    expect(find.text('Status: cancelled'), findsOneWidget);
    expect(
      find.text(
        'Voice Start was cancelled. A later Start uses a new command ID.',
      ),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}

final class _RecoverableVoiceGateway implements VoiceGateway {
  static const _commandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
  static const _surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';

  VoiceStartState _state = VoiceStartState.preparing;
  int recoverCalls = 0;
  int cancelCalls = 0;

  @override
  String? get pendingCommandId =>
      _state == VoiceStartState.cancelled ? null : _commandId;

  VoiceStartStatus _status() => VoiceStartStatus(
    commandId: _commandId,
    surfaceSessionId: _surfaceId,
    state: _state,
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
  Future<VoiceStartStatus> stop() async => _status();

  @override
  Future<VoiceStartStatus?> readStatus(String commandId) async => _status();
}
