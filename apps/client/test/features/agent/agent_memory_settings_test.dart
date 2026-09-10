import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_memory.dart';
import 'package:floe_client/features/agent/agent_memory_settings.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/server/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

void main() {
  testWidgets('memory settings presents saved memory in user language', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestVaultGateway(personId: 'person-1'),
      personId: 'person-1',
    )..memoryOverview = AgentMemoryOverview.fromJson(_overview);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SingleChildScrollView(
            child: AgentMemorySettings(controller: controller, onBack: () {}),
          ),
        ),
      ),
    );

    expect(find.text('Saved memories'), findsOneWidget);
    expect(find.text('Prefers focused mornings'), findsOneWidget);
    expect(find.textContaining('Learned with your approval'), findsOneWidget);
    expect(find.textContaining('confidence'), findsNothing);
    expect(find.byKey(const ValueKey('memory-memory-1')), findsOneWidget);
  });

  testWidgets('memory summary opens the management surface', (tester) async {
    final controller = AgentController(
      gateway: TestVaultGateway(personId: 'person-1'),
      personId: 'person-1',
    )..memoryOverview = AgentMemoryOverview.fromJson(_overview);
    addTearDown(controller.dispose);
    var opened = false;

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: AgentMemorySettingsCard(
            controller: controller,
            onManage: () => opened = true,
          ),
        ),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('manage-memory')));

    expect(opened, isTrue);
    expect(find.text('1 saved · 2 pending'), findsOneWidget);
  });

  testWidgets('data privacy opens the dedicated memory page', (tester) async {
    final gateway = _MemoryGateway();
    final controller = AgentController(gateway: gateway, personId: 'person-1');
    addTearDown(controller.dispose);
    await controller.load();

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SettingsScreen(client: null, agentController: controller),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('1 saved · 2 pending'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('manage-memory')));
    await tester.pumpAndSettle();

    expect(find.text('Saved memories'), findsOneWidget);
    expect(find.text('Prefers focused mornings'), findsOneWidget);
    expect(gateway.reads, 1);
  });
}

final class _MemoryGateway extends TestVaultGateway
    implements AgentMemoryGateway {
  _MemoryGateway() : super(personId: 'person-1') {
    state = AgentVaultState.ready;
  }

  int reads = 0;

  @override
  Future<AgentMemoryOverview> readMemory(String personId) async {
    reads++;
    return AgentMemoryOverview.fromJson(_overview);
  }
}

const _overview = <String, Object?>{
  'schema_version': 1,
  'person_id': 'person-1',
  'saved_count': 1,
  'pending_count': 2,
  'memories': [
    {
      'target_id': 'memory-1',
      'revision': 2,
      'statement': 'Prefers focused mornings',
      'memory_kind': 'preference',
      'epistemic_status': 'fact',
      'confidence_millis': 900,
      'source_count': 1,
      'origin': 'learned',
      'created_at': '2026-09-10T01:00:00Z',
    },
  ],
};
