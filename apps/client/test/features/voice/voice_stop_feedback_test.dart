import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/voice/application/voice_gateway.dart';
import 'package:floe_client/features/voice/infrastructure/app_wire_voice_gateway.dart';
import 'package:floe_client/features/voice/presentation/voice_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  setUpAll(_loadBundledFonts);

  testWidgets(
    'pending Stop is named, visible and waits for owner confirmation',
    (tester) async {
      _setViewport(tester, const Size(390, 844));
      final gateway = _SyntheticVoiceGateway(recovered: _active);
      final harness = await _mount(tester, gateway: gateway);
      final semantics = tester.ensureSemantics();
      try {
        await tester.tap(find.text('Stop voice'));
        await tester.pump();

        expect(gateway.stopCalls, 1);
        expect(find.text('Stopping voice'), findsOneWidget);
        expect(find.text('Voice session open'), findsNothing);
        expect(find.text('Microphone: on'), findsOneWidget);
        expect(find.text('Service: connected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
        final stopButton = tester.getSemantics(find.byType(FilledButton).first);
        expect(stopButton.label, 'Stop voice');
        expect(
          stopButton.getSemanticsData().flagsCollection.isEnabled,
          ui.Tristate.isFalse,
        );
        expect(
          stopButton.getSemanticsData().flagsCollection.isLiveRegion,
          isTrue,
        );
        expect(tester.getRect(find.text('Stopping voice')).top, greaterThan(0));
        expect(tester.takeException(), isNull);
        await expectLater(
          find.byType(VoiceScreen),
          matchesGoldenFile('goldens/voice-stop-pending-390.png'),
        );

        // A repeated pointer activation while the button is disabled cannot
        // submit a second Stop command.
        await tester.tap(find.byType(FilledButton).first);
        await tester.pump();
        expect(gateway.stopCalls, 1);

        gateway.pendingCommandId = null;
        gateway.stopResult.complete(_stopped);
        await tester.pumpAndSettle();

        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Stopping voice'), findsNothing);
        expect(find.text('Microphone: stopped'), findsOneWidget);
        expect(find.text('Service: disconnected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);
        expect(tester.takeException(), isNull);
      } finally {
        semantics.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets(
    '320px 200% Stop status and recovery remain scroll-reachable inside safe area',
    (tester) async {
      _setViewport(tester, const Size(320, 720), safeTop: 24, safeBottom: 34);
      final gateway = _SyntheticVoiceGateway(recovered: _active);
      final harness = await _mount(tester, gateway: gateway, textScale: 2);
      final semantics = tester.ensureSemantics();
      try {
        await tester.scrollUntilVisible(
          find.text('Stop voice'),
          100,
          scrollable: find.byType(Scrollable).first,
        );
        final stopRect = tester.getRect(find.text('Stop voice'));
        expect(stopRect.top, greaterThanOrEqualTo(24));
        expect(stopRect.bottom, lessThan(686));
        expect(tester.takeException(), isNull);
        await tester.tap(find.text('Stop voice'));
        await tester.pump();

        // Bring the live status into view after activating Stop lower in the
        // large-text list. The status remains a normal, unclipped list item.
        await tester.drag(find.byType(ListView), const Offset(0, 600));
        await tester.pump(const Duration(milliseconds: 100));
        expect(find.text('Stopping voice'), findsOneWidget);
        final statusRect = tester.getRect(find.text('Stopping voice'));
        expect(statusRect.top, greaterThanOrEqualTo(24));
        expect(statusRect.bottom, lessThan(686));
        expect(find.text('Microphone: on'), findsOneWidget);
        expect(find.text('Service: connected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
        expect(tester.takeException(), isNull);
        await expectLater(
          find.byType(VoiceScreen),
          matchesGoldenFile('goldens/voice-stop-pending-320-200.png'),
        );

        gateway.stopResult.complete(_stopped);
        gateway.pendingCommandId = null;
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(tester.takeException(), isNull);
      } finally {
        semantics.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets('failed Stop stays unconfirmed and Check status recovers it', (
    tester,
  ) async {
    _setViewport(tester, const Size(320, 720), safeTop: 24, safeBottom: 34);
    final gateway = _SyntheticVoiceGateway(
      recovered: _active,
      stopFailure: const AppWireTransportException(
        'command_outcome_unknown',
        'Stop outcome is not confirmed. Check the status before trying again.',
        commandOutcome: CommandOutcome.indeterminate,
      ),
    );
    final harness = await _mount(tester, gateway: gateway, textScale: 2);
    final semantics = tester.ensureSemantics();
    try {
      await tester.scrollUntilVisible(
        find.text('Stop voice'),
        100,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.tap(find.text('Stop voice'));
      await tester.pump();
      await tester.drag(find.byType(ListView), const Offset(0, 600));
      await tester.pump(const Duration(milliseconds: 100));

      expect(find.text('Stopping voice'), findsNothing);
      expect(find.text('Voice session open'), findsOneWidget);
      final statusReadLoading = tester
          .getSemantics(find.byType(FilledButton).first)
          .getSemanticsData();
      expect(statusReadLoading.label, 'Loading');
      expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
      gateway.readResult.complete(null);
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Stop outcome is not confirmed. Check the status before trying again.',
        ),
        findsOneWidget,
      );
      expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);

      await tester.drag(find.byType(ListView), const Offset(0, -600));
      await tester.pumpAndSettle();
      await tester.drag(find.byType(ListView), const Offset(0, -300));
      await tester.pumpAndSettle();
      final checkStatusRect = tester.getRect(find.text('Check status'));
      expect(checkStatusRect.top, greaterThanOrEqualTo(24));
      expect(checkStatusRect.bottom, lessThan(686));
      gateway.queueReadResult(_closing);
      await tester.tap(find.text('Check status'));
      await tester.pumpAndSettle();

      expect(gateway.readStatusCalls, 2);
      await tester.drag(find.byType(ListView), const Offset(0, 1000));
      await tester.pumpAndSettle();
      expect(find.text('Stopping voice'), findsNothing);
      expect(find.text('Voice session open'), findsOneWidget);
      expect(find.text('Microphone: on'), findsOneWidget);
      expect(find.text('Service: disconnecting'), findsOneWidget);
      expect(find.text('Stopped on this device'), findsNothing);
      expect(find.text('Check status'), findsOneWidget);
      expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
      expect(tester.takeException(), isNull);
    } finally {
      semantics.dispose();
      await tester.pumpWidget(const SizedBox.shrink());
      harness.dispose();
    }
  });

  testWidgets('a returned closing receipt is not presented as completed Stop', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 844));
    final gateway = _SyntheticVoiceGateway(
      recovered: _active,
      stopResponse: _closing,
    );
    final harness = await _mount(tester, gateway: gateway);
    try {
      await tester.tap(find.text('Stop voice'));
      await tester.pumpAndSettle();

      expect(find.text('Stopping voice'), findsNothing);
      expect(find.text('Voice session open'), findsOneWidget);
      expect(find.text('Microphone: on'), findsOneWidget);
      expect(find.text('Service: disconnecting'), findsOneWidget);
      expect(find.text('Stopped on this device'), findsNothing);
      expect(find.text('Check status'), findsOneWidget);
      expect(find.text('Stop voice'), findsOneWidget);
      expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
      expect(tester.takeException(), isNull);
    } finally {
      await tester.pumpWidget(const SizedBox.shrink());
      harness.dispose();
    }
  });

  testWidgets('Start, Cancel and Check status busy states are not stopping', (
    tester,
  ) async {
    for (final action in ['Start voice', 'Cancel connection', 'Check status']) {
      final initial = switch (action) {
        'Start voice' => null,
        'Cancel connection' => _pending,
        'Check status' => _active,
        _ => throw StateError('Unexpected synthetic action.'),
      };
      final gateway = _SyntheticVoiceGateway(
        recovered: initial,
        pendingCommandId: action == 'Start voice'
            ? null
            : _SyntheticVoiceGateway._commandId,
      );
      final harness = await _mount(tester, gateway: gateway);
      SemanticsHandle? semantics;
      try {
        await tester.tap(find.text(action));
        await tester.pump();
        expect(find.text('Stopping voice'), findsNothing, reason: action);
        if (action == 'Check status') {
          semantics = tester.ensureSemantics();
          expect(find.text('Voice session open'), findsOneWidget);
          expect(find.text('Service: connected'), findsOneWidget);
          final stopLoading = tester
              .getSemantics(find.byType(FilledButton).first)
              .getSemanticsData();
          expect(stopLoading.label, 'Loading');
        }
        expect(tester.takeException(), isNull);

        switch (action) {
          case 'Start voice':
            gateway.startResult.complete(_pending);
          case 'Cancel connection':
            gateway.cancelResult.complete(_cancelled);
          case 'Check status':
            gateway.readResult.complete(_active);
        }
        await tester.pumpAndSettle();
      } finally {
        semantics?.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    }
  });

  testWidgets(
    'recovery that settles during Stop cannot replace its status or facts',
    (tester) async {
      _setViewport(tester, const Size(390, 844));
      final transport = _RaceVoiceTransport();
      final voice = AppWireVoiceGateway(transport);
      final runtimeGateway = _ControlledRuntimeGateway();
      final harness = await _mount(
        tester,
        gateway: voice,
        runtimeGateway: runtimeGateway,
      );
      final semantics = tester.ensureSemantics();
      try {
        expect(find.text('Voice session open'), findsOneWidget);
        expect(find.text('Microphone: on'), findsOneWidget);
        expect(find.text('Service: connected'), findsOneWidget);
        final recovery = transport.holdNextRecovery();
        await _notifyReady(runtimeGateway, harness.runtime, tester, 'before');
        expect(transport.recoverableQueries, 2);

        await tester.tap(find.text('Stop voice'));
        await tester.pump();
        expect(transport.stopCommands, 1);
        expect(transport.commands, hasLength(1));
        expect(transport.commands.single['command'], {
          'kind': 'voice.stop',
          'voice_session_id': _SyntheticVoiceGateway._commandId,
          'surface_session_id': _SyntheticVoiceGateway._surfaceId,
        });

        // A null journal result is followed by an exact readback of the
        // previously known command. Neither read may erase pending Stop UI.
        recovery.complete(_recoverableReply(null));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 1));
        expect(transport.statusReads, 1);
        expect(find.text('Stopping voice'), findsOneWidget);
        expect(find.text('Stop voice'), findsOneWidget);
        expect(find.text('Voice session open'), findsNothing);
        expect(find.text('Microphone: on'), findsOneWidget);
        expect(find.text('Service: connected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isFalse);
        expect(
          tester.getSemantics(find.byType(FilledButton).first).label,
          'Stop voice',
        );
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        expect(transport.stopCommands, 1);

        transport.stopResult.complete(_stopReply());
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Stop voice'), findsNothing);
        expect(find.text('Start voice'), findsOneWidget);
        expect(find.text('Microphone: stopped'), findsOneWidget);
        expect(find.text('Service: disconnected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);
        expect(transport.stopCommands, 1);
        expect(tester.takeException(), isNull);
      } finally {
        semantics.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets(
    'pre-Stop recovery completing after Stop cannot overwrite its receipt',
    (tester) async {
      _setViewport(tester, const Size(390, 844));
      final transport = _RaceVoiceTransport();
      final voice = AppWireVoiceGateway(transport);
      final runtimeGateway = _ControlledRuntimeGateway();
      final harness = await _mount(
        tester,
        gateway: voice,
        runtimeGateway: runtimeGateway,
      );
      try {
        final recovery = transport.holdNextRecovery();
        await _notifyReady(
          runtimeGateway,
          harness.runtime,
          tester,
          'before-late',
        );
        await tester.tap(find.text('Stop voice'));
        await tester.pump();
        expect(transport.stopCommands, 1);

        transport.stopResult.complete(_stopReply());
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Microphone: stopped'), findsOneWidget);
        expect(find.text('Service: disconnected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);

        // Deliberately complete with a stale but recoverable call snapshot.
        recovery.complete(_recoverableReply(_activeWireStatus()));
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Start voice'), findsOneWidget);
        expect(find.text('Voice session open'), findsNothing);
        expect(find.text('Microphone: stopped'), findsOneWidget);
        expect(find.text('Service: disconnected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);
        expect(transport.stopCommands, 1);
        expect(tester.takeException(), isNull);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets(
    'recovery started during Stop is fenced, then a fresh settled read applies',
    (tester) async {
      _setViewport(tester, const Size(390, 844));
      final transport = _RaceVoiceTransport();
      final voice = AppWireVoiceGateway(transport);
      final runtimeGateway = _ControlledRuntimeGateway();
      final harness = await _mount(
        tester,
        gateway: voice,
        runtimeGateway: runtimeGateway,
      );
      try {
        await tester.tap(find.text('Stop voice'));
        await tester.pump();
        expect(transport.stopCommands, 1);

        final lateRecovery = transport.holdNextRecovery();
        await _notifyReady(runtimeGateway, harness.runtime, tester, 'during');
        expect(transport.recoverableQueries, 2);

        transport.stopResult.complete(_stopReply());
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Start voice'), findsOneWidget);
        lateRecovery.complete(_recoverableReply(_activeWireStatus()));
        await tester.pumpAndSettle();
        expect(find.text('Stopped on this device'), findsOneWidget);
        expect(find.text('Microphone: stopped'), findsOneWidget);
        expect(find.text('Service: disconnected'), findsOneWidget);
        expect(tester.widget<PopScope>(find.byType(PopScope)).canPop, isTrue);
        expect(transport.stopCommands, 1);

        final freshRecovery = transport.holdNextRecovery();
        await _notifyReady(runtimeGateway, harness.runtime, tester, 'fresh');
        freshRecovery.complete(_recoverableReply(_activeWireStatus()));
        await tester.pumpAndSettle();
        expect(find.text('Voice session open'), findsOneWidget);
        expect(find.text('Stop voice'), findsOneWidget);
        expect(find.text('Microphone: on'), findsOneWidget);
        expect(find.text('Service: connected'), findsOneWidget);
        expect(transport.recoverableQueries, 3);
        expect(transport.stopCommands, 1);
        expect(transport.commands, hasLength(1));
        expect(tester.takeException(), isNull);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets(
    'stale reads cannot replace newer Start or Cancel results or identity',
    (tester) async {
      _setViewport(tester, const Size(390, 844));
      final transport = _StartCancelRaceTransport();
      final voice = AppWireVoiceGateway(transport);
      final runtimeGateway = _ControlledRuntimeGateway();
      final harness = await _mount(
        tester,
        gateway: voice,
        runtimeGateway: runtimeGateway,
      );
      try {
        expect(find.text('Start voice'), findsOneWidget);
        expect(voice.pendingCommandId, isNull);

        final preStartRead = transport.holdNextRecovery();
        await _notifyReady(
          runtimeGateway,
          harness.runtime,
          tester,
          'before-start',
        );
        expect(transport.recoverableQueries, 2);
        await tester.tap(find.text('Start voice'));
        await tester.pumpAndSettle();

        final startRequest = transport.commands.single;
        final startedId = startRequest['command_id'] as String;
        expect(startRequest['command'], {
          'kind': 'voice.start',
          'surface_session_id': _SyntheticVoiceGateway._commandId,
        });
        expect(voice.pendingCommandId, startedId);
        expect(find.text('Connection pending'), findsOneWidget);
        expect(find.text('Cancel connection'), findsOneWidget);
        expect(find.text('Microphone: not started'), findsOneWidget);

        preStartRead.complete(
          _recoverableReply(
            _preparingWireStatus(_SyntheticVoiceGateway._commandId),
          ),
        );
        await tester.pumpAndSettle();
        expect(voice.pendingCommandId, startedId);
        expect(find.text('Connection pending'), findsOneWidget);
        expect(find.text('Cancel connection'), findsOneWidget);
        expect(find.text('Voice session open'), findsNothing);

        final duringCancelRead = transport.holdNextRecovery();
        await _notifyReady(
          runtimeGateway,
          harness.runtime,
          tester,
          'before-cancel',
        );
        expect(transport.recoverableQueries, 4);
        await tester.tap(find.text('Cancel connection'));
        await tester.pumpAndSettle();

        final cancelRequest = transport.commands.last;
        expect(cancelRequest['command'], {
          'kind': 'voice.start.cancel',
          'surface_session_id': _SyntheticVoiceGateway._commandId,
        });
        expect(cancelRequest['command_id'], startedId);
        expect(voice.pendingCommandId, isNull);
        expect(find.text('Connection cancelled'), findsOneWidget);
        expect(find.text('Start voice'), findsOneWidget);

        duringCancelRead.complete(
          _recoverableReply(_preparingWireStatus(startedId)),
        );
        await tester.pumpAndSettle();
        expect(voice.pendingCommandId, isNull);
        expect(find.text('Connection cancelled'), findsOneWidget);
        expect(find.text('Start voice'), findsOneWidget);
        expect(find.text('Cancel connection'), findsNothing);
        expect(transport.commands, hasLength(2));
        expect(
          transport.commands.map((request) => request['command']['kind']),
          ['voice.start', 'voice.start.cancel'],
        );
        expect(tester.takeException(), isNull);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        harness.dispose();
      }
    },
  );

  testWidgets('a pending runtime recovery completing after disposal is inert', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 844));
    final transport = _RaceVoiceTransport();
    final voice = AppWireVoiceGateway(transport);
    final runtimeGateway = _ControlledRuntimeGateway();
    final harness = await _mount(
      tester,
      gateway: voice,
      runtimeGateway: runtimeGateway,
    );
    final recovery = transport.holdNextRecovery();
    await _notifyReady(runtimeGateway, harness.runtime, tester, 'dispose');
    expect(transport.recoverableQueries, 2);

    await tester.pumpWidget(const SizedBox.shrink());
    recovery.complete(_recoverableReply(_activeWireStatus()));
    await tester.pumpAndSettle();

    expect(transport.commands, isEmpty);
    expect(transport.statusReads, 0);
    expect(tester.takeException(), isNull);
    harness.dispose();
  });

  testWidgets('a pending Stop receipt completing after disposal is inert', (
    tester,
  ) async {
    _setViewport(tester, const Size(390, 844));
    final transport = _RaceVoiceTransport();
    final voice = AppWireVoiceGateway(transport);
    final harness = await _mount(
      tester,
      gateway: voice,
      runtimeGateway: _ControlledRuntimeGateway(),
    );

    await tester.tap(find.text('Stop voice'));
    await tester.pump();
    expect(transport.stopCommands, 1);
    await tester.pumpWidget(const SizedBox.shrink());
    transport.stopResult.complete(_stopReply());
    await tester.pumpAndSettle();

    expect(transport.stopCommands, 1);
    expect(tester.takeException(), isNull);
    harness.dispose();
  });
}

void _setViewport(
  WidgetTester tester,
  Size size, {
  double safeTop = 0,
  double safeBottom = 0,
}) {
  final padding = FakeViewPadding(top: safeTop, bottom: safeBottom);
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  tester.view.viewPadding = padding;
  tester.view.padding = padding;
  tester.view.viewInsets = FakeViewPadding.zero;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetViewPadding);
  addTearDown(tester.view.resetPadding);
  addTearDown(tester.view.resetViewInsets);
}

Future<_Harness> _mount(
  WidgetTester tester, {
  required VoiceGateway gateway,
  RuntimeGateway? runtimeGateway,
  double textScale = 1,
}) async {
  final runtime = RuntimeController(
    gateway: runtimeGateway ?? _SyntheticRuntimeGateway(),
    personId: 'synthetic-person',
  );
  await runtime.open();
  await tester.pumpWidget(
    MaterialApp(
      theme: FloeTheme.light,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: Scaffold(
        body: SafeArea(
          child: Center(
            child: Dialog(
              insetPadding: const EdgeInsets.all(16),
              child: ConstrainedBox(
                constraints: const BoxConstraints(
                  maxWidth: 560,
                  maxHeight: 640,
                ),
                child: VoiceScreen(
                  ownerGateways: LocalOwnerGateways(
                    runtime: runtime,
                    voice: gateway,
                  ),
                  agentGateway: _SyntheticAgentGateway(),
                  personId: 'synthetic-person',
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return _Harness(runtime, gateway);
}

Future<void> _loadBundledFonts() async {
  final manifest = jsonDecode(
    await rootBundle.loadString('FontManifest.json'),
  ) as List<dynamic>;
  for (final value in manifest) {
    final entry = value as Map<String, dynamic>;
    final loader = FontLoader(entry['family'] as String);
    for (final font in entry['fonts'] as List<dynamic>) {
      loader.addFont(
        rootBundle.load((font as Map<String, dynamic>)['asset'] as String),
      );
    }
    await loader.load();
  }
}

final class _Harness {
  _Harness(this.runtime, this.gateway);
  final RuntimeController runtime;
  final VoiceGateway gateway;
  void dispose() {
    runtime.dispose();
  }
}

final class _ControlledRuntimeGateway implements RuntimeGateway {
  final List<Completer<RuntimeReadinessSnapshot>> _readiness = [];
  int readinessCalls = 0;

  Completer<RuntimeReadinessSnapshot> holdNextReadiness() {
    final completer = Completer<RuntimeReadinessSnapshot>();
    _readiness.add(completer);
    return completer;
  }

  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) {
    readinessCalls++;
    if (_readiness.isEmpty) {
      return Future.value(
        const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
      );
    }
    return _readiness.removeAt(0).future;
  }

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      throw StateError('Synthetic voice race never prepares the Runtime.');

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      throw StateError('Synthetic voice race never polls the Runtime.');

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      throw StateError('Synthetic voice race never acknowledges the Runtime.');
}

final class _RaceVoiceTransport implements AppWireTransport {
  final List<Map<String, dynamic>> commands = [];
  final List<Completer<Map<String, dynamic>>> _heldRecoveries = [];
  final Completer<Map<String, dynamic>> stopResult = Completer();
  int recoverableQueries = 0;
  int statusReads = 0;

  int get stopCommands => commands
      .where((request) => request['command']?['kind'] == 'voice.stop')
      .length;

  Completer<Map<String, dynamic>> holdNextRecovery() {
    final completer = Completer<Map<String, dynamic>>();
    _heldRecoveries.add(completer);
    return completer;
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    commands.add(request);
    if (request['command']?['kind'] != 'voice.stop') {
      throw StateError('The synthetic race permits only the requested Stop.');
    }
    return stopResult.future;
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final kind = request['query']?['kind'];
    if (kind == 'voice.start.recoverable') {
      recoverableQueries++;
      if (recoverableQueries == 1) {
        return Future.value(_recoverableReply(_activeWireStatus()));
      }
      if (_heldRecoveries.isEmpty) {
        throw StateError('No synthetic recoverable read was queued.');
      }
      return _heldRecoveries.removeAt(0).future;
    }
    if (kind == 'voice.start.status') {
      statusReads++;
      return Future.value(_statusReply(_stoppedWireStatus()));
    }
    throw StateError('Unexpected synthetic Voice query: $kind');
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw StateError('The synthetic race has no event transport.');

  @override
  Future<void> close() async {}
}

final class _StartCancelRaceTransport implements AppWireTransport {
  final List<Map<String, dynamic>> commands = [];
  final List<Completer<Map<String, dynamic>>> _heldRecoveries = [];
  int recoverableQueries = 0;

  Completer<Map<String, dynamic>> holdNextRecovery() {
    final completer = Completer<Map<String, dynamic>>();
    _heldRecoveries.add(completer);
    return completer;
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    commands.add(request);
    final kind = request['command']?['kind'];
    if (kind == 'voice.start') {
      return {
        'kind': kind,
        'status': _preparingWireStatus(
          request['command_id'] as String,
          request['command']['surface_session_id'] as String,
        ),
      };
    }
    if (kind == 'voice.start.cancel') {
      return {
        'kind': kind,
        'status': _cancelledWireStatus(
          request['command_id'] as String,
          request['command']['surface_session_id'] as String,
        ),
      };
    }
    throw StateError('Unexpected synthetic command: $kind');
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    final kind = request['query']?['kind'];
    if (kind == 'voice.start.recoverable') {
      recoverableQueries++;
      if (_heldRecoveries.isNotEmpty) return _heldRecoveries.removeAt(0).future;
      return Future.value({'kind': kind, 'status': null});
    }
    throw StateError('Unexpected synthetic Voice query: $kind');
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw StateError('Synthetic race has no event transport.');

  @override
  Future<void> close() async {}
}

Map<String, dynamic> _cancelledWireStatus(String commandId, String surfaceId) =>
    {
      'command_id': commandId,
      'surface_session_id': surfaceId,
      'state': 'cancelled',
      'media': null,
      'remote': null,
      'stop_requested': null,
      'local_lease_released': null,
    };

Future<void> _notifyReady(
  _ControlledRuntimeGateway gateway,
  RuntimeController runtime,
  WidgetTester tester,
  String incident,
) async {
  final readiness = gateway.holdNextReadiness();
  runtime.reportFailure(
    AppOwnerException.fromAppWire(
      'vault_locked',
      ownerFailure: OwnerFailure(
        domain: 'vault',
        category: 'user_configuration',
        reason: 'vault_locked',
        incidentId: 'synthetic-$incident',
        correlationId: 'synthetic-correlation-$incident',
        reloadRequired: true,
        sealSession: true,
        recovery: 'reobserve',
        safeActions: const {'retry'},
      ),
    ),
  );
  await tester.pump();
  expect(gateway.readinessCalls, greaterThan(1));
  readiness.complete(
    const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready),
  );
  // Stop owns an indeterminate-duration loading indicator, so settle only a
  // bounded pair of frames while the synthetic recovery request is held.
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 1));
}

Map<String, dynamic> _recoverableReply(Map<String, dynamic>? status) => {
  'kind': 'voice.start.recoverable',
  'status': status,
};

Map<String, dynamic> _statusReply(Map<String, dynamic>? status) => {
  'kind': 'voice.start.status',
  'status': status,
};

Map<String, dynamic> _stopReply() => {
  'kind': 'voice.stop',
  'status': _stoppedWireStatus(),
};

Map<String, dynamic> _activeWireStatus() => {
  'command_id': _SyntheticVoiceGateway._commandId,
  'surface_session_id': _SyntheticVoiceGateway._surfaceId,
  'state': 'bound',
  'media': 'capturing',
  'remote': 'connected',
  'stop_requested': false,
  'local_lease_released': false,
};

Map<String, dynamic> _preparingWireStatus(
  String commandId, [
  String surfaceId = _SyntheticVoiceGateway._commandId,
]) => {
  'command_id': commandId,
  'surface_session_id': surfaceId,
  'state': 'preparing',
  'media': null,
  'remote': null,
  'stop_requested': null,
  'local_lease_released': null,
};

Map<String, dynamic> _stoppedWireStatus() => {
  'command_id': _SyntheticVoiceGateway._commandId,
  'surface_session_id': _SyntheticVoiceGateway._surfaceId,
  'state': 'bound',
  'media': 'stopped',
  'remote': 'closed',
  'stop_requested': true,
  'local_lease_released': true,
};

final class _SyntheticRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);
  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      throw StateError('Synthetic voice tests never prepare the Runtime.');
  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      throw StateError('Synthetic voice tests never poll the Runtime.');
  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      throw StateError('Synthetic voice tests never acknowledge the Runtime.');
}

final class _SyntheticAgentGateway implements AgentConversationGateway {
  @override
  ConversationRuntimeGateway get conversationRuntime =>
      throw StateError('Only the synthetic conversation snapshot is used.');
  @override
  AgentInteractionGateway get interactionGateway =>
      throw StateError('Only the synthetic conversation snapshot is used.');
  @override
  Future<AgentSession> startConversation(String personId) async =>
      throw StateError('Voice UI tests never start a conversation.');
  @override
  Future<AgentSession?> settlePendingSessionStart(String personId) async =>
      throw StateError('Voice UI tests never settle a conversation.');
  @override
  Future<AgentSession?> resumeConversation(String personId) async =>
      _syntheticSession;
  @override
  Future<AgentSession> loadConversation(
    String personId,
    String sessionId,
  ) async => throw StateError('Voice UI tests never load a conversation.');
  @override
  Future<AgentSession> loadEarlierConversation(
    String personId,
    String sessionId,
    String beforeMessageId,
  ) async =>
      throw StateError('Voice UI tests never load conversation history.');
}

final AgentSession _syntheticSession = AgentSession.fromJson({
  'id': 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
  'person_id': 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e',
  'revision': 0,
  'active_turn': null,
  'last_outcome': null,
  'continuation_ref': null,
  'has_earlier_messages': false,
  'messages': <Object?>[],
  'usage': {
    'unknown_token_attempts': 0,
    'unknown_cost_attempts': 0,
    'model_attempts': 0,
    'estimated_tokens': 0,
    'iterations': 0,
    'capability_calls': 0,
    'tokens': 0,
    'cost_micros': 0,
    'estimated_cost_micros': 0,
  },
});

final class _SyntheticVoiceGateway implements VoiceGateway {
  _SyntheticVoiceGateway({
    required this.recovered,
    this.pendingCommandId = _commandId,
    this.stopResponse,
    this.stopFailure,
    List<VoiceStartStatus?> immediateReadResults = const [],
  }) : _immediateReadResults = List.of(immediateReadResults);

  static const _commandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
  static const _surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';

  final VoiceStartStatus? recovered;
  final VoiceStartStatus? stopResponse;
  final Object? stopFailure;
  final List<VoiceStartStatus?> _immediateReadResults;
  final Completer<VoiceStartStatus> stopResult = Completer();
  final Completer<VoiceStartStatus> startResult = Completer();
  final Completer<VoiceStartStatus> cancelResult = Completer();
  final Completer<VoiceStartStatus?> readResult = Completer();
  @override
  String? pendingCommandId;
  int stopCalls = 0;
  int startCalls = 0;
  int cancelCalls = 0;
  int readStatusCalls = 0;

  @override
  Future<VoiceStartStatus?> recoverPendingStart() async => recovered;

  @override
  Future<VoiceStartStatus> start(String surfaceSessionId) {
    startCalls++;
    pendingCommandId ??= _commandId;
    return startResult.future;
  }

  @override
  Future<VoiceStartStatus> cancelPendingStart() {
    cancelCalls++;
    return cancelResult.future;
  }

  @override
  Future<VoiceStartStatus> stop() {
    stopCalls++;
    if (stopFailure != null) return Future.error(stopFailure!);
    if (stopResponse != null) {
      return Future<VoiceStartStatus>.value(stopResponse!);
    }
    return stopResult.future;
  }

  @override
  Future<VoiceStartStatus?> readStatus(String commandId) {
    readStatusCalls++;
    if (_immediateReadResults.isNotEmpty) {
      return Future.value(_immediateReadResults.removeAt(0));
    }
    return readResult.future;
  }

  void queueReadResult(VoiceStartStatus? status) {
    _immediateReadResults.add(status);
  }
}

const _pending = VoiceStartStatus(
  commandId: _SyntheticVoiceGateway._commandId,
  surfaceSessionId: _SyntheticVoiceGateway._surfaceId,
  state: VoiceStartState.preparing,
  media: VoiceMediaState.preparing,
  remote: VoiceRemoteState.dispatching,
);

const _active = VoiceStartStatus(
  commandId: _SyntheticVoiceGateway._commandId,
  surfaceSessionId: _SyntheticVoiceGateway._surfaceId,
  state: VoiceStartState.bound,
  media: VoiceMediaState.capturing,
  remote: VoiceRemoteState.connected,
  stopRequested: false,
  localLeaseReleased: false,
);

const _closing = VoiceStartStatus(
  commandId: _SyntheticVoiceGateway._commandId,
  surfaceSessionId: _SyntheticVoiceGateway._surfaceId,
  state: VoiceStartState.bound,
  media: VoiceMediaState.capturing,
  remote: VoiceRemoteState.closeRequested,
  stopRequested: true,
  localLeaseReleased: false,
);

const _stopped = VoiceStartStatus(
  commandId: _SyntheticVoiceGateway._commandId,
  surfaceSessionId: _SyntheticVoiceGateway._surfaceId,
  state: VoiceStartState.bound,
  media: VoiceMediaState.stopped,
  remote: VoiceRemoteState.closed,
  stopRequested: true,
  localLeaseReleased: true,
);

const _cancelled = VoiceStartStatus(
  commandId: _SyntheticVoiceGateway._commandId,
  surfaceSessionId: _SyntheticVoiceGateway._surfaceId,
  state: VoiceStartState.cancelled,
  media: VoiceMediaState.stopped,
  remote: VoiceRemoteState.closed,
);
