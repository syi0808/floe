import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/voice/presentation/voice_screen.dart';

void main() {
  testWidgets('Voice screen presents the native backend boundary cleanly', (
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
      find.textContaining('This build has no native audio backend.'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}
